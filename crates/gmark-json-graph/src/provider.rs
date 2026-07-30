// @author kongweiguang

use crate::{
    CancellationSignal, DocumentSnapshot, JsonGraphError, JsonGraphProjection, JsonGraphRequest,
    SourceLocator, parser,
};

#[derive(Default)]
pub struct JsonGraphProvider;

impl JsonGraphProvider {
    pub fn build(
        &self,
        document: &dyn DocumentSnapshot,
        request: &JsonGraphRequest,
        cancellation: &dyn CancellationSignal,
    ) -> Result<JsonGraphSnapshot, JsonGraphError> {
        if document.revision().0 != request.revision {
            return Err(JsonGraphError::SourceChanged);
        }
        let (range, root_path, root_label) = request.root.as_ref().map_or_else(
            || (0..document.len(), "$".to_owned(), "$".to_owned()),
            |root| {
                (
                    root.source.range.clone(),
                    root.json_path.to_string(),
                    root.label.to_string(),
                )
            },
        );
        if range.start > range.end || range.end > document.len() {
            return Err(JsonGraphError::InvalidRange {
                start: range.start,
                end: range.end,
                len: document.len(),
            });
        }
        let projection = parser::parse(
            document,
            range,
            request.item_limit.max(1),
            cancellation,
            root_path,
            root_label,
        )?;
        let locators = projection
            .nodes
            .iter()
            .map(|node| node.source.clone())
            .collect();
        Ok(JsonGraphSnapshot {
            document_epoch: request.document_epoch,
            revision: request.revision,
            generation: request.generation,
            projection,
            locators,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphSnapshot {
    pub document_epoch: u64,
    pub revision: u64,
    pub generation: u64,
    projection: JsonGraphProjection,
    locators: Vec<SourceLocator>,
}

impl JsonGraphRequest {
    pub fn accepts(&self, snapshot: &JsonGraphSnapshot) -> bool {
        self.document_epoch == snapshot.document_epoch
            && self.revision == snapshot.revision
            && self.generation == snapshot.generation
    }
}

impl JsonGraphSnapshot {
    pub fn projection(&self) -> &JsonGraphProjection {
        &self.projection
    }

    pub fn source_locators(&self) -> &[SourceLocator] {
        &self.locators
    }
}

impl gmark_document_core::DerivedProjectionSnapshot for JsonGraphSnapshot {
    fn document_epoch(&self) -> u64 {
        self.document_epoch
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn status(&self) -> gmark_document_core::DerivedProjectionStatus {
        if self.projection.truncated {
            gmark_document_core::DerivedProjectionStatus::LimitExceeded
        } else {
            gmark_document_core::DerivedProjectionStatus::Ready
        }
    }

    fn source_locators(&self) -> &[SourceLocator] {
        &self.locators
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
