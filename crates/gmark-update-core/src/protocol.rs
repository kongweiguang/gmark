// @author kongweiguang

//! Versioned file protocol shared by the app-side and helper-side adapters.

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{
    Result, UpdateCoreError,
    manifest::{SignedManifest, VerifiedManifest, parse_verified_manifest},
    policy::{
        ArtifactFormat, Platform, validate_official_artifact_url, validate_sha256,
        validate_system_trust,
    },
    staging::verify_artifact_file,
};

pub const MAX_APPLY_PLAN_BYTES: u64 = 64 * 1024;
const MAX_APPLY_RESULT_BYTES: u64 = 64 * 1024;

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

/// Performs the helper's filesystem checks after the pure plan checks succeed.
pub fn validate_apply_plan_files(plan: &ApplyPlanV1, platform: &Platform) -> Result<()> {
    if !plan.artifact_path.is_file() || !plan.signed_envelope_path.is_file() {
        return Err(UpdateCoreError::Protocol(
            "verified update files are missing".to_owned(),
        ));
    }
    validate_apply_plan(plan, platform)?;
    validate_platform_target_on_disk(plan, platform)
}

/// Revalidates the signed v2 manifest and artifact bytes before a helper applies a plan.
pub fn verify_apply_plan_artifact(
    plan: &ApplyPlanV1,
    key: &VerifyingKey,
    platform: &Platform,
) -> Result<VerifiedManifest> {
    validate_apply_plan_files(plan, platform)?;
    let envelope = fs::read(&plan.signed_envelope_path)
        .map_err(|error| UpdateCoreError::Io(format!("failed to read signed manifest: {error}")))?;
    let verified = parse_verified_manifest(&envelope, key)?;
    let SignedManifest::V2(manifest) = &verified.manifest else {
        return Err(UpdateCoreError::Protocol(
            "signed manifest does not match the apply plan".to_owned(),
        ));
    };
    if manifest.version != plan.target_version || manifest.paused {
        return Err(UpdateCoreError::Protocol(
            "signed manifest does not match the apply plan".to_owned(),
        ));
    }
    let artifact = manifest
        .artifacts
        .values()
        .find(|artifact| artifact.url == plan.artifact_url)
        .ok_or_else(|| {
            UpdateCoreError::Protocol("apply artifact is absent from signed manifest".to_owned())
        })?;
    if artifact.size != plan.artifact_size
        || !artifact.sha256.eq_ignore_ascii_case(&plan.artifact_sha256)
        || artifact.format.as_protocol_name() != plan.artifact_format
        || validate_system_trust(artifact.system_trust, platform).is_err()
    {
        return Err(UpdateCoreError::Protocol(
            "signed artifact metadata does not match the apply plan".to_owned(),
        ));
    }
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

/// Writes the version-text acknowledgement or cancellation marker expected by the helper.
pub fn write_helper_signal(plan: &ApplyPlanV1, signal: HelperSignalV1) -> Result<()> {
    let path = signal.path(plan);
    let parent = path.parent().ok_or_else(|| {
        UpdateCoreError::Protocol("helper signal path has no parent directory".to_owned())
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        UpdateCoreError::Io(format!("failed to create helper signal directory: {error}"))
    })?;
    let mut file = File::create(path)
        .map_err(|error| UpdateCoreError::Io(format!("failed to create helper signal: {error}")))?;
    let bytes = match signal {
        HelperSignalV1::Acknowledgement => {
            StartupAcknowledgementV1::for_target_version(&plan.target_version).marker_bytes()
        }
        HelperSignalV1::Cancellation => CancellationV1::MARKER_BYTES.to_vec(),
    };
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| UpdateCoreError::Io(format!("failed to persist helper signal: {error}")))
}

/// Returns whether the corresponding marker file has been written by the peer process.
pub fn helper_signal_present(plan: &ApplyPlanV1, signal: HelperSignalV1) -> Result<bool> {
    match fs::metadata(signal.path(plan)) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
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
        "windows" if !is_regular_non_symlink(&plan.target_path) => Err(UpdateCoreError::Protocol(
            "Windows update target is not the installed gmark executable".to_owned(),
        )),
        "macos" if !plan.target_path.is_dir() => Err(UpdateCoreError::Protocol(
            "macOS update target is not a gmark application bundle".to_owned(),
        )),
        "linux" if !is_regular_non_symlink(&plan.target_path) => Err(UpdateCoreError::Protocol(
            "Linux update target is not a regular AppImage file".to_owned(),
        )),
        _ => Ok(()),
    }
}

fn is_regular_non_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
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
    set_apply_plan_private(temporary.as_file())?;
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| UpdateCoreError::Io(format!("failed to commit {label}: {}", error.error)))
}

#[cfg(not(unix))]
fn write_apply_plan_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    write_atomic(path, bytes, "update apply plan")
}

#[cfg(unix)]
fn set_apply_plan_private(file: &File) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| {
            UpdateCoreError::Io(format!("failed to secure update apply plan: {error}"))
        })
}

fn read_bounded(path: &Path, max_bytes: u64, label: &str) -> Result<Vec<u8>> {
    let length = fs::metadata(path)
        .map_err(|error| UpdateCoreError::Io(format!("failed to inspect {label}: {error}")))?
        .len();
    if length == 0 || length > max_bytes {
        return Err(UpdateCoreError::Protocol(format!(
            "{label} exceeds its size limit"
        )));
    }
    fs::read(path).map_err(|error| UpdateCoreError::Io(format!("failed to read {label}: {error}")))
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
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| UpdateCoreError::Io(format!("failed to commit {label}: {}", error.error)))
}
