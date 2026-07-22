// @author kongweiguang

use super::parts::catalog::I18nLanguagePack;
use super::{I18nManager, I18nStrings, language_id_for_locale_preferences};
use crate::config::GmarkConfigDirs;
use crate::theme::ThemeManager;

#[test]
fn built_in_chinese_strings_are_utf8() {
    let strings = I18nStrings::zh_cn();
    assert_eq!(strings.menu_file, "文件");
    assert_eq!(strings.menu_export, "导出");
    assert_eq!(strings.menu_language, "语言");
    assert_eq!(strings.new_document_untyped, "未指定类型");
    assert_eq!(
        I18nStrings::en_us().new_document_untyped,
        "Unspecified Type"
    );
    assert_eq!(strings.new_document_csv, "CSV 文档");
    assert_eq!(I18nStrings::en_us().new_document_csv, "CSV Document");
    assert_eq!(strings.save_failed_title, "保存失败");
    assert_eq!(strings.export_failed_title, "导出失败");
    assert_eq!(strings.view_mode_switch_to_source, "切换到源码");
    assert_eq!(strings.context_menu_insert, "插入");
    assert_eq!(strings.table_insert_title, "插入表格");
    assert_eq!(strings.image_loading_without_alt, "正在加载图片...");
    assert_eq!(strings.workspace_no_file_title, "未打开文件夹");
    assert_eq!(
        I18nStrings::en_us().workspace_no_file_title,
        "No Folder Open"
    );
    assert_eq!(
        strings.help_check_updates_message,
        "正在检查 gmark 的最新版本..."
    );
    assert_eq!(strings.update_open_release, "下载并安装");
    assert_eq!(strings.help_about_github_label, "GitHub");
    assert_eq!(
        strings.help_about_star_message,
        "如果本项目对您有帮助，那不妨给本项目一颗 Star⭐，十分感谢！"
    );
}

#[test]
fn large_document_strings_are_complete_and_language_specific() {
    let zh = I18nStrings::zh_cn();
    let en = I18nStrings::en_us();

    assert_eq!(
        zh.large_document.keys().collect::<Vec<_>>(),
        en.large_document.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        zh.large_document_text("recovered_structured_paused"),
        "恢复的编辑保存前，结构化视图已暂停"
    );
    assert_eq!(
        en.large_document_text("recovered_structured_paused"),
        "Structured view is paused until recovered edits are saved"
    );

    for (key, zh_value) in &zh.large_document {
        let en_value = &en.large_document[key];
        assert!(!zh_value.trim().is_empty(), "Chinese value is empty: {key}");
        assert!(!en_value.trim().is_empty(), "English value is empty: {key}");
        assert_ne!(
            zh_value, en_value,
            "language-specific value fell back: {key}"
        );
    }
}

#[test]
fn manager_switches_builtin_languages() {
    let mut manager = I18nManager::default();
    assert_eq!(manager.current_language_id(), "en-US");
    assert_eq!(manager.strings().menu_file, "File");
    assert_eq!(manager.strings().menu_export, "Export");

    assert!(manager.set_language_by_id("zh-CN"));
    assert_eq!(manager.current_language_id(), "zh-CN");
    assert_eq!(manager.strings().menu_file, "文件");
    assert_eq!(manager.strings().menu_export, "导出");
    assert!(!manager.set_language_by_id("zh-CN"));
    assert!(!manager.set_language_by_id("missing"));
}

#[test]
fn language_catalog_contains_chinese_and_english() {
    let manager = I18nManager::default();
    let ids = manager
        .available_languages()
        .iter()
        .map(|entry| (entry.id.as_str(), entry.name.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![("zh-CN", "简体中文"), ("en-US", "English")]);
}

#[test]
fn manager_can_be_constructed_with_known_language() {
    let manager = I18nManager::new_with_language_id("zh-CN");
    assert_eq!(manager.current_language_id(), "zh-CN");
    assert_eq!(manager.strings().menu_file, "文件");

    let fallback = I18nManager::new_with_language_id("missing");
    assert_eq!(fallback.current_language_id(), "en-US");
    assert_eq!(fallback.strings().menu_file, "File");
}

#[test]
fn theme_switch_does_not_modify_selected_language() {
    let mut theme_manager = ThemeManager::default();
    let mut i18n_manager = I18nManager::new_with_language_id("zh-CN");

    assert!(theme_manager.set_theme_by_id("gmark"));
    assert!(!i18n_manager.set_language_by_id("missing"));

    assert_eq!(theme_manager.current_theme_id(), "gmark");
    assert_eq!(i18n_manager.current_language_id(), "zh-CN");
    assert_eq!(i18n_manager.strings().menu_file, "文件");
}

#[test]
fn locale_preferences_map_to_builtin_languages() {
    assert_eq!(language_id_for_locale_preferences(["zh-CN"]), "zh-CN");
    assert_eq!(language_id_for_locale_preferences(["zh-HK"]), "zh-CN");
    assert_eq!(language_id_for_locale_preferences(["zh-Hant-TW"]), "zh-CN");
    assert_eq!(language_id_for_locale_preferences(["zh_SG.UTF-8"]), "zh-CN");
    assert_eq!(language_id_for_locale_preferences(["en-US"]), "en-US");
    assert_eq!(language_id_for_locale_preferences(["en_GB.UTF-8"]), "en-US");
    assert_eq!(
        language_id_for_locale_preferences(["fr-FR", "zh-CN"]),
        "zh-CN"
    );
    assert_eq!(
        language_id_for_locale_preferences(Vec::<&str>::new()),
        "en-US"
    );
    assert_eq!(language_id_for_locale_preferences(["fr-FR"]), "en-US");
    assert_eq!(language_id_for_locale_preferences(["!!!"]), "en-US");
}

#[test]
fn imports_jsonc_language_pack_and_persists_normalized_json() {
    let root = std::env::temp_dir().join(format!("gmark-i18n-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let source = root.join("language.jsonc");
    std::fs::write(
        &source,
        r#"{
                // Required metadata.
                "id": "ja-JP",
                "name": "日本語",
                "author": "",
                "strings": {
                    "menu_file": "ファイル",
                    "menu_export": ""
                }
            }"#,
    )
    .expect("language config should be written");

    let dirs = GmarkConfigDirs::from_root(&root);
    let mut manager = I18nManager::default();
    let imported_id = manager
        .import_language_config_with_dirs(&source, &dirs)
        .expect("language config should import");

    assert_eq!(imported_id, "ja-JP");
    assert_eq!(manager.current_language_id(), "ja-JP");
    assert_eq!(manager.strings().menu_file, "ファイル");
    assert_eq!(manager.strings().menu_export, "Export");
    assert!(
        manager
            .available_languages()
            .iter()
            .any(|entry| entry.id == "ja-JP" && entry.name == "日本語")
    );

    let normalized = std::fs::read_to_string(dirs.languages_dir().join("ja-JP.json"))
        .expect("normalized language config should exist");
    assert!(normalized.contains("\"menu_file\": \"ファイル\""));
    assert!(!normalized.contains("menu_export"));
    assert!(!normalized.contains("author"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn custom_language_cannot_override_builtin_language_id() {
    let root = std::env::temp_dir().join(format!("gmark-i18n-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let source = root.join("language.json");
    std::fs::write(
        &source,
        r#"{
                "id": "en-US",
                "name": "Override",
                "strings": { "menu_file": "Override" }
            }"#,
    )
    .expect("language config should be written");

    let dirs = GmarkConfigDirs::from_root(&root);
    let mut manager = I18nManager::default();
    let err = manager
        .import_language_config_with_dirs(&source, &dirs)
        .expect_err("built-in language ids should be rejected");
    assert!(err.to_string().contains("built-in language"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn language_pack_json_falls_back_for_missing_strings() {
    let pack = I18nLanguagePack::from_json(
        r#"{
                "id": "zh-CN",
                "name": "简体中文",
                "strings": {
                    "menu_file": "文件菜单",
                    "unsaved_changes_hint": "legacy hint",
                    "drop_replace_hint": "legacy hint",
                    "unknown_field": "ignored"
                }
            }"#,
    )
    .expect("language pack should load");

    assert_eq!(pack.id, "zh-CN");
    assert_eq!(pack.name, "简体中文");
    assert_eq!(pack.strings.menu_file, "文件菜单");
    assert_eq!(pack.strings.menu_export, "导出");
    assert_eq!(pack.strings.info_dialog_ok, "确定");
    assert_eq!(pack.strings.update_open_release, "下载并安装");
    assert_eq!(pack.strings.help_about_github_label, "GitHub");
    assert_eq!(pack.strings.slash_commands["table"], "表格");
    assert_eq!(
        pack.strings.help_about_star_message,
        "如果本项目对您有帮助，那不妨给本项目一颗 Star⭐，十分感谢！"
    );
}

#[test]
fn partial_slash_command_map_merges_with_language_defaults() {
    let pack = I18nLanguagePack::from_json(
        r#"{
                "id": "en-US",
                "strings": {
                    "slash_commands": { "table": "Grid" }
                }
            }"#,
    )
    .expect("language pack should load");

    assert_eq!(pack.strings.slash_commands["table"], "Grid");
    assert_eq!(pack.strings.slash_commands["heading_1"], "Heading 1");
    assert_eq!(
        pack.strings.slash_commands["no_results"],
        "No matching block type"
    );
}

#[test]
fn unknown_language_pack_falls_back_to_english_strings() {
    let pack = I18nLanguagePack::from_json(
        r#"{
                "id": "fr-FR",
                "strings": {
                    "menu_file": "Fichier"
                }
            }"#,
    )
    .expect("language pack should load");

    assert_eq!(pack.id, "fr-FR");
    assert_eq!(pack.name, "fr-FR");
    assert_eq!(pack.strings.menu_file, "Fichier");
    assert_eq!(pack.strings.menu_export, "Export");
    assert_eq!(pack.strings.info_dialog_ok, "OK");
    assert_eq!(pack.strings.update_open_release, "Download and Install");
    assert_eq!(pack.strings.menu_open_recent_file, "Open Recent File");
    assert_eq!(pack.strings.menu_no_recent_files, "No Recent Files");
    assert_eq!(
        pack.strings.recent_file_missing_title,
        "Recent File Missing"
    );
    assert_eq!(pack.strings.help_about_github_label, "GitHub");
    assert_eq!(
        pack.strings.help_about_star_message,
        "If this project helps you, consider giving it a Star⭐. Thank you!"
    );
}
