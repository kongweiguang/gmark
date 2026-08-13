// @author kongweiguang

//! Process-wide resident-document opening and ownership.
//!
//! The module declarations below intentionally preserve the original
//! application-facing names while separating registry, source, runtime, and
//! watcher responsibilities.  This keeps the refactor structural and prevents
//! callers from observing a new ownership boundary.

#[path = "document_service_parts/runtime.rs"]
mod runtime;
#[path = "document_service_parts/service.rs"]
mod service;
#[path = "document_service_parts/source.rs"]
mod source;
#[path = "document_service_parts/types.rs"]
mod types;
#[path = "document_service_parts/watcher.rs"]
mod watcher;
#[path = "document_service_parts/watcher_runtime.rs"]
mod watcher_runtime;

pub(crate) use service::DocumentService;
pub(crate) use source::ResidentMarkdownSource;
#[cfg(test)]
pub(crate) use types::DocumentServiceError;
pub(crate) use types::{
    SaveAsTargetReservation, SharedDocumentHostOpen, SharedExistingOpen, SharedResidentOpen,
    SharedSaveAsTarget,
};
#[cfg(test)]
pub(crate) use watcher_runtime::{dispatch_external_conflict, process_external_change};

#[cfg(test)]
#[path = "../../tests/unit/app/document_service.rs"]
mod tests;
