// @author kongweiguang

use super::*;

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

#[test]
fn registry_keys_use_single_backslash_and_empty_marker_deletes_key() {
    assert!(!UNINSTALL_KEY.contains("\\\\"));
    assert!(!OPEN_WITH_KEY.contains("\\\\"));
    assert_eq!(
        registry_delete_args(UNINSTALL_KEY),
        ["delete", UNINSTALL_KEY, "/f"]
    );
}

#[test]
fn install_location_matching_accepts_paths_with_spaces() {
    assert!(install_location_matches(
        "InstallLocation    REG_SZ    C:\\Program Files\\GMark\\",
        Path::new(r"C:\Program Files\GMark")
    ));
}
