// @author kongweiguang

use gmark_source_tools::{
    HighlightEngine, SourceLanguage, TokenClass, highlight_fenced_code, highlight_source,
};

#[test]
fn unknown_fences_fall_back_to_plain_text_and_json_keeps_lexical_spans() {
    let plain = highlight_fenced_code(Some("not-a-language"), "anything");
    assert_eq!(plain.language, SourceLanguage::PlainText);
    assert_eq!(plain.engine, HighlightEngine::PlainTextFallback);
    assert!(plain.spans.is_empty());

    let source = "{\"name\":\"界\",\"ok\":true}";
    let json = highlight_source(SourceLanguage::Json, source);
    assert_eq!(json.engine, HighlightEngine::JsonFallback);
    assert!(json.spans.iter().any(|span| {
        span.class == TokenClass::Property && span.range.slice(source).ok() == Some("\"name\"")
    }));
    assert!(
        json.spans
            .iter()
            .any(|span| span.class == TokenClass::Constant)
    );
}

#[cfg(feature = "code-highlight-official")]
#[test]
fn official_grammar_bundle_produces_semantic_rust_spans() {
    let highlighted = highlight_fenced_code(Some("rs line-numbers"), "fn answer() -> u8 { 42 }");
    assert_eq!(highlighted.language, SourceLanguage::Rust);
    assert_eq!(highlighted.engine, HighlightEngine::TreeSitter);
    assert!(!highlighted.spans.is_empty());
}

#[cfg(feature = "code-highlight-config")]
#[test]
fn config_grammar_bundle_produces_yaml_spans() {
    let highlighted = highlight_source(SourceLanguage::Yaml, "name: gmark\nenabled: true\n");
    assert_eq!(highlighted.engine, HighlightEngine::TreeSitter);
    assert!(!highlighted.spans.is_empty());
}

#[cfg(all(
    feature = "code-highlight-core",
    not(feature = "code-highlight-official")
))]
#[test]
fn core_without_official_grammars_remains_a_safe_fallback() {
    let highlighted = highlight_source(SourceLanguage::Rust, "fn answer() {}");
    assert_eq!(highlighted.engine, HighlightEngine::PlainTextFallback);
    assert!(highlighted.spans.is_empty());
}
