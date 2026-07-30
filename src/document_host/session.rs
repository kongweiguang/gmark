// @author kongweiguang

//! Session planning and recovery-journal adaptation for the document host.

use super::*;

pub(super) fn session_plan(
    profile: &gmark_document_core::DocumentProfile,
    probe: &OpenProbe,
    strategy: OpenStrategy,
    retain_resident_session: bool,
) -> gmark_document_core::OpenPlan {
    let mut policy = gmark_document_core::LoadingPolicy {
        max_resident_bytes: Some(probe.options.max_resident_bytes),
        force_safe_source: probe.force_safe_source,
    };
    // 恢复日志可能显式要求 Paged，即使当前文件已缩小；仍沿用打开时阈值，
    // 只把本次会话强制为安全 Source，不污染偏好设置。
    if strategy == OpenStrategy::Paged
        && gmark_document_core::OpenPolicyResolver
            .resolve(policy, profile)
            .backend
            == gmark_document_core::DocumentBackendKind::Resident
    {
        policy.force_safe_source = true;
    }
    let plan = gmark_document_core::OpenPolicyResolver.resolve(policy, profile);
    if retain_resident_session
        && strategy == OpenStrategy::Resident
        && plan.backend == gmark_document_core::DocumentBackendKind::Paged
    {
        let mut resident = gmark_document_core::OpenPolicyResolver.resolve(
            gmark_document_core::LoadingPolicy {
                max_resident_bytes: Some(u64::MAX),
                ..gmark_document_core::LoadingPolicy::default()
            },
            profile,
        );
        resident.limits = plan.limits;
        return resident;
    }
    plan
}

/// Probe 后只在这里安装正文后端；格式 Controller 之后只能持有统一 session。
pub(super) fn build_document_session(
    probe: &OpenProbe,
    original_source: &FileSource,
    utf8_source: FileSource,
    index: LineIndex,
    retain_resident_session: bool,
) -> Result<DocumentSession, PagedDocumentError> {
    let profile = probe.profile();
    let plan = session_plan(&profile, probe, probe.strategy, retain_resident_session);
    let source_identity = original_source.identity()?;
    if source_identity != probe.identity {
        return Err(PagedDocumentError::SourceChanged);
    }
    let file_identity = gmark_document_runtime::FileIdentity::from(&source_identity);
    let store = if probe.strategy == OpenStrategy::Resident {
        let bytes = utf8_source.read_range(0, utf8_source.identity()?.len)?;
        if original_source.identity()? != probe.identity {
            return Err(PagedDocumentError::SourceChanged);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| PagedDocumentError::Binary)?;
        gmark_document_runtime::DocumentStore::Resident(Box::new(
            gmark_document_runtime::ResidentDocument::new(
                text,
                probe.encoding.clone(),
                source_identity,
            ),
        ))
    } else {
        let document = PieceDocument::open(utf8_source, index)?;
        gmark_document_runtime::DocumentStore::Paged(Box::new(PagedDocumentAdapter::new(document)))
    };
    if original_source.identity()? != probe.identity {
        return Err(PagedDocumentError::SourceChanged);
    }
    DocumentSession::new(profile, store, plan, file_identity)
        .map_err(|error| PagedDocumentError::InvalidTransaction(error.to_string()))
}

pub(super) fn build_paged_session(
    probe: &OpenProbe,
    document: PieceDocument,
    identity: gmark_paged_document::FileIdentity,
) -> Result<DocumentSession, PagedDocumentError> {
    let profile = probe.profile();
    let mut plan = session_plan(&profile, probe, OpenStrategy::Paged, false);
    if derived_views_enabled(probe.strategy) {
        // 恢复正文用 PieceDocument 保留 undo/redo，所以后端仍是 Paged；但原文件在
        // Resident 阈值内时，结构索引由恢复快照构建，必须继承其表格/图视图白名单。
        let resident_plan = session_plan(&profile, probe, OpenStrategy::Resident, false);
        plan.allowed_views = resident_plan.allowed_views;
    }
    DocumentSession::new(
        profile,
        gmark_document_runtime::DocumentStore::Paged(Box::new(PagedDocumentAdapter::new(document))),
        plan,
        gmark_document_runtime::FileIdentity::from(&identity),
    )
    .map_err(|error| PagedDocumentError::InvalidTransaction(error.to_string()))
}

pub(super) fn verify_saved_session_readback(
    expected: &DocumentSession,
    actual: &DocumentSession,
    cancellation: &SearchCancellation,
) -> Result<(), PagedDocumentError> {
    if expected.len() != actual.len() {
        return Err(PagedDocumentError::InvalidTransaction(
            "saved readback length differs from the save snapshot".into(),
        ));
    }
    const VERIFY_CHUNK_BYTES: u64 = 8 * 1024 * 1024;
    let mut start = 0u64;
    while start < expected.len() {
        if cancellation.is_cancelled() {
            return Err(PagedDocumentError::Cancelled);
        }
        let end = start.saturating_add(VERIFY_CHUNK_BYTES).min(expected.len());
        if expected.read_range(start..end)? != actual.read_range(start..end)? {
            return Err(PagedDocumentError::InvalidTransaction(
                "saved readback bytes differ from the save snapshot".into(),
            ));
        }
        start = end;
    }
    Ok(())
}

type StructureInput = (FileSource, LineIndex, Option<Arc<[u8]>>);

pub(super) fn structure_input_for_session(
    document: &DocumentSession,
    prepared_source: &PreparedUtf8Source,
    prepared_index: &LineIndex,
    cancellation: &SearchCancellation,
) -> Result<StructureInput, PagedDocumentError> {
    if document.store.kind() == gmark_document_core::DocumentBackendKind::Paged {
        return Ok((
            prepared_source.source().clone(),
            prepared_index.clone(),
            None,
        ));
    }
    if cancellation.is_cancelled() {
        return Err(PagedDocumentError::Cancelled);
    }
    let bytes: Arc<[u8]> = document
        .snapshot()
        .read_range(0..document.len())
        .map_err(|error| PagedDocumentError::InvalidTransaction(error.to_string()))?
        .into();
    Ok((
        prepared_source.source().clone(),
        prepared_index.clone(),
        Some(bytes),
    ))
}

pub(super) fn modifier_horizontal_wheel_delta(
    shift: bool,
    control: bool,
    delta_x: f32,
    delta_y: f32,
) -> Option<f32> {
    ((shift || control) && delta_y.abs() >= delta_x.abs()).then_some(delta_y)
}

pub(super) fn recovery_view_id(mode: DocumentHostViewMode) -> DocumentViewId {
    match mode {
        DocumentHostViewMode::Source => DocumentViewId::source(),
        DocumentHostViewMode::Live => DocumentViewId::new("live"),
        DocumentHostViewMode::Structure => DocumentViewId::new("preview"),
        DocumentHostViewMode::Split => DocumentViewId::new("split"),
    }
}

/// Recovery backend selection follows the installed document store, not the file's
/// most recent probe. This keeps a resident session on the runtime journal contract
/// even when its on-disk file later grows past the open threshold.
pub(crate) enum DocumentRecoveryJournal {
    Resident(Box<ResidentRecoveryJournal>),
    Paged(PagedRecoveryJournal),
}

impl DocumentRecoveryJournal {
    pub(super) fn create(
        recovery_dir: &Path,
        source: &FileSource,
        encoding: TextEncoding,
        document: &DocumentSession,
    ) -> Result<Self, PagedDocumentError> {
        match document.store.kind() {
            gmark_document_core::DocumentBackendKind::Resident => {
                let source_document = document.resident_source_document().ok_or_else(|| {
                    PagedDocumentError::Recovery(
                        "resident recovery requires a resident source document".to_owned(),
                    )
                })?;
                ResidentRecoveryJournal::create_formatted(
                    recovery_dir,
                    Some(source.path().to_path_buf()),
                    source_document.text(),
                    source_document.source_format(),
                )
                .map(|journal| Self::Resident(Box::new(journal)))
                .map_err(map_resident_recovery_error)
            }
            gmark_document_core::DocumentBackendKind::Paged => {
                PagedRecoveryJournal::create(recovery_dir, source, encoding).map(Self::Paged)
            }
        }
    }

    /// Resident journals receive the resulting source snapshot so undo/redo and
    /// formatting-only changes retain the same journal semantics as direct edits.
    pub(super) fn record_after_change(
        &mut self,
        document: &DocumentSession,
        record: &RecoveryRecord,
    ) -> Result<(), gmark_document_core::PersistenceError> {
        match self {
            Self::Resident(journal) => {
                let source_document = document.resident_source_document().ok_or_else(|| {
                    gmark_document_core::PersistenceError::Recovery(
                        "resident recovery requires a resident source document".to_owned(),
                    )
                })?;
                journal
                    .record_formatted(
                        &source_document.text(),
                        source_document.source_format(),
                        record
                            .selection
                            .unwrap_or_else(|| document.source_selection()),
                        record.view_id.as_str(),
                    )
                    .map(|_| ())
                    .map_err(|error| {
                        gmark_document_core::PersistenceError::Recovery(error.to_string())
                    })
            }
            Self::Paged(journal) => journal.record(record),
        }
    }

    /// A successful save/discard removes the old durable session. Resident
    /// journals additionally refresh their in-memory clean baseline so a failed
    /// removal remains retryable through the existing error path.
    pub(super) fn checkpoint(
        &mut self,
        document: &DocumentSession,
    ) -> Result<(), PagedDocumentError> {
        match self {
            Self::Resident(journal) => {
                let source_document = document.resident_source_document().ok_or_else(|| {
                    PagedDocumentError::Recovery(
                        "resident recovery requires a resident source document".to_owned(),
                    )
                })?;
                journal
                    .checkpoint_formatted(
                        Some(document.file_identity.canonical_path.clone()),
                        source_document.text(),
                        source_document.source_format(),
                    )
                    .map_err(map_resident_recovery_error)
            }
            Self::Paged(journal) => journal.checkpoint(),
        }
    }

    pub(super) fn discard(self) -> Result<(), PagedDocumentError> {
        match self {
            Self::Resident(journal) => journal.discard().map_err(map_resident_recovery_error),
            Self::Paged(journal) => journal.checkpoint(),
        }
    }
}

fn map_resident_recovery_error(error: ResidentRecoveryError) -> PagedDocumentError {
    PagedDocumentError::Recovery(error.to_string())
}

pub(super) fn record_recovery_transaction(
    journal: &mut DocumentRecoveryJournal,
    document: &DocumentSession,
    base_revision: u64,
    range: Range<u64>,
    replacement: impl Into<Arc<str>>,
    selection: Option<SourceSelection>,
    view_id: DocumentViewId,
) -> Result<(), gmark_document_core::PersistenceError> {
    journal.record_after_change(
        document,
        &RecoveryRecord {
            action: RecoveryAction::Transaction(Transaction::new(
                gmark_document_core::DocumentRevision(base_revision),
                vec![SourceEdit::new(range, replacement)],
            )),
            selection,
            view_id,
        },
    )
}

pub(super) fn derived_views_enabled(strategy: OpenStrategy) -> bool {
    strategy == OpenStrategy::Resident
}
