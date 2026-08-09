// @author kongweiguang

//! Linux AppImage replacement policy for an ApplyPlanV2 transaction.
//!
//! The downloaded artifact lives under the transaction directory, which may
//! be on a different filesystem from the installed AppImage.  Installation
//! therefore copies the verified bytes into a unique temporary file beside
//! the target and commits with one same-directory rename.

use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

use gmark_update_core::{ApplyPlanV2, StagedApplyArtifact};

pub fn install(plan: &ApplyPlanV2, artifact: &mut StagedApplyArtifact) -> Result<(), String> {
    let target = &plan.target_path;
    let parent = target
        .parent()
        .ok_or_else(|| "AppImage target has no parent directory".to_owned())?;
    validate_parent(parent)?;
    validate_target(target)?;
    validate_backup_location(plan, parent)?;
    ensure_parent_writable(parent, plan)?;
    preflight_space(parent, plan.artifact_size)?;

    let current_mode = fs::symlink_metadata(target)
        .map_err(|error| format!("failed to inspect current AppImage: {error}"))?
        .permissions()
        .mode();
    create_backup(target, &plan.backup_path, parent, current_mode, plan)?;

    let temporary = unique_temp_path(parent, plan, "install");
    if let Err(error) = copy_verified_artifact(artifact, &temporary, current_mode) {
        remove_owned_temp(&temporary);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary, target) {
        remove_owned_temp(&temporary);
        return Err(format!(
            "failed to atomically install new AppImage: {error}"
        ));
    }
    sync_directory(parent)?;
    Ok(())
}

pub fn rollback(plan: &ApplyPlanV2) -> Result<(), String> {
    let target = &plan.target_path;
    let parent = target
        .parent()
        .ok_or_else(|| "AppImage target has no parent directory".to_owned())?;
    validate_parent(parent)?;
    validate_backup_location(plan, parent)?;
    validate_backup(&plan.backup_path)?;
    validate_target_or_missing(target)?;

    let mode = fs::symlink_metadata(&plan.backup_path)
        .map_err(|error| format!("failed to inspect AppImage backup: {error}"))?
        .permissions()
        .mode();
    let temporary = unique_temp_path(parent, plan, "rollback");
    if let Err(error) = copy_file_to_temp(&plan.backup_path, &temporary, mode) {
        remove_owned_temp(&temporary);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary, target) {
        remove_owned_temp(&temporary);
        return Err(format!(
            "failed to atomically restore AppImage backup: {error}"
        ));
    }
    sync_directory(parent)?;
    Ok(())
}

fn create_backup(
    target: &Path,
    backup: &Path,
    parent: &Path,
    mode: u32,
    plan: &ApplyPlanV2,
) -> Result<(), String> {
    if fs::symlink_metadata(backup).is_ok() {
        return Err("AppImage backup already exists; refusing to overwrite it".to_owned());
    }

    // A hard link is both cheap and crash-safe when the target and backup are
    // on the same filesystem.  Cross-device/unsupported filesystems fall
    // back to a fully synced copy through a unique temporary sibling.
    match fs::hard_link(target, backup) {
        Ok(()) => {
            sync_directory(parent)?;
            Ok(())
        }
        Err(hard_link_error) => {
            if fs::symlink_metadata(backup).is_ok() {
                return Err(format!(
                    "failed to create unique AppImage backup: {hard_link_error}"
                ));
            }
            let temporary = unique_temp_path(parent, plan, "backup");
            if let Err(error) = copy_file_to_temp(target, &temporary, mode) {
                remove_owned_temp(&temporary);
                return Err(format!(
                    "failed to back up current AppImage after hard-link fallback: {error}"
                ));
            }
            if let Err(error) = fs::rename(&temporary, backup) {
                remove_owned_temp(&temporary);
                return Err(format!("failed to commit AppImage backup: {error}"));
            }
            sync_directory(parent).map_err(|error| {
                format!("AppImage backup was created but parent sync failed: {error}")
            })
        }
    }
}

fn copy_verified_artifact(
    artifact: &mut StagedApplyArtifact,
    destination: &Path,
    mode: u32,
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
        .set_permissions(fs::Permissions::from_mode(mode | 0o700))
        .map_err(|error| format!("failed to set AppImage permissions: {error}"))?;
    io::copy(artifact.as_file_mut(), &mut output)
        .map_err(|error| format!("failed to copy verified AppImage artifact: {error}"))?;
    output
        .sync_all()
        .map_err(|error| format!("failed to sync AppImage install temporary: {error}"))
}

fn copy_file_to_temp(source: &Path, destination: &Path, mode: u32) -> Result<(), String> {
    let mut input = fs::File::open(source)
        .map_err(|error| format!("failed to open AppImage backup source: {error}"))?;
    let mut output = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("failed to create AppImage rollback temporary: {error}"))?;
    output
        .set_permissions(fs::Permissions::from_mode(mode | 0o700))
        .map_err(|error| format!("failed to set restored AppImage permissions: {error}"))?;
    io::copy(&mut input, &mut output)
        .map_err(|error| format!("failed to copy AppImage backup: {error}"))?;
    output
        .sync_all()
        .map_err(|error| format!("failed to sync AppImage rollback temporary: {error}"))
}

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

fn remove_owned_temp(path: &Path) {
    let _ = fs::remove_file(path);
}

fn validate_parent(parent: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(parent)
        .map_err(|error| format!("failed to inspect AppImage parent: {error}"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("AppImage parent must be a real directory".to_owned());
    }
    Ok(())
}

fn validate_backup_location(plan: &ApplyPlanV2, parent: &Path) -> Result<(), String> {
    if plan.backup_path.parent() != Some(parent) {
        return Err("AppImage backup must be a sibling of the installed target".to_owned());
    }
    let transaction = plan.transaction_id.hyphenated().to_string();
    if !plan
        .backup_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains(&transaction))
    {
        return Err("AppImage backup is not transaction-owned".to_owned());
    }
    Ok(())
}

fn validate_target(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect AppImage target: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("AppImage target must be a regular non-symlink file".to_owned());
    }
    Ok(())
}

fn validate_target_or_missing(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            Ok(())
        }
        Ok(_) => Err("AppImage target is not a regular non-symlink file".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to inspect AppImage target: {error}")),
    }
}

fn validate_backup(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect AppImage backup: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("AppImage backup is not a regular non-link file".to_owned());
    }
    Ok(())
}

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
