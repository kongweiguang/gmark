// @author kongweiguang

use super::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static TEST_RENDERER_CALLS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();

fn test_renderer(source: &str) -> anyhow::Result<String> {
    let calls = TEST_RENDERER_CALLS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut calls = calls.lock().expect("renderer calls mutex poisoned");
    *calls.entry(source.to_string()).or_default() += 1;
    drop(calls);
    render_mermaid_raw(source)
}

fn reset_renderer_calls(source: &str) {
    let calls = TEST_RENDERER_CALLS.get_or_init(|| Mutex::new(HashMap::new()));
    calls
        .lock()
        .expect("renderer calls mutex poisoned")
        .remove(source);
}

fn renderer_calls(source: &str) -> usize {
    let calls = TEST_RENDERER_CALLS.get_or_init(|| Mutex::new(HashMap::new()));
    calls
        .lock()
        .expect("renderer calls mutex poisoned")
        .get(source)
        .copied()
        .unwrap_or(0)
}

fn unique_mermaid_source(label: &str) -> MermaidSource {
    MermaidSource {
        raw: format!("```mermaid\nflowchart LR\nA[{}] --> B\n```", label),
        body: format!("flowchart LR\nA[{}] --> B", label),
        info: "mermaid".to_string(),
    }
}

fn remove_cache_file(path: &Path) {
    if path.exists() {
        fs::remove_file(path).expect("remove cache file");
    }
}

#[test]
fn detects_mermaid_info_string() {
    assert!(is_mermaid_info_string(Some("mermaid")));
    assert!(is_mermaid_info_string(Some("MMD title")));
    assert!(!is_mermaid_info_string(Some("rust")));
    assert!(!is_mermaid_info_string(None));
}

#[test]
fn parses_backtick_mermaid_fence() {
    let parsed = parse_mermaid_fence_source("```mermaid\nflowchart LR\nA --> B\n```")
        .expect("mermaid fence");
    assert_eq!(parsed.info, "mermaid");
    assert_eq!(parsed.body, "flowchart LR\nA --> B");
}

#[test]
fn parses_tilde_mmd_fence() {
    let parsed =
        parse_mermaid_fence_source("~~~MMD\nflowchart LR\nA --> B\n~~~").expect("mermaid fence");
    assert_eq!(parsed.info, "MMD");
    assert_eq!(parsed.body, "flowchart LR\nA --> B");
}

#[test]
fn rejects_unclosed_mermaid_fence() {
    assert!(parse_mermaid_fence_source("```mermaid\nflowchart LR").is_none());
}

#[test]
fn cache_key_changes_with_source() {
    assert_ne!(
        mermaid_cache_key("flowchart LR\nA --> B"),
        mermaid_cache_key("flowchart LR\nA --> C")
    );
}

#[test]
fn semantic_line_count_ignores_comments_blank_lines_and_frontmatter() {
    let source = "---\ntitle: Demo\n---\nflowchart LR\n%% comment\n\nA --> B\nB --> C";
    assert_eq!(semantic_mermaid_line_count(source), 3);
}

#[test]
fn display_scale_uses_intrinsic_size_and_caps_growth() {
    let simple = "flowchart LR\nA --> B\nB --> C";
    assert_eq!(
        mermaid_display_scale(simple, 240.0, 120.0, 720.0, 960.0),
        1.0
    );

    let complex = std::iter::once("flowchart LR".to_string())
        .chain((0..20).map(|index| format!("A{index} --> A{}", index + 1)))
        .collect::<Vec<_>>()
        .join("\n");
    let scale = mermaid_display_scale(&complex, 260.0, 140.0, 720.0, 960.0);
    assert!(scale > 1.0);
    assert!(scale <= MERMAID_MAX_SCALE);
    assert!(260.0 * scale <= 720.0 + 0.5);
}

#[test]
fn display_scale_does_not_overgrow_already_wide_diagrams() {
    let complex = std::iter::once("flowchart LR".to_string())
        .chain((0..30).map(|index| format!("A{index} --> A{}", index + 1)))
        .collect::<Vec<_>>()
        .join("\n");
    let scale = mermaid_display_scale(&complex, 1400.0, 400.0, 720.0, 960.0);

    assert!((scale - 720.0 / 1400.0).abs() < 0.001);
    assert!(1400.0 * scale <= 720.0 + 0.5);
}

#[test]
fn display_cache_key_changes_with_scale() {
    let source = "flowchart LR\nA --> B";
    assert_ne!(
        mermaid_display_cache_key(source, 1.0, MermaidThemeMode::Light),
        mermaid_display_cache_key(source, 2.0, MermaidThemeMode::Light)
    );
}

#[test]
fn display_cache_key_changes_with_theme() {
    let source = "flowchart LR\nA --> B";
    assert_ne!(
        mermaid_display_cache_key(source, 1.0, MermaidThemeMode::Light),
        mermaid_display_cache_key(source, 1.0, MermaidThemeMode::Dark)
    );
}

#[test]
fn dark_renderer_uses_dark_mermaid_palette() {
    let svg = render_mermaid_raw_with_theme("flowchart LR\nA --> B", MermaidThemeMode::Dark)
        .expect("dark Mermaid SVG");

    assert!(svg.contains("#1f2020") || svg.contains("#333333"));
    assert!(svg.contains("#e0dfdf") || svg.contains("#ccc"));
}

#[test]
fn display_svg_scaling_rewrites_root_dimensions() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50" viewBox="0 0 100 50"><rect width="100" height="50"/></svg>"#;
    let (scaled, size) = scale_mermaid_svg_for_display(svg, 2.0).expect("scaled svg");

    assert_eq!(
        size,
        MermaidSvgSize {
            width: 200.0,
            height: 100.0
        }
    );
    assert!(scaled.contains(r#"width="200.000""#));
    assert!(scaled.contains(r#"height="100.000""#));
    assert!(scaled.contains(r#"viewBox="0 0 100 50""#));
}

#[test]
fn display_svg_scaling_removes_responsive_root_attrs() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" style="max-width: 240px; aspect-ratio: 2;" viewBox="0 0 120 60"><text>x</text></svg>"#;
    let (scaled, size) = scale_mermaid_svg_for_display(svg, 1.5).expect("scaled svg");

    assert_eq!(
        size,
        MermaidSvgSize {
            width: 180.0,
            height: 90.0
        }
    );
    let root = &scaled[..scaled.find('>').unwrap()];
    assert!(root.contains(r#"width="180.000""#));
    assert!(root.contains(r#"height="90.000""#));
    assert!(!root.contains("100%"));
    assert!(!root.contains("max-width"));
    assert!(!root.contains("style="));
}

#[test]
fn renders_basic_flowchart_svg() {
    let svg = render_mermaid_to_svg("flowchart LR\nA --> B", MermaidThemeMode::Light).expect("svg");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("</svg>"));
}

#[test]
fn display_render_uses_scaled_intrinsic_size() {
    let source =
        parse_mermaid_fence_source("```mermaid\nflowchart LR\nA --> B\n```").expect("source");
    let rendered = render_mermaid_svg_for_display(&source, 720.0, 960.0, MermaidThemeMode::Light)
        .expect("display svg");

    assert!(rendered.display_width > 1.0);
    assert!(rendered.display_height > 1.0);
    assert!(rendered.display_scale >= 1.0);
    assert!(
        rendered
            .svg
            .contains(&format!("width=\"{:.3}\"", rendered.display_width))
    );
    assert!(
        rendered
            .svg
            .contains(&format!("height=\"{:.3}\"", rendered.display_height))
    );
    assert!(rendered.path.exists());
}

#[test]
fn invalid_mermaid_returns_error() {
    assert!(
        render_mermaid_to_svg("not a real mermaid diagram ::::", MermaidThemeMode::Light).is_err()
    );
}

#[test]
fn display_cache_hit_does_not_call_renderer_again() {
    let cache = tempfile::tempdir().expect("isolated Mermaid cache");
    let source = unique_mermaid_source("display-cache-hit-does-not-call-renderer-again");
    let base_key = mermaid_cache_key(&source.body);
    let base_path = mermaid_cache_file_path_in(cache.path(), "base", &base_key).expect("base path");
    remove_cache_file(&base_path);

    reset_renderer_calls(&source.body);
    let first = render_mermaid_svg_for_display_in_cache(
        &source,
        720.0,
        960.0,
        MermaidThemeMode::Light,
        test_renderer,
        cache.path(),
    )
    .expect("first render");
    assert_eq!(renderer_calls(&source.body), 1);
    let display_path = first.path.clone();

    let second = render_mermaid_svg_for_display_in_cache(
        &source,
        720.0,
        960.0,
        MermaidThemeMode::Light,
        test_renderer,
        cache.path(),
    )
    .expect("cached render");
    assert_eq!(renderer_calls(&source.body), 1);
    assert_eq!(second.path, display_path);
    assert_eq!(second.display_width, first.display_width);
    assert_eq!(second.display_height, first.display_height);

    remove_cache_file(&display_path);
    remove_cache_file(&base_path);
}

#[test]
fn display_cache_miss_reuses_base_cache() {
    let cache = tempfile::tempdir().expect("isolated Mermaid cache");
    let source = unique_mermaid_source("display-cache-miss-reuses-base-cache");
    let base_key = mermaid_cache_key(&source.body);
    let base_path = mermaid_cache_file_path_in(cache.path(), "base", &base_key).expect("base path");
    remove_cache_file(&base_path);

    reset_renderer_calls(&source.body);
    let first = render_mermaid_svg_for_display_in_cache(
        &source,
        720.0,
        960.0,
        MermaidThemeMode::Light,
        test_renderer,
        cache.path(),
    )
    .expect("first render");
    assert_eq!(renderer_calls(&source.body), 1);
    remove_cache_file(&first.path);

    let second = render_mermaid_svg_for_display_in_cache(
        &source,
        720.0,
        960.0,
        MermaidThemeMode::Light,
        test_renderer,
        cache.path(),
    )
    .expect("display rebuild");
    assert_eq!(renderer_calls(&source.body), 1);
    assert!(second.path.exists());
    assert_eq!(second.display_width, first.display_width);
    assert_eq!(second.display_height, first.display_height);

    remove_cache_file(&second.path);
    remove_cache_file(&base_path);
}

#[test]
fn display_scale_change_reuses_base_cache_with_new_display_file() {
    let cache = tempfile::tempdir().expect("isolated Mermaid cache");
    let source = unique_mermaid_source("display-scale-change-reuses-base-cache");
    let base_key = mermaid_cache_key(&source.body);
    let base_path = mermaid_cache_file_path_in(cache.path(), "base", &base_key).expect("base path");
    remove_cache_file(&base_path);

    reset_renderer_calls(&source.body);
    let narrow = render_mermaid_svg_for_display_in_cache(
        &source,
        240.0,
        320.0,
        MermaidThemeMode::Light,
        test_renderer,
        cache.path(),
    )
    .expect("narrow render");
    assert_eq!(renderer_calls(&source.body), 1);

    let wide = render_mermaid_svg_for_display_in_cache(
        &source,
        900.0,
        1200.0,
        MermaidThemeMode::Light,
        test_renderer,
        cache.path(),
    )
    .expect("wide render");
    assert_eq!(renderer_calls(&source.body), 1);
    assert!(wide.path.exists());

    remove_cache_file(&narrow.path);
    remove_cache_file(&wide.path);
    remove_cache_file(&base_path);
}
