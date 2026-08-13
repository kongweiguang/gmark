// @author kongweiguang

//! Linux AppImage replacement policy for an ApplyPlanV2 transaction.
//!
//! The staged artifact can live on a different filesystem from the installed
//! AppImage, so this adapter copies it into a transaction-owned sibling before
//! committing one same-directory rename.  The protocol still carries
//! `backup_path` for wire compatibility, but Linux never creates or consumes
//! that path: there is no post-commit rollback window to make it authoritative.

use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
    process::Command,
};

use gmark_update_core::{ApplyPlanV2, StagedApplyArtifact};

use super::PlatformInstallFailure;

/// Copies the verified artifact beside the target and commits it atomically.
///
/// Keeping every write in the target directory makes the final rename a
/// single-filesystem operation; refusing links and preserving the existing
/// mode prevents a plan from redirecting the replacement or dropping the
/// executable permission of an AppImage.
pub fn install(
    plan: &ApplyPlanV2,
    artifact: &mut StagedApplyArtifact,
) -> Result<(), PlatformInstallFailure> {
    let target = &plan.target_path;
    let parent = target
        .parent()
        .ok_or_else(|| "AppImage target has no parent directory".to_owned())?;
    validate_clean_absolute_path(target, "AppImage target")?;
    validate_path_components(target, "AppImage target")?;
    validate_install_root(plan)?;
    validate_parent(parent)?;
    validate_target(target)?;
    ensure_parent_writable(parent, plan)?;
    preflight_space(parent, plan.artifact_size)?;

    let current_mode = fs::symlink_metadata(target)
        .map_err(|error| format!("failed to inspect current AppImage: {error}"))?
        .permissions()
        .mode()
        & 0o7777;
    let temporary = unique_temp_path(parent, plan, "install");
    if let Err(error) =
        copy_verified_artifact(artifact, &temporary, current_mode, plan.artifact_size)
    {
        remove_owned_temp(&temporary);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary, target) {
        remove_owned_temp(&temporary);
        return Err(format!(
            "failed to atomically install new AppImage: {error}"
        ));
    }
    sync_directory(parent).map_err(PlatformInstallFailure::committed_or_unknown)
}

/// Copies a guarded staged artifact into a fresh sibling and flushes it.
///
/// `StagedApplyArtifact` has already passed the core's signature, size, and
/// hash checks; checking the byte count again here prevents a short copy from
/// becoming the visible target if a future staging implementation changes.
fn copy_verified_artifact(
    artifact: &mut StagedApplyArtifact,
    destination: &Path,
    mode: u32,
    expected_size: u64,
) -> Result<(), String> {
    artifact
        .rewind()
        .map_err(|error| format!("failed to rewind verified AppImage artifact: {error}"))?;
    let mut output = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("failed to create AppImage install temporary: {error}"))?;
    output
        .set_permissions(fs::Permissions::from_mode(mode))
        .map_err(|error| format!("failed to inherit AppImage permissions: {error}"))?;
    let copied = io::copy(artifact.as_file_mut(), &mut output)
        .map_err(|error| format!("failed to copy verified AppImage artifact: {error}"))?;
    if copied != expected_size {
        return Err(format!(
            "verified AppImage artifact size changed during copy: expected {expected_size}, got {copied}"
        ));
    }
    output
        .sync_all()
        .map_err(|error| format!("failed to sync AppImage install temporary: {error}"))
}

/// Returns a transaction-owned temporary sibling without following links.
///
/// The later `create_new` open is the race-resistant ownership check; the
/// probe only improves diagnostics and avoids predictable name collisions.
fn unique_temp_path(parent: &Path, plan: &ApplyPlanV2, operation: &str) -> PathBuf {
    let transaction = plan.transaction_id.hyphenated();
    for attempt in 0..100u32 {
        let name = format!(
            ".gmark-update-{operation}-{transaction}-{}-{attempt}.tmp",
            std::process::id()
        );
        let candidate = parent.join(name);
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
    }
    // The caller's create_new open still protects against the improbable
    // exhaustion race; this path remains owned by this transaction.
    parent.join(format!(
        ".gmark-update-{operation}-{}-{}.tmp",
        transaction,
        std::process::id()
    ))
}

/// Removes only the temporary path owned by this invocation.
///
/// Cleanup is best effort because the original installation remains the
/// recovery point whenever the failure happens before the atomic rename.
fn remove_owned_temp(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Rejects relative, normalized-path escapes before touching the filesystem.
///
/// The core validator enforces this for a trusted V2 plan; repeating the
/// invariant at the platform boundary protects direct callers and future
/// refactors that might bypass the protocol validator.
fn validate_clean_absolute_path(path: &Path, label: &str) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(format!("{label} path must be an absolute normalized path"));
    }
    Ok(())
}

/// Rejects symlinked/reparse-like path components before any replacement.
///
/// Checking every existing component closes the ancestor-link escape that a
/// leaf-only `symlink_metadata` check would otherwise leave open.
fn validate_path_components(path: &Path, label: &str) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(format!("failed to inspect {label} path component: {error}"));
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(format!("{label} path contains a symlink"));
        }
    }
    Ok(())
}

/// Keeps the replacement lexically bound to the plan's expected install root.
///
/// Core validation already enforces this relationship for trusted V2 plans;
/// the adapter repeats it so a future direct caller cannot turn a valid
/// transaction into an arbitrary absolute-path write.
fn validate_install_root(plan: &ApplyPlanV2) -> Result<(), String> {
    let root = &plan.expected_install_root;
    validate_clean_absolute_path(root, "expected AppImage install root")?;
    validate_path_components(root, "expected AppImage install root")?;
    if !(plan.target_path == root.as_path() || plan.target_path.starts_with(root)) {
        return Err("AppImage target escapes the expected install root".to_owned());
    }
    Ok(())
}

/// Requires the target's immediate parent to be a real directory.
///
/// A same-directory rename is only an atomic replacement when that directory
/// is the intended installation location rather than a symlink redirect.
fn validate_parent(parent: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(parent)
        .map_err(|error| format!("failed to inspect AppImage parent: {error}"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("AppImage parent must be a real directory".to_owned());
    }
    Ok(())
}

/// Requires the current target to be a regular non-link file.
///
/// Replacing a link would make the update follow an attacker-controlled
/// destination; rejecting it also keeps failure-before-commit side-effect free.
fn validate_target(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect AppImage target: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("AppImage target must be a regular non-symlink file".to_owned());
    }
    Ok(())
}

/// Confirms that the target directory can host and remove a temporary sibling.
///
/// Failing before copying avoids consuming update state when the installed
/// AppImage location is read-only or otherwise unavailable.
fn ensure_parent_writable(parent: &Path, plan: &ApplyPlanV2) -> Result<(), String> {
    let probe = unique_temp_path(parent, plan, "write-probe");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|error| format!("AppImage parent is not writable: {error}"))?;
    drop(file);
    fs::remove_file(&probe)
        .map_err(|error| format!("failed to remove AppImage write probe: {error}"))
}

/// Fails early when the target directory cannot hold the staged replacement.
///
/// The temporary sibling briefly consumes the full artifact size, so reserving
/// a small margin avoids starting a copy that cannot complete.
fn preflight_space(parent: &Path, required: u64) -> Result<(), String> {
    let output = Command::new("df")
        .args(["-Pk", parent.to_string_lossy().as_ref()])
        .output()
        .map_err(|error| format!("failed to inspect available disk space: {error}"))?;
    if !output.status.success() {
        return Err("failed to inspect available disk space".to_owned());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .nth(1)
        .ok_or_else(|| "available disk space response was empty".to_owned())?;
    let available_kib = line
        .split_whitespace()
        .nth(3)
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| "available disk space response was invalid".to_owned())?;
    let required_kib = required.saturating_add(1024 - 1) / 1024;
    if available_kib < required_kib.saturating_add(1024) {
        return Err("insufficient disk space for the update".to_owned());
    }
    Ok(())
}

/// Flushes the parent directory so the atomic rename survives a crash.
///
/// The temporary file is synced before the rename; syncing the directory after
/// it makes the new directory entry durable without retaining a backup tree.
fn sync_directory(path: &Path) -> Result<(), String> {
    let directory = File::open(path)
        .map_err(|error| format!("failed to open AppImage parent for sync: {error}"))?;
    directory
        .sync_all()
        .map_err(|error| format!("failed to sync AppImage parent directory: {error}"))
}

#[cfg(test)]
#[path = "../../../tests/unit/bin/gmark_update_helper/linux.rs"]
mod tests;
