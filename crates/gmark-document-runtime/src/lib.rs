// @author kongweiguang

//! 文档会话运行时：统一 Resident Rope 与 Paged PieceDocument 的权威状态。

use std::io::{Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmark_document_core::{
    DocumentBackendKind, DocumentProfile, DocumentRevision, DocumentSnapshot, DocumentViewId,
    DocumentViewState, EditError, LoadingLimits, OpenError, OpenPlan, OpenReason, SourceSelection,
    Transaction,
};
use gmark_paged_document::{
    EncodedSavePlan, ExternalChange, FileSource, LineIndex, PagedDocument, PagedDocumentError,
    SearchCancellation, SearchMatch, SearchOptions, ViewportRequest, ViewportSnapshot,
};
use thiserror::Error;

mod resident;

pub use resident::ResidentDocument;

/// 打开时校验过的文件身份。后端类型不属于持久身份，每次打开都必须重新规划。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileIdentity {
    pub canonical_path: PathBuf,
    pub len: u64,
    pub modified_nanos: Option<u128>,
    pub platform_id: Option<Arc<str>>,
}

impl From<&gmark_paged_document::FileIdentity> for FileIdentity {
    fn from(value: &gmark_paged_document::FileIdentity) -> Self {
        Self {
            canonical_path: value.path.clone(),
            len: value.len,
            modified_nanos: value.modified_nanos,
            platform_id: value
                .os_file_id
                .as_ref()
                .map(|value| Arc::<str>::from(format!("{value:?}"))),
        }
    }
}

/// 两个已知后端的显式和类型；格式 Provider 不得据此分支视图能力。
#[derive(Clone)]
pub enum DocumentStore {
    Resident(Box<ResidentDocument>),
    Paged(Box<PagedDocument>),
}

impl DocumentStore {
    pub const fn kind(&self) -> DocumentBackendKind {
        match self {
            Self::Resident(_) => DocumentBackendKind::Resident,
            Self::Paged(_) => DocumentBackendKind::Paged,
        }
    }

    pub fn revision(&self) -> DocumentRevision {
        match self {
            Self::Resident(document) => DocumentRevision(document.revision()),
            Self::Paged(document) => DocumentRevision(document.revision()),
        }
    }

    pub fn snapshot(&self) -> Arc<dyn DocumentSnapshot> {
        match self {
            Self::Resident(document) => document.snapshot(),
            Self::Paged(document) => Arc::new((**document).clone()),
        }
    }

    fn apply_transaction(&mut self, transaction: &Transaction) -> Result<(), SessionEditError> {
        if transaction.base_revision != self.revision() {
            return Err(SessionEditError::Edit(EditError::StaleRevision {
                expected: self.revision(),
                actual: transaction.base_revision,
            }));
        }
        match self {
            Self::Resident(document) => document
                .apply_transaction(transaction)
                .map_err(SessionEditError::Resident),
            Self::Paged(document) => apply_paged_transaction(document, transaction),
        }
    }
}

/// Tab 唯一权威文档状态。正文、revision、dirty 与视图恢复状态不得在 UI 另存副本。
#[derive(Clone)]
pub struct DocumentSession {
    pub profile: DocumentProfile,
    pub store: DocumentStore,
    pub active_view: DocumentViewId,
    pub view_state: DocumentViewState,
    pub dirty: bool,
    pub file_identity: FileIdentity,
    pub loading_limits: LoadingLimits,
    resident_growth_reason: Option<OpenReason>,
    allowed_views: Arc<[DocumentViewId]>,
}

impl DocumentSession {
    pub fn new(
        profile: DocumentProfile,
        store: DocumentStore,
        plan: OpenPlan,
        file_identity: FileIdentity,
    ) -> Result<Self, OpenError> {
        if store.kind() != plan.backend {
            return Err(OpenError::BackendMismatch {
                planned: plan.backend,
                actual: store.kind(),
            });
        }
        let loading_limits = plan.limits;
        let allowed_views: Arc<[DocumentViewId]> = plan
            .allowed_views
            .into_iter()
            .map(|descriptor| descriptor.id)
            .collect::<Vec<_>>()
            .into();
        if !allowed_views.contains(&plan.initial_view) {
            return Err(OpenError::InitialViewUnavailable(plan.initial_view));
        }
        let view_state = DocumentViewState {
            active_view: Some(plan.initial_view.clone()),
            ..DocumentViewState::default()
        };
        Ok(Self {
            profile,
            store,
            active_view: plan.initial_view,
            view_state,
            dirty: false,
            file_identity,
            loading_limits,
            resident_growth_reason: None,
            allowed_views,
        })
    }

    pub fn allowed_views(&self) -> &[DocumentViewId] {
        &self.allowed_views
    }

    pub fn resident_source_document(&self) -> Option<&gmark_document::SourceDocument> {
        match &self.store {
            DocumentStore::Resident(document) => Some(document.source_document()),
            DocumentStore::Paged(_) => None,
        }
    }

    pub fn resident_source_document_mut(&mut self) -> Option<&mut gmark_document::SourceDocument> {
        match &mut self.store {
            DocumentStore::Resident(document) => Some(document.source_document_mut()),
            DocumentStore::Paged(_) => None,
        }
    }

    pub fn refresh_resident_source_state(&mut self) {
        if let DocumentStore::Resident(document) = &mut self.store {
            document.refresh_source_state();
            self.dirty = !document.is_pristine();
        }
        self.refresh_resident_profile();
    }

    pub fn set_active_view(&mut self, view: DocumentViewId) -> Result<(), OpenError> {
        if !self.allowed_views.contains(&view) {
            return Err(OpenError::ViewUnavailable(view));
        }
        self.active_view = view.clone();
        self.view_state.active_view = Some(view);
        Ok(())
    }

    pub fn snapshot(&self) -> Arc<dyn DocumentSnapshot> {
        self.store.snapshot()
    }

    pub fn apply_transaction(
        &mut self,
        transaction: &Transaction,
        selection_after: SourceSelection,
    ) -> Result<DocumentRevision, SessionEditError> {
        self.store.apply_transaction(transaction)?;
        self.view_state.source.selection = selection_after;
        if !transaction.edits.is_empty() {
            self.dirty = true;
        }
        self.refresh_resident_profile();
        Ok(self.store.revision())
    }

    /// 保存协调器完成原子替换和回读后才可提交新基线。
    pub fn mark_persisted(&mut self, identity: FileIdentity) {
        self.file_identity = identity;
        if let DocumentStore::Resident(document) = &mut self.store {
            document.mark_persisted();
        }
        self.dirty = false;
    }

    pub fn mark_current_content_persisted(&mut self) {
        if let DocumentStore::Resident(document) = &mut self.store {
            document.mark_persisted();
        }
        self.dirty = false;
    }

    pub fn mark_resident_snapshot_persisted(&mut self, text: impl Into<Arc<str>>) {
        if let DocumentStore::Resident(document) = &mut self.store {
            document.mark_persisted_text(text);
            self.dirty = !document.is_pristine();
        }
    }

    pub fn revision(&self) -> u64 {
        self.store.revision().0
    }

    pub fn len(&self) -> u64 {
        match &self.store {
            DocumentStore::Resident(document) => document.len(),
            DocumentStore::Paged(document) => document.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match &self.store {
            DocumentStore::Resident(document) => document.is_empty(),
            DocumentStore::Paged(document) => document.is_empty(),
        }
    }

    pub fn is_pristine(&self) -> bool {
        match &self.store {
            DocumentStore::Resident(document) => document.is_pristine(),
            DocumentStore::Paged(document) => document.is_pristine(),
        }
    }

    pub fn line_count(&self) -> u64 {
        match &self.store {
            DocumentStore::Resident(document) => document.line_count(),
            DocumentStore::Paged(document) => document.line_count(),
        }
    }

    pub fn resident_growth_reason(&self) -> Option<OpenReason> {
        self.resident_growth_reason
    }

    fn refresh_resident_profile(&mut self) {
        let DocumentStore::Resident(document) = &self.store else {
            return;
        };
        self.profile.len = document.len();
        self.profile.estimated_lines = document.line_count();
        self.profile.estimated_structural_units = document.structural_units();
        if self.resident_growth_reason.is_none() {
            self.resident_growth_reason = self.loading_limits.exceeded_reason(&self.profile);
        }
    }

    pub fn line_range(&self, line: u64) -> Option<Range<u64>> {
        match &self.store {
            DocumentStore::Resident(document) => document.line_range(line),
            DocumentStore::Paged(document) => document.line_range(line),
        }
    }

    pub fn line_for_offset(&self, offset: u64) -> Option<u64> {
        match &self.store {
            DocumentStore::Resident(document) => document.line_for_offset(offset),
            DocumentStore::Paged(document) => document.line_for_offset(offset),
        }
    }

    pub fn line_index(&self) -> Option<LineIndex> {
        match &self.store {
            DocumentStore::Resident(_) => None,
            DocumentStore::Paged(document) => Some(document.line_index()),
        }
    }

    pub fn source_selection(&self) -> SourceSelection {
        match &self.store {
            DocumentStore::Resident(document) => document.source_selection(),
            DocumentStore::Paged(document) => document.source_selection(),
        }
    }

    pub fn set_selection(&mut self, range: Range<u64>, reversed: bool) {
        match &mut self.store {
            DocumentStore::Resident(document) => document.set_selection(range, reversed),
            DocumentStore::Paged(document) => document.set_selection(range, reversed),
        }
    }

    pub fn set_source_selection(&mut self, selection: SourceSelection) {
        match &mut self.store {
            DocumentStore::Resident(document) => document.set_source_selection(selection),
            DocumentStore::Paged(document) => document.set_source_selection(selection),
        }
    }

    pub fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>, PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => document.read_range(range),
            DocumentStore::Paged(document) => document.read_range(range),
        }
    }

    pub fn serialized_bytes(&self) -> Result<Vec<u8>, PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => document.encoded_bytes(),
            DocumentStore::Paged(document) => document.read_range(0..document.len()),
        }
    }

    pub fn read_range_cancellable(
        &self,
        range: Range<u64>,
        cancellation: &SearchCancellation,
    ) -> Result<Vec<u8>, PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => {
                document.read_range_cancellable(range, cancellation)
            }
            DocumentStore::Paged(document) => document.read_range_cancellable(range, cancellation),
        }
    }

    pub fn read_viewport(
        &self,
        request: &ViewportRequest,
    ) -> Result<ViewportSnapshot, PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => document.read_viewport(request),
            DocumentStore::Paged(document) => document.read_viewport(request),
        }
    }

    pub fn read_viewport_cancellable(
        &self,
        request: &ViewportRequest,
        cancellation: &SearchCancellation,
    ) -> Result<ViewportSnapshot, PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => {
                document.read_viewport_cancellable(request, cancellation)
            }
            DocumentStore::Paged(document) => {
                document.read_viewport_cancellable(request, cancellation)
            }
        }
    }

    pub fn write_to(&self, output: impl Write) -> Result<(), PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => document.write_to(output),
            DocumentStore::Paged(document) => document.write_to(output),
        }
    }

    pub fn write_to_cancellable(
        &self,
        output: impl Write,
        cancellation: &SearchCancellation,
    ) -> Result<(), PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => {
                document.write_to_cancellable(output, cancellation)
            }
            DocumentStore::Paged(document) => document.write_to_cancellable(output, cancellation),
        }
    }

    pub fn search(
        &self,
        query: &str,
        options: SearchOptions,
        cancellation: &SearchCancellation,
    ) -> Result<Vec<SearchMatch>, PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => document.search(query, options, cancellation),
            DocumentStore::Paged(document) => document.search(query, options, cancellation),
        }
    }

    pub fn external_change(&self) -> Result<ExternalChange, PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => document.external_change(),
            DocumentStore::Paged(document) => document.external_change(),
        }
    }

    pub fn accept_external_append(
        &mut self,
        source: FileSource,
        index: LineIndex,
    ) -> Result<(), PagedDocumentError> {
        match &mut self.store {
            DocumentStore::Resident(document) => document.accept_external_append(source),
            DocumentStore::Paged(document) => document.accept_external_append(source, index),
        }
    }

    pub fn replace_text(
        &mut self,
        range: Range<u64>,
        replacement: &str,
    ) -> Result<(), PagedDocumentError> {
        let result = match &mut self.store {
            DocumentStore::Resident(document) => document.replace_text(range, replacement),
            DocumentStore::Paged(document) => document.replace_text(range, replacement),
        };
        if result.is_ok() {
            self.dirty = !self.is_pristine();
            self.refresh_resident_profile();
        }
        result
    }

    pub fn replace_text_reader(
        &mut self,
        range: Range<u64>,
        reader: impl Read,
    ) -> Result<(), PagedDocumentError> {
        let result = match &mut self.store {
            DocumentStore::Resident(document) => document.replace_text_reader(range, reader),
            DocumentStore::Paged(document) => document.replace_text_reader(range, reader),
        };
        if result.is_ok() {
            self.dirty = !self.is_pristine();
            self.refresh_resident_profile();
        }
        result
    }

    pub fn apply_source_transaction(
        &mut self,
        transaction: &Transaction,
    ) -> Result<(), PagedDocumentError> {
        let selection = transaction
            .edits
            .iter()
            .min_by_key(|edit| edit.range.start)
            .map_or_else(
                || self.source_selection(),
                |edit| {
                    SourceSelection::collapsed(
                        edit.range
                            .start
                            .saturating_add(edit.replacement.len() as u64),
                        gmark_document_core::SourceAffinity::After,
                    )
                },
            );
        self.apply_transaction(transaction, selection)
            .map(|_| ())
            .map_err(|error| PagedDocumentError::InvalidTransaction(error.to_string()))
    }

    pub fn undo(&mut self) -> bool {
        let changed = match &mut self.store {
            DocumentStore::Resident(document) => document.undo(),
            DocumentStore::Paged(document) => document.undo(),
        };
        if changed {
            self.dirty = !self.is_pristine();
            self.refresh_resident_profile();
        }
        changed
    }

    pub fn redo(&mut self) -> bool {
        let changed = match &mut self.store {
            DocumentStore::Resident(document) => document.redo(),
            DocumentStore::Paged(document) => document.redo(),
        };
        if changed {
            self.dirty = !self.is_pristine();
            self.refresh_resident_profile();
        }
        changed
    }

    pub fn save_atomic_cancellable(
        &mut self,
        path: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<(), PagedDocumentError> {
        match &mut self.store {
            DocumentStore::Resident(document) => {
                document.save_atomic_cancellable(path, cancellation)
            }
            DocumentStore::Paged(document) => document.save_atomic_cancellable(path, cancellation),
        }
    }

    pub fn save_encoded_atomic_cancellable(
        &self,
        plan: &EncodedSavePlan,
        path: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<gmark_paged_document::FileIdentity, PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => {
                document.save_copy_atomic_cancellable(path, cancellation)
            }
            DocumentStore::Paged(document) => {
                document.save_encoded_atomic_cancellable(plan, path, cancellation)
            }
        }
    }

    pub fn save_encoded_atomic_as_cancellable(
        &self,
        plan: &EncodedSavePlan,
        path: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<gmark_paged_document::FileIdentity, PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => {
                document.save_copy_atomic_cancellable(path, cancellation)
            }
            DocumentStore::Paged(document) => {
                document.save_encoded_atomic_as_cancellable(plan, path, cancellation)
            }
        }
    }

    pub fn save_range_atomic_cancellable(
        &self,
        range: Range<u64>,
        path: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<(), PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => {
                document.save_range_atomic_cancellable(range, path, cancellation)
            }
            DocumentStore::Paged(document) => {
                document.save_range_atomic_cancellable(range, path, cancellation)
            }
        }
    }

    pub fn save_encoded_range_atomic_cancellable(
        &self,
        plan: &EncodedSavePlan,
        range: Range<u64>,
        path: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<gmark_paged_document::FileIdentity, PagedDocumentError> {
        match &self.store {
            DocumentStore::Resident(document) => {
                document.save_range_atomic_cancellable(range, &path, cancellation)?;
                FileSource::open(path.as_ref())?.identity()
            }
            DocumentStore::Paged(document) => {
                document.save_encoded_range_atomic_cancellable(plan, range, path, cancellation)
            }
        }
    }
}

impl DocumentSnapshot for DocumentSession {
    fn revision(&self) -> DocumentRevision {
        self.store.revision()
    }

    fn len(&self) -> u64 {
        DocumentSession::len(self)
    }

    fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>, gmark_document_core::SnapshotError> {
        DocumentSession::read_range(self, range).map_err(|error| match error {
            PagedDocumentError::InvalidRange { start, end, len } => {
                gmark_document_core::SnapshotError::InvalidRange { start, end, len }
            }
            PagedDocumentError::RangeTooLarge => gmark_document_core::SnapshotError::RangeTooLarge,
            error => gmark_document_core::SnapshotError::Read(error.to_string()),
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SessionEditError {
    #[error(transparent)]
    Edit(#[from] EditError),
    #[error("Resident transaction 失败: {0}")]
    Resident(String),
    #[error("Paged transaction 失败: {0}")]
    Paged(String),
}

fn apply_paged_transaction(
    document: &mut PagedDocument,
    transaction: &Transaction,
) -> Result<(), SessionEditError> {
    document
        .apply_transaction(transaction)
        .map_err(|error| SessionEditError::Paged(error.to_string()))
}

#[cfg(test)]
mod tests;
