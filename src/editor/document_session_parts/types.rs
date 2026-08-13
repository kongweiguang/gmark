// @author kongweiguang

use super::*;

/// 适配器自身的错误边界。
///
/// Controller 中携带 `DocumentError` 的 Session 错误保留为结构化错误；锁、后端
/// 类型和其它 Controller 错误不能被伪造成某个源码编辑错误。
#[derive(Clone, Debug)]
pub(crate) enum EditorDocumentSessionError {
    Document(DocumentError),
    Controller(ControllerError),
    /// A top-level shell (for example an image/error or pane host wrapper)
    /// intentionally has no editable Controller or lease.
    Shell,
    NotResident,
    RecoveryFormatMismatch,
    InvalidViewId,
    ViewAlreadyRegistered(DocumentViewInstanceId),
}

impl EditorDocumentSessionError {
    fn from_controller(error: ControllerError) -> Self {
        if let Some(document_error) = error.source_document_error() {
            Self::Document(document_error.clone())
        } else {
            Self::Controller(error)
        }
    }
}

impl fmt::Display for EditorDocumentSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(error) => error.fmt(formatter),
            Self::Controller(error) => error.fmt(formatter),
            Self::Shell => formatter.write_str("editor document shell has no resident controller"),
            Self::NotResident => formatter.write_str("editor document is not resident"),
            Self::RecoveryFormatMismatch => {
                formatter.write_str("recovery source format does not match resident document")
            }
            Self::InvalidViewId => formatter.write_str("document view id must not be nil"),
            Self::ViewAlreadyRegistered(view_id) => {
                write!(
                    formatter,
                    "document view id is already registered: {view_id:?}"
                )
            }
        }
    }
}

impl std::error::Error for EditorDocumentSessionError {}

impl From<DocumentError> for EditorDocumentSessionError {
    fn from(error: DocumentError) -> Self {
        Self::Document(error)
    }
}

impl From<ControllerError> for EditorDocumentSessionError {
    fn from(error: ControllerError) -> Self {
        Self::from_controller(error)
    }
}

/// 最后一个 adapter clone 释放时关闭对应的 Controller view；其中的
/// `Arc<DocumentLease>` 保证普通 `EditorDocumentSession::clone` 不增加 lease。
pub(super) struct ViewLease {
    pub(super) handle: DocumentHandle,
    pub(super) lease: Arc<DocumentLease>,
    pub(super) view_id: DocumentViewInstanceId,
    pub(super) events: std::sync::Mutex<DocumentEventSubscription>,
    pub(super) pending: std::sync::Mutex<Option<DocumentEventPoll>>,
}

impl Drop for ViewLease {
    fn drop(&mut self) {
        // 即使 Controller 已经 poisoned，也必须继续让 DocumentLease 释放；
        // DocumentLease 自身的 Drop 会在字段析构时完成 registry 生命周期收口。
        if let Ok(mut controller) = self.handle.lock() {
            controller.close_view(self.view_id);
        }
    }
}

/// Markdown 编辑器对共享文档 Controller 的窄 facade。
///
/// Clone 只共享同一视图 lease；要打开同一正文的新视图，必须显式调用
/// [`EditorDocumentSession::fork_view`] 或从另一个 handle/lease 构造 adapter。
#[derive(Clone)]
pub(crate) struct EditorDocumentSession {
    pub(super) handle: Option<DocumentHandle>,
    pub(super) view: Option<Arc<ViewLease>>,
}

pub(crate) struct DocumentEventPoll {
    pub(crate) events: Vec<DocumentEvent>,
    pub(crate) snapshot: Option<DocumentStateSnapshot>,
}

impl DocumentEventPoll {
    pub(crate) fn is_empty(&self) -> bool {
        self.events.is_empty() && self.snapshot.is_none()
    }

    pub(crate) fn merge(&mut self, mut other: Self) {
        self.events.append(&mut other.events);
        if other.snapshot.is_some() {
            self.snapshot = other.snapshot;
        }
    }
}
