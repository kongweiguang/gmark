// @author kongweiguang

use gmark_i18n::{
    BUILTIN_LANGUAGE_EN_US_ID, BUILTIN_LANGUAGE_ZH_CN_ID, DEFAULT_LANGUAGE_ID, I18nCatalog,
    LanguageSelection, language_id_for_locale_preferences, normalize_locale,
};
use std::sync::Arc;

#[test]
fn builtins_preserve_the_complete_ui_key_set() {
    let english = I18nCatalog::new_with_language_id(BUILTIN_LANGUAGE_EN_US_ID).strings_clone();
    let chinese = I18nCatalog::new_with_language_id(BUILTIN_LANGUAGE_ZH_CN_ID).strings_clone();

    assert_eq!(english.scalars().len(), 405);
    assert_eq!(
        english.scalars().keys().collect::<Vec<_>>(),
        chinese.scalars().keys().collect::<Vec<_>>()
    );
    assert_eq!(
        english.groups().keys().collect::<Vec<_>>(),
        chinese.groups().keys().collect::<Vec<_>>()
    );
    assert_eq!(english.groups()["slash_commands"].len(), 71);
    assert_eq!(english.groups()["large_document"].len(), 112);
    assert_eq!(
        english.groups()["slash_commands"]
            .keys()
            .collect::<Vec<_>>(),
        chinese.groups()["slash_commands"]
            .keys()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        english.groups()["large_document"]
            .keys()
            .collect::<Vec<_>>(),
        chinese.groups()["large_document"]
            .keys()
            .collect::<Vec<_>>()
    );

    assert_eq!(english.get("menu_file"), Some("File"));
    assert_eq!(chinese.get("menu_file"), Some("文件"));
    assert_eq!(english.get("new_document_csv"), Some("CSV Document"));
    assert_eq!(chinese.get("new_document_csv"), Some("CSV 文档"));
    assert_eq!(
        english.get("large_document.recovered_structured_paused"),
        Some("Structured view is paused until recovered edits are saved")
    );
    assert_eq!(
        chinese.get("large_document.recovered_structured_paused"),
        Some("恢复的编辑保存前，结构化视图已暂停")
    );
    assert_eq!(english.get("slash_commands.table"), Some("Table"));
    assert_eq!(chinese.get("slash_commands.table"), Some("表格"));

    for value in english.scalars().values().chain(chinese.scalars().values()) {
        assert!(!value.trim().is_empty());
    }
}

#[test]
fn builtin_catalog_order_and_selection_match_legacy_behavior() {
    let mut catalog = I18nCatalog::default();
    assert_eq!(catalog.current_language_id(), DEFAULT_LANGUAGE_ID);
    assert_eq!(catalog.strings().get("menu_export"), Some("Export"));
    assert!(Arc::ptr_eq(&catalog.strings_arc(), &catalog.strings_arc()));
    assert_eq!(
        catalog
            .available_languages()
            .iter()
            .map(|entry| (entry.id.as_str(), entry.name.as_str()))
            .collect::<Vec<_>>(),
        vec![("zh-CN", "简体中文"), ("en-US", "English")]
    );

    assert_eq!(catalog.select_language("zh-CN"), LanguageSelection::Changed);
    assert_eq!(catalog.current_language_id(), "zh-CN");
    assert_eq!(catalog.strings().get("menu_export"), Some("导出"));
    assert_eq!(
        catalog.select_language("zh-CN"),
        LanguageSelection::Unchanged
    );
    assert_eq!(
        catalog.select_language("missing"),
        LanguageSelection::NotFound
    );
    assert!(!catalog.set_language_by_id("missing"));

    let fallback = I18nCatalog::new_with_language_id("missing");
    assert_eq!(fallback.current_language_id(), "en-US");
}

#[test]
fn locale_aliases_select_the_same_builtin_languages() {
    assert_eq!(normalize_locale(" zh_SG.UTF-8 "), Some("zh-SG".to_owned()));
    assert_eq!(
        normalize_locale("en_GB.UTF-8@calendar=gregorian"),
        Some("en-GB".to_owned())
    );
    assert_eq!(normalize_locale("!!!"), None);
    assert_eq!(normalize_locale(""), None);

    assert_eq!(language_id_for_locale_preferences(["zh-CN"]), "zh-CN");
    assert_eq!(language_id_for_locale_preferences(["zh-Hant-TW"]), "zh-CN");
    assert_eq!(language_id_for_locale_preferences(["zh_SG.UTF-8"]), "zh-CN");
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
}
