// @author kongweiguang

//! Windows Inno Setup installation policy.

use std::{fs, path::Path, process::Command};

use gmark_update_core::{ApplyPlanV2, StagedApplyArtifact};

use super::PlatformInstallFailure;

const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{7E04F75C-109D-4C5E-9E7B-BDE8F91FD0E1}_is1";

/// Keeps the native Inno progress window and durable log while preventing
/// installer-side prompts from racing the helper's lifecycle handoff.
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

/// Runs the verified installer only after the real install root and executable
/// are checked, because the helper must never create a second installation
/// location or replace an unexpected path.
pub fn install(
    plan: &ApplyPlanV2,
    artifact: &mut StagedApplyArtifact,
) -> Result<(), PlatformInstallFailure> {
    let target = &plan.target_path;
    let install_root = &plan.expected_install_root;
    validate_directory(install_root, "Windows installation root")?;
    validate_regular(target, "Windows target")?;
    validate_protocol_file(&plan.installer_log_path, "installer log")?;

    let install_parent = install_root
        .parent()
        .ok_or_else(|| "Windows installation root has no parent directory".to_owned())?;
    let status = Command::new(artifact.path())
        .args(installer_args(&plan.installer_log_path))
        .current_dir(install_parent)
        .status()
        .map_err(|error| format!("failed to start Windows installer: {error}"))?;
    if !status.success() {
        return Err(PlatformInstallFailure::committed_or_unknown(format!(
            "Windows installer exited with {status}"
        )));
    }
    validate_install_location(install_root)
        .and_then(|()| validate_regular(target, "installed Windows target"))
        .and_then(|()| validate_installed_version(target, &plan.target_version))
        .map_err(PlatformInstallFailure::committed_or_unknown)
}

/// 执行安装后的真实目标而不是信任 Inno 退出码，防止错误 payload 被当成目标版本并自动启动。
fn validate_installed_version(target: &Path, expected_version: &str) -> Result<(), String> {
    let output = Command::new(target)
        .arg("--version")
        .output()
        .map_err(|error| format!("failed to query installed Windows version: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "installed Windows target version check exited with {}",
            output.status
        ));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| "installed Windows target returned a non-UTF-8 version".to_owned())?;
    validate_installed_version_output(&stdout, expected_version)
}

/// 严格比较 CLI 的单行版本输出，避免包含目标版本号的诊断文字被误认为安装成功。
fn validate_installed_version_output(stdout: &str, expected_version: &str) -> Result<(), String> {
    let expected = format!("Gmark {expected_version}");
    if stdout.trim() != expected {
        return Err(format!(
            "installed Windows target version mismatch: expected '{expected}', got '{}'",
            stdout.trim()
        ));
    }
    Ok(())
}

/// Re-reads Inno's uninstall registration after installation so a successful
/// process exit cannot be mistaken for an install into a different directory.
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

/// Compares registry output after normalizing Windows path spelling, because
/// `reg query` may quote values or leave a trailing separator inconsistently.
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

/// Keeps path comparison case-insensitive and separator-tolerant, matching
/// Windows filesystem semantics without resolving a path through a link.
fn normalize_windows_path(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

/// Rejects links and non-files before invoking Inno, because installer failure
/// must not leave the helper operating on an attacker-controlled executable.
fn validate_regular(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {label}: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!("{label} must be a regular non-link file"));
    }
    Ok(())
}

/// Requires a real directory for Inno's fixed per-user target, preventing a
/// junction or symlink from redirecting the installer outside the validated root.
fn validate_directory(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {label}: {error}"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(format!("{label} must be a real non-link directory"));
    }
    Ok(())
}

/// Allows Inno to create its log while rejecting a pre-existing link or
/// directory at that path, so diagnostics cannot overwrite another object.
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
