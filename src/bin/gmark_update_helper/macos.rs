// @author kongweiguang

//! macOS `.app.tar.gz` extraction and bundle replacement policy.

use std::{
    fs, io,
    path::{Component, Path},
    process::Command,
};

use flate2::read::GzDecoder;
use gmark_update_core::{ApplyPlanV2, StagedApplyArtifact};
use tempfile::Builder;

const MAX_ENTRIES: usize = 20_000;
const MAX_UNPACKED_BYTES: u64 = 512 * 1024 * 1024;
const INSTALL_AUTHORIZATION_SCRIPT: &str = r#"
on run argv
  set targetPath to quoted form of item 1 of argv
  set backupPath to quoted form of item 2 of argv
  set stagedPath to quoted form of item 3 of argv
  do shell script "/bin/mv " & targetPath & " " & backupPath & " && /bin/mv " & stagedPath & " " & targetPath with administrator privileges
end run
"#;
const ROLLBACK_AUTHORIZATION_SCRIPT: &str = r#"
on run argv
  set targetPath to quoted form of item 1 of argv
  set backupPath to quoted form of item 2 of argv
  do shell script "/bin/rm -rf " & targetPath & " && /bin/mv " & backupPath & " " & targetPath with administrator privileges
end run
"#;

pub fn install(plan: &ApplyPlanV2, artifact: &mut StagedApplyArtifact) -> Result<(), String> {
    let parent = plan
        .target_path
        .parent()
        .ok_or_else(|| "application bundle has no parent directory".to_owned())?;
    let transaction = plan
        .transaction_dir()
        .ok_or_else(|| "macOS update has no transaction directory".to_owned())?;
    validate_backup_location(plan, parent)?;
    let staging = match Builder::new().prefix(".gmark-update-").tempdir_in(parent) {
        Ok(staging) => staging,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Builder::new()
            .prefix(".gmark-update-")
            .tempdir_in(transaction)
            .map_err(|fallback| {
                format!(
                    "failed to create macOS staging directory in install root ({error}); transaction fallback failed: {fallback}"
                )
            })?,
        Err(error) => {
            return Err(format!("failed to create macOS staging directory: {error}"));
        }
    };
    artifact
        .rewind()
        .map_err(|error| format!("failed to rewind macOS updater archive: {error}"))?;
    extract_archive(artifact, staging.path())?;
    let staged_app = staging.path().join("gmark.app");
    validate_bundle(&staged_app, &plan.target_version)?;
    verify_codesign(&staged_app)?;

    ensure_directory_or_missing(&plan.backup_path, "update backup")?;
    if fs::symlink_metadata(&plan.backup_path).is_ok() {
        return Err("macOS update backup already exists; refusing to overwrite it".to_owned());
    }
    if let Err(error) = fs::rename(&plan.target_path, &plan.backup_path) {
        if error.kind() == io::ErrorKind::PermissionDenied {
            return install_with_authorization(&plan.target_path, &plan.backup_path, &staged_app);
        }
        return Err(format!("failed to back up current application: {error}"));
    }
    if let Err(error) = fs::rename(&staged_app, &plan.target_path) {
        return match fs::rename(&plan.backup_path, &plan.target_path) {
            Ok(()) => Err(format!("failed to install new application bundle: {error}")),
            Err(restore_error) => Err(format!(
                "failed to install new application bundle: {error}; failed to restore previous application: {restore_error}"
            )),
        };
    }
    Ok(())
}

pub fn rollback(plan: &ApplyPlanV2) -> Result<(), String> {
    let parent = plan
        .target_path
        .parent()
        .ok_or_else(|| "application bundle has no parent directory".to_owned())?;
    validate_backup_location(plan, parent)?;
    ensure_directory_or_missing(&plan.target_path, "update target")?;
    ensure_directory_or_missing(&plan.backup_path, "update backup")?;
    if fs::symlink_metadata(&plan.backup_path).is_err() {
        return Err("macOS update backup is missing".to_owned());
    }
    let direct = (|| {
        if fs::symlink_metadata(&plan.target_path).is_ok() {
            fs::remove_dir_all(&plan.target_path)
                .map_err(|error| format!("failed to remove failed application: {error}"))?;
        }
        fs::rename(&plan.backup_path, &plan.target_path)
            .map_err(|error| format!("failed to restore application backup: {error}"))
    })();
    match direct {
        Ok(()) => Ok(()),
        Err(error) => rollback_with_authorization(&plan.target_path, &plan.backup_path).map_err(
            |authorization| format!("{error}; authorization fallback failed: {authorization}"),
        ),
    }
}

fn extract_archive(artifact: &mut StagedApplyArtifact, destination: &Path) -> Result<(), String> {
    let decoder = GzDecoder::new(artifact.as_file_mut());
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| format!("failed to read macOS updater archive: {error}"))?;
    let mut count = 0usize;
    let mut bytes = 0u64;
    for item in entries {
        count = count.saturating_add(1);
        if count > MAX_ENTRIES {
            return Err("macOS updater archive contains too many entries".to_owned());
        }
        let mut entry = item.map_err(|error| format!("invalid updater archive entry: {error}"))?;
        let kind = entry.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() || !kind.is_file() && !kind.is_dir() {
            return Err("macOS updater archive contains a special file or link".to_owned());
        }
        let path = entry
            .path()
            .map_err(|error| format!("invalid updater archive path: {error}"))?
            .into_owned();
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(
                "macOS updater archive attempted to escape the staging directory".to_owned(),
            );
        }
        bytes = bytes.saturating_add(entry.header().size().unwrap_or(0));
        if bytes > MAX_UNPACKED_BYTES {
            return Err("macOS updater archive exceeds its unpacked size limit".to_owned());
        }
        entry
            .unpack_in(destination)
            .map_err(|error| format!("failed to unpack macOS updater archive: {error}"))?;
    }
    Ok(())
}

fn validate_bundle(bundle: &Path, target_version: &str) -> Result<(), String> {
    ensure_directory_or_missing(bundle, "staged application")?;
    let executable = bundle.join("Contents/MacOS/gmark");
    if !is_real_file(&executable) {
        return Err("macOS updater archive has no gmark application executable".to_owned());
    }
    let plist = bundle.join("Contents/Info.plist");
    let bytes =
        fs::read(&plist).map_err(|error| format!("failed to read staged Info.plist: {error}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let identifier = plist_value(&plist, "CFBundleIdentifier")
        .or_else(|| plist_text_value(&text, "CFBundleIdentifier"));
    if identifier.as_deref() != Some("com.kongweiguang.gmark") {
        return Err("staged application bundle has an unexpected CFBundleIdentifier".to_owned());
    }
    let short_version = plist_value(&plist, "CFBundleShortVersionString")
        .or_else(|| plist_text_value(&text, "CFBundleShortVersionString"));
    let bundle_version = plist_value(&plist, "CFBundleVersion")
        .or_else(|| plist_text_value(&text, "CFBundleVersion"));
    if !bundle_version_matches(
        short_version.as_deref(),
        bundle_version.as_deref(),
        target_version,
    ) {
        return Err(
            "staged application bundle version does not match the update target".to_owned(),
        );
    }
    Ok(())
}

fn bundle_version_matches(short: Option<&str>, bundle: Option<&str>, target: &str) -> bool {
    let Some(short) = short.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    if short == target {
        return true;
    }
    let (Ok(found), Ok(expected)) = (
        semver::Version::parse(short),
        semver::Version::parse(target),
    ) else {
        return false;
    };
    if found == expected {
        return true;
    }
    // Apple packaging commonly keeps a numeric CFBundleShortVersionString
    // while carrying a prerelease in CFBundleVersion.  Accept that explicit
    // contract, but never silently discard a prerelease marker.
    !expected.pre.is_empty()
        && found.major == expected.major
        && found.minor == expected.minor
        && found.patch == expected.patch
        && bundle
            .map(str::trim)
            .is_some_and(|value| value == target || value.ends_with(target))
}

fn plist_text_value(text: &str, key: &str) -> Option<String> {
    text.split(key)
        .nth(1)
        .and_then(|tail| tail.split("<string>").nth(1))
        .and_then(|tail| tail.split("</string>").next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn plist_value(path: &Path, key: &str) -> Option<String> {
    let output = Command::new("/usr/libexec/PlistBuddy")
        .arg("-c")
        .arg(format!("Print :{key}"))
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn verify_codesign(bundle: &Path) -> Result<(), String> {
    let status = Command::new("codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(bundle)
        .status()
        .map_err(|error| format!("failed to verify macOS code signature: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "macOS code-signature verification failed: {status}"
        ))
    }
}

fn install_with_authorization(target: &Path, backup: &Path, staged: &Path) -> Result<(), String> {
    ensure_directory_or_missing(backup, "update backup")?;
    if fs::symlink_metadata(backup).is_ok() {
        return Err("macOS update backup already exists; refusing to overwrite it".to_owned());
    }
    let status = Command::new("osascript")
        .args(["-e", INSTALL_AUTHORIZATION_SCRIPT])
        .arg(target)
        .arg(backup)
        .arg(staged)
        .status()
        .map_err(|error| format!("failed to request macOS update authorization: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        let detail = if fs::symlink_metadata(backup).is_ok() {
            format!("recoverable backup remains at {}", backup.display())
        } else {
            "the existing application was not moved".to_owned()
        };
        Err(format!(
            "macOS update authorization was denied or failed; {detail}"
        ))
    }
}

fn rollback_with_authorization(target: &Path, backup: &Path) -> Result<(), String> {
    let status = Command::new("osascript")
        .args(["-e", ROLLBACK_AUTHORIZATION_SCRIPT])
        .arg(target)
        .arg(backup)
        .status()
        .map_err(|error| format!("failed to request macOS rollback authorization: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "macOS rollback authorization was denied or failed; backup remains at {}",
            backup.display()
        ))
    }
}

fn ensure_directory_or_missing(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            Ok(())
        }
        Ok(_) => Err(format!(
            "{label} is not an expected real application directory"
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to inspect {label}: {error}")),
    }
}

fn validate_backup_location(plan: &ApplyPlanV2, parent: &Path) -> Result<(), String> {
    if plan.backup_path.parent() != Some(parent) {
        return Err("macOS update backup must be a sibling of the application bundle".to_owned());
    }
    let transaction = plan.transaction_id.hyphenated().to_string();
    if !plan
        .backup_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains(&transaction))
    {
        return Err("macOS update backup is not transaction-owned".to_owned());
    }
    Ok(())
}

fn is_real_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "../../../tests/unit/bin/gmark_update_helper/macos.rs"]
mod tests;
