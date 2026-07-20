// @author kongweiguang

use super::{
    HtmlComputedStyle, column_axis_gutter_visible, html_node_visual_style, inline_word_chunks,
    yaml_frontmatter_body,
};
use crate::components::{Block, BlockKind, BlockRecord, InlineTextTree, parse_html_document};
use crate::components::{TableAxisKind, TableAxisMarker};
use crate::i18n::I18nManager;
use crate::theme::{Theme, ThemeManager};
use gpui::{Hsla, Rgba, TestAppContext, px};

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
