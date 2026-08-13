// @author kongweiguang

use gmark_paged_document::{ExternalChange, PagedDocumentError, SearchCancellation};
use gpui::{SharedString, Task};

use super::DocumentRecoveryJournal;
use super::SharedDocument;
use super::session::RetiredRecoveryJournal;

pub(crate) trait RecoveryDocument {
    fn checkpoint_recovery(
        &self,
        journal: &mut DocumentRecoveryJournal,
    ) -> Result<(), PagedDocumentError>;
}

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

impl RecoveryDocument for gmark_document_runtime::DocumentSession {
    fn checkpoint_recovery(
        &self,
        journal: &mut DocumentRecoveryJournal,
    ) -> Result<(), PagedDocumentError> {
        journal.checkpoint(self)
    }
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
    /// 已保存但暂未删除的旧日志单独排队；不能放回 active 槽，否则安装新日志时会丢失重试权。
    pub(crate) retired_recovery_journals: Vec<RetiredRecoveryJournal>,
    pub(crate) recovery_error: Option<SharedString>,
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
            retired_recovery_journals: Vec::new(),
            recovery_error: None,
            lifetime_cancellation,
            index_task: Task::ready(()),
            source_task: Task::ready(()),
            search_task: Task::ready(()),
            external_task: Task::ready(()),
        }
    }

    pub(crate) fn cancel_all(&mut self) {
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
    }

    /// Installs the newly active journal independently from cleanup of the previous one.
    ///
    /// 保存或重载已经成功时，旧日志即使删除失败也只能进入 retired 队列；active 槽必须
    /// 立即属于新日志，这样后续编辑仍有保护，而旧日志可在下一次成功持久化时重试清理。
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
