// @author kongweiguang

use super::*;
use gpui::rgba;

#[test]
fn parses_single_line_display_math() {
    let parsed = parse_display_math_source("$$x^2$$").expect("display math");
    assert_eq!(parsed.body, "x^2");
    assert_eq!(parsed.raw, "$$x^2$$");
}

#[test]
fn parses_multiline_display_math() {
    let parsed = parse_display_math_source("$$\n\\int_0^1 x^2 dx\n$$").expect("display math");
    assert_eq!(parsed.body, "\\int_0^1 x^2 dx");
}

#[test]
fn rejects_unclosed_display_math() {
    assert!(parse_display_math_source("$$\n\\frac{1}{2}").is_none());
}

#[test]
fn cache_key_changes_with_theme_inputs() {
    let first = latex_cache_key("\\frac{1}{2}", Hsla::from(rgba(0xffffffff)), 18.0);
    let second = latex_cache_key("\\frac{1}{2}", Hsla::from(rgba(0x000000ff)), 18.0);
    assert_ne!(first, second);
}

#[test]
fn display_math_font_size_scales_base_text_size() {
    assert_eq!(display_math_font_size(20.0), 25.0);
}

#[test]
fn inline_math_font_size_scales_base_text_size() {
    assert!((inline_math_font_size(20.0) - 22.4).abs() < 0.001);
}

#[test]
fn renders_basic_formula_svg() {
    let svg = render_latex_to_svg("\\frac{1}{2}", Hsla::from(rgba(0xffffffff)), 18.0).expect("svg");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("</svg>"));
}

#[test]
fn invalid_latex_returns_error() {
    assert!(render_latex_to_svg("\\frac{a}", Hsla::from(rgba(0xffffffff)), 18.0).is_err());
}
