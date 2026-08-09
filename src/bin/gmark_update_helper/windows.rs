// @author kongweiguang

//! Windows Inno Setup installation and registry recovery policy.

use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
};

use gmark_update_core::{ApplyPlanV2, StagedApplyArtifact};

const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{7E04F75C-109D-4C5E-9E7B-BDE8F91FD0E1}_is1";
const OPEN_WITH_KEY: &str = r"HKCU\Software\Classes\Applications\gmark.exe";

#[must_use]
pub fn installer_args(log_path: &Path) -> Vec<String> {
    vec![
        "/SILENT".to_owned(),
        "/NOCANCEL".to_owned(),
        "/NORESTART".to_owned(),
        "/NOCLOSEAPPLICATIONS".to_owned(),
        "/SP-".to_owned(),
        format!("/LOG={}", log_path.display()),
    ]
}

#[must_use]
#[cfg(test)]
pub fn registry_delete_args(key: &str) -> [&str; 3] {
    ["delete", key, "/f"]
}

pub fn install(plan: &ApplyPlanV2, artifact: &mut StagedApplyArtifact) -> Result<(), String> {
    let target = &plan.target_path;
    let install_root = &plan.expected_install_root;
    validate_directory(install_root, "Windows installation root")?;
    validate_regular(target, "Windows target")?;
    validate_protocol_file(&plan.installer_log_path, "installer log")?;
    validate_backup_location(plan, install_root)?;
    if fs::symlink_metadata(&plan.backup_path).is_ok() {
        return Err(
            "Windows installation backup already exists; refusing to overwrite it".to_owned(),
        );
    }

    let backups = registry_backup_paths(plan)?;
    export_registry(UNINSTALL_KEY, &backups.0)?;
    export_registry(OPEN_WITH_KEY, &backups.1)?;

    let install_parent = install_root
        .parent()
        .ok_or_else(|| "Windows installation root has no parent directory".to_owned())?;
    fs::rename(install_root, &plan.backup_path).map_err(|error| {
        format!("failed to back up complete Windows installation directory: {error}")
    })?;

    let status = Command::new(artifact.path())
        .args(installer_args(&plan.installer_log_path))
        .current_dir(install_parent)
        .status()
        .map_err(|error| format!("failed to start Windows installer: {error}"))?;
    if !status.success() {
        return Err(format!("Windows installer exited with {status}"));
    }
    validate_install_location(install_root)?;
    validate_regular(target, "installed Windows target")
}

pub fn rollback(plan: &ApplyPlanV2) -> Result<(), String> {
    validate_directory(&plan.backup_path, "Windows installation backup")?;
    let install_root = &plan.expected_install_root;
    remove_directory_if_exists(install_root, "Windows installation root")?;
    fs::rename(&plan.backup_path, install_root).map_err(|error| {
        format!("failed to restore complete Windows installation directory: {error}")
    })?;
    let backups = registry_backup_paths(plan)?;
    restore_registry(UNINSTALL_KEY, &backups.0)?;
    restore_registry(OPEN_WITH_KEY, &backups.1)?;
    Ok(())
}

fn validate_install_location(expected_root: &Path) -> Result<(), String> {
    let output = Command::new("reg")
        .args(["query", UNINSTALL_KEY, "/v", "InstallLocation"])
        .output()
        .map_err(|error| format!("failed to query Windows install location: {error}"))?;
    if !output.status.success() {
        return Err("Windows installer did not create the expected uninstall key".to_owned());
    }
    if !install_location_matches(&String::from_utf8_lossy(&output.stdout), expected_root) {
        return Err(
            "Windows uninstall InstallLocation does not match the expected installation root"
                .to_owned(),
        );
    }
    Ok(())
}

#[must_use]
pub fn install_location_matches(output: &str, expected_root: &Path) -> bool {
    let expected = normalize_windows_path(&expected_root.to_string_lossy());
    output.lines().any(|line| {
        let mut fields = line.split_whitespace();
        if !fields
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("InstallLocation"))
        {
            return false;
        }
        let _value_type = fields.next();
        let actual = fields.collect::<Vec<_>>().join(" ");
        !actual.is_empty() && normalize_windows_path(&actual) == expected
    })
}

fn normalize_windows_path(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

fn validate_backup_location(plan: &ApplyPlanV2, install_root: &Path) -> Result<(), String> {
    let expected_parent = install_root
        .parent()
        .ok_or_else(|| "Windows installation root has no parent directory".to_owned())?;
    if plan.backup_path.parent() != Some(expected_parent) {
        return Err(
            "Windows installation backup must be a sibling of the expected install root".to_owned(),
        );
    }
    let transaction = plan.transaction_id.hyphenated().to_string();
    if !plan
        .backup_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains(&transaction))
    {
        return Err("Windows installation backup is not transaction-owned".to_owned());
    }
    Ok(())
}

fn registry_backup_paths(plan: &ApplyPlanV2) -> Result<(PathBuf, PathBuf), String> {
    let directory = plan
        .transaction_dir()
        .ok_or_else(|| "Windows transaction has no directory".to_owned())?;
    Ok((
        directory.join("registry-uninstall.reg"),
        directory.join("registry-open-with.reg"),
    ))
}

fn export_registry(key: &str, path: &Path) -> Result<(), String> {
    let status = Command::new("reg")
        .args(["export", key, &path.to_string_lossy(), "/y"])
        .status()
        .map_err(|error| format!("failed to back up Windows registry key: {error}"))?;
    if status.success() {
        return Ok(());
    }
    let query = Command::new("reg")
        .args(["query", key])
        .status()
        .map_err(|error| {
            format!("failed to determine whether Windows registry key exists: {error}")
        })?;
    if query.success() {
        return Err(format!("failed to back up Windows registry key {key}"));
    }
    // An empty marker is an explicit absent-key state. Rollback must execute
    // `reg delete /f` for this marker; it is not a no-op.
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(|_| ())
        .map_err(|error| format!("failed to record missing Windows registry key: {error}"))
}

fn restore_registry(key: &str, path: &Path) -> Result<(), String> {
    if fs::metadata(path)
        .map(|metadata| metadata.len() == 0)
        .unwrap_or(false)
    {
        return delete_registry_key(key);
    }
    if !path.exists() {
        return Ok(());
    }
    let status = Command::new("reg")
        .args(["import", &path.to_string_lossy()])
        .status()
        .map_err(|error| format!("failed to restore Windows registry key: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Windows registry restore exited with {status}"))
    }
}

fn delete_registry_key(key: &str) -> Result<(), String> {
    let status = Command::new("reg")
        .args(["delete", key, "/f"])
        .status()
        .map_err(|error| format!("failed to delete Windows registry key {key}: {error}"))?;
    if status.success() {
        return Ok(());
    }
    // `reg delete` returns a failure status when the key is already absent;
    // verify that state before treating it as a successful idempotent delete.
    let query = Command::new("reg")
        .args(["query", key])
        .status()
        .map_err(|error| format!("failed to verify deleted Windows registry key {key}: {error}"))?;
    if query.success() {
        Err(format!("Windows registry delete exited with {status}"))
    } else {
        Ok(())
    }
}

fn validate_regular(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {label}: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!("{label} must be a regular non-link file"));
    }
    Ok(())
}

fn validate_directory(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {label}: {error}"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(format!("{label} must be a real non-link directory"));
    }
    Ok(())
}

fn remove_directory_if_exists(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path).map_err(|error| format!("failed to remove {label}: {error}"))
        }
        Ok(_) => Err(format!("{label} must be a real non-link directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to inspect {label}: {error}")),
    }
}

fn validate_protocol_file(path: &Path, label: &str) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(format!("{label} must be a regular non-link file"));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/bin/gmark_update_helper/windows.rs"]
mod tests;
