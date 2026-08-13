// @author kongweiguang

//! Focused tests for the process-wide document service.
//!
//! The parent keeps shared fixtures and imports; child files only partition
//! test scenarios so the test contract remains unchanged.

// @author kongweiguang

use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;

use gmark_document::SourceDocument;
use gmark_document_core::{
    DocumentBackendKind, DocumentRevision, DocumentViewInstanceId, LoadingPolicy, SourceEdit,
    SourceSelection, TextEncoding, Transaction,
};
use gmark_document_runtime::{
    DocumentCommand, DocumentEvent, DocumentId, RegistryOpen, SaveFailureCode,
};
use gmark_paged_document::{FileSource, OpenStrategy, prepare_utf8_source};

use super::{
    DocumentService, DocumentServiceError, ResidentMarkdownSource, SaveAsTargetReservation,
    SharedExistingOpen, dispatch_external_conflict, process_external_change,
};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn test_error(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
    Box::new(io::Error::other(message.into()))
}

fn write_fixture(root: &Path) -> TestResult<PathBuf> {
    let path = root.join("note.md");
    std::fs::write(&path, b"# shared\n")?;
    Ok(path)
}

fn source(path: &Path, text: &str) -> ResidentMarkdownSource {
    ResidentMarkdownSource::from_text(
        text,
        path.to_path_buf(),
        TextEncoding::Utf8 { bom: false },
        LoadingPolicy::default().effective_limits(),
    )
}

fn open_disk_as_untitled(
    service: &DocumentService,
    path: &Path,
) -> TestResult<super::SharedResidentOpen> {
    let policy = LoadingPolicy::default();
    let probe = crate::document_io::probe_document_with_policy(path, policy)?;
    let opened =
        crate::document_io::read_resident_text_from_probe(path, &probe, policy.effective_limits())?;
    let resident = ResidentMarkdownSource::from_opened(path, opened);
    Ok(service.open_untitled(Some(DocumentId::new()), resident)?)
}

#[path = "document_service_parts/opening.rs"]
mod opening;
#[path = "document_service_parts/recovery.rs"]
mod recovery;
#[path = "document_service_parts/watchers.rs"]
mod watchers;
