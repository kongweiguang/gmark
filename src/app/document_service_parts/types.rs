// @author kongweiguang

//! Values returned by the process-wide document service.
//!
//! These wrappers retain lease ownership and metadata at the same boundary, so
//! callers do not need to know which implementation part created them.

use std::fmt;
use std::path::PathBuf;

use gmark_document_core::{DocumentFormat, LoadingLimits, TextEncoding};
use gmark_document_runtime::{
    DocumentHandle, DocumentId, DocumentLease, DocumentRegistryKey, RegistryOpen,
};
use gmark_paged_document::{OpenProbe, OpenStrategy};

use super::runtime::map_registry_error;
use super::service::DocumentService;

/// Result of opening one process-wide resident document.
///
/// `open` distinguishes the `Opening` owner from a waiter that joined an
/// existing slot.  The lease is the only lifetime token returned to a window;
/// cloning a handle alone cannot keep the registry entry alive.
pub(crate) struct SharedResidentOpen {
    pub(crate) lease: DocumentLease,
    pub(crate) open: RegistryOpen,
    pub(crate) document_id: DocumentId,
    /// UI-facing encoding label retained from the existing file adapter.
    pub(crate) encoding: crate::document_io::DocumentEncoding,
    /// Runtime encoding used by the authoritative resident session.
    pub(crate) text_encoding: TextEncoding,
    pub(crate) loading_limits: LoadingLimits,
    pub(crate) key: DocumentRegistryKey,
    pub(crate) file_path: PathBuf,
}

impl std::fmt::Debug for SharedResidentOpen {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedResidentOpen")
            .field("open", &self.open)
            .field("document_id", &self.document_id)
            .field("encoding", &self.encoding)
            .field("text_encoding", &self.text_encoding)
            .field("loading_limits", &self.loading_limits)
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl SharedResidentOpen {
    pub(crate) fn handle(&self) -> DocumentHandle {
        self.lease.handle()
    }

    pub(crate) fn lease_count(&self) -> usize {
        self.handle().lease_count()
    }
}

/// Result of opening a structured or paged file through the shared runtime.
/// The host consumes the lease and probe; it never opens or decodes a second
/// source body.
pub(crate) struct SharedDocumentHostOpen {
    pub(crate) lease: DocumentLease,
    pub(crate) open: RegistryOpen,
    pub(crate) probe: OpenProbe,
    pub(crate) document_id: DocumentId,
    pub(crate) encoding: crate::document_io::DocumentEncoding,
    pub(crate) text_encoding: TextEncoding,
    pub(crate) loading_limits: LoadingLimits,
    pub(crate) key: DocumentRegistryKey,
    pub(crate) file_path: PathBuf,
}

/// Save As reservation owned by the process service.  The runtime reservation
/// still guards the target key, while commit also re-keys the service watcher
/// so the shared handle follows the new canonical path.
pub(crate) struct DocumentSaveAsReservation {
    pub(super) service: DocumentService,
    pub(super) reservation: Option<gmark_document_runtime::SaveAsReservation>,
    pub(super) source_key: DocumentRegistryKey,
    pub(super) target_key: DocumentRegistryKey,
    pub(super) target_path: PathBuf,
}

/// Existing target returned by the registry's atomic Save As reservation
/// attempt.  The lease is transferred to the caller so a switch-to-existing
/// action can safely keep that shared Controller alive while the source editor
/// decides whether to close or activate its own view.
pub(crate) struct SharedSaveAsTarget {
    pub(crate) lease: DocumentLease,
    pub(crate) document_id: DocumentId,
    pub(crate) key: DocumentRegistryKey,
    pub(crate) file_path: PathBuf,
    pub(crate) encoding: crate::document_io::DocumentEncoding,
    pub(crate) text_encoding: TextEncoding,
    pub(crate) loading_limits: LoadingLimits,
    pub(crate) probe: OpenProbe,
}

impl std::fmt::Debug for SharedSaveAsTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedSaveAsTarget")
            .field("document_id", &self.document_id)
            .field("key", &self.key)
            .field("file_path", &self.file_path)
            .field("encoding", &self.encoding)
            .field("text_encoding", &self.text_encoding)
            .field("loading_limits", &self.loading_limits)
            .field("probe", &self.probe)
            .finish_non_exhaustive()
    }
}

impl SharedSaveAsTarget {
    pub(crate) fn handle(&self) -> DocumentHandle {
        self.lease.handle()
    }

    pub(crate) fn lease_count(&self) -> usize {
        self.handle().lease_count()
    }

    /// Consumes the occupied-target lease and exposes the existing shared
    /// document without probing or reading its body again.
    pub(crate) fn into_existing_open(self) -> Result<SharedExistingOpen, DocumentServiceError> {
        let Self {
            lease,
            document_id,
            key,
            file_path,
            encoding,
            text_encoding,
            loading_limits,
            probe,
        } = self;
        if probe.strategy == OpenStrategy::Paged || probe.format != DocumentFormat::Markdown {
            return Ok(SharedExistingOpen::Host(SharedDocumentHostOpen {
                lease,
                open: RegistryOpen::Existing,
                probe,
                document_id,
                encoding,
                text_encoding,
                loading_limits,
                key,
                file_path,
            }));
        }
        Ok(SharedExistingOpen::Resident(SharedResidentOpen {
            lease,
            open: RegistryOpen::Existing,
            document_id,
            encoding,
            text_encoding,
            loading_limits,
            key,
            file_path,
        }))
    }
}

pub(crate) enum SharedExistingOpen {
    Resident(SharedResidentOpen),
    Host(SharedDocumentHostOpen),
}

impl std::fmt::Debug for SharedExistingOpen {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resident(open) => formatter.debug_tuple("Resident").field(open).finish(),
            Self::Host(open) => formatter.debug_tuple("Host").field(open).finish(),
        }
    }
}

/// Atomic Save As target outcome.  `Occupied` carries a real lease on the
/// existing target Controller; callers must not overwrite that target and may
/// instead offer a switch-to-existing action.
pub(crate) enum SaveAsTargetReservation {
    Reserved(DocumentSaveAsReservation),
    Occupied(SharedSaveAsTarget),
}

impl std::fmt::Debug for SaveAsTargetReservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reserved(reservation) => formatter
                .debug_tuple("Reserved")
                .field(reservation)
                .finish(),
            Self::Occupied(target) => formatter.debug_tuple("Occupied").field(target).finish(),
        }
    }
}

impl std::fmt::Debug for DocumentSaveAsReservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DocumentSaveAsReservation")
            .field("source_key", &self.source_key)
            .field("target_key", &self.target_key)
            .field("target_path", &self.target_path)
            .finish_non_exhaustive()
    }
}

impl DocumentSaveAsReservation {
    pub(crate) fn target(&self) -> &DocumentRegistryKey {
        &self.target_key
    }

    pub(crate) fn commit(mut self) -> Result<DocumentHandle, DocumentServiceError> {
        let reservation = self.reservation.take().ok_or_else(|| {
            DocumentServiceError::Registry("Save As reservation was already consumed".to_owned())
        })?;
        let handle = reservation.commit().map_err(map_registry_error)?;
        self.service.rekey_watcher_after_save_as(
            &self.source_key,
            &self.target_key,
            &self.target_path,
            &handle,
        );
        Ok(handle)
    }

    pub(crate) fn release(mut self) {
        self.reservation.take();
    }
}

impl std::fmt::Debug for SharedDocumentHostOpen {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedDocumentHostOpen")
            .field("open", &self.open)
            .field("probe", &self.probe)
            .field("document_id", &self.document_id)
            .field("encoding", &self.encoding)
            .field("text_encoding", &self.text_encoding)
            .field("loading_limits", &self.loading_limits)
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl SharedDocumentHostOpen {
    pub(crate) fn handle(&self) -> DocumentHandle {
        self.lease.handle()
    }

    pub(crate) fn lease_count(&self) -> usize {
        self.handle().lease_count()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DocumentServiceError {
    PathNormalization(String),
    OpenFailed(String),
    Registry(String),
    NonMarkdownSource,
}

impl fmt::Display for DocumentServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathNormalization(message) => {
                write!(formatter, "document path normalization failed: {message}")
            }
            Self::OpenFailed(message) => write!(formatter, "document open failed: {message}"),
            Self::Registry(message) => write!(formatter, "document registry failed: {message}"),
            Self::NonMarkdownSource => {
                formatter.write_str("resident document source must use Markdown format")
            }
        }
    }
}

impl std::error::Error for DocumentServiceError {}
