// @author kongweiguang

use gmark_source_tools::{
    FormatterError, SourceLanguage, format_json, format_json_lines, format_source,
};

#[test]
fn json_formatting_is_idempotent_and_preserves_lexemes() {
    let source =
        "{\"z\":1e2,\"a\":\"\\u4e16\\u754c\",\"actual\":\"世界\",\"nested\":[true,false]}\n";
    let expected = "{\n  \"z\": 1e2,\n  \"a\": \"\\u4e16\\u754c\",\n  \"actual\": \"世界\",\n  \"nested\": [\n    true,\n    false\n  ]\n}\n";
    assert_eq!(format_json(source), Ok(expected.to_owned()));
    assert_eq!(format_json(expected), Ok(expected.to_owned()));
    let result = format_source(SourceLanguage::Json, source);
    assert!(matches!(result, Ok(result) if result.changed && result.text == expected));
}

#[test]
fn json_formatting_returns_errors_without_a_candidate() {
    assert!(matches!(
        format_json("{\"a\":}"),
        Err(FormatterError::InvalidJson { .. })
    ));
    for invalid in ["1e+-2", "{\u{000B}\"a\":1}"] {
        assert!(matches!(
            format_json(invalid),
            Err(FormatterError::InvalidJson { .. })
        ));
    }
    assert!(matches!(
        format_json_lines("{\"a\":1}\n{\"b\":}\n"),
        Err(FormatterError::InvalidJsonLine { record: 2, .. })
    ));
    assert!(matches!(
        format_source(SourceLanguage::Rust, "fn main() {}"),
        Err(FormatterError::Unavailable {
            language: SourceLanguage::Rust
        })
    ));
}

#[test]
fn json_lines_stays_one_record_per_line() {
    assert_eq!(
        format_json_lines(" { \"b\" : 2 }\n[ 1, 2 ]\n"),
        Ok("{\"b\":2}\n[1,2]\n".to_owned())
    );
}
