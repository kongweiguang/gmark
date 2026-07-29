// @author kongweiguang

use std::path::Path;

use gmark_source_tools::{
    ByteRange, ByteRangeError, SourceLanguage, detect_language, resolve_fence_language,
};

#[test]
fn language_aliases_and_extensions_keep_legacy_compatibility() {
    assert_eq!(SourceLanguage::from_alias("RS"), Some(SourceLanguage::Rust));
    assert_eq!(
        SourceLanguage::from_alias("shell"),
        Some(SourceLanguage::Bash)
    );
    assert_eq!(
        SourceLanguage::from_alias("c#"),
        Some(SourceLanguage::CSharp)
    );
    assert_eq!(
        SourceLanguage::from_alias("golang"),
        Some(SourceLanguage::Go)
    );
    assert_eq!(SourceLanguage::from_alias("jsonl"), None);
    assert_eq!(
        resolve_fence_language(Some("typescript title=example")),
        Some(SourceLanguage::TypeScript)
    );
    assert_eq!(
        detect_language(Path::new("Cargo.lock")),
        SourceLanguage::Toml
    );
    assert_eq!(
        detect_language(Path::new("component.TSX")),
        SourceLanguage::TypeScriptTsx
    );
    assert_eq!(
        detect_language(Path::new("shape.geojson")),
        SourceLanguage::Json
    );
    assert_eq!(
        detect_language(Path::new("events.ndjson")),
        SourceLanguage::JsonLines
    );
    assert_eq!(detect_language(Path::new("header.h")), SourceLanguage::C);
    assert_eq!(
        detect_language(Path::new("header.hpp")),
        SourceLanguage::Cpp
    );
    assert_eq!(
        detect_language(Path::new("unknown.data")),
        SourceLanguage::PlainText
    );
}

#[test]
fn byte_ranges_validate_unicode_boundaries_without_panicking() {
    let source = "A界B";
    let range = ByteRange::from_source_offsets(source, 1, 4);
    assert_eq!(range.and_then(|range| range.slice(source)), Ok("界"));
    assert!(matches!(
        ByteRange::from_source_offsets(source, 2, 4),
        Err(ByteRangeError::InvalidUtf8Boundary { offset: 2 })
    ));
    assert!(matches!(
        ByteRange::new(5, 4),
        Err(ByteRangeError::Reversed { start: 5, end: 4 })
    ));
    assert!(matches!(
        ByteRange::new(0, 99).and_then(|range| range.validate_for(source)),
        Err(ByteRangeError::OutsideSource { .. })
    ));
}
