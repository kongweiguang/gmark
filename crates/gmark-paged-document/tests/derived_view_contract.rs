// @author kongweiguang

use std::any::Any;
use std::sync::Arc;

use gmark_document_core::{
    DEFAULT_DELIMITED_COLUMN_WINDOW, DEFAULT_DELIMITED_ROW_WINDOW, DelimitedCellProjection,
    DelimitedWindowProjection, DerivedProjectionProvider, DerivedProjectionRequest,
    DerivedProjectionSnapshot, DerivedProjectionStatus, DocumentFormat, DocumentRevision,
    DocumentSnapshot, DocumentViewId, DocumentViewRegistry, ProjectionCancellation,
    ProjectionError, SourceEdit, SourceLocator, SourceViewState, Transaction, ViewDescriptor,
    ViewFormat,
};
use gmark_paged_document::{
    FileSource, LineIndex, MAX_SYSTEM_CLIPBOARD_BYTES, PagedDocument, PagedDocumentError,
    PieceDocument, SearchCancellation, SelectionTransfer, SourceAffinity, SourceAnchor,
    SourceSelection, selection_transfer_for_len,
};

struct TestSnapshot {
    epoch: u64,
    revision: u64,
    generation: u64,
    locators: Vec<SourceLocator>,
}

#[test]
fn delimited_models_keep_source_ranges_and_bounded_windows() {
    let table = DelimitedWindowProjection {
        record_range: 100..132,
        column_range: 4..12,
        cells: Arc::from([DelimitedCellProjection {
            record_index: 101,
            column_index: 7,
            source: SourceLocator::new(80..91),
            display_value: Arc::from("Ada, Lovelace"),
        }]),
    };
    assert_eq!(table.cells[0].source.range, 80..91);
    assert!(table.record_range.contains(&table.cells[0].record_index));
    assert!(table.column_range.contains(&table.cells[0].column_index));
    assert_eq!(DEFAULT_DELIMITED_ROW_WINDOW, 512);
    assert_eq!(DEFAULT_DELIMITED_COLUMN_WINDOW, 16);

    assert_eq!(DocumentViewId::markdown_live().as_str(), "markdown-live");
    assert_eq!(DocumentViewId::markdown_split().as_str(), "markdown-split");
    assert_eq!(
        DocumentViewId::markdown_preview().as_str(),
        "markdown-preview"
    );
}

#[test]
fn derived_edit_commits_one_source_transaction_and_rejects_stale_revision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("derived-edit.txt");
    std::fs::write(&path, "alpha beta gamma").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PagedDocument::new(PieceDocument::open(source, index).unwrap());
    let edit = Transaction {
        base_revision: DocumentRevision(document.revision()),
        edits: vec![
            SourceEdit {
                range: 0..5,
                replacement: Arc::from("ALPHA"),
            },
            SourceEdit {
                range: 11..16,
                replacement: Arc::from("GAMMA"),
            },
        ],
    };
    document.apply_transaction(&edit).unwrap();
    assert_eq!(
        String::from_utf8(document.read_range(0..document.len()).unwrap()).unwrap(),
        "ALPHA beta GAMMA"
    );
    assert!(document.undo(), "the batch must be one undo transaction");
    assert_eq!(
        String::from_utf8(document.read_range(0..document.len()).unwrap()).unwrap(),
        "alpha beta gamma"
    );
    assert!(matches!(
        document.apply_transaction(&edit),
        Err(PagedDocumentError::SourceChanged)
    ));
}

#[test]
fn source_selection_keeps_direction_without_changing_the_normalized_range() {
    let forward = SourceSelection::from_range(7..19, false);
    let reversed = SourceSelection::from_range(7..19, true);
    assert_eq!(forward.range(), 7..19);
    assert!(!forward.reversed());
    assert_eq!(reversed.range(), 7..19);
    assert!(reversed.reversed());
    assert_eq!(
        SourceSelection::from_range(9..9, true),
        SourceSelection::collapsed(9, SourceAffinity::Before)
    );
    assert_eq!(
        SourceViewState::default().top_byte_anchor,
        SourceAnchor::new(0, SourceAffinity::Before)
    );
}

#[test]
fn large_adapter_preserves_source_anchor_affinity_and_clamps_each_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source-affinity.txt");
    std::fs::write(&path, "alpha beta").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut adapter = PagedDocument::new(PieceDocument::open(source, index).unwrap());
    let selection = SourceSelection {
        anchor: SourceAnchor::new(500, SourceAffinity::After),
        head: SourceAnchor::new(2, SourceAffinity::Before),
    };

    adapter.set_source_selection(selection);
    assert_eq!(
        adapter.source_selection(),
        SourceSelection {
            anchor: SourceAnchor::new(adapter.len(), SourceAffinity::After),
            head: SourceAnchor::new(2, SourceAffinity::Before),
        }
    );
    assert_eq!(adapter.selection(), (2..adapter.len(), true));

    adapter.replace_text(2..5, "X").unwrap();
    assert_eq!(
        adapter.source_selection(),
        SourceSelection::collapsed(3, SourceAffinity::After)
    );
}

#[test]
fn large_adapter_undo_redo_restores_directional_source_selection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source-selection-history.txt");
    std::fs::write(&path, "a🙂b\nsecond").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut adapter = PagedDocument::new(PieceDocument::open(source, index).unwrap());
    let before = SourceSelection {
        anchor: SourceAnchor::new(5, SourceAffinity::After),
        head: SourceAnchor::new(1, SourceAffinity::Before),
    };
    adapter.set_source_selection(before);

    adapter.replace_text(1..5, "中").unwrap();
    let after = adapter.source_selection();
    assert_eq!(after, SourceSelection::collapsed(4, SourceAffinity::After));

    assert!(adapter.undo());
    assert_eq!(adapter.source_selection(), before);
    assert!(adapter.redo());
    assert_eq!(adapter.source_selection(), after);
}

impl DerivedProjectionSnapshot for TestSnapshot {
    fn document_epoch(&self) -> u64 {
        self.epoch
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn status(&self) -> DerivedProjectionStatus {
        DerivedProjectionStatus::Ready
    }

    fn source_locators(&self) -> &[SourceLocator] {
        &self.locators
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct TestProvider {
    descriptor: ViewDescriptor,
}

impl DerivedProjectionProvider for TestProvider {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn build(
        &self,
        document: &dyn DocumentSnapshot,
        request: &DerivedProjectionRequest,
        cancellation: &dyn ProjectionCancellation,
    ) -> Result<Arc<dyn DerivedProjectionSnapshot>, ProjectionError> {
        if cancellation.is_cancelled() {
            return Err(ProjectionError::Cancelled);
        }
        assert_eq!(document.revision().0, request.revision);
        Ok(Arc::new(TestSnapshot {
            epoch: request.document_epoch,
            revision: request.revision,
            generation: request.generation,
            locators: vec![SourceLocator::new(4..12)],
        }))
    }
}

#[test]
fn registry_only_exposes_registered_views_for_the_document_format() {
    let provider = Arc::new(TestProvider {
        descriptor: ViewDescriptor {
            id: DocumentViewId::json_graph(),
            label: Arc::from("JSON Graph"),
            icon: Arc::from("graph"),
            supported_formats: Arc::from([ViewFormat::Json]),
            available: true,
            read_only: true,
            max_items: Some(1_500),
        },
    });
    let mut registry = DocumentViewRegistry::default();
    assert!(registry.register(provider));
    assert_eq!(registry.available(&DocumentFormat::Json).len(), 1);
    assert!(registry.available(&DocumentFormat::PlainText).is_empty());
    assert!(
        registry
            .available_provider(&DocumentViewId::json_graph(), &DocumentFormat::Json)
            .is_some()
    );
    assert!(
        registry
            .available_provider(&DocumentViewId::json_graph(), &DocumentFormat::JsonLines)
            .is_none()
    );
    assert_eq!(
        registry
            .first_available_provider(&DocumentFormat::Json)
            .unwrap()
            .descriptor()
            .id,
        DocumentViewId::json_graph()
    );
    let json_views = registry.views_for(&DocumentFormat::Json);
    assert_eq!(json_views[0].id, DocumentViewId::source());
    assert_eq!(json_views[1].id, DocumentViewId::json_graph());
    assert_eq!(registry.views_for(&DocumentFormat::PlainText).len(), 1);
}

#[test]
fn clipboard_boundary_and_stale_source_transaction_are_explicit_contracts() {
    assert_eq!(
        selection_transfer_for_len(MAX_SYSTEM_CLIPBOARD_BYTES),
        SelectionTransfer::Clipboard
    );
    assert_eq!(
        selection_transfer_for_len(MAX_SYSTEM_CLIPBOARD_BYTES + 1),
        SelectionTransfer::ExportFile
    );

    let edit = Transaction {
        base_revision: DocumentRevision(41),
        edits: Vec::new(),
    };
    assert_eq!(edit.base_revision, DocumentRevision(41));
    assert_ne!(edit.base_revision, DocumentRevision(42));
}

#[test]
fn derived_snapshot_preserves_revision_generation_and_source_ranges() {
    struct TestDocument;
    impl DocumentSnapshot for TestDocument {
        fn revision(&self) -> gmark_document_core::DocumentRevision {
            gmark_document_core::DocumentRevision(11)
        }

        fn len(&self) -> u64 {
            16
        }

        fn read_range(
            &self,
            range: std::ops::Range<u64>,
        ) -> Result<Vec<u8>, gmark_document_core::SnapshotError> {
            Ok(vec![0; (range.end - range.start) as usize])
        }
    }
    let provider = TestProvider {
        descriptor: ViewDescriptor {
            id: DocumentViewId::delimited_table(),
            label: Arc::from("Table"),
            icon: Arc::from("table"),
            supported_formats: Arc::from([ViewFormat::Delimited]),
            available: true,
            read_only: true,
            max_items: None,
        },
    };
    let request = DerivedProjectionRequest {
        document_epoch: 7,
        revision: 11,
        generation: 13,
        root: None,
        item_limit: 128,
    };
    let snapshot = provider
        .build(&TestDocument, &request, &SearchCancellation::default())
        .unwrap();
    assert!(request.accepts(snapshot.as_ref()));
    assert_eq!(snapshot.document_epoch(), 7);
    assert_eq!(snapshot.revision(), 11);
    assert_eq!(snapshot.generation(), 13);
    assert_eq!(snapshot.status(), DerivedProjectionStatus::Ready);
    assert_eq!(snapshot.source_locators(), &[SourceLocator::new(4..12)]);
}
