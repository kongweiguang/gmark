// @author kongweiguang

use super::*;

#[test]
fn reports_utf8_byte_ranges_and_replacements() {
    let text = "中文 sentnce";
    let diagnostics = check_spelling(text);
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| &text[diagnostic.range.clone()] == "sentnce")
        .expect("misspelling should be reported");
    assert!(diagnostic.range.start >= "中文 ".len());
    assert!(
        diagnostic
            .replacements
            .iter()
            .any(|value| value == "sentence")
    );
}

#[test]
fn markdown_free_code_like_identifier_does_not_break_utf8_mapping() {
    let text = "café funciton";
    for diagnostic in check_spelling(text) {
        assert!(text.is_char_boundary(diagnostic.range.start));
        assert!(text.is_char_boundary(diagnostic.range.end));
    }
}

#[test]
fn chinese_only_text_is_left_untouched_by_the_english_dictionary() {
    assert!(check_spelling("这是一个本地优先的中文文档。").is_empty());
}

#[test]
fn unicode_prefix_keeps_replacement_text_and_byte_ranges_stable() {
    let text = "🙂 café sentnce";
    let diagnostic = check_spelling(text)
        .into_iter()
        .find(|diagnostic| diagnostic.original == "sentnce")
        .expect("Harper should report the English misspelling");

    assert_eq!(&text[diagnostic.range.clone()], "sentnce");
    assert!(
        diagnostic
            .replacements
            .iter()
            .any(|value| value == "sentence")
    );
}
