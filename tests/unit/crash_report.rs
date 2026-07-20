// @author kongweiguang

use super::*;

#[test]
fn report_excludes_panic_text_user_paths_and_thread_names() {
    let temp = tempfile::tempdir().unwrap();
    let secret = r"C:\Users\alice\private\draft.md: unreleased paragraph";
    let path = write_report(
        temp.path(),
        ReportInput {
            payload_kind: "String",
            source_file: Some("editor.rs"),
            line: Some(42),
            column: Some(7),
            thread_class: "named-worker",
        },
    )
    .unwrap();
    let json = fs::read_to_string(path).unwrap();
    assert!(!json.contains(secret));
    assert!(!json.contains("alice"));
    assert!(!json.contains("draft.md"));
    assert!(json.contains("\"source_file\": \"editor.rs\""));
    assert!(json.contains("\"payload_kind\": \"String\""));
    assert!(json.len() <= MAX_REPORT_BYTES);
}

#[test]
fn reports_use_unique_atomic_names_and_leave_no_temporary_files() {
    let temp = tempfile::tempdir().unwrap();
    let input = ReportInput {
        payload_kind: "str",
        source_file: Some("main.rs"),
        line: Some(1),
        column: Some(1),
        thread_class: "main",
    };
    let first = write_report(temp.path(), input).unwrap();
    let second = write_report(temp.path(), input).unwrap();
    assert_ne!(first, second);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
    assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
        entry
            .unwrap()
            .path()
            .extension()
            .and_then(|value| value.to_str())
            == Some("json")
    }));
}

#[test]
fn folder_open_command_keeps_the_path_as_one_argument() {
    let temp = tempfile::tempdir().unwrap();
    let report_dir = temp.path().join("崩溃 reports");
    let command = folder_open_command(&report_dir);
    #[cfg(target_os = "windows")]
    assert_eq!(command.get_program(), "explorer.exe");
    #[cfg(target_os = "macos")]
    assert_eq!(command.get_program(), "open");
    #[cfg(all(unix, not(target_os = "macos")))]
    assert_eq!(command.get_program(), "xdg-open");
    let arguments = command.get_args().collect::<Vec<_>>();
    assert_eq!(arguments.last().copied(), Some(report_dir.as_os_str()));
    assert_eq!(
        arguments
            .iter()
            .filter(|argument| **argument == report_dir.as_os_str())
            .count(),
        1
    );
}

#[test]
fn report_directory_creation_handles_spaces_and_unicode() {
    let temp = tempfile::tempdir().unwrap();
    let report_dir = temp.path().join("崩溃 reports");
    ensure_reports_directory(&report_dir).unwrap();
    assert!(report_dir.is_dir());
}

#[test]
fn report_directory_creation_does_not_replace_a_conflicting_file() {
    let temp = tempfile::tempdir().unwrap();
    let report_dir = temp.path().join("crash-reports");
    fs::write(&report_dir, "sentinel").unwrap();
    let error = ensure_reports_directory(&report_dir).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("failed to create crash report directory")
    );
    assert_eq!(fs::read_to_string(report_dir).unwrap(), "sentinel");
}

#[test]
fn pruning_is_bounded_and_preserves_unowned_files() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("keep.txt"), "sentinel").unwrap();
    fs::write(temp.path().join(".crash-interrupted.tmp"), "partial").unwrap();
    for index in 0..25 {
        fs::write(
            temp.path().join(format!("crash-{index:03}.json")),
            format!("{index}"),
        )
        .unwrap();
    }

    prune_reports(temp.path(), REPORT_LIMIT).unwrap();

    let names = fs::read_dir(temp.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        names.iter().filter(|name| name.ends_with(".json")).count(),
        REPORT_LIMIT
    );
    assert!(names.iter().any(|name| name == "keep.txt"));
    assert!(!names.iter().any(|name| name.ends_with(".tmp")));
}

#[test]
fn panic_hook_subprocess_child() {
    let Some(report_dir) = std::env::var_os("GMARK_CRASH_TEST_DIR") else {
        return;
    };
    install_with_dir(PathBuf::from(report_dir)).unwrap();
    panic!("C:\\Users\\alice\\private\\draft.md unreleased paragraph");
}

#[test]
fn installed_hook_writes_a_redacted_report_before_process_failure() {
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "crash_report::tests::panic_hook_subprocess_child",
            "--nocapture",
        ])
        .env("GMARK_CRASH_TEST_DIR", temp.path())
        .output()
        .unwrap();
    assert!(!output.status.success());

    let reports = fs::read_dir(temp.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 1);
    let json = fs::read_to_string(&reports[0]).unwrap();
    assert!(!json.contains("alice"));
    assert!(!json.contains("draft.md"));
    assert!(!json.contains("unreleased paragraph"));
    assert!(json.contains("\"payload_kind\": \"str\""));
    assert!(json.contains("\"source_file\": \"crash_report.rs\""));
}
