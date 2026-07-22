// @author kongweiguang

use thiserror::Error;

use crate::{DocumentBackendKind, DocumentViewId};

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum OpenError {
    #[error("document probe failed: {0}")]
    Probe(String),
    #[error("document changed while it was being opened")]
    SourceChanged,
    #[error("document is binary or uses an unsupported encoding")]
    UnsupportedText,
    #[error("planned backend {planned:?} does not match installed backend {actual:?}")]
    BackendMismatch {
        planned: DocumentBackendKind,
        actual: DocumentBackendKind,
    },
    #[error("initial document view is unavailable: {0:?}")]
    InitialViewUnavailable(DocumentViewId),
    #[error("document view is unavailable: {0:?}")]
    ViewUnavailable(DocumentViewId),
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ProjectionError {
    #[error("projection was cancelled")]
    Cancelled,
    #[error("projection source revision is stale")]
    SourceChanged,
    #[error("projection item limit was exceeded")]
    LimitExceeded,
    #[error("invalid projection source range {start}..{end} for document length {len}")]
    InvalidSourceRange { start: u64, end: u64, len: u64 },
    #[error("invalid JSON near byte {offset}: {message}")]
    InvalidJson { offset: u64, message: String },
    #[error("projection failed: {0}")]
    Build(String),
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PersistenceError {
    #[error("source changed on disk before save")]
    SourceChanged,
    #[error("atomic save failed: {0}")]
    AtomicWrite(String),
    #[error("recovery journal failed: {0}")]
    Recovery(String),
}
