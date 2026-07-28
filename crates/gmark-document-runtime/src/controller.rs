// @author kongweiguang

//! 共享文档 Controller 与 Registry。
//!
//! 文件、Watcher、Recovery 等 adapter 由应用层提供；本模块只串行化文档命令、保存
//! 请求和代次校验。因此跨窗口可以共享正文、撤销和保存队列，而每个 DocumentTab 仍
//! 独占 selection、滚动和活动 ViewMode。

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use gmark_document_core::{DocumentId, DocumentRevision, Transaction, TransactionId};
use thiserror::Error;

use crate::{DocumentSession, FileIdentity, SessionEditError};

/// Controller 的唯一窄写入口。UI/Provider 不直接拿后端实体或写磁盘。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentCommand {
    ApplyTransaction {
        transaction_id: TransactionId,
        transaction: Transaction,
    },
    Undo {
        transaction_id: TransactionId,
    },
    Redo {
        transaction_id: TransactionId,
    },
    /// 合并为“保存最新 revision”；实际 IO 由 Save adapter 在收到事件后执行。
    RequestSave,
    SaveSucceeded {
        revision: DocumentRevision,
        identity: FileIdentity,
    },
    SaveFailed {
        revision: DocumentRevision,
        code: SaveFailureCode,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFailureCode {
    Cancelled,
    Conflict,
    PermissionDenied,
    NoSpace,
    Uncertain,
    Other,
}

/// Controller 事件不含正文、路径或外部错误字符串，适合状态栏、恢复和安全诊断订阅。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentEvent {
    Changed {
        document_id: DocumentId,
        transaction_id: TransactionId,
        revision: DocumentRevision,
        dirty: bool,
    },
    SaveRequested {
        document_id: DocumentId,
        revision: DocumentRevision,
    },
    SaveCommitted {
        document_id: DocumentId,
        revision: DocumentRevision,
        dirty: bool,
    },
    SaveFailed {
        document_id: DocumentId,
        revision: DocumentRevision,
        code: SaveFailureCode,
    },
}

/// 写入器的串行队列。一次在途保存只允许有一个“最新”待保存 revision。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SaveQueue {
    in_flight: Option<DocumentRevision>,
    pending: Option<DocumentRevision>,
}

impl SaveQueue {
    fn request(&mut self, revision: DocumentRevision) -> Option<DocumentRevision> {
        match self.in_flight {
            Some(in_flight) if in_flight == revision => None,
            Some(_) => {
                self.pending = Some(revision);
                None
            }
            None => {
                self.in_flight = Some(revision);
                Some(revision)
            }
        }
    }

    fn complete(
        &mut self,
        revision: DocumentRevision,
    ) -> Result<Option<DocumentRevision>, ControllerError> {
        if self.in_flight != Some(revision) {
            return Err(ControllerError::UnexpectedSaveCompletion {
                expected: self.in_flight,
                actual: revision,
            });
        }
        self.in_flight = None;
        let next = self.pending.take().filter(|pending| *pending != revision);
        if let Some(next) = next {
            self.in_flight = Some(next);
        }
        Ok(next)
    }

    fn fail(&mut self, revision: DocumentRevision) -> Result<(), ControllerError> {
        if self.in_flight != Some(revision) {
            return Err(ControllerError::UnexpectedSaveCompletion {
                expected: self.in_flight,
                actual: revision,
            });
        }
        self.in_flight = None;
        Ok(())
    }
}

/// DocumentSession 的副作用协调器；自身不保存 selection、滚动或投影状态。
pub struct DocumentController {
    document_id: DocumentId,
    session: DocumentSession,
    save_queue: SaveQueue,
    events: VecDeque<DocumentEvent>,
}

impl DocumentController {
    pub fn new(document_id: DocumentId, session: DocumentSession) -> Self {
        Self {
            document_id,
            session,
            save_queue: SaveQueue::default(),
            events: VecDeque::new(),
        }
    }

    pub fn document_id(&self) -> DocumentId {
        self.document_id
    }

    pub fn session(&self) -> &DocumentSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut DocumentSession {
        &mut self.session
    }

    pub fn dispatch(&mut self, command: DocumentCommand) -> Result<(), ControllerError> {
        match command {
            DocumentCommand::ApplyTransaction {
                transaction_id,
                transaction,
            } => {
                let revision = self
                    .session
                    .apply_transaction_without_selection(&transaction)?;
                self.events.push_back(DocumentEvent::Changed {
                    document_id: self.document_id,
                    transaction_id,
                    revision,
                    dirty: self.session.is_dirty(),
                });
            }
            DocumentCommand::Undo { transaction_id } => {
                if self.session.undo() {
                    self.events.push_back(DocumentEvent::Changed {
                        document_id: self.document_id,
                        transaction_id,
                        revision: self.session.revision_token(),
                        dirty: self.session.is_dirty(),
                    });
                }
            }
            DocumentCommand::Redo { transaction_id } => {
                if self.session.redo() {
                    self.events.push_back(DocumentEvent::Changed {
                        document_id: self.document_id,
                        transaction_id,
                        revision: self.session.revision_token(),
                        dirty: self.session.is_dirty(),
                    });
                }
            }
            DocumentCommand::RequestSave => {
                if let Some(revision) = self.save_queue.request(self.session.revision_token()) {
                    self.events.push_back(DocumentEvent::SaveRequested {
                        document_id: self.document_id,
                        revision,
                    });
                }
            }
            DocumentCommand::SaveSucceeded { revision, identity } => {
                self.session.mark_persisted_if_current(revision, identity);
                self.events.push_back(DocumentEvent::SaveCommitted {
                    document_id: self.document_id,
                    revision,
                    dirty: self.session.is_dirty(),
                });
                if let Some(next) = self.save_queue.complete(revision)? {
                    self.events.push_back(DocumentEvent::SaveRequested {
                        document_id: self.document_id,
                        revision: next,
                    });
                }
            }
            DocumentCommand::SaveFailed { revision, code } => {
                self.save_queue.fail(revision)?;
                self.events.push_back(DocumentEvent::SaveFailed {
                    document_id: self.document_id,
                    revision,
                    code,
                });
            }
        }
        Ok(())
    }

    pub fn drain_events(&mut self) -> impl Iterator<Item = DocumentEvent> + '_ {
        self.events.drain(..)
    }
}

/// 跨窗口共享的文档句柄。锁只覆盖 Controller 状态转换，慢速 IO 由 adapter 在锁外执行。
#[derive(Clone)]
pub struct DocumentHandle(Arc<Mutex<DocumentController>>);

impl DocumentHandle {
    pub fn new(controller: DocumentController) -> Self {
        Self(Arc::new(Mutex::new(controller)))
    }

    pub fn lock(&self) -> Result<MutexGuard<'_, DocumentController>, ControllerError> {
        self.0.lock().map_err(|_| ControllerError::Poisoned)
    }

    fn reference_count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

/// 同一规范路径共享；硬链接的不同规范路径不共享，因为原子替换会断开其链接关系。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DocumentRegistryKey {
    Untitled(DocumentId),
    File(PathBuf),
}

impl DocumentRegistryKey {
    pub fn for_file(identity: &FileIdentity) -> Self {
        #[cfg(target_os = "windows")]
        {
            return Self::File(PathBuf::from(
                identity
                    .canonical_path
                    .as_os_str()
                    .to_string_lossy()
                    .to_lowercase(),
            ));
        }
        #[cfg(not(target_os = "windows"))]
        {
            Self::File(identity.canonical_path.clone())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistryOpen {
    Existing,
    Inserted,
}

/// 应用级 Registry；打开同一文件时返回同一 Handle，而 Tab 仍从自身 ViewState 开始。
#[derive(Default)]
pub struct DocumentRegistry {
    documents: Mutex<BTreeMap<DocumentRegistryKey, DocumentHandle>>,
}

impl DocumentRegistry {
    pub fn open_or_insert(
        &self,
        key: DocumentRegistryKey,
        create: impl FnOnce() -> Result<DocumentController, ControllerError>,
    ) -> Result<(DocumentHandle, RegistryOpen), ControllerError> {
        let mut documents = self
            .documents
            .lock()
            .map_err(|_| ControllerError::Poisoned)?;
        if let Some(handle) = documents.get(&key) {
            return Ok((handle.clone(), RegistryOpen::Existing));
        }
        let handle = DocumentHandle::new(create()?);
        documents.insert(key, handle.clone());
        Ok((handle, RegistryOpen::Inserted))
    }

    /// 只有 Registry 与调用者各持有一个引用时才释放，避免关闭一个窗口破坏其它窗口。
    pub fn release_if_unused(
        &self,
        key: &DocumentRegistryKey,
        handle: &DocumentHandle,
    ) -> Result<bool, ControllerError> {
        if handle.reference_count() > 2 {
            return Ok(false);
        }
        let mut documents = self
            .documents
            .lock()
            .map_err(|_| ControllerError::Poisoned)?;
        let matches = documents
            .get(key)
            .is_some_and(|registered| Arc::ptr_eq(&registered.0, &handle.0));
        if matches {
            documents.remove(key);
        }
        Ok(matches)
    }
}

#[derive(Debug, Error)]
pub enum ControllerError {
    #[error(transparent)]
    Session(#[from] SessionEditError),
    #[error("document controller lock was poisoned")]
    Poisoned,
    #[error("save completion for {actual:?} does not match in-flight save {expected:?}")]
    UnexpectedSaveCompletion {
        expected: Option<DocumentRevision>,
        actual: DocumentRevision,
    },
}

#[cfg(test)]
mod tests {
    use std::fs;

    use gmark_document_core::{
        DocumentFormat, DocumentProfile, LoadingPolicy, SourceEdit, TextEncoding,
    };
    use gmark_paged_document::FileSource;

    use super::*;
    use crate::{DocumentStore, ResidentDocument};

    fn session() -> DocumentSession {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
        let path = directory.path().join("controller.txt");
        fs::write(&path, "one").unwrap_or_else(|error| panic!("fixture write: {error}"));
        let source_identity = FileSource::open(&path)
            .and_then(|source| source.identity())
            .unwrap_or_else(|error| panic!("source identity: {error}"));
        let identity = FileIdentity::from(&source_identity);
        let profile = DocumentProfile {
            len: 3,
            format: DocumentFormat::PlainText,
            encoding: TextEncoding::Utf8 { bom: false },
            estimated_lines: 1,
            estimated_structural_units: 0,
        };
        DocumentSession::new(
            profile.clone(),
            DocumentStore::Resident(Box::new(ResidentDocument::new(
                "one",
                profile.encoding.clone(),
                source_identity,
            ))),
            LoadingPolicy::default().resolve(&profile),
            identity,
        )
        .unwrap_or_else(|error| panic!("session: {error}"))
    }

    #[test]
    fn saves_coalesce_and_old_save_does_not_clear_newer_edits() {
        let mut controller = DocumentController::new(DocumentId::from_raw(1), session());
        controller
            .dispatch(DocumentCommand::ApplyTransaction {
                transaction_id: TransactionId(1),
                transaction: Transaction::new(
                    DocumentRevision(0),
                    vec![SourceEdit::new(0..1, "t")],
                ),
            })
            .unwrap_or_else(|error| panic!("first edit: {error}"));
        controller
            .dispatch(DocumentCommand::RequestSave)
            .unwrap_or_else(|error| panic!("first save request: {error}"));
        controller
            .dispatch(DocumentCommand::ApplyTransaction {
                transaction_id: TransactionId(2),
                transaction: Transaction::new(
                    DocumentRevision(1),
                    vec![SourceEdit::new(1..2, "w")],
                ),
            })
            .unwrap_or_else(|error| panic!("second edit: {error}"));
        controller
            .dispatch(DocumentCommand::RequestSave)
            .unwrap_or_else(|error| panic!("second save request: {error}"));
        let identity = controller.session().file_identity.clone();
        controller
            .dispatch(DocumentCommand::SaveSucceeded {
                revision: DocumentRevision(1),
                identity,
            })
            .unwrap_or_else(|error| panic!("old save completion: {error}"));

        assert!(controller.session().is_dirty());
        assert!(controller.drain_events().any(|event| {
            matches!(
                event,
                DocumentEvent::SaveRequested {
                    revision: DocumentRevision(2),
                    ..
                }
            )
        }));
    }

    #[test]
    fn registry_returns_the_same_handle_for_the_same_path() {
        let registry = DocumentRegistry::default();
        let identity = session().file_identity.clone();
        let key = DocumentRegistryKey::for_file(&identity);
        let (first, state) = registry
            .open_or_insert(key.clone(), || {
                Ok(DocumentController::new(DocumentId::from_raw(1), session()))
            })
            .unwrap_or_else(|error| panic!("first open: {error}"));
        assert_eq!(state, RegistryOpen::Inserted);
        let (second, state) = registry
            .open_or_insert(key, || {
                Ok(DocumentController::new(DocumentId::from_raw(2), session()))
            })
            .unwrap_or_else(|error| panic!("second open: {error}"));
        assert_eq!(state, RegistryOpen::Existing);
        assert!(Arc::ptr_eq(&first.0, &second.0));
    }
}
