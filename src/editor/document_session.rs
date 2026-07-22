// @author kongweiguang

//! Markdown Editor 对统一 DocumentSession 的窄 facade。

use std::ops::Deref;
use std::path::PathBuf;

use gmark_document::{
    DocumentError, DocumentSnapshot, LineEnding, Revision, SourceDocument, SourceFormatSnapshot,
    SourceFormatSummary, Transaction,
};
use gmark_document_core::{
    DocumentFormat, DocumentProfile, LoadingLimits, LoadingPolicy, OpenReason, SourceSelection,
    TextEncoding,
};
use gmark_document_runtime::{DocumentSession, DocumentStore, FileIdentity, ResidentDocument};

/// Markdown controller 继续使用成熟的 SourceDocument API，但正文只存放在 session 中。
#[derive(Clone)]
pub(super) struct EditorDocumentSession {
    session: DocumentSession,
}

impl EditorDocumentSession {
    pub(super) fn new(source: SourceDocument) -> Self {
        Self::new_with_open_context(
            source,
            LoadingPolicy::default().effective_limits(),
            TextEncoding::Utf8 { bom: false },
            None,
        )
    }

    pub(super) fn new_with_open_context(
        source: SourceDocument,
        limits: LoadingLimits,
        text_encoding: TextEncoding,
        source_identity: Option<gmark_paged_document::FileIdentity>,
    ) -> Self {
        let text = source.text();
        let len = text.len() as u64;
        let profile = DocumentProfile {
            format: DocumentFormat::Markdown,
            encoding: text_encoding,
            len,
            estimated_lines: text.lines().count().max(1) as u64,
            estimated_structural_units: 0,
        };
        let plan = LoadingPolicy {
            max_resident_bytes: Some(limits.max_resident_bytes),
            max_resident_lines: Some(limits.max_resident_lines),
            max_structural_units: Some(limits.max_structural_units),
            ..LoadingPolicy::default()
        }
        .resolve(&profile);
        let paged_identity =
            source_identity.unwrap_or_else(|| gmark_paged_document::FileIdentity {
                path: PathBuf::new(),
                len,
                modified_nanos: None,
                os_file_id: None,
            });
        let identity = FileIdentity::from(&paged_identity);
        let store = DocumentStore::Resident(Box::new(ResidentDocument::from_source_document(
            source,
            profile.encoding.clone(),
            paged_identity,
        )));
        let session = DocumentSession::new(profile, store, plan, identity)
            .expect("Markdown Resident plan must match its store");
        Self { session }
    }

    pub(super) fn apply_transaction(
        &mut self,
        transaction: Transaction,
    ) -> Result<DocumentSnapshot, DocumentError> {
        let result = self.source_mut().apply_transaction(transaction);
        self.session.refresh_resident_source_state();
        result
    }

    pub(super) fn undo(&mut self) -> Result<Option<DocumentSnapshot>, DocumentError> {
        let result = self.source_mut().undo();
        self.session.refresh_resident_source_state();
        result
    }

    pub(super) fn redo(&mut self) -> Result<Option<DocumentSnapshot>, DocumentError> {
        let result = self.source_mut().redo();
        self.session.refresh_resident_source_state();
        result
    }

    pub(super) fn normalize_line_endings(
        &mut self,
        ending: LineEnding,
    ) -> Result<Option<DocumentSnapshot>, DocumentError> {
        let result = self.source_mut().normalize_line_endings(ending);
        self.session.refresh_resident_source_state();
        result
    }

    pub(super) fn restore_source_format(&mut self, format: SourceFormatSnapshot) -> bool {
        let restored = self.source_mut().restore_source_format(format);
        self.session.refresh_resident_source_state();
        restored
    }

    pub(super) fn revision(&self) -> Revision {
        self.source().revision()
    }

    pub(super) fn snapshot(&self) -> DocumentSnapshot {
        self.source().snapshot()
    }

    pub(super) fn text(&self) -> String {
        self.source().text()
    }

    pub(super) fn len(&self) -> usize {
        self.source().len()
    }

    pub(super) fn source_format(&self) -> SourceFormatSnapshot {
        self.source().source_format()
    }

    pub(super) fn source_format_summary(&self) -> SourceFormatSummary {
        self.source().source_format_summary()
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.session.dirty
    }

    pub(super) fn resident_growth_reason(&self) -> Option<OpenReason> {
        self.session.resident_growth_reason()
    }

    pub(super) fn mark_dirty(&mut self) {
        self.session.dirty = true;
    }

    pub(super) fn mark_persisted(&mut self) {
        self.session.mark_current_content_persisted();
    }

    pub(super) fn mark_persisted_snapshot(&mut self, text: &str) {
        self.session.mark_resident_snapshot_persisted(text);
    }

    /// Resident 编辑器把可恢复的 Source 状态写回统一会话；UI 不再维护第二份选择真值。
    pub(super) fn sync_source_selection(&mut self, selection: SourceSelection) {
        self.session.view_state.source.selection = selection;
    }

    #[cfg(test)]
    pub(super) fn source_selection(&self) -> SourceSelection {
        self.session.view_state.source.selection
    }

    fn source(&self) -> &SourceDocument {
        self.session
            .resident_source_document()
            .expect("EditorDocumentSession is always Resident")
    }

    fn source_mut(&mut self) -> &mut SourceDocument {
        self.session
            .resident_source_document_mut()
            .expect("EditorDocumentSession is always Resident")
    }
}

impl From<SourceDocument> for EditorDocumentSession {
    fn from(source: SourceDocument) -> Self {
        Self::new(source)
    }
}

impl Deref for EditorDocumentSession {
    type Target = SourceDocument;

    fn deref(&self) -> &Self::Target {
        self.source()
    }
}
