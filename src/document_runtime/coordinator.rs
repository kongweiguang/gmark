// @author kongweiguang

use gmark_document_core::PersistenceError;
use gmark_paged_document::{
    ExternalChange, PagedDocumentError, PagedRecoveryJournal, SearchCancellation,
};
use gpui::{SharedString, Task};

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

pub(crate) fn map_persistence_error(error: PagedDocumentError) -> PersistenceError {
    match error {
        PagedDocumentError::SourceChanged => PersistenceError::SourceChanged,
        PagedDocumentError::Recovery(message) => PersistenceError::Recovery(message),
        error => PersistenceError::AtomicWrite(error.to_string()),
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
    pub(crate) recovery_journal: Option<PagedRecoveryJournal>,
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
}
