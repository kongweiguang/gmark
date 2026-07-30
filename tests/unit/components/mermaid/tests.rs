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
fn themed_cache_key_includes_the_render_strategy_version() {
    let source = "flowchart TD\nA --> B";
    let mut legacy_hasher = DefaultHasher::new();
    mermaid_cache_key(source).hash(&mut legacy_hasher);
    MermaidThemeMode::Light.hash(&mut legacy_hasher);

    assert_ne!(
        mermaid_themed_cache_key(source, MermaidThemeMode::Light),
        format!("{:016x}", legacy_hasher.finish())
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
        MERMAID_SIMPLE_MAX_SCALE
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
fn simple_wide_diagrams_still_shrink_to_the_content_column() {
    let source = "flowchart LR\nA --> B";

    assert!((mermaid_display_scale(source, 1_000.0, 120.0, 720.0, 960.0) - 0.72).abs() < 0.001);
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
fn flowchart_renderer_avoids_tight_grid_routing_bends() {
    let options = mermaid_render_options("flowchart TD\nA --> B");

    assert!(!options.layout.flowchart.routing.enable_grid_router);
    assert_eq!(options.layout.node_spacing, MERMAID_FLOWCHART_SPACING);
    assert_eq!(options.layout.rank_spacing, MERMAID_FLOWCHART_SPACING);
}

#[test]
fn decision_branches_use_side_ports_top_entries_and_shared_label_geometry() {
    let source = "flowchart TD\nA{校验通过?}\nA -->|是| B[提交成功]\nA -->|否| C[提示错误]";
    let options = mermaid_render_options(source);
    let parsed = mermaid_rs_renderer::parse_mermaid_strict(source).expect("flowchart source");
    let mut layout =
        mermaid_rs_renderer::compute_layout(&parsed.graph, &options.theme, &options.layout);
    normalize_flowchart_decision_branches(&mut layout, FlowchartDirection::TopDown);

    let decision = layout.nodes.get("A").expect("decision node");
    for edge in &layout.edges {
        let target = layout.nodes.get(&edge.to).expect("target node");
        let start = edge.points.first().copied().expect("branch start");
        let end = edge.points.last().copied().expect("branch end");
        let label = edge.label_anchor.expect("branch label anchor");
        assert!((start.1 - (decision.y + decision.height * 0.75)).abs() < 0.001);
        assert!((end.0 - (target.x + target.width / 2.0)).abs() < 0.001);
        assert!((end.1 + MERMAID_ARROW_TIP_EXTENSION - target.y).abs() < 0.001);
        assert_eq!(label, edge.points[1]);
        assert_eq!(label.0, end.0);
    }
    assert!(layout.edges[0].points[0].0 < decision.x + decision.width / 2.0);
    assert!(layout.edges[1].points[0].0 > decision.x + decision.width / 2.0);
}

#[test]
fn rendered_decision_labels_share_visible_backgrounds() {
    let source = "flowchart TD\nA{校验通过?}\nA -->|是| B[提交成功]\nA -->|否| C[提示错误]";
    let svg =
        render_mermaid_raw_with_theme(source, MermaidThemeMode::Dark).expect("dark flowchart SVG");

    assert_eq!(svg.matches("fill-opacity=\"0.95\"").count(), 2);
    assert!(!svg.contains("fill-opacity=\"0.00\""));
}

#[test]
fn forward_flowchart_edges_become_smooth_curves_with_the_original_end_tangent() {
    let svg = r#"<svg><path id="edge-2" class="edgePath" d="M 140.114,314.495 L 140.114,324.120 Q 140.114,331.995 135.145,338.105 L 108.863,370.422 Q 103.894,376.531 103.894,384.406 L 103.894,394.031" marker-end="url(#arrow)" /></svg>"#;
    let smoothed = smooth_forward_flowchart_edges("flowchart TD\nA --> B", svg);

    assert!(smoothed.contains("d=\"M 140.114,314.495 C "));
    assert!(smoothed.contains("103.894,394.031\""));
    assert!(!smoothed.contains(" Q "));
    assert!(smoothed.contains("marker-end"));
    let curve = smoothed.split(" C ").nth(1).expect("cubic curve");
    let coordinates = curve
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',' || ch == '"')
        .filter_map(|part| part.parse::<f32>().ok())
        .take(6)
        .collect::<Vec<_>>();
    assert_eq!(coordinates.len(), 6);
    assert!((coordinates[2] - coordinates[4]).abs() < 0.001);
}

#[test]
fn reverse_flowchart_edges_keep_their_avoidance_route() {
    let svg = r#"<svg><path class="edgePath" d="M 100,300 L 120,250 Q 130,240 140,200" /></svg>"#;

    assert_eq!(
        smooth_forward_flowchart_edges("flowchart TD\nA --> B", svg),
        svg
    );
}

#[test]
fn short_curved_edges_without_port_segments_are_left_unchanged() {
    let svg = r#"<svg><path class="edgePath" d="M 100,100 Q 120,120 140,160" /></svg>"#;

    assert_eq!(
        smooth_forward_flowchart_edges("flowchart TD\nA --> B", svg),
        svg
    );
}

#[test]
fn display_svg_scaling_rewrites_root_dimensions() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50" viewBox="0 0 100 50"><rect x="0" y="0" width="100" height="50" fill="#333333"/><rect width="100" height="50"/></svg>"##;
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
    assert!(scaled.contains(r#"<rect x="0" y="0" width="100" height="50" fill="none""#));
    assert!(scaled.contains(r#"<rect width="100" height="50"/>"#));
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
