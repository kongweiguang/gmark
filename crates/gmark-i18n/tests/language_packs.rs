// @author kongweiguang

use std::fs;
use std::io::Write;

use gmark_i18n::{
    CustomLanguagePack, I18nCatalog, I18nError, LanguagePack, MAX_LANGUAGE_PACK_BYTES,
};
use tempfile::{Builder, tempdir};

#[test]
fn jsonc_custom_pack_is_normalized_installed_and_uses_english_fallback() {
    let custom = CustomLanguagePack::from_jsonc(
        r#"{
            // Required metadata.
            "id": "ja-JP",
            "name": " 日本語 ",
            "author": "",
            "strings": {
                "menu_file": "ファイル",
                "menu_export": "",
                "unknown_field": "discarded"
            }
        }"#,
    )
    .expect("test JSONC pack should parse");
    assert_eq!(custom.pack().id, "ja-JP");
    assert_eq!(custom.pack().name, "日本語");
    assert_eq!(custom.pack().strings.get("menu_file"), Some("ファイル"));
    assert_eq!(custom.pack().strings.get("menu_export"), Some("Export"));
    assert_eq!(custom.pack().strings.get("unknown_field"), None);

    let normalized = custom
        .normalized_json_pretty()
        .expect("normalization should serialize");
    assert!(normalized.contains("\"menu_file\": \"ファイル\""));
    assert!(!normalized.contains("menu_export"));
    assert!(!normalized.contains("unknown_field"));
    assert!(!normalized.contains("author"));

    let mut catalog = I18nCatalog::default();
    catalog
        .upsert_custom_language(custom.into_pack())
        .expect("custom pack should install");
    assert!(catalog.set_language_by_id("ja-JP"));
    assert_eq!(catalog.strings().get("menu_file"), Some("ファイル"));
}

#[test]
fn imported_pack_writes_a_safe_normalized_file() {
    let root = tempdir().expect("temporary directory should be created");
    let source = root.path().join("source.jsonc");
    fs::write(
        &source,
        r#"{
            "id": "pt-BR",
            "name": "Português",
            "strings": { "menu_file": "Arquivo" }
        }"#,
    )
    .expect("test source should be written");

    let mut catalog = I18nCatalog::default();
    let imported = catalog
        .import_language_file(&source, root.path().join("languages"))
        .expect("test pack should import");
    assert_eq!(imported.id, "pt-BR");
    assert_eq!(
        imported.path.file_name().and_then(|name| name.to_str()),
        Some("pt-BR.json")
    );
    assert_eq!(catalog.current_language_id(), "pt-BR");
    assert_eq!(catalog.strings().get("menu_file"), Some("Arquivo"));
    assert!(imported.path.is_file());
}

#[test]
fn invalid_builtin_ids_corrupt_inputs_and_oversized_files_are_classified() {
    let builtin = CustomLanguagePack::from_json(
        r#"{ "id": "en-US", "name": "Override", "strings": { "menu_file": "Override" } }"#,
    )
    .expect_err("built-in id should be rejected");
    assert!(matches!(builtin, I18nError::BuiltinLanguageOverride { .. }));

    let unsafe_id = CustomLanguagePack::from_json(r#"{ "id": "../../escape", "name": "Unsafe" }"#)
        .expect_err("path-like id should be rejected");
    assert!(matches!(unsafe_id, I18nError::InvalidLanguageId { .. }));

    let corrupted = LanguagePack::from_json("{ this is not JSON }")
        .expect_err("corrupt JSON should be rejected");
    assert!(matches!(corrupted, I18nError::InvalidJson { .. }));

    let mut file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temporary pack file should be created");
    file.write_all(&vec![b' '; MAX_LANGUAGE_PACK_BYTES + 1])
        .expect("oversized test input should be written");
    let oversized = LanguagePack::from_file(file.path()).expect_err("oversized file should fail");
    assert!(matches!(oversized, I18nError::FileTooLarge { .. }));
}

#[test]
fn directory_loading_keeps_valid_packs_when_neighbors_are_bad() {
    let root = tempdir().expect("temporary directory should be created");
    let language_dir = root.path().join("languages");
    fs::create_dir_all(&language_dir).expect("language directory should be created");
    fs::write(
        language_dir.join("valid.json"),
        r#"{ "id": "ko-KR", "name": "한국어", "strings": { "menu_file": "파일" } }"#,
    )
    .expect("valid pack should be written");
    fs::write(language_dir.join("broken.json"), "{").expect("broken pack should be written");

    let mut catalog = I18nCatalog::default();
    let report = catalog
        .load_language_directory(&language_dir)
        .expect("directory scan should finish");
    assert_eq!(report.loaded.len(), 1);
    assert_eq!(report.loaded[0].id, "ko-KR");
    assert_eq!(report.rejected.len(), 1);
    assert!(catalog.set_language_by_id("ko-KR"));
    assert_eq!(catalog.strings().get("menu_file"), Some("파일"));
}

#[test]
fn reimporting_the_active_custom_language_refreshes_its_bundle() {
    let mut catalog = I18nCatalog::default();
    let initial = CustomLanguagePack::from_json(
        r#"{ "id": "ja-JP", "name": "Japanese", "strings": { "menu_file": "File A" } }"#,
    )
    .expect("initial custom pack should parse");
    catalog
        .upsert_custom_language(initial.into_pack())
        .expect("initial custom pack should install");
    assert!(catalog.set_language_by_id("ja-JP"));

    let replacement = CustomLanguagePack::from_json(
        r#"{ "id": "ja-JP", "name": "Japanese", "strings": { "menu_file": "File B" } }"#,
    )
    .expect("replacement custom pack should parse");
    catalog
        .upsert_custom_language(replacement.into_pack())
        .expect("replacement custom pack should install");

    assert!(!catalog.set_language_by_id("ja-JP"));
    assert_eq!(catalog.strings().get("menu_file"), Some("File B"));
}
