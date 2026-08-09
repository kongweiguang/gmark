// @author kongweiguang

//! Versioned file protocol shared by the app-side and helper-side adapters.

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{
    Result, UpdateCoreError,
    manifest::VerifiedManifest,
    policy::{ArtifactFormat, Platform, validate_official_artifact_url, validate_sha256},
    staging::verify_artifact_file,
};

mod artifact;
mod v2;

pub use artifact::{StagedApplyArtifact, stage_and_verify_apply_plan_artifact};
pub use v2::*;

pub const MAX_APPLY_PLAN_BYTES: u64 = 64 * 1024;
/// Maximum size of either the legacy result or a v2 result file.
pub const MAX_APPLY_RESULT_BYTES: u64 = 64 * 1024;
/// Maximum size of an atomic v2 progress snapshot.
pub const MAX_APPLY_PROGRESS_BYTES: u64 = 64 * 1024;
/// Explicit v2 alias kept for callers that want the schema in the name.
pub const MAX_APPLY_RESULT_V2_BYTES: u64 = MAX_APPLY_RESULT_BYTES;
/// Human-readable diagnostics are deliberately bounded independently of JSON size.
pub const MAX_APPLY_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_STARTUP_ACKNOWLEDGEMENT_BYTES: usize = 1024;

/// JSON plan handed to `gmark-update-helper --apply-plan`; fields are wire-compatible.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyPlanV1 {
    pub schema_version: u8,
    pub parent_pid: u32,
    pub current_version: String,
    pub target_version: String,
    pub artifact_path: PathBuf,
    pub artifact_url: String,
    pub artifact_size: u64,
    pub artifact_sha256: String,
    pub artifact_format: String,
    pub signed_envelope_path: PathBuf,
    pub target_path: PathBuf,
    pub backup_path: PathBuf,
    pub relaunch_path: PathBuf,
    pub acknowledgement_path: PathBuf,
    pub cancellation_path: PathBuf,
    pub result_path: PathBuf,
    pub helper_log_path: PathBuf,
}

impl ApplyPlanV1 {
    pub const SCHEMA_VERSION: u8 = 1;
}

/// Identifies the file side of the established acknowledgement/cancellation handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelperSignalV1 {
    Acknowledgement,
    Cancellation,
}

impl HelperSignalV1 {
    #[must_use]
    pub fn path(self, plan: &ApplyPlanV1) -> &Path {
        match self {
            Self::Acknowledgement => &plan.acknowledgement_path,
            Self::Cancellation => &plan.cancellation_path,
        }
    }
}

/// Startup acknowledgement content written by the newly launched app process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupAcknowledgementV1 {
    pub version: String,
}

impl StartupAcknowledgementV1 {
    #[must_use]
    pub fn for_target_version(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
        }
    }

    /// The existing app/helper protocol is a version text line rather than JSON.
    #[must_use]
    pub fn marker_bytes(&self) -> Vec<u8> {
        let mut bytes = self.version.as_bytes().to_vec();
        bytes.push(b'\n');
        bytes
    }
}

/// Cancellation marker content written before the parent application exits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CancellationV1;

impl CancellationV1 {
    pub const MARKER_BYTES: &'static [u8] = b"cancelled\n";
}

/// Result JSON written by the helper after a succeeded or failed transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyResultV1 {
    pub schema_version: u8,
    pub status: String,
    /// Older helper result files omitted this display-only field; retain their
    /// cache-recovery compatibility while always writing it in new results.
    #[serde(default)]
    pub from_version: String,
    pub to_version: String,
    pub message: String,
}

#[derive(Deserialize)]
struct ApplyResultReadV1 {
    schema_version: u8,
    status: String,
    #[serde(default)]
    from_version: String,
    to_version: String,
    message: String,
}

impl ApplyResultV1 {
    pub const SCHEMA_VERSION: u8 = 1;

    #[must_use]
    pub fn succeeded(from_version: impl Into<String>, to_version: impl Into<String>) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            status: "succeeded".to_owned(),
            from_version: from_version.into(),
            to_version: to_version.into(),
            message: "update installed and acknowledged".to_owned(),
        }
    }

    #[must_use]
    pub fn failed(
        from_version: impl Into<String>,
        to_version: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            status: "failed".to_owned(),
            from_version: from_version.into(),
            to_version: to_version.into(),
            message: message.into(),
        }
    }
}

/// Reads a size-bounded plan before JSON parsing, mirroring helper entry-point behavior.
pub fn read_apply_plan(path: impl AsRef<Path>) -> Result<ApplyPlanV1> {
    let bytes = read_bounded(path.as_ref(), MAX_APPLY_PLAN_BYTES, "apply plan")?;
    serde_json::from_slice(&bytes)
        .map_err(|error| UpdateCoreError::Protocol(format!("invalid apply plan: {error}")))
}

/// Reads and structurally validates the single plan explicitly supplied to the helper.
///
/// SECURITY: no plan-derived file is opened here. The bounded read is limited to
/// `path`; all following validation is lexical and side-effect free.
pub fn read_validated_apply_plan(
    path: impl AsRef<Path>,
    platform: &Platform,
) -> Result<ApplyPlanV1> {
    let path = path.as_ref();
    let plan = read_apply_plan(path)?;
    validate_apply_plan_at_path(&plan, path, platform)?;
    Ok(plan)
}

/// Writes an apply plan atomically so the helper never observes a partial JSON object.
pub fn write_apply_plan(path: impl AsRef<Path>, plan: &ApplyPlanV1) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(plan).map_err(|error| {
        UpdateCoreError::Protocol(format!("failed to serialize update apply plan: {error}"))
    })?;
    if bytes.len() as u64 > MAX_APPLY_PLAN_BYTES {
        return Err(UpdateCoreError::Protocol(
            "apply plan exceeds its size limit".to_owned(),
        ));
    }
    write_apply_plan_atomic(path.as_ref(), &bytes)
}

/// Validates pure helper-protocol invariants without replacing an installed binary.
pub fn validate_apply_plan(plan: &ApplyPlanV1, platform: &Platform) -> Result<()> {
    if plan.schema_version != ApplyPlanV1::SCHEMA_VERSION {
        return Err(UpdateCoreError::Protocol(
            "unsupported apply plan schema".to_owned(),
        ));
    }
    let target = semver::Version::parse(&plan.target_version)
        .map_err(|error| UpdateCoreError::Protocol(format!("invalid target version: {error}")))?;
    let current = semver::Version::parse(&plan.current_version)
        .map_err(|error| UpdateCoreError::Protocol(format!("invalid current version: {error}")))?;
    if target <= current {
        return Err(UpdateCoreError::Protocol(
            "target version must be newer than current version".to_owned(),
        ));
    }
    if plan.target_version.len().saturating_add(1) > MAX_STARTUP_ACKNOWLEDGEMENT_BYTES {
        return Err(UpdateCoreError::Protocol(
            "target version exceeds the acknowledgement size limit".to_owned(),
        ));
    }
    validate_plan_paths(plan)?;
    if plan.artifact_size == 0 || plan.artifact_size > crate::MAX_ARTIFACT_BYTES {
        return Err(UpdateCoreError::Protocol(
            "apply plan has invalid artifact bounds or digest".to_owned(),
        ));
    }
    validate_sha256(&plan.artifact_sha256, "apply plan artifact").map_err(|_| {
        UpdateCoreError::Protocol("apply plan has invalid artifact bounds or digest".to_owned())
    })?;
    validate_official_artifact_url(&plan.artifact_url)?;
    validate_platform_plan(plan, platform)
}

/// Binds a plan's fixed transaction layout to the explicitly supplied plan location.
pub fn validate_apply_plan_at_path(
    plan: &ApplyPlanV1,
    plan_path: impl AsRef<Path>,
    platform: &Platform,
) -> Result<()> {
    validate_apply_plan(plan, platform)?;
    validate_plan_path(plan, plan_path.as_ref())
}

/// Performs the helper's filesystem checks after the pure plan checks succeed.
pub fn validate_apply_plan_files(plan: &ApplyPlanV1, platform: &Platform) -> Result<()> {
    // SECURITY: establish all structural/path invariants before touching any
    // pathname obtained from a plan.
    validate_apply_plan(plan, platform)?;
    if !artifact::is_regular_non_reparse_file(&plan.artifact_path)
        || !artifact::is_regular_non_reparse_file(&plan.signed_envelope_path)
    {
        return Err(UpdateCoreError::Protocol(
            "verified update files are missing".to_owned(),
        ));
    }
    validate_platform_target_on_disk(plan, platform)
}

/// Revalidates the signed v2 manifest and artifact bytes for callers that do not consume it.
///
/// The helper must use [`stage_and_verify_apply_plan_artifact`] so the artifact
/// it consumes is the exact file whose bytes were checked.
pub fn verify_apply_plan_artifact(
    plan: &ApplyPlanV1,
    key: &VerifyingKey,
    platform: &Platform,
) -> Result<VerifiedManifest> {
    let verified = artifact::verify_apply_plan_manifest(plan, key, platform)?;
    verify_artifact_file(
        &plan.artifact_path,
        plan.artifact_size,
        &plan.artifact_sha256,
    )?;
    Ok(verified)
}

/// Reads a bounded helper result while retaining legacy display compatibility.
pub fn read_apply_result(path: impl AsRef<Path>) -> Result<ApplyResultV1> {
    let bytes = read_bounded(path.as_ref(), MAX_APPLY_RESULT_BYTES, "update result")?;
    parse_apply_result(&bytes)
}

/// Parses the exact bounded bytes used by a caller for fingerprinting or display.
/// This avoids observing two different result-file revisions across separate reads.
pub fn parse_apply_result(bytes: &[u8]) -> Result<ApplyResultV1> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_APPLY_RESULT_BYTES {
        return Err(UpdateCoreError::Protocol(
            "update result exceeds its size limit".to_owned(),
        ));
    }
    let result: ApplyResultReadV1 = serde_json::from_slice(bytes)
        .map_err(|error| UpdateCoreError::Protocol(format!("invalid update result: {error}")))?;
    let result = ApplyResultV1 {
        schema_version: result.schema_version,
        status: result.status,
        from_version: result.from_version,
        to_version: result.to_version,
        message: result.message,
    };
    validate_apply_result_schema(&result)?;
    Ok(result)
}

/// Atomically writes the result JSON that the next app process consumes.
pub fn write_apply_result(path: impl AsRef<Path>, result: &ApplyResultV1) -> Result<()> {
    validate_apply_result_for_write(result)?;
    let bytes = serde_json::to_vec_pretty(result).map_err(|error| {
        UpdateCoreError::Protocol(format!("failed to serialize update result: {error}"))
    })?;
    write_atomic(path.as_ref(), &bytes, "update result")
}

/// Writes an exact, bounded version acknowledgement without following a marker pathname.
pub fn write_startup_acknowledgement(path: impl AsRef<Path>, target_version: &str) -> Result<()> {
    let marker = StartupAcknowledgementV1::for_target_version(target_version).marker_bytes();
    if marker.len() > MAX_STARTUP_ACKNOWLEDGEMENT_BYTES {
        return Err(UpdateCoreError::Protocol(
            "startup acknowledgement exceeds its size limit".to_owned(),
        ));
    }
    write_atomic(path.as_ref(), &marker, "startup acknowledgement")
}

/// Writes the version-text acknowledgement or cancellation marker expected by the helper.
pub fn write_helper_signal(plan: &ApplyPlanV1, signal: HelperSignalV1) -> Result<()> {
    match signal {
        HelperSignalV1::Acknowledgement => {
            write_startup_acknowledgement(&plan.acknowledgement_path, &plan.target_version)
        }
        HelperSignalV1::Cancellation => write_atomic(
            &plan.cancellation_path,
            CancellationV1::MARKER_BYTES,
            "cancellation marker",
        ),
    }
}

/// Returns whether the peer wrote the corresponding valid marker.
///
/// Acknowledgements must match the target version byte-for-byte. This rejects empty,
/// partial, stale, symlinked, and otherwise substituted markers instead of treating
/// pathname existence as proof that the relaunched app started.
pub fn helper_signal_present(plan: &ApplyPlanV1, signal: HelperSignalV1) -> Result<bool> {
    match signal {
        HelperSignalV1::Acknowledgement => startup_acknowledgement_matches(plan),
        HelperSignalV1::Cancellation => marker_file_present(&plan.cancellation_path),
    }
}

/// Checks the exact V1 acknowledgement marker expected for this transaction.
pub fn startup_acknowledgement_matches(plan: &ApplyPlanV1) -> Result<bool> {
    let expected =
        StartupAcknowledgementV1::for_target_version(&plan.target_version).marker_bytes();
    if expected.len() > MAX_STARTUP_ACKNOWLEDGEMENT_BYTES {
        return Err(UpdateCoreError::Protocol(
            "startup acknowledgement exceeds its size limit".to_owned(),
        ));
    }
    let Some(actual) = read_bounded_if_exists(
        &plan.acknowledgement_path,
        MAX_STARTUP_ACKNOWLEDGEMENT_BYTES as u64,
        "startup acknowledgement",
    )?
    else {
        return Ok(false);
    };
    if actual != expected {
        return Err(UpdateCoreError::Protocol(
            "startup acknowledgement does not match the update target".to_owned(),
        ));
    }
    Ok(true)
}

fn marker_file_present(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.file_type().is_file() && !metadata.file_type().is_symlink()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(UpdateCoreError::Io(format!(
            "failed to inspect helper signal: {error}"
        ))),
    }
}

/// Clears a stale marker before starting a fresh helper transaction.
pub fn clear_helper_signal(plan: &ApplyPlanV1, signal: HelperSignalV1) -> Result<()> {
    match fs::remove_file(signal.path(plan)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(UpdateCoreError::Io(format!(
            "failed to clear helper signal: {error}"
        ))),
    }
}

fn validate_plan_paths(plan: &ApplyPlanV1) -> Result<()> {
    for (label, path) in [
        ("update artifact", &plan.artifact_path),
        ("signed manifest", &plan.signed_envelope_path),
        ("update target", &plan.target_path),
        ("update backup", &plan.backup_path),
        ("update relaunch", &plan.relaunch_path),
        ("startup acknowledgement", &plan.acknowledgement_path),
        ("cancellation marker", &plan.cancellation_path),
        ("update result", &plan.result_path),
        ("helper log", &plan.helper_log_path),
    ] {
        validate_clean_absolute_path(path, label)?;
    }
    let transaction_dir = plan.artifact_path.parent().ok_or_else(|| {
        UpdateCoreError::Protocol("update artifact has no transaction directory".to_owned())
    })?;
    let updates_root = transaction_dir.parent().ok_or_else(|| {
        UpdateCoreError::Protocol("update transaction has no cache root".to_owned())
    })?;
    if plan.artifact_path != transaction_dir.join("artifact.ready")
        || plan.signed_envelope_path != transaction_dir.join("manifest.envelope.json")
        || plan.acknowledgement_path != transaction_dir.join("startup-ack")
        || plan.cancellation_path != transaction_dir.join("cancel-install")
        || plan.result_path != updates_root.join("last-result.json")
        || plan.helper_log_path != updates_root.join("last-helper.log")
    {
        return Err(UpdateCoreError::Protocol(
            "apply plan paths do not match the versioned update protocol".to_owned(),
        ));
    }
    let expected_transaction = format!("v{}", plan.target_version);
    if transaction_dir.file_name().and_then(|name| name.to_str())
        != Some(expected_transaction.as_str())
    {
        return Err(UpdateCoreError::Protocol(
            "apply plan transaction does not match the target version".to_owned(),
        ));
    }
    Ok(())
}

fn validate_plan_path(plan: &ApplyPlanV1, plan_path: &Path) -> Result<()> {
    validate_clean_absolute_path(plan_path, "apply plan")?;
    let transaction_dir = plan.artifact_path.parent().ok_or_else(|| {
        UpdateCoreError::Protocol("update artifact has no transaction directory".to_owned())
    })?;
    if plan_path != transaction_dir.join("apply-plan.json") {
        return Err(UpdateCoreError::Protocol(
            "apply plan path does not match the versioned update protocol".to_owned(),
        ));
    }
    Ok(())
}

fn validate_clean_absolute_path(path: &Path, label: &str) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(UpdateCoreError::Protocol(format!(
            "{label} path is not an absolute normalized path"
        )));
    }
    Ok(())
}

fn validate_platform_plan(plan: &ApplyPlanV1, platform: &Platform) -> Result<()> {
    let expected_format = match platform.os.as_str() {
        "windows" => ArtifactFormat::WindowsSetupExe,
        "macos" => ArtifactFormat::MacosAppTarGz,
        "linux" => ArtifactFormat::LinuxAppImage,
        _ => {
            return Err(UpdateCoreError::Protocol(
                "this platform cannot apply gmark updates".to_owned(),
            ));
        }
    };
    if ArtifactFormat::from_protocol_name(&plan.artifact_format) != Some(expected_format) {
        return Err(UpdateCoreError::Protocol(format!(
            "artifact format '{}' is invalid for this platform",
            plan.artifact_format
        )));
    }

    let target_parent = plan.target_path.parent().ok_or_else(|| {
        UpdateCoreError::Protocol("update target has no parent directory".to_owned())
    })?;
    let target_name = plan
        .target_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            UpdateCoreError::Protocol("update target has no valid file name".to_owned())
        })?;
    if plan.backup_path != target_parent.join(format!("{target_name}.gmark-update-backup")) {
        return Err(UpdateCoreError::Protocol(
            "backup path is not the required sibling of the update target".to_owned(),
        ));
    }

    match platform.os.as_str() {
        "windows"
            if plan.relaunch_path != plan.target_path
                || !target_name.eq_ignore_ascii_case("gmark.exe") =>
        {
            Err(UpdateCoreError::Protocol(
                "Windows update target is not the installed gmark executable".to_owned(),
            ))
        }
        "macos"
            if target_name != "gmark.app"
                || plan
                    .target_path
                    .extension()
                    .and_then(|value| value.to_str())
                    != Some("app")
                || plan.relaunch_path != plan.target_path.join("Contents/MacOS/gmark") =>
        {
            Err(UpdateCoreError::Protocol(
                "macOS update target is not a gmark application bundle".to_owned(),
            ))
        }
        "linux" if plan.relaunch_path != plan.target_path => Err(UpdateCoreError::Protocol(
            "Linux update target is not a regular AppImage file".to_owned(),
        )),
        _ => Ok(()),
    }
}

fn validate_platform_target_on_disk(plan: &ApplyPlanV1, platform: &Platform) -> Result<()> {
    match platform.os.as_str() {
        "windows" if !artifact::is_regular_non_reparse_file(&plan.target_path) => {
            Err(UpdateCoreError::Protocol(
                "Windows update target is not the installed gmark executable".to_owned(),
            ))
        }
        "macos" if !artifact::is_directory_non_reparse(&plan.target_path) => {
            Err(UpdateCoreError::Protocol(
                "macOS update target is not a gmark application bundle".to_owned(),
            ))
        }
        "linux" if !artifact::is_regular_non_reparse_file(&plan.target_path) => {
            Err(UpdateCoreError::Protocol(
                "Linux update target is not a regular AppImage file".to_owned(),
            ))
        }
        _ => Ok(()),
    }
}

fn validate_apply_result_for_write(result: &ApplyResultV1) -> Result<()> {
    validate_apply_result_schema(result)?;
    if !matches!(result.status.as_str(), "succeeded" | "failed") {
        return Err(UpdateCoreError::Protocol(
            "update result has an unsupported status".to_owned(),
        ));
    }
    Ok(())
}

fn validate_apply_result_schema(result: &ApplyResultV1) -> Result<()> {
    if result.schema_version != ApplyResultV1::SCHEMA_VERSION {
        return Err(UpdateCoreError::Protocol(
            "unsupported update result schema".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn write_apply_plan_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let label = "update apply plan";
    let parent = path.parent().ok_or_else(|| {
        UpdateCoreError::Protocol(format!("{label} path has no parent directory"))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        UpdateCoreError::Io(format!("failed to create {label} directory: {error}"))
    })?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| UpdateCoreError::Io(format!("failed to create {label}: {error}")))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| UpdateCoreError::Io(format!("failed to write {label}: {error}")))?;
    artifact::set_private_file(temporary.as_file())?;
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| UpdateCoreError::Io(format!("failed to commit {label}: {}", error.error)))
}

#[cfg(not(unix))]
fn write_apply_plan_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    write_atomic(path, bytes, "update apply plan")
}

fn read_bounded(path: &Path, max_bytes: u64, label: &str) -> Result<Vec<u8>> {
    let file = artifact::open_regular_file_no_follow(path)
        .map_err(|error| UpdateCoreError::Io(format!("failed to open {label}: {error}")))?;
    read_bounded_file(file, max_bytes, label)
}

fn read_bounded_if_exists(path: &Path, max_bytes: u64, label: &str) -> Result<Option<Vec<u8>>> {
    let file = match artifact::open_regular_file_no_follow(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(UpdateCoreError::Io(format!(
                "failed to open {label}: {error}"
            )));
        }
    };
    read_bounded_file(file, max_bytes, label).map(Some)
}

fn read_bounded_file(file: File, max_bytes: u64, label: &str) -> Result<Vec<u8>> {
    let length = file
        .metadata()
        .map_err(|error| UpdateCoreError::Io(format!("failed to inspect {label}: {error}")))?
        .len();
    if length == 0 || length > max_bytes {
        return Err(UpdateCoreError::Protocol(format!(
            "{label} exceeds its size limit"
        )));
    }
    let capacity = usize::try_from(length)
        .map_err(|_| UpdateCoreError::Protocol(format!("{label} exceeds its size limit")))?;
    let mut bytes = Vec::with_capacity(capacity);
    let maximum_read = max_bytes
        .checked_add(1)
        .ok_or_else(|| UpdateCoreError::Protocol(format!("{label} exceeds its size limit")))?;
    file.take(maximum_read)
        .read_to_end(&mut bytes)
        .map_err(|error| UpdateCoreError::Io(format!("failed to read {label}: {error}")))?;
    if bytes.is_empty() || bytes.len() as u64 > max_bytes {
        return Err(UpdateCoreError::Protocol(format!(
            "{label} exceeds its size limit"
        )));
    }
    Ok(bytes)
}

fn write_atomic(path: &Path, bytes: &[u8], label: &str) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        UpdateCoreError::Protocol(format!("{label} path has no parent directory"))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        UpdateCoreError::Io(format!("failed to create {label} directory: {error}"))
    })?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| UpdateCoreError::Io(format!("failed to create {label}: {error}")))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| UpdateCoreError::Io(format!("failed to write {label}: {error}")))?;
    artifact::set_private_file(temporary.as_file())?;
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| UpdateCoreError::Io(format!("failed to commit {label}: {}", error.error)))
}
