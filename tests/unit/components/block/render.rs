// @author kongweiguang

use super::super::MermaidViewMode;
use super::{
    HtmlComputedStyle, block_content_insets, column_axis_gutter_visible, html_node_visual_style,
    inline_word_chunks, mermaid_preview_canvas_height, mermaid_workbench_body_height,
    yaml_frontmatter_body,
};
use crate::components::{Block, BlockKind, BlockRecord, InlineTextTree, parse_html_document};
use crate::components::{CalloutVariant, TableAxisKind, TableAxisMarker};
use crate::i18n::I18nManager;
use crate::theme::{Theme, ThemeManager};
use gpui::{Hsla, Rgba, TestAppContext, px};

#[test]
fn tall_mermaid_preview_preserves_readable_height_for_internal_scrolling() {
    assert_eq!(mermaid_preview_canvas_height(2_400.0, 12.0, 360.0), 2_424.0);
    assert_eq!(mermaid_preview_canvas_height(120.0, 12.0, 360.0), 360.0);
}

#[test]
fn short_mermaid_preview_stays_compact() {
    let heights = mermaid_workbench_body_height(
        MermaidViewMode::Preview,
        1_200.0,
        900.0,
        Some(180.0),
        2,
        20.0,
    );

    assert_eq!(heights.preview_height, 360.0);
    assert_eq!(heights.body_height, 360.0);
}

#[test]
fn tall_mermaid_preview_grows_with_the_successful_svg() {
    let heights = mermaid_workbench_body_height(
        MermaidViewMode::Preview,
        1_200.0,
        1_000.0,
        Some(480.0),
        2,
        20.0,
    );

    assert_eq!(heights.preview_height, 504.0);
    assert_eq!(heights.body_height, 504.0);
}

#[test]
fn mermaid_preview_expands_before_using_internal_scrolling() {
    let viewport_capped = mermaid_workbench_body_height(
        MermaidViewMode::Preview,
        1_200.0,
        800.0,
        Some(2_000.0),
        2,
        20.0,
    );
    let absolute_capped = mermaid_workbench_body_height(
        MermaidViewMode::Preview,
        1_200.0,
        2_000.0,
        Some(2_000.0),
        2,
        20.0,
    );
    let small_viewport_floor = mermaid_workbench_body_height(
        MermaidViewMode::Preview,
        1_200.0,
        400.0,
        Some(2_000.0),
        2,
        20.0,
    );

    assert_eq!(viewport_capped.body_height, 1_200.0);
    assert_eq!(absolute_capped.body_height, 1_440.0);
    assert_eq!(small_viewport_floor.body_height, 720.0);
}

#[test]
fn mermaid_source_height_tracks_lines_and_caps_at_the_viewport_budget() {
    let short_source =
        mermaid_workbench_body_height(MermaidViewMode::Source, 1_200.0, 1_000.0, None, 4, 20.0);
    let long_source =
        mermaid_workbench_body_height(MermaidViewMode::Source, 1_200.0, 1_000.0, None, 100, 20.0);

    assert_eq!(short_source.body_height, 360.0);
    assert_eq!(long_source.source_height, 700.0);
    assert_eq!(long_source.body_height, 700.0);
}

#[test]
fn wide_mermaid_split_uses_the_taller_side() {
    let preview_driven =
        mermaid_workbench_body_height(MermaidViewMode::Split, 900.0, 1_200.0, Some(500.0), 2, 20.0);
    let source_driven = mermaid_workbench_body_height(
        MermaidViewMode::Split,
        900.0,
        1_200.0,
        Some(100.0),
        35,
        20.0,
    );

    assert_eq!(preview_driven.body_height, 524.0);
    assert_eq!(preview_driven.source_height, 524.0);
    assert_eq!(source_driven.body_height, 724.0);
    assert_eq!(source_driven.preview_height, 724.0);
}

#[test]
fn narrow_mermaid_split_keeps_both_panes_readable() {
    let compact =
        mermaid_workbench_body_height(MermaidViewMode::Split, 600.0, 900.0, Some(100.0), 1, 20.0);
    let tall = mermaid_workbench_body_height(
        MermaidViewMode::Split,
        600.0,
        900.0,
        Some(1_000.0),
        100,
        20.0,
    );

    assert_eq!(compact.source_height, 280.0);
    assert_eq!(compact.preview_height, 280.0);
    assert_eq!(compact.body_height, 561.0);
    assert_eq!(tall.source_height, 360.0);
    assert_eq!(tall.preview_height, 360.0);
    assert_eq!(tall.body_height, 721.0);
}

#[test]
fn frontmatter_style_requires_a_complete_yaml_document() {
    assert_eq!(
        yaml_frontmatter_body("---\nname: example\n---"),
        Some("name: example")
    );
    assert_eq!(
        yaml_frontmatter_body("---\nname: example\n..."),
        Some("name: example")
    );
    assert_eq!(yaml_frontmatter_body("---\n---"), Some(""));
    assert_eq!(yaml_frontmatter_body("---\nname: incomplete"), None);
    assert_eq!(yaml_frontmatter_body("body\n---"), None);
}

#[test]
fn top_gutter_only_appears_for_column_axis_state() {
    assert!(!column_axis_gutter_visible(None, None));
    assert!(!column_axis_gutter_visible(
        Some(TableAxisMarker {
            kind: TableAxisKind::Row,
            index: 0,
        }),
        None,
    ));
    assert!(column_axis_gutter_visible(
        Some(TableAxisMarker {
            kind: TableAxisKind::Column,
            index: 0,
        }),
        None,
    ));
    assert!(column_axis_gutter_visible(
        None,
        Some(TableAxisMarker {
            kind: TableAxisKind::Column,
            index: 0,
        }),
    ));
}

#[test]
fn grouped_surfaces_own_their_horizontal_padding() {
    assert_eq!(block_content_insets(36.0, 12.0, 2, false), (60.0, 36.0));
    assert_eq!(block_content_insets(36.0, 12.0, 2, true), (24.0, 0.0));
}

#[test]
fn callout_surfaces_use_workbench_semantic_roles() {
    let theme = Theme::default_theme();
    let (accent, background) =
        super::callout_accent_and_background(CalloutVariant::Warning, &theme);

    assert_eq!(accent, theme.colors.workbench.warning);
    assert_eq!(background.h, accent.h);
    assert_eq!(background.s, accent.s);
    assert!(background.a < accent.a);
}

fn assert_color_near(color: Hsla, red: u8, green: u8, blue: u8, alpha: u8) {
    let color = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as i16;
    assert!((channel(color.r) - red as i16).abs() <= 1);
    assert!((channel(color.g) - green as i16).abs() <= 1);
    assert!((channel(color.b) - blue as i16).abs() <= 1);
    assert!((channel(color.a) - alpha as i16).abs() <= 1);
}

#[test]
fn inline_word_chunks_split_text_runs_for_wrapping() {
    // Plain runs split per word so the flex-wrap row can break between
    // words and keep neighboring inline math on the same visual line.
    assert_eq!(
        inline_word_chunks("Fusce x malesuada", false, false),
        vec!["Fusce ", "x ", "malesuada"],
    );
    // Trailing whitespace stays attached so spacing survives the split.
    assert_eq!(inline_word_chunks("end ", false, false), vec!["end "]);
    assert!(inline_word_chunks("", false, false).is_empty());
}

#[test]
fn inline_word_chunks_keep_boxed_runs_whole() {
    // Inline code and background highlights keep their box continuous.
    assert_eq!(
        inline_word_chunks("let x = 2", true, false),
        vec!["let x = 2"],
    );
    assert_eq!(
        inline_word_chunks("highlighted text", false, true),
        vec!["highlighted text"],
    );
}

#[test]
fn html_render_style_inherits_color_and_font_size() {
    let theme = Theme::default_theme();
    let doc = parse_html_document(
        "<div style=\"color:blue; font-size:20px\"><span style=\"font-size:120%\">x</span></div>",
    );
    let root = HtmlComputedStyle::root(&theme);
    let parent = html_node_visual_style(&doc.nodes[0], root, &theme);
    let child = html_node_visual_style(&doc.nodes[0].children[0], parent.computed, &theme);

    assert_color_near(parent.computed.color, 0, 0, 255, 255);
    assert_color_near(child.computed.color, 0, 0, 255, 255);
    assert!((child.computed.font_size - 24.0).abs() < 0.01);
}

#[test]
fn html_render_style_overrides_link_and_mark_defaults() {
    let theme = Theme::default_theme();
    let link_doc = parse_html_document("<a style=\"color:red\">x</a>");
    let link_style =
        html_node_visual_style(&link_doc.nodes[0], HtmlComputedStyle::root(&theme), &theme);
    assert_color_near(link_style.computed.color, 255, 0, 0, 255);

    let mark_doc = parse_html_document("<mark style=\"background-color:#123\">x</mark>");
    let mark_style =
        html_node_visual_style(&mark_doc.nodes[0], HtmlComputedStyle::root(&theme), &theme);
    assert_color_near(mark_style.background.unwrap(), 0x11, 0x22, 0x33, 0xff);
}

#[test]
fn html_render_style_does_not_inherit_background_color() {
    let theme = Theme::default_theme();
    let doc =
        parse_html_document("<div style=\"background-color:#112233\"><span>child</span></div>");
    let root = HtmlComputedStyle::root(&theme);
    let parent = html_node_visual_style(&doc.nodes[0], root, &theme);
    let child = html_node_visual_style(&doc.nodes[0].children[0], parent.computed, &theme);

    assert_color_near(parent.background.unwrap(), 0x11, 0x22, 0x33, 0xff);
    assert!(child.background.is_none());
}

#[gpui::test]
async fn code_language_input_docks_in_top_toolbar(cx: &mut TestAppContext) {
    cx.update(|cx| {
        I18nManager::init(cx);
        ThemeManager::init(cx);
    });
    let (block, cx) = cx.add_window_view(|_window, cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::CodeBlock {
                    language: Some("rust".into()),
                },
                InlineTextTree::plain("fn main() {}\n"),
            ),
        )
    });

    cx.update(|window, cx| {
        block.update(cx, |block, _cx| {
            block.focus_handle.focus(window);
        });
        window.draw(cx).clear();
    });
    cx.run_until_parked();

    let (text_bounds, language_bounds) = block.read_with(cx, |block, _cx| {
        (
            block.last_bounds.expect("code text should render"),
            block
                .code_language_last_bounds
                .expect("language input should render"),
        )
    });
    let surface_bounds = cx
        .debug_bounds("code-block-surface")
        .expect("code surface should render");
    let text_inset = f32::from(text_bounds.left() - surface_bounds.left());
    let text_end_inset = f32::from(surface_bounds.right() - text_bounds.right());
    assert!(
        (text_inset - 12.0).abs() <= 0.5,
        "code surface should start at the shared rendered-content edge; text_inset={text_inset}, text_bounds={text_bounds:?}, surface_bounds={surface_bounds:?}"
    );
    assert!(
        (text_end_inset - 12.0).abs() <= 0.5,
        "code surface should preserve the same right content edge; text_end_inset={text_end_inset}, text_bounds={text_bounds:?}, surface_bounds={surface_bounds:?}"
    );
    assert!(language_bounds.top() < text_bounds.top());
    let left_gap = f32::from(language_bounds.left() - text_bounds.left());
    assert!(
        left_gap.abs() <= 12.0,
        "expected language input to align with code content; left_gap={left_gap}, text_bounds={text_bounds:?}, language_bounds={language_bounds:?}"
    );
    assert!(language_bounds.size.width <= px(156.0));
}

#[gpui::test]
async fn separator_uses_the_shared_rendered_content_edges(cx: &mut TestAppContext) {
    cx.update(|cx| {
        I18nManager::init(cx);
        ThemeManager::init(cx);
    });
    let (_block, cx) = cx.add_window_view(|_window, cx| {
        Block::with_record(
            cx,
            BlockRecord::new(BlockKind::Separator, InlineTextTree::plain("---")),
        )
    });

    cx.update(|window, cx| window.draw(cx).clear());
    let shell = cx
        .debug_bounds("separator-shell")
        .expect("separator shell should render");
    let surface = cx
        .debug_bounds("separator-surface")
        .expect("separator surface should render");
    let expected_inset = Theme::default_theme().dimensions.block_padding_x
        + super::super::slash_command::BLOCK_GUTTER_TEXT_RESERVE;
    let left_inset = f32::from(surface.left() - shell.left());
    let right_inset = f32::from(shell.right() - surface.right());
    assert!(
        (left_inset - expected_inset).abs() <= 0.5,
        "separator should start at the paragraph content edge; left_inset={left_inset}, expected={expected_inset}"
    );
    assert!(
        (right_inset - expected_inset).abs() <= 0.5,
        "separator should end at the paragraph content edge; right_inset={right_inset}, expected={expected_inset}"
    );
}
