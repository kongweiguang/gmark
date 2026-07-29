// @author kongweiguang

use gmark_i18n::{I18nCatalog, LanguagePack, interpolate};

#[test]
fn translation_lookup_interpolates_and_keeps_missing_keys_visible() {
    let catalog = I18nCatalog::default();
    let strings = catalog.strings();

    assert_eq!(
        strings.format_text(
            "update_available_message_template",
            &[("current", "0.1.0"), ("{latest}", "0.2.0")],
        ),
        "Current version: 0.1.0\nLatest version: 0.2.0\nOpen GitHub Releases to download it?"
    );
    assert_eq!(
        strings.translate("large_document.not_a_real_key"),
        "large_document.not_a_real_key"
    );
    assert_eq!(
        strings.translate_group("large_document", "not_a_real_key"),
        "not_a_real_key"
    );
    assert_eq!(
        interpolate(
            "{first} then {second} then {missing}",
            &[("first", "one"), ("{second}", "two")]
        ),
        "one then two then {missing}"
    );
}

#[test]
fn partial_nested_maps_merge_over_the_selected_builtin_fallback() {
    let english = LanguagePack::from_json(
        r#"{
            "id": "en-US",
            "strings": { "slash_commands": { "table": "Grid" } }
        }"#,
    )
    .expect("test language pack should parse");
    assert_eq!(english.strings.get("slash_commands.table"), Some("Grid"));
    assert_eq!(
        english.strings.get("slash_commands.heading_1"),
        Some("Heading 1")
    );

    let chinese = LanguagePack::from_json(
        r#"{
            "id": "zh-CN",
            "strings": { "large_document": { "no_results": "没有结果（外部）" } }
        }"#,
    )
    .expect("test language pack should parse");
    assert_eq!(
        chinese.strings.get("large_document.no_results"),
        Some("没有结果（外部）")
    );
    assert_eq!(chinese.strings.get("large_document.byte"), Some("字节"));
}
