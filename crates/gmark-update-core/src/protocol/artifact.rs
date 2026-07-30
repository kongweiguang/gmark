// @author kongweiguang

//! Verified artifact staging and file-opening primitives for the apply protocol.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Seek as _},
    path::Path,
};

use ed25519_dalek::VerifyingKey;
use tempfile::{Builder, TempPath};

use crate::{
    Result, UpdateCoreError,
    manifest::{SignedManifest, VerifiedManifest, parse_verified_manifest},
    policy::{Platform, validate_system_trust},
    staging::copy_and_verify_bounded,
};

use super::{ApplyPlanV1, validate_apply_plan_files};

/// A private, verified copy of the artifact that the helper can consume directly.
///
/// SECURITY: the helper must not reopen `ApplyPlanV1::artifact_path` after verification.
/// The file handle is deliberately declared before its cleanup path, so it is closed before
/// `TempPath` removes the path. On Windows that handle shares only read access, denying a
/// second writer, deletion, or rename until the platform consumer has finished with it.
pub struct StagedApplyArtifact {
    file: File,
    cleanup_path: TempPath,
}

impl StagedApplyArtifact {
    #[must_use]
    pub fn path(&self) -> &Path {
        self.cleanup_path.as_ref()
    }

    #[must_use]
    pub fn as_file(&self) -> &File {
        &self.file
    }

    pub fn as_file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    pub fn rewind(&mut self) -> Result<()> {
        self.file.rewind().map_err(|error| {
            UpdateCoreError::Io(format!("failed to rewind staged update artifact: {error}"))
        })
    }
}

/// Stages a unique private copy while hashing it against the signed plan metadata.
///
/// The returned file, rather than the mutable cache pathname, is what a helper must
/// pass to an installer, archive reader, or replacement operation.
pub fn stage_and_verify_apply_plan_artifact(
    plan: &ApplyPlanV1,
    key: &VerifyingKey,
    platform: &Platform,
    staging_directory: impl AsRef<Path>,
) -> Result<StagedApplyArtifact> {
    verify_apply_plan_manifest(plan, key, platform)?;
    let mut source = open_regular_file_no_follow(&plan.artifact_path).map_err(|error| {
        UpdateCoreError::Io(format!("failed to open verified update artifact: {error}"))
    })?;
    let mut builder = Builder::new();
    builder.prefix(".gmark-update-artifact-");
    #[cfg(target_os = "windows")]
    builder.suffix(".exe");
    let mut staged = builder.tempfile_in(staging_directory).map_err(|error| {
        UpdateCoreError::Io(format!("failed to stage update artifact: {error}"))
    })?;
    set_private_file(staged.as_file())?;
    let copied = copy_and_verify_bounded(
        &mut source,
        staged.as_file_mut(),
        plan.artifact_size,
        &plan.artifact_sha256,
    )?;
    if copied != plan.artifact_size {
        return Err(UpdateCoreError::Truncated {
            expected: plan.artifact_size,
            actual: copied,
        });
    }
    staged.as_file().sync_all().map_err(|error| {
        UpdateCoreError::Io(format!("failed to sync staged update artifact: {error}"))
    })?;

    // Closing the mutable handle before reopening is essential on Windows: a pre-existing
    // writer makes the guarded reopen fail, and the subsequent verification happens through
    // the handle that prevents later writes, deletion, and rename.
    let cleanup_path = staged.into_temp_path();
    let mut file = open_staged_file_for_consumption(&cleanup_path).map_err(|error| {
        UpdateCoreError::Io(format!("failed to protect staged update artifact: {error}"))
    })?;
    verify_staged_file(&mut file, plan.artifact_size, &plan.artifact_sha256)?;
    Ok(StagedApplyArtifact { file, cleanup_path })
}

pub(super) fn verify_apply_plan_manifest(
    plan: &ApplyPlanV1,
    key: &VerifyingKey,
    platform: &Platform,
) -> Result<VerifiedManifest> {
    validate_apply_plan_files(plan, platform)?;
    let envelope = super::read_bounded(
        &plan.signed_envelope_path,
        crate::MAX_ENVELOPE_BYTES as u64,
        "signed manifest",
    )?;
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
    Ok(verified)
}

pub(super) fn is_regular_non_reparse_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| is_regular_non_reparse_metadata(&metadata))
        .unwrap_or(false)
}

pub(super) fn is_directory_non_reparse(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| is_directory_non_reparse_metadata(&metadata))
        .unwrap_or(false)
}

pub(super) fn set_private_file(file: &File) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| {
                UpdateCoreError::Io(format!("failed to secure update protocol file: {error}"))
            })
    }
    #[cfg(not(unix))]
    {
        // Windows uses the inherited per-user ACL. `NamedTempFile` still creates a
        // fresh random leaf; callers must not place update roots in shared directories.
        let _ = file;
        Ok(())
    }
}

pub(super) fn open_regular_file_no_follow(path: &Path) -> io::Result<File> {
    let initial = fs::symlink_metadata(path)?;
    if !is_regular_non_reparse_metadata(&initial) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path is not a regular non-reparse file",
        ));
    }
    let file = open_file_no_follow(path)?;
    let opened = file.metadata()?;
    if !is_regular_non_reparse_metadata(&opened) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "opened path is not a regular non-reparse file",
        ));
    }
    Ok(file)
}

fn verify_staged_file(file: &mut File, expected_size: u64, expected_sha256: &str) -> Result<()> {
    let actual_size = file
        .metadata()
        .map_err(|error| {
            UpdateCoreError::Io(format!("failed to inspect staged update artifact: {error}"))
        })?
        .len();
    if actual_size != expected_size {
        return Err(UpdateCoreError::Truncated {
            expected: expected_size,
            actual: actual_size,
        });
    }
    file.rewind().map_err(|error| {
        UpdateCoreError::Io(format!("failed to rewind staged update artifact: {error}"))
    })?;
    let copied = copy_and_verify_bounded(file, &mut io::sink(), expected_size, expected_sha256)?;
    if copied != expected_size {
        return Err(UpdateCoreError::Truncated {
            expected: expected_size,
            actual: copied,
        });
    }
    file.rewind().map_err(|error| {
        UpdateCoreError::Io(format!("failed to rewind staged update artifact: {error}"))
    })
}

#[cfg(windows)]
fn open_staged_file_for_consumption(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    let initial = fs::symlink_metadata(path)?;
    if !is_regular_non_reparse_metadata(&initial) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "staged path is not a regular non-reparse file",
        ));
    }
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let opened = file.metadata()?;
    if !is_regular_non_reparse_metadata(&opened) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "opened staged path is not a regular non-reparse file",
        ));
    }
    Ok(file)
}

#[cfg(not(windows))]
fn open_staged_file_for_consumption(path: &Path) -> io::Result<File> {
    open_regular_file_no_follow(path)
}

fn is_regular_non_reparse_metadata(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_file()
        && !metadata.file_type().is_symlink()
        && !is_reparse_point(metadata)
}

fn is_directory_non_reparse_metadata(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_dir()
        && !metadata.file_type().is_symlink()
        && !is_reparse_point(metadata)
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn open_file_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;

    const O_NOFOLLOW: i32 = 0x2_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW)
        .open(path)
}

#[cfg(target_os = "macos")]
fn open_file_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;

    const O_NOFOLLOW: i32 = 0x100;
    OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW)
        .open(path)
}

#[cfg(windows)]
fn open_file_no_follow(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(all(
    unix,
    not(any(target_os = "linux", target_os = "android", target_os = "macos"))
))]
fn open_file_no_follow(path: &Path) -> io::Result<File> {
    // A final-component check still rejects known links on Unix variants without
    // a stable flag constant available in the standard library.
    File::open(path)
}

#[cfg(not(any(unix, windows)))]
fn open_file_no_follow(path: &Path) -> io::Result<File> {
    File::open(path)
}
