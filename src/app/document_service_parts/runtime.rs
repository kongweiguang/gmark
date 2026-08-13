// @author kongweiguang

//! Runtime controller and session construction helpers.
//!
//! These builders preserve the existing loading policy decisions while keeping
//! source metadata and watcher coordination outside the runtime boundary.

use std::path::{Path, PathBuf};

use gmark_document_core::{
    DocumentBackendKind, DocumentFormat, DocumentProfile, LoadingLimits, LoadingPolicy, OpenPlan,
    OpenPolicyResolver, TextEncoding,
};
use gmark_document_runtime::{
    ControllerError, DocumentController, DocumentId, DocumentRegistryKey, DocumentSession,
    DocumentStore, FileIdentity, ResidentDocument,
};
use gmark_paged_document::{LineIndex, OpenProbe, OpenStrategy, PagedDocument, PreparedUtf8Source};

use super::source::ResidentMarkdownSource;
use super::types::DocumentServiceError;

pub(super) fn build_host_controller(
    document_id: DocumentId,
    probe: OpenProbe,
    _policy: LoadingPolicy,
    prepared: PreparedUtf8Source,
) -> Result<DocumentController, ControllerError> {
    let session = build_host_session(probe, prepared)?;
    Ok(DocumentController::new(document_id, session))
}

pub(super) fn build_host_session(
    probe: OpenProbe,
    prepared: PreparedUtf8Source,
) -> Result<DocumentSession, ControllerError> {
    let source_identity = prepared_original_identity(&prepared)?;
    if source_identity != probe.identity {
        return Err(ControllerError::open_failed(
            "source changed while preparing the shared document".to_owned(),
        ));
    }
    let profile = probe.profile();
    let plan = host_plan(&profile, &probe);
    let file_identity = FileIdentity::from(&source_identity);
    let store = if probe.strategy == OpenStrategy::Paged {
        let index = LineIndex::build(prepared.source())
            .map_err(|error| ControllerError::open_failed(error.to_string()))?;
        let document = PagedDocument::from_prepared(prepared, index)
            .map_err(|error| ControllerError::open_failed(error.to_string()))?;
        DocumentStore::Paged(Box::new(document))
    } else {
        let source = prepared.source();
        let len = source
            .identity()
            .map_err(|error| ControllerError::open_failed(error.to_string()))?
            .len;
        let bytes = source
            .read_range(0, len)
            .map_err(|error| ControllerError::open_failed(error.to_string()))?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| ControllerError::open_failed("prepared source is not UTF-8".to_owned()))?;
        DocumentStore::Resident(Box::new(ResidentDocument::new(
            text,
            probe.encoding.clone(),
            source_identity.clone(),
        )))
    };
    let session = DocumentSession::new(profile, store, plan, file_identity)
        .map_err(|error| ControllerError::open_failed(error.to_string()))?;
    Ok(session)
}

pub(super) fn prepared_original_identity(
    prepared: &PreparedUtf8Source,
) -> Result<gmark_paged_document::FileIdentity, ControllerError> {
    if let Some(plan) = prepared.save_plan() {
        return Ok(plan.original_identity().clone());
    }
    prepared
        .source()
        .identity()
        .map_err(|error| ControllerError::open_failed(error.to_string()))
}

pub(super) fn host_plan(profile: &DocumentProfile, probe: &OpenProbe) -> OpenPlan {
    let mut policy = LoadingPolicy {
        max_resident_bytes: Some(probe.options.max_resident_bytes),
        force_safe_source: probe.force_safe_source,
    };
    if probe.strategy == OpenStrategy::Paged
        && OpenPolicyResolver.resolve(policy, profile).backend == DocumentBackendKind::Resident
    {
        policy.force_safe_source = true;
    }
    OpenPolicyResolver.resolve(policy, profile)
}

pub(super) fn build_controller(
    document_id: DocumentId,
    source: ResidentMarkdownSource,
    initially_dirty: bool,
) -> Result<DocumentController, ControllerError> {
    if source.profile.format != DocumentFormat::Markdown {
        return Err(ControllerError::open_failed(
            DocumentServiceError::NonMarkdownSource.to_string(),
        ));
    }
    let limits = source.loading_limits;
    let plan = resident_plan(&source.profile, limits);
    let identity = FileIdentity::from(&source.file_identity);
    let store = DocumentStore::Resident(Box::new(ResidentDocument::from_source_document(
        source.source,
        source.profile.encoding.clone(),
        source.file_identity,
    )));
    let mut session = DocumentSession::new(source.profile, store, plan, identity)
        .map_err(|error| ControllerError::open_failed(error.to_string()))?;
    session.dirty = initially_dirty;
    Ok(DocumentController::new(document_id, session))
}

pub(super) fn resident_plan(profile: &DocumentProfile, limits: LoadingLimits) -> OpenPlan {
    let mut plan = LoadingPolicy {
        max_resident_bytes: Some(u64::MAX),
        force_safe_source: false,
    }
    .resolve(profile);
    // The backend is intentionally resident, while the effective threshold is
    // still frozen for this session and can explain a later growth warning.
    plan.limits = limits;
    plan
}

pub(super) fn markdown_profile(text: &str, encoding: TextEncoding) -> DocumentProfile {
    DocumentProfile {
        len: text.len() as u64,
        format: DocumentFormat::Markdown,
        encoding,
        estimated_lines: text.lines().count().max(1) as u64,
        estimated_structural_units: text
            .bytes()
            .filter(|byte| matches!(*byte, b'|' | b',' | b'\t' | b'{' | b'}' | b'[' | b']'))
            .count() as u64,
    }
}

pub(super) fn display_encoding(encoding: &TextEncoding) -> crate::document_io::DocumentEncoding {
    match encoding {
        TextEncoding::Utf8 { .. } => crate::document_io::DocumentEncoding::Utf8,
        TextEncoding::Utf16Le => {
            crate::document_io::DocumentEncoding::Legacy("UTF-16LE".to_owned())
        }
        TextEncoding::Utf16Be => {
            crate::document_io::DocumentEncoding::Legacy("UTF-16BE".to_owned())
        }
        TextEncoding::Legacy(label) => crate::document_io::DocumentEncoding::Legacy(label.clone()),
    }
}

pub(super) fn normalize_path(path: &Path) -> Result<PathBuf, DocumentServiceError> {
    if path.as_os_str().is_empty() {
        return Err(DocumentServiceError::PathNormalization(
            "document path is empty".to_owned(),
        ));
    }
    dunce::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .map_err(|error| DocumentServiceError::PathNormalization(error.to_string()))
}

pub(super) fn file_key(path: &Path) -> DocumentRegistryKey {
    #[cfg(target_os = "windows")]
    {
        DocumentRegistryKey::File(PathBuf::from(
            path.as_os_str().to_string_lossy().to_lowercase(),
        ))
    }
    #[cfg(not(target_os = "windows"))]
    DocumentRegistryKey::File(path.to_path_buf())
}

pub(super) fn registry_key_path(key: &DocumentRegistryKey) -> PathBuf {
    match key {
        DocumentRegistryKey::File(path) => path.clone(),
        DocumentRegistryKey::Untitled(_) => PathBuf::new(),
    }
}

pub(super) fn map_registry_error(error: ControllerError) -> DocumentServiceError {
    match error {
        ControllerError::OpenFailed(message) => DocumentServiceError::OpenFailed(message),
        other => DocumentServiceError::Registry(other.to_string()),
    }
}
