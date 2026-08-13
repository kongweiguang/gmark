// @author kongweiguang

use super::*;

#[test]
/// Protects prerelease identifiers from being silently reduced to a stable version.
fn bundle_version_requires_target_version_and_preserves_prerelease() {
    assert!(bundle_version_matches(Some("1.2.3"), None, "1.2.3"));
    assert!(bundle_version_matches(
        Some("1.2.3"),
        Some("1.2.3-beta.1"),
        "1.2.3-beta.1"
    ));
    assert!(!bundle_version_matches(
        Some("1.2.3"),
        Some("1.2.3"),
        "1.2.3-beta.1"
    ));
}

#[test]
/// Keeps archive extraction tests focused on lexical escape rejection while
/// preserving the conventional `./bundle` tar layout.
fn archive_path_validation_rejects_parent_but_allows_dot_components() {
    assert!(validate_archive_path(Path::new("../gmark.app")).is_err());
    assert!(validate_archive_path(Path::new("./gmark.app")).is_ok());
    assert!(validate_archive_path(Path::new("/tmp/gmark.app")).is_err());
    assert!(validate_archive_path(Path::new("gmark.app/Contents/MacOS/gmark")).is_ok());
}

#[test]
/// Ensures the staging fallback cannot cross a filesystem boundary.
fn same_volume_staging_is_required_before_exchange() {
    let Ok(root) = tempfile::tempdir() else {
        panic!("temporary root unavailable");
    };
    let transaction = root.path().join("transaction");
    if let Err(error) = fs::create_dir(&transaction) {
        panic!("failed to create transaction directory: {error}");
    }
    assert!(same_volume(root.path(), &transaction).is_ok_and(|same| same));
}

#[test]
/// Exercises the actual macOS directory exchange and its ephemeral old side.
fn atomic_exchange_keeps_the_old_bundle_only_in_ephemeral_staging() {
    let Ok(root) = tempfile::tempdir() else {
        panic!("temporary root unavailable");
    };
    let Ok(staging) = Builder::new()
        .prefix(".gmark-update-test-")
        .tempdir_in(root.path())
    else {
        panic!("temporary staging unavailable");
    };
    let target = root.path().join("gmark.app");
    let staged = staging.path().join("gmark.app");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("failed to create target bundle: {error}");
    }
    if let Err(error) = fs::create_dir_all(&staged) {
        panic!("failed to create staged bundle: {error}");
    }
    if let Err(error) = fs::write(target.join("marker"), b"old") {
        panic!("failed to write old bundle marker: {error}");
    }
    if let Err(error) = fs::write(staged.join("marker"), b"new") {
        panic!("failed to write new bundle marker: {error}");
    }

    if let Err(error) = atomic_swap_paths(&staged, &target) {
        panic!("atomic exchange failed: {error}");
    }
    assert_eq!(
        fs::read(target.join("marker")).ok().as_deref(),
        Some(&b"new"[..])
    );
    assert_eq!(
        fs::read(staged.join("marker")).ok().as_deref(),
        Some(&b"old"[..])
    );
}

#[test]
/// Locks the elevated fallback to the same atomic primitive and no destructive command.
fn authorization_fallback_uses_atomic_exchange_without_a_persistent_copy_or_delete() {
    assert!(INSTALL_AUTHORIZATION_SCRIPT.contains("renameatx_np"));
    assert!(INSTALL_AUTHORIZATION_SCRIPT.contains("2);"));
    assert!(!INSTALL_AUTHORIZATION_SCRIPT.contains("backup"));
    assert!(!INSTALL_AUTHORIZATION_SCRIPT.contains("/bin/rm"));
}
