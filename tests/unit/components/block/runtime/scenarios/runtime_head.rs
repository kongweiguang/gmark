// @author kongweiguang

use std::ops::Range;
use std::sync::Arc;

use super::projection::{
    expanded_display_cursor_offset_for_clean, expanded_display_offset_for_clean,
};
use crate::components::markdown::code_highlight::CodeLanguageKey;
use crate::components::markdown::inline::{
    InlineFragment, InlineInsertionAttributes, InlineLinkHit, InlineScript, InlineStyle,
    InlineTextTree,
};
use crate::components::markdown::link::parse_link_reference_definitions;
use crate::components::{
    Block, BlockKind, BlockRecord, CopyAsMarkdown, DeleteBack, IndentBlock, Newline,
    PasteAsPlainText, TableCellPosition,
};
use crate::i18n::I18nManager;
use crate::theme::ThemeManager;
use gpui::{
    AppContext, ClipboardItem, EntityInputHandler, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, TestAppContext, point, px,
};

fn assert_only_code_range(block: &Block, expected: Range<usize>) {
    let code_ranges = block
        .inline_spans()
        .iter()
        .filter(|span| span.style.code)
        .map(|span| span.range.clone())
        .collect::<Vec<_>>();
    assert_eq!(code_ranges, vec![expected]);
}

#[gpui::test]
async fn copy_as_markdown_preserves_selected_inline_formatting(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("before **bold** after"),
            ),
        )
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.move_to(7, block_cx);
            block.select_to(11, block_cx);
            block.on_copy_as_markdown(&CopyAsMarkdown, window, block_cx);
        });
    });

    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("**bold**")
    );
}

#[gpui::test]
async fn paste_as_plain_text_bypasses_image_url_conversion(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph("")));
    let image_url = "https://example.com/photo.png";

    cx.update(|window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(image_url.to_owned()));
        block.update(cx, |block, block_cx| {
            block.on_paste_as_plain_text(&PasteAsPlainText, window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), image_url);
        assert_eq!(block.record.title.serialize_markdown(), image_url);
    });
}

#[gpui::test]
async fn tab_inserts_character_in_paragraph(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph("ab")));

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.move_to(1, block_cx);
            block.on_indent_block(&IndentBlock, window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "a    b");
        assert_eq!(block.selected_range, 5..5);
    });
}
