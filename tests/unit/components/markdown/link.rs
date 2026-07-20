// @author kongweiguang

use super::{
    LinkReferenceDefinition, is_supported_autolink_target, parse_link_reference_definitions,
};

#[test]
fn parses_link_reference_definitions_with_title_and_first_wins() {
    let definitions = parse_link_reference_definitions(
        "[Ref Link]: https://first.example \"Caption\"\n[ref link]: https://second.example",
    );
    assert_eq!(
        definitions.get("ref link"),
        Some(&LinkReferenceDefinition {
            destination: "https://first.example".to_string(),
            title: Some("Caption".to_string()),
        })
    );
}

#[test]
fn parses_container_scoped_link_reference_definitions_and_skips_raw_blocks() {
    let definitions = parse_link_reference_definitions(
        [
            "> [quoted ref]: https://quoted.example \"Quoted\"",
            "- [list ref]: https://list.example",
            "1) [ordered ref]: https://ordered.example",
            "> ```md",
            "> [code ref]: https://ignored-code.example",
            "> ```",
            "",
            "<div>",
            "[html ref]: https://ignored-html.example",
            "</div>",
        ]
        .join("\n")
        .as_str(),
    );

    assert_eq!(
        definitions.get("quoted ref"),
        Some(&LinkReferenceDefinition {
            destination: "https://quoted.example".to_string(),
            title: Some("Quoted".to_string()),
        })
    );
    assert_eq!(
        definitions.get("list ref"),
        Some(&LinkReferenceDefinition {
            destination: "https://list.example".to_string(),
            title: None,
        })
    );
    assert_eq!(
        definitions.get("ordered ref"),
        Some(&LinkReferenceDefinition {
            destination: "https://ordered.example".to_string(),
            title: None,
        })
    );
    assert!(!definitions.contains_key("code ref"));
    assert!(!definitions.contains_key("html ref"));
}

#[test]
fn supports_http_https_and_mailto_autolinks() {
    assert!(is_supported_autolink_target("https://example.com"));
    assert!(is_supported_autolink_target("http://example.com"));
    assert!(is_supported_autolink_target("mailto:test@example.com"));
    assert!(!is_supported_autolink_target("./relative/path"));
    assert!(!is_supported_autolink_target("span>x</span"));
}
