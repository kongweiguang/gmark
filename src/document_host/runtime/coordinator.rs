// @author kongweiguang

use gmark_document_core::{DocumentRevision, RecoveryRecord};
use gmark_document_runtime::DocumentSaveSnapshot;
use gmark_paged_document::{ExternalChange, PagedDocumentError, SearchCancellation};
use gpui::{SharedString, Task};

use super::DocumentRecoveryJournal;
#[cfg(test)]
use super::SharedDocument;
use super::recovery_worker::RecoveryWorker;
use super::session::RetiredRecoveryJournal;

#[cfg(test)]
pub(crate) trait RecoveryDocument {
    fn checkpoint_recovery(
        &self,
        journal: &mut DocumentRecoveryJournal,
    ) -> Result<(), PagedDocumentError>;
}

#[cfg(test)]
impl RecoveryDocument for SharedDocument {
    fn checkpoint_recovery(
        &self,
        journal: &mut DocumentRecoveryJournal,
    ) -> Result<(), PagedDocumentError> {
        self.with_session(|session| journal.checkpoint(session))
            .map_err(|error| PagedDocumentError::InvalidTransaction(error.to_string()))
            .and_then(|result| result)
    }
}

#[cfg(test)]
impl RecoveryDocument for gmark_document_runtime::DocumentSession {
    fn checkpoint_recovery(
        &self,
        journal: &mut DocumentRecoveryJournal,
    ) -> Result<(), PagedDocumentError> {
        journal.checkpoint(self)
    }
}

/// 保留日志安装前唯一一个 Resident 恢复命令，避免首个编辑在后台创建日志
/// 的窗口内被静默丢弃，同时不为 Paged 的有序命令制造可合并的旁路队列。
pub(crate) struct PendingRecoveryRecord {
    pub(crate) snapshot: DocumentSaveSnapshot,
    pub(crate) record: RecoveryRecord,
}

pub(crate) struct SaveCoordinator {
    pub(crate) generation: u64,
    pub(crate) cancellation: Option<SearchCancellation>,
    pub(crate) task: Task<()>,
}

impl Default for SaveCoordinator {
    fn default() -> Self {
        Self {
            generation: 0,
            cancellation: None,
            task: Task::ready(()),
        }
    }
}

/// 统一拥有文档后台任务、取消令牌和代次门禁。
///
/// Controller 可以发起任务，但只有这里的 generation 与 cancellation 决定结果能否安装。
pub(crate) struct DocumentCoordinator {
    pub(crate) source_generation: u64,
    pub(crate) source_cancellation: Option<SearchCancellation>,
    pub(crate) search_generation: u64,
    pub(crate) search_cancellation: Option<SearchCancellation>,
    pub(crate) external_status: Option<SharedString>,
    pub(crate) pending_external_change: Option<ExternalChange>,
    pub(crate) external_monitor_paused: bool,
    pub(crate) external_generation: u64,
    pub(crate) index_generation: u64,
    pub(crate) index_cancellation: Option<SearchCancellation>,
    pub(crate) save: SaveCoordinator,
    pub(crate) recovery_journal: Option<DocumentRecoveryJournal>,
    /// Distinguish an intentionally disabled journal from one still being
    /// created so an early edit can report degraded recovery rather than fake durability.
    pub(crate) recovery_enabled: bool,
    /// Journal ownership moves here before the first edit; the UI only keeps
    /// the bounded sender and never performs journal I/O itself.
    pub(crate) recovery_worker: Option<RecoveryWorker>,
    /// 空文件日志异步创建期间只允许保留最新 Resident 快照；Paged 命令必须保持顺序，
    /// 因而不会进入这个旁路槽位。
    pub(crate) pending_recovery_record: Option<PendingRecoveryRecord>,
    /// 已保存但暂未删除的旧日志单独排队；不能放回 active 槽，否则安装新日志时会丢失重试权。
    pub(crate) retired_recovery_journals: Vec<RetiredRecoveryJournal>,
    pub(crate) recovery_error: Option<SharedString>,
    /// Recovery setup and worker callbacks use this host generation so a
    /// closed/reloaded view cannot install a late journal result.
    pub(crate) recovery_generation: u64,
    pub(crate) lifetime_cancellation: SearchCancellation,
    pub(crate) index_task: Task<()>,
    pub(crate) source_task: Task<()>,
    pub(crate) search_task: Task<()>,
    pub(crate) external_task: Task<()>,
}

impl DocumentCoordinator {
    pub(crate) fn new(lifetime_cancellation: SearchCancellation) -> Self {
        Self {
            source_generation: 0,
            source_cancellation: None,
            search_generation: 0,
            search_cancellation: None,
            external_status: None,
            pending_external_change: None,
            external_monitor_paused: false,
            external_generation: 0,
            index_generation: 0,
            index_cancellation: None,
            save: SaveCoordinator::default(),
            recovery_journal: None,
            recovery_enabled: false,
            recovery_worker: None,
            pending_recovery_record: None,
            retired_recovery_journals: Vec::new(),
            recovery_error: None,
            recovery_generation: 0,
            lifetime_cancellation,
            index_task: Task::ready(()),
            source_task: Task::ready(()),
            search_task: Task::ready(()),
            external_task: Task::ready(()),
        }
    }

    /// 在日志尚未安装时暂存一个不可变 Resident 命令，保证异步创建窗口不丢首个编辑。
    /// 返回 `false` 表示 Paged 命令不能被合并，调用方必须报告明确的降级错误。
    pub(crate) fn stage_pending_recovery(
        &mut self,
        snapshot: DocumentSaveSnapshot,
        record: RecoveryRecord,
    ) -> bool {
        if snapshot.source_format.is_none() {
            return false;
        }
        let revision = snapshot.revision;
        if self
            .pending_recovery_record
            .as_ref()
            .is_none_or(|pending| pending.snapshot.revision <= revision)
        {
            self.pending_recovery_record = Some(PendingRecoveryRecord { snapshot, record });
        }
        true
    }

    /// 只转移 pending 的所有权，让 worker 在安装成功后立即提交而不复制正文。
    pub(crate) fn take_pending_recovery(&mut self) -> Option<PendingRecoveryRecord> {
        self.pending_recovery_record.take()
    }

    /// 将 worker 尚未接受的命令放回有界槽位，以便调用方保留可重试状态和错误证据。
    pub(crate) fn restore_pending_recovery(&mut self, pending: PendingRecoveryRecord) {
        let _ = self.stage_pending_recovery(pending.snapshot, pending.record);
    }

    /// 丢弃已被保存或明确 discard 的旧 pending，避免日志稍后安装时重放已不再脏的正文。
    pub(crate) fn clear_pending_recovery_through(&mut self, revision: DocumentRevision) {
        if self
            .pending_recovery_record
            .as_ref()
            .is_some_and(|pending| pending.snapshot.revision <= revision)
        {
            self.pending_recovery_record = None;
        }
    }

    pub(crate) fn cancel_all(&mut self) {
        self.recovery_generation = self.recovery_generation.wrapping_add(1);
        self.lifetime_cancellation.cancel();
        for cancellation in [
            self.source_cancellation.take(),
            self.search_cancellation.take(),
            self.index_cancellation.take(),
            self.save.cancellation.take(),
        ]
        .into_iter()
        .flatten()
        {
            cancellation.cancel();
        }
        self.source_task = Task::ready(());
        self.search_task = Task::ready(());
        self.index_task = Task::ready(());
        self.external_task = Task::ready(());
        self.save.task = Task::ready(());
        // Do not clear `pending_recovery_record`: a journal-creation failure
        // must leave the latest immutable Resident evidence available for a
        // later install/retry rather than turning close into silent loss.
    }

    /// Keep the old failure-injection contract available to unit tests; the
    /// production reload path uses the recovery worker and never calls this
    /// synchronous compatibility hook.
    #[cfg(test)]
    pub(crate) fn replace_recovery_journal_after_persistence(
        &mut self,
        replacement: Option<DocumentRecoveryJournal>,
        document: &impl RecoveryDocument,
    ) -> Result<(), PagedDocumentError> {
        let retry_error = self.retry_retired_recovery_journals().err();
        let previous = std::mem::replace(&mut self.recovery_journal, replacement);
        let checkpoint_error = previous.and_then(|mut journal| {
            let result = document.checkpoint_recovery(&mut journal);
            match result {
                Ok(()) => None,
                Err(error) => {
                    let (retired, retirement_error) = journal.retire_for_cleanup();
                    self.retired_recovery_journals.push(retired);
                    Some(retirement_error.unwrap_or(error))
                }
            }
        });

        match (retry_error, checkpoint_error) {
            (Some(error), _) | (None, Some(error)) => Err(error),
            (None, None) => Ok(()),
        }
    }

    /// Keeps every failed removal in the queue so one bad path cannot discard later work.
    pub(crate) fn retry_retired_recovery_journals(&mut self) -> Result<(), PagedDocumentError> {
        let mut pending = Vec::with_capacity(self.retired_recovery_journals.len());
        let mut first_error = None;
        for journal in std::mem::take(&mut self.retired_recovery_journals) {
            match journal.retry_cleanup() {
                Ok(()) => {}
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                    pending.push(journal);
                }
            }
        }
        self.retired_recovery_journals = pending;
        first_error.map_or(Ok(()), Err)
    }
}
