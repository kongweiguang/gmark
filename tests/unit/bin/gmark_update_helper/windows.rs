// @author kongweiguang

use super::*;

/// Locks the exact Inno flags so a later cleanup cannot hide progress or
/// re-enable prompts while the parent process is already exiting.
#[test]
fn installer_arguments_are_exact_and_do_not_suppress_messages() {
    let args = installer_args(Path::new(r"C:\tx\installer.log"));
    assert_eq!(
        args,
        vec![
            "/SILENT",
            "/NOCANCEL",
            "/NORESTART",
            "/NOCLOSEAPPLICATIONS",
            "/SP-",
            r"/LOG=C:\tx\installer.log",
        ]
    );
    assert!(!args.iter().any(|arg| arg == "/SUPPRESSMSGBOXES"));
}

/// Guards the reduced Windows handoff against reintroducing helper-owned
/// directory or registry snapshots that the installer now handles itself.
#[test]
fn windows_install_does_not_snapshot_install_directory_or_registry() {
    // Keep the no-rollback contract explicit so a future refactor cannot
    // reintroduce side effects that the platform installer already owns.
    let source = include_str!("../../../../src/bin/gmark_update_helper/windows.rs");
    for forbidden in [
        "backup_path",
        "registry-uninstall.reg",
        "registry-open-with.reg",
        "export_registry",
        "restore_registry",
        "fs::rename",
    ] {
        assert!(
            !source.contains(forbidden),
            "unexpected snapshot code: {forbidden}"
        );
    }
}

/// Keeps the fixed uninstall key available for post-install path validation.
#[test]
fn windows_install_keeps_the_fixed_uninstall_key_for_location_validation() {
    assert!(!UNINSTALL_KEY.contains("\\\\"));
}

/// Covers registry output with spaces because the default per-user Inno path
/// contains `Program Files` on production installations.
#[test]
fn install_location_matching_accepts_paths_with_spaces() {
    assert!(install_location_matches(
        "InstallLocation    REG_SZ    C:\\Program Files\\GMark\\",
        Path::new(r"C:\Program Files\GMark")
    ));
}

/// 锁定安装后版本探测为精确匹配，防止旧版输出中偶然包含目标版本号而通过门禁。
#[test]
fn installed_version_output_requires_the_exact_target() {
    assert!(validate_installed_version_output("Gmark 0.2.1\r\n", "0.2.1").is_ok());
    let error = validate_installed_version_output("Gmark 0.2.0 (target 0.2.1)", "0.2.1")
        .expect_err("mixed version output must fail");
    assert!(error.contains("version mismatch"));
}
