// @author kongweiguang

use super::parse_file_url;
use std::path::PathBuf;

#[test]
fn parses_file_url_with_spaces() {
    assert_eq!(
        parse_file_url("file:///Users/example/My%20Notes/test%20file.md"),
        Some(PathBuf::from("/Users/example/My Notes/test file.md"))
    );
}

#[test]
fn parses_file_url_with_unicode() {
    assert_eq!(
        parse_file_url("file:///Users/example/Notes/%E2%9C%93-%E6%96%87.md"),
        Some(PathBuf::from("/Users/example/Notes/✓-文.md"))
    );
}

#[test]
fn parses_localhost_authority() {
    assert_eq!(
        parse_file_url("file://localhost/Users/example/test.md"),
        Some(PathBuf::from("/Users/example/test.md"))
    );
}

#[test]
fn rejects_non_file_scheme() {
    assert_eq!(parse_file_url("https://example.com/test.md"), None);
}

#[test]
fn passes_plain_path_through() {
    assert_eq!(
        parse_file_url("notes/100% literal.md"),
        Some(PathBuf::from("notes/100% literal.md"))
    );
}

#[test]
fn rejects_remote_file_authority() {
    assert_eq!(parse_file_url("file://example.com/share/test.md"), None);
}
