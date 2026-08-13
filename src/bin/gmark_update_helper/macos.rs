// @author kongweiguang

//! macOS `.app.tar.gz` extraction and bundle replacement policy.

use std::{
    ffi::CString,
    fs::{self, File},
    io,
    os::{
        raw::{c_char, c_int, c_uint},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::{Component, Path, PathBuf},
    process::Command,
};

use flate2::read::GzDecoder;
use gmark_update_core::{ApplyPlanV2, StagedApplyArtifact};

use super::PlatformInstallFailure;
use tempfile::{Builder, TempDir};

const MAX_ENTRIES: usize = 20_000;
const MAX_UNPACKED_BYTES: u64 = 512 * 1024 * 1024;
const AT_FDCWD: c_int = -2;
const RENAME_SWAP: c_uint = 0x0002;

// Reason: renameatx_np provides atomic bundle exchange; remove when Rust std exposes an equivalent safe API.
#[allow(unsafe_code)]
unsafe extern "C" {
    fn renameatx_np(
        fromfd: c_int,
        from: *const c_char,
        tofd: c_int,
        to: *const c_char,
        flags: c_uint,
    ) -> c_int;
}

// The authorization fallback repeats the same atomic exchange while running
// with administrator privileges; it never moves the old bundle to a durable
// side path or performs a destructive delete before the exchange succeeds.
const INSTALL_AUTHORIZATION_SCRIPT: &str = r#"
on run argv
  set stagedPath to quoted form of item 1 of argv
  set targetPath to quoted form of item 2 of argv
  set jxa to "ObjC.import('Darwin'); function run(argv) { if (argv.length != 2) { throw new Error('invalid arguments'); } var result = $.renameatx_np(-2, argv[0], -2, argv[1], 2); if (result != 0) { throw new Error('renameatx_np failed: ' + result); } return result; }"
  set command to "/usr/bin/osascript -l JavaScript -e " & quoted form of jxa & " -- " & stagedPath & " " & targetPath
  do shell script command with administrator privileges
end run
"#;

/// Installs a verified bundle without creating a persistent recovery tree.
///
/// Keeping the old bundle as the exchange's temporary side lets every failure
/// before the commit leave the installed application untouched, while the
/// temporary directory removes the old side after a successful commit.
pub fn install(
    plan: &ApplyPlanV2,
    artifact: &mut StagedApplyArtifact,
) -> Result<(), PlatformInstallFailure> {
    let target = &plan.target_path;
    let parent = target
        .parent()
        .ok_or_else(|| "application bundle has no parent directory".to_owned())?;
    let transaction = plan
        .transaction_dir()
        .ok_or_else(|| "macOS update has no transaction directory".to_owned())?;
    validate_install_location(plan, parent)?;
    validate_path_components(transaction, "macOS update transaction")?;
    ensure_real_directory(transaction, "macOS update transaction")?;

    let staging = create_staging_directory(parent, transaction)?;
    if !same_volume(staging.path(), parent)? {
        return Err(
            "macOS update staging directory must be on the same volume as the installed application"
                .to_owned()
                .into(),
        );
    }
    artifact
        .rewind()
        .map_err(|error| format!("failed to rewind macOS updater archive: {error}"))?;
    extract_archive(artifact, staging.path())?;
    let staged_app = staging.path().join("gmark.app");
    validate_bundle(&staged_app, &plan.target_version)?;
    verify_codesign(&staged_app)?;
    sync_directory(staging.path())?;

    match atomic_swap_paths(&staged_app, target) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            install_with_authorization(target, &staged_app)
                .map_err(PlatformInstallFailure::committed_or_unknown)?;
        }
        Err(error) => {
            return Err(
                format!("failed to atomically exchange macOS application bundle: {error}").into(),
            );
        }
    }
    // A failed directory sync is reported after the exchange and therefore
    // cannot trigger a restoration of the already-installed new bundle.
    sync_directory(parent).map_err(|error| {
        PlatformInstallFailure::committed_or_unknown(format!(
            "macOS bundle exchange committed but could not be synced: {error}"
        ))
    })
}

/// Creates staging on the install volume, falling back only when authorization
/// is needed and the transaction directory is still on that same volume.
fn create_staging_directory(parent: &Path, transaction: &Path) -> Result<TempDir, String> {
    match Builder::new().prefix(".gmark-update-").tempdir_in(parent) {
        Ok(staging) => Ok(staging),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            if !same_volume(parent, transaction)? {
                return Err(format!(
                    "cannot prepare macOS update on the install volume ({error}); update transaction is on another volume"
                ));
            }
            Builder::new()
                .prefix(".gmark-update-")
                .tempdir_in(transaction)
                .map_err(|fallback| {
                    format!(
                        "failed to create macOS staging directory in the transaction ({error}); authorization fallback staging failed: {fallback}"
                    )
                })
        }
        Err(error) => Err(format!("failed to create macOS staging directory: {error}")),
    }
}

/// Validates that the helper can only exchange the exact installed Gmark
/// bundle, preventing a trusted plan from redirecting the commit elsewhere.
fn validate_install_location(plan: &ApplyPlanV2, parent: &Path) -> Result<(), String> {
    if plan.expected_install_root != plan.target_path {
        return Err("macOS expected install root must equal the application target".to_owned());
    }
    if plan.expected_install_root.parent() != Some(parent)
        || plan.target_path.file_name().and_then(|name| name.to_str()) != Some("gmark.app")
    {
        return Err("macOS update target is not the installed gmark.app location".to_owned());
    }
    validate_clean_absolute_path(&plan.target_path, "macOS update target")?;
    validate_path_components(&plan.target_path, "macOS update target")?;
    ensure_real_directory(parent, "macOS application parent")?;
    ensure_real_directory(&plan.target_path, "installed macOS application")
}

/// Extracts the archive under a private directory while rejecting links and
/// path components that could escape the transaction-owned staging root.
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
        if kind.is_symlink() || kind.is_hard_link() || (!kind.is_file() && !kind.is_dir()) {
            return Err("macOS updater archive contains a special file or link".to_owned());
        }
        let path = entry
            .path()
            .map_err(|error| format!("invalid updater archive path: {error}"))?
            .into_owned();
        validate_archive_path(&path)?;
        let entry_size = entry
            .header()
            .size()
            .map_err(|error| format!("invalid macOS updater archive entry size: {error}"))?;
        bytes = bytes.saturating_add(entry_size);
        if bytes > MAX_UNPACKED_BYTES {
            return Err("macOS updater archive exceeds its unpacked size limit".to_owned());
        }
        entry
            .unpack_in(destination)
            .map_err(|error| format!("failed to unpack macOS updater archive: {error}"))?;
    }
    Ok(())
}

/// Rejects non-normalized archive names so extraction never depends on tar's
/// interpretation of `.` or parent-directory components.
fn validate_archive_path(path: &Path) -> Result<(), String> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("macOS updater archive attempted to escape the staging directory".to_owned());
    }
    Ok(())
}

/// Validates the bundle identity and target version before any install path is
/// exchanged, so a signed but mispackaged archive cannot become executable.
fn validate_bundle(bundle: &Path, target_version: &str) -> Result<(), String> {
    validate_clean_absolute_path(bundle, "staged macOS application")?;
    validate_path_components(bundle, "staged macOS application")?;
    ensure_real_directory(bundle, "staged application")?;
    let executable = bundle.join("Contents/MacOS/gmark");
    if !is_real_file(&executable) {
        return Err("macOS updater archive has no gmark application executable".to_owned());
    }
    let plist = bundle.join("Contents/Info.plist");
    if !is_real_file(&plist) {
        return Err("macOS updater archive has no real Info.plist".to_owned());
    }
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

/// Accepts Apple's short-version/build-number split without dropping a
/// prerelease marker that is part of the signed update version.
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
    !expected.pre.is_empty()
        && found.major == expected.major
        && found.minor == expected.minor
        && found.patch == expected.patch
        && bundle
            .map(str::trim)
            .is_some_and(|value| value == target || value.ends_with(target))
}

/// Uses PlistBuddy when available and lets the XML fallback keep test and
/// recovery environments useful when that macOS utility is unavailable.
fn plist_text_value(text: &str, key: &str) -> Option<String> {
    text.split(key)
        .nth(1)
        .and_then(|tail| tail.split("<string>").nth(1))
        .and_then(|tail| tail.split("</string>").next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Reads one fixed plist key without allowing a malformed plist to abort the
/// helper process or bypass the later identity and version checks.
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

/// Requires the platform verifier to accept the complete bundle before the
/// exchange, keeping signature failures entirely pre-commit.
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

/// Requests administrator privileges only for the same atomic swap primitive;
/// callers conservatively treat bridge failures as commit-unknown because the
/// privileged exchange may have completed before AppleScript reported them.
fn install_with_authorization(target: &Path, staged: &Path) -> Result<(), String> {
    ensure_real_directory(staged, "staged application")?;
    ensure_real_directory(target, "installed macOS application")?;
    let status = Command::new("osascript")
        .args(["-e", INSTALL_AUTHORIZATION_SCRIPT])
        .arg(staged)
        .arg(target)
        .status()
        .map_err(|error| format!("failed to request macOS update authorization: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("macOS update authorization was denied or atomic exchange failed; the existing application remains installed".to_owned())
    }
}

/// Calls macOS's directory-aware atomic exchange while retaining the old
/// bundle at `staged` until the temporary directory is dropped.
// Reason: this FFI call is the commit boundary; remove when a vetted safe atomic directory exchange is available.
#[allow(unsafe_code)]
fn atomic_swap_paths(staged: &Path, target: &Path) -> io::Result<()> {
    let staged = c_path(staged)?;
    let target = c_path(target)?;
    let result = unsafe {
        renameatx_np(
            AT_FDCWD,
            staged.as_ptr(),
            AT_FDCWD,
            target.as_ptr(),
            RENAME_SWAP,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Converts a platform path to the exact byte representation expected by the
/// libc entry point, rejecting the impossible NUL-containing case explicitly.
fn c_path(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "macOS update path contains an embedded NUL byte",
        )
    })
}

/// Confirms the transaction and install paths share a device before relying on
/// renameatx_np, whose cross-volume failure would otherwise occur at commit.
fn same_volume(left: &Path, right: &Path) -> Result<bool, String> {
    let left_device = fs::metadata(left)
        .map_err(|error| format!("failed to inspect macOS staging volume: {error}"))?
        .dev();
    let right_device = fs::metadata(right)
        .map_err(|error| format!("failed to inspect macOS install volume: {error}"))?
        .dev();
    Ok(left_device == right_device)
}

/// Rejects relative and dot-containing paths at the platform boundary even
/// though the V2 protocol validator already applies the same lexical policy.
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

/// Checks every existing ancestor so an attacker cannot redirect a safe leaf
/// through a symlink between validation and the atomic exchange.
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

/// Requires a real directory at every mutation boundary so an update cannot
/// turn a symlink or regular file into an installation root.
fn ensure_real_directory(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {label}: {error}"))?;
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(format!("{label} must be a real non-link directory"))
    }
}

/// Flushes a directory entry after extraction or commit so a crash cannot
/// publish a name whose contents were not made durable first.
fn sync_directory(path: &Path) -> Result<(), String> {
    let directory = File::open(path)
        .map_err(|error| format!("failed to open macOS update directory for sync: {error}"))?;
    directory
        .sync_all()
        .map_err(|error| format!("failed to sync macOS update directory: {error}"))
}

/// Distinguishes a regular executable or plist from a symlink before it is
/// accepted as part of a signed application bundle.
fn is_real_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "../../../tests/unit/bin/gmark_update_helper/macos.rs"]
mod tests;
