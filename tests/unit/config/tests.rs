// @author kongweiguang

use super::{
    GmarkConfigDirs, RECENT_FILES_LIMIT, load_or_create_installation_id_with_dirs,
    parse_jsonc_value, prune_empty_json_values, read_recent_files_with_dirs,
    record_recent_file_with_dirs, remove_recent_file_with_dirs, sanitize_config_file_stem,
    strip_jsonc_comments,
};

#[test]
fn ui_check_override_isolates_every_config_artifact() {
    let root = std::env::temp_dir().join(format!("gmark-ui-check-config-{}", uuid::Uuid::new_v4()));

    let dirs = GmarkConfigDirs::from_system_override(Some(root.clone())).unwrap();

    assert_eq!(dirs.app_config_file(), root.join("config.toml"));
    assert_eq!(dirs.instance_lock_file(), root.join("instance.lock"));
    assert_eq!(dirs.recovery_dir(), root.join("recovery"));
    assert_eq!(
        dirs.workspace_session_file(),
        root.join("workspace-session.json")
    );
}
use serde_json::json;
use std::path::{Path, PathBuf};

#[test]
fn jsonc_comments_are_stripped_without_touching_strings() {
    let text = r#"
        {
            // line comment
            "url": "https://example.com/a//b",
            "text": "/* not a comment */",
            /* block comment */
            "value": 1
        }
        "#;

    let parsed = parse_jsonc_value(text).expect("jsonc should parse");
    assert_eq!(parsed["url"], "https://example.com/a//b");
    assert_eq!(parsed["text"], "/* not a comment */");
    assert_eq!(parsed["value"], 1);
    assert!(strip_jsonc_comments(text).is_ok());
}

#[test]
fn empty_values_are_pruned_recursively() {
    let mut value = json!({
        "name": "",
        "colors": {
            "text_default": null,
            "selection": "#fff"
        },
        "items": ["", null]
    });

    assert!(!prune_empty_json_values(&mut value));
    assert_eq!(value, json!({ "colors": { "selection": "#fff" } }));
}

#[test]
fn config_file_stems_are_sanitized() {
    assert_eq!(
        sanitize_config_file_stem("My Theme / Blue"),
        "My_Theme_Blue"
    );
    assert_eq!(sanitize_config_file_stem("  ...  "), "custom");
}

#[test]
fn missing_recent_history_file_returns_empty_list() {
    let root = std::env::temp_dir().join(format!("gmark-config-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);

    assert!(read_recent_files_with_dirs(&dirs).unwrap().is_empty());
    assert!(!dirs.history_file().exists());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn empty_recent_history_write_does_not_create_file() {
    let root = std::env::temp_dir().join(format!("gmark-config-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);

    super::write_recent_files_with_dirs(&[], &dirs).unwrap();

    assert!(!dirs.history_file().exists());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn blank_recent_file_path_is_rejected() {
    let root = std::env::temp_dir().join(format!("gmark-config-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);

    assert!(record_recent_file_with_dirs(Path::new("   "), &dirs).is_err());
    assert!(!dirs.history_file().exists());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn recent_history_filters_empty_lines_and_deduplicates() {
    let root = std::env::temp_dir().join(format!("gmark-config-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        dirs.history_file(),
        "  \nC:\\one.md\n\nC:\\two.md\nC:\\one.md\n",
    )
    .unwrap();

    let paths = read_recent_files_with_dirs(&dirs).unwrap();
    assert_eq!(
        paths,
        vec![PathBuf::from("C:\\one.md"), PathBuf::from("C:\\two.md")]
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn recent_history_filters_legacy_gmark_temp_fixture_paths() {
    let root = std::env::temp_dir().join(format!("gmark-config-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);
    let fixture_path = std::env::temp_dir().join(format!(
        "gmark-drop-save-replace-{}-123.md",
        std::process::id()
    ));
    let real_path = PathBuf::from("C:\\notes\\real.md");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        dirs.history_file(),
        format!("{}\n{}\n", fixture_path.display(), real_path.display()),
    )
    .unwrap();

    let paths = read_recent_files_with_dirs(&dirs).unwrap();
    assert_eq!(paths, vec![real_path]);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn recording_gmark_temp_fixture_path_is_noop() {
    let root = std::env::temp_dir().join(format!("gmark-config-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);
    let fixture_path = std::env::temp_dir().join(format!(
        "gmark-drop-dirty-discard-{}-123.md",
        std::process::id()
    ));

    assert!(
        record_recent_file_with_dirs(&fixture_path, &dirs)
            .unwrap()
            .is_empty()
    );
    assert!(!dirs.history_file().exists());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ordinary_temp_markdown_file_can_still_be_recorded() {
    let root = std::env::temp_dir().join(format!("gmark-config-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);
    let path = std::env::temp_dir().join(format!("manual-note-{}.md", std::process::id()));

    let paths = record_recent_file_with_dirs(&path, &dirs).unwrap();

    assert_eq!(paths, vec![path]);
    assert!(dirs.history_file().exists());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn recording_recent_file_moves_it_to_front_and_truncates() {
    let root = std::env::temp_dir().join(format!("gmark-config-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);

    for index in 0..(RECENT_FILES_LIMIT + 2) {
        record_recent_file_with_dirs(&PathBuf::from(format!("file-{index}.md")), &dirs).unwrap();
    }
    record_recent_file_with_dirs(&PathBuf::from("file-3.md"), &dirs).unwrap();

    let paths = read_recent_files_with_dirs(&dirs).unwrap();
    assert_eq!(paths.len(), RECENT_FILES_LIMIT);
    assert_eq!(paths[0], PathBuf::from("file-3.md"));
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.as_path() == Path::new("file-3.md"))
            .count(),
        1
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn removing_recent_file_persists_history_without_it() {
    let root = std::env::temp_dir().join(format!("gmark-config-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);
    record_recent_file_with_dirs(&PathBuf::from("one.md"), &dirs).unwrap();
    record_recent_file_with_dirs(&PathBuf::from("two.md"), &dirs).unwrap();

    let paths = remove_recent_file_with_dirs(&PathBuf::from("one.md"), &dirs).unwrap();

    assert_eq!(paths, vec![PathBuf::from("two.md")]);
    assert_eq!(
        read_recent_files_with_dirs(&dirs).unwrap(),
        vec![PathBuf::from("two.md")]
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn removing_last_recent_file_deletes_history_file() {
    let root = std::env::temp_dir().join(format!("gmark-config-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);
    let path = PathBuf::from("only.md");
    record_recent_file_with_dirs(&path, &dirs).unwrap();
    assert!(dirs.history_file().exists());

    let paths = remove_recent_file_with_dirs(&path, &dirs).unwrap();

    assert!(paths.is_empty());
    assert!(!dirs.history_file().exists());
    assert!(read_recent_files_with_dirs(&dirs).unwrap().is_empty());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn installation_id_is_created_once_and_remains_stable() {
    let root = std::env::temp_dir().join(format!("gmark-config-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);

    let first = load_or_create_installation_id_with_dirs(&dirs).unwrap();
    let second = load_or_create_installation_id_with_dirs(&dirs).unwrap();

    assert_eq!(first, second);
    assert_eq!(
        std::fs::read_to_string(dirs.installation_id_file())
            .unwrap()
            .trim(),
        first.to_string()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn invalid_installation_id_is_rejected_without_replacing_cohort() {
    let root = std::env::temp_dir().join(format!("gmark-config-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(dirs.installation_id_file(), "not-a-uuid\n").unwrap();

    assert!(load_or_create_installation_id_with_dirs(&dirs).is_err());
    assert_eq!(
        std::fs::read_to_string(dirs.installation_id_file()).unwrap(),
        "not-a-uuid\n"
    );
    let _ = std::fs::remove_dir_all(root);
}
