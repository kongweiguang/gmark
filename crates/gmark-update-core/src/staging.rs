// @author kongweiguang

//! Bounded, adapter-neutral download and staging primitives.

use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::{
    Result, UpdateCoreError,
    policy::{MAX_ARTIFACT_BYTES, MAX_ENVELOPE_BYTES, hex_sha256, validate_sha256},
};

const MAX_PARTIAL_METADATA_BYTES: u64 = 64 * 1024;

/// HTTP validators retained beside a partial artifact using the existing JSON keys.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialMetadata {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// Neutral range request data an HTTP adapter can translate into `Range` / `If-Range`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumeRequest {
    pub offset: u64,
    pub if_range: Option<String>,
}

/// Terminal state from a streaming bounded copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundedTransferOutcome {
    Complete { downloaded: u64 },
    Paused { downloaded: u64 },
}

/// Cooperative pause flag shared safely between an adapter UI and download worker.
#[derive(Clone, Default)]
pub struct DownloadControl(Arc<AtomicBool>);

impl DownloadControl {
    /// Requests a pause at the next bounded-copy chunk boundary.
    pub fn pause(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Reports whether a pause has been requested.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Adapter-facing transfer state, retained from the existing updater event protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadEvent {
    Started { downloaded: u64, total: u64 },
    Progress { downloaded: u64, total: u64 },
    Verifying,
    Finished { path: PathBuf },
    Paused { downloaded: u64, total: u64 },
}

/// Standard files in a versioned update transaction directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagingPaths {
    pub transaction_dir: PathBuf,
    pub part_path: PathBuf,
    pub ready_path: PathBuf,
    pub partial_metadata_path: PathBuf,
    pub envelope_path: PathBuf,
}

impl StagingPaths {
    /// Creates paths only; adapters decide when to create directories or issue HTTP requests.
    pub fn for_version(updates_root: impl AsRef<Path>, version: &str) -> Result<Self> {
        semver::Version::parse(version).map_err(|error| {
            UpdateCoreError::Manifest(format!(
                "signed update version is not valid SemVer: {error}"
            ))
        })?;
        let transaction_dir = updates_root.as_ref().join(format!("v{version}"));
        Ok(Self {
            part_path: transaction_dir.join("artifact.part"),
            ready_path: transaction_dir.join("artifact.ready"),
            partial_metadata_path: transaction_dir.join("partial.json"),
            envelope_path: transaction_dir.join("manifest.envelope.json"),
            transaction_dir,
        })
    }

    /// Ensures the transaction directory exists before a caller writes staged files.
    pub fn create_transaction_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.transaction_dir).map_err(|error| {
            UpdateCoreError::Io(format!("failed to create update cache directory: {error}"))
        })
    }

    /// Atomically persists the exact signed envelope that authorized the artifact.
    pub fn write_signed_envelope(&self, envelope: &[u8]) -> Result<()> {
        if envelope.is_empty() || envelope.len() > MAX_ENVELOPE_BYTES {
            return Err(UpdateCoreError::Envelope(
                "signed manifest envelope exceeds its size limit".to_owned(),
            ));
        }
        write_atomic(&self.envelope_path, envelope, "signed update manifest")
    }

    /// Promotes an already verified partial artifact without platform replacement logic.
    pub fn commit_ready(&self) -> Result<()> {
        fs::rename(&self.part_path, &self.ready_path).map_err(|error| {
            UpdateCoreError::Io(format!(
                "failed to commit verified update artifact: {error}"
            ))
        })?;
        // A stale sidecar cannot change a committed artifact. Keep legacy
        // cache-recovery behavior by not failing a successful promotion when
        // its best-effort cleanup cannot be completed.
        let _ = fs::remove_file(&self.partial_metadata_path);
        sync_parent(&self.ready_path)
    }
}

/// Plans a resumable request from an existing partial length and validators.
pub fn resume_request(
    partial_len: u64,
    artifact_size: u64,
    metadata: &PartialMetadata,
) -> Result<Option<ResumeRequest>> {
    validate_artifact_size(artifact_size)?;
    if partial_len > artifact_size {
        return Err(UpdateCoreError::Download(
            "partial update exceeds the expected artifact size".to_owned(),
        ));
    }
    if partial_len == 0 {
        return Ok(None);
    }
    Ok(Some(ResumeRequest {
        offset: partial_len,
        if_range: metadata
            .etag
            .clone()
            .or_else(|| metadata.last_modified.clone()),
    }))
}

/// Copies a response body with hard size bounds and a caller-owned pause decision.
pub fn copy_bounded(
    reader: &mut impl Read,
    writer: &mut impl Write,
    initial_offset: u64,
    expected_size: u64,
    mut should_pause: impl FnMut() -> bool,
    mut on_progress: impl FnMut(u64),
) -> Result<BoundedTransferOutcome> {
    validate_artifact_size(expected_size)?;
    if initial_offset > expected_size {
        return Err(UpdateCoreError::TooLarge);
    }

    let mut downloaded = initial_offset;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        if should_pause() {
            writer.flush().map_err(|error| {
                UpdateCoreError::Io(format!("failed to persist paused update: {error}"))
            })?;
            return Ok(BoundedTransferOutcome::Paused { downloaded });
        }
        let read = reader.read(&mut buffer).map_err(|error| {
            UpdateCoreError::Download(format!("failed while reading update artifact: {error}"))
        })?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or(UpdateCoreError::TooLarge)?;
        if downloaded > expected_size || downloaded > MAX_ARTIFACT_BYTES {
            return Err(UpdateCoreError::TooLarge);
        }
        writer.write_all(&buffer[..read]).map_err(|error| {
            UpdateCoreError::Io(format!("failed to write update artifact: {error}"))
        })?;
        on_progress(downloaded);
    }
    writer.flush().map_err(|error| {
        UpdateCoreError::Io(format!("failed to durably write update artifact: {error}"))
    })?;
    if downloaded != expected_size {
        return Err(UpdateCoreError::Truncated {
            expected: expected_size,
            actual: downloaded,
        });
    }
    Ok(BoundedTransferOutcome::Complete { downloaded })
}

/// Copies an artifact with a hard size limit and verifies its SHA-256 digest.
///
/// Legacy update manifests do not carry an expected artifact length, so their
/// HTTP adapter uses this bounded variant rather than the resumable transfer
/// primitive above. The caller still owns response and file lifecycle policy.
pub fn copy_and_verify_bounded(
    reader: &mut impl Read,
    writer: &mut impl Write,
    max_bytes: u64,
    expected_sha256: &str,
) -> Result<u64> {
    if max_bytes == 0 || max_bytes > MAX_ARTIFACT_BYTES {
        return Err(UpdateCoreError::TooLarge);
    }
    validate_sha256(expected_sha256, "update artifact")?;

    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            UpdateCoreError::Download(format!("failed while reading update artifact: {error}"))
        })?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or(UpdateCoreError::TooLarge)?;
        if total > max_bytes || total > MAX_ARTIFACT_BYTES {
            return Err(UpdateCoreError::TooLarge);
        }
        writer.write_all(&buffer[..read]).map_err(|error| {
            UpdateCoreError::Io(format!("failed to write update artifact: {error}"))
        })?;
        hasher.update(&buffer[..read]);
    }

    let actual = hex_sha256(hasher.finalize().into());
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(UpdateCoreError::HashMismatch {
            expected: expected_sha256.to_ascii_lowercase(),
            actual,
        });
    }
    Ok(total)
}

/// Rechecks a complete staged artifact before it can be exposed to a helper.
pub fn verify_artifact_file(
    path: impl AsRef<Path>,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<()> {
    validate_artifact_size(expected_size)?;
    validate_sha256(expected_sha256, "update artifact")?;
    let path = path.as_ref();
    let actual_size = fs::metadata(path)
        .map_err(|error| {
            UpdateCoreError::Io(format!("failed to inspect update artifact: {error}"))
        })?
        .len();
    if actual_size != expected_size {
        return Err(UpdateCoreError::Truncated {
            expected: expected_size,
            actual: actual_size,
        });
    }
    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(UpdateCoreError::HashMismatch {
            expected: expected_sha256.to_ascii_lowercase(),
            actual,
        });
    }
    Ok(())
}

/// Loads partial metadata, treating a missing sidecar as a clean resumable state.
pub fn read_partial_metadata(path: impl AsRef<Path>) -> Result<PartialMetadata> {
    let path = path.as_ref();
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PartialMetadata::default());
        }
        Err(error) => {
            return Err(UpdateCoreError::Io(format!(
                "failed to inspect update metadata: {error}"
            )));
        }
    };
    if metadata.len() > MAX_PARTIAL_METADATA_BYTES {
        return Err(UpdateCoreError::Download(
            "partial update metadata exceeds its size limit".to_owned(),
        ));
    }
    let bytes = fs::read(path)
        .map_err(|error| UpdateCoreError::Io(format!("failed to read update metadata: {error}")))?;
    serde_json::from_slice(&bytes).map_err(|error| {
        UpdateCoreError::Download(format!("invalid partial update metadata: {error}"))
    })
}

/// Atomically writes only the JSON metadata used to validate a resumed response.
pub fn write_partial_metadata(path: impl AsRef<Path>, metadata: &PartialMetadata) -> Result<()> {
    let bytes = serde_json::to_vec(metadata).map_err(|error| {
        UpdateCoreError::Download(format!("failed to serialize update metadata: {error}"))
    })?;
    write_atomic(path.as_ref(), &bytes, "update metadata")
}

/// Parses the start offset of an HTTP `Content-Range` response header.
#[must_use]
pub fn parse_content_range_start(value: &str) -> Option<u64> {
    value
        .strip_prefix("bytes ")?
        .split_once('-')?
        .0
        .parse()
        .ok()
}

fn validate_artifact_size(size: u64) -> Result<()> {
    if size == 0 || size > MAX_ARTIFACT_BYTES {
        return Err(UpdateCoreError::TooLarge);
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .map_err(|error| UpdateCoreError::Io(format!("failed to read update artifact: {error}")))?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            UpdateCoreError::Io(format!("failed to hash update artifact: {error}"))
        })?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or(UpdateCoreError::TooLarge)?;
        if total > MAX_ARTIFACT_BYTES {
            return Err(UpdateCoreError::TooLarge);
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_sha256(hasher.finalize().into()))
}

fn write_atomic(path: &Path, bytes: &[u8], label: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| UpdateCoreError::Io(format!("{label} path has no parent directory")))?;
    fs::create_dir_all(parent).map_err(|error| {
        UpdateCoreError::Io(format!("failed to create {label} directory: {error}"))
    })?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| UpdateCoreError::Io(format!("failed to create {label}: {error}")))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| UpdateCoreError::Io(format!("failed to write {label}: {error}")))?;
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| UpdateCoreError::Io(format!("failed to commit {label}: {}", error.error)))
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| UpdateCoreError::Io("update artifact has no parent directory".to_owned()))?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            UpdateCoreError::Io(format!("failed to sync update cache directory: {error}"))
        })
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> Result<()> {
    // Windows lacks a portable directory fsync; the renamed file was flushed by the caller.
    Ok(())
}
