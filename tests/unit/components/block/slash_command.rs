// @author kongweiguang

use gpui::{AppContext, Bounds, point, px, size};

use super::{
    SLASH_COMMANDS, SlashCommand, boundary_available_index, filter_slash_commands,
    selected_available_index, slash_menu_placement,
};
use crate::components::{
    Block, BlockKind, BlockRecord, EditingCommandId, EditingContext, InlineTextTree,
};

#[test]
fn filters_english_chinese_and_pinyin_initial_aliases() {
    assert_eq!(filter_slash_commands("h2"), vec![SlashCommand::Heading2]);
    assert_eq!(filter_slash_commands("表格"), vec![SlashCommand::Table]);
    assert_eq!(filter_slash_commands("dmk"), vec![SlashCommand::CodeBlock]);
    assert_eq!(filter_slash_commands("missing"), Vec::new());
}

#[test]
fn every_slash_command_uses_a_local_svg_icon() {
    let mut paths = SLASH_COMMANDS
        .into_iter()
        .map(|command| command.descriptor().icon_path)
        .collect::<Vec<_>>();
    assert!(paths.iter().all(|path| path.starts_with("icon/ui/")));
    assert!(paths.iter().all(|path| path.ends_with(".svg")));
    paths.sort_unstable();
    paths.dedup();
    assert_eq!(paths.len(), SLASH_COMMANDS.len());
}

#[test]
fn keyboard_navigation_skips_structurally_disabled_commands() {
    let commands = [
        EditingCommandId::MoveBlockUp,
        EditingCommandId::DuplicateBlock,
        EditingCommandId::MoveBlockDown,
        EditingCommandId::DeleteBlock,
    ];
    let only_block = EditingContext {
        sibling_index: 0,
        sibling_count: 1,
        ..EditingContext::default()
    };

    assert_eq!(
        boundary_available_index(&commands, false, only_block),
        Some(1)
    );
    assert_eq!(
        boundary_available_index(&commands, true, only_block),
        Some(3)
    );
    assert_eq!(
        selected_available_index(&commands, 1, 1, only_block),
        Some(3)
    );
    assert_eq!(
        selected_available_index(&commands, 3, 1, only_block),
        Some(1)
    );
}

#[test]
fn recent_commands_form_one_distinct_group_for_scroll_indexing() {
    let state = super::SlashMenuState {
        query: String::new(),
        filtered: vec![
            EditingCommandId::Table,
            EditingCommandId::Heading1,
            EditingCommandId::Paragraph,
        ],
        selected: 0,
        trigger_range: 0..0,
        recent_count: 2,
        programmatic_text: None,
        programmatic_allow_raw: false,
        structural_revision: 0,
    };
    assert_eq!(super::slash_menu_child_index(&state, 0), 1);
    assert_eq!(super::slash_menu_child_index(&state, 1), 2);
    assert_eq!(super::slash_menu_child_index(&state, 2), 4);
}

#[test]
fn slash_menu_flips_and_clamps_to_narrow_viewports() {
    let near_bottom = slash_menu_placement(
        Bounds::new(point(px(180.0), px(360.0)), size(px(1.0), px(20.0))),
        size(px(240.0), px(400.0)),
        304.0,
    );
    assert!(near_bottom.above);
    assert_eq!(near_bottom.left, 8.0);
    assert_eq!(near_bottom.width, 224.0);
    assert!(near_bottom.top >= 8.0);

    let near_top = slash_menu_placement(
        Bounds::new(point(px(12.0), px(10.0)), size(px(1.0), px(20.0))),
        size(px(640.0), px(400.0)),
        304.0,
    );
    assert!(!near_top.above);
    assert_eq!(near_top.top, 34.0);
    assert!(near_top.top + near_top.max_height <= 392.0);
}

#[gpui::test]
async fn slash_menu_opens_in_heading_and_list_rich_text_blocks(cx: &mut gpui::TestAppContext) {
    for kind in [
        BlockKind::Heading { level: 1 },
        BlockKind::BulletedListItem,
        BlockKind::Quote,
    ] {
        let block = cx.new(|cx| {
            Block::with_record(
                cx,
                BlockRecord::new(kind, InlineTextTree::plain("alpha /h2")),
            )
        });
        block.update(cx, |block, cx| {
            block.selected_range = 9..9;
            block.refresh_slash_menu(cx);
            assert!(block.slash_menu.is_some());
        });
    }
}

#[gpui::test]
async fn slash_session_tracks_trigger_anchor_and_closes_when_cursor_leaves(
    cx: &mut gpui::TestAppContext,
) {
    let typed = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph("/h2 /h2")));
    typed.update(cx, |block, cx| {
        block.selected_range = 3..3;
        block.refresh_slash_menu(cx);
        assert_eq!(block.slash_menu.as_ref().unwrap().trigger_range, 0..3);

        block.selected_range = 7..7;
        block.refresh_slash_menu(cx);
        assert_eq!(block.slash_menu.as_ref().unwrap().trigger_range, 4..7);
    });

    let programmatic = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph("alpha")));
    programmatic.update(cx, |block, cx| {
        block.selected_range = 5..5;
        block.open_insert_command_menu(cx);
        assert!(block.slash_menu.is_some());
        block.selected_range = 0..0;
        block.refresh_slash_menu(cx);
        assert!(block.slash_menu.is_none());
    });
}

#[gpui::test]
async fn slash_menu_is_suppressed_during_ime_source_and_table_cell_editing(
    cx: &mut gpui::TestAppContext,
) {
    let ime = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph("/h")));
    ime.update(cx, |block, cx| {
        block.selected_range = 2..2;
        block.marked_range = Some(1..2);
        block.refresh_slash_menu(cx);
        assert!(block.slash_menu.is_none());
    });

    let source = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph("/h")));
    source.update(cx, |block, cx| {
        block.set_source_document_mode();
        block.selected_range = 2..2;
        block.refresh_slash_menu(cx);
        assert!(block.slash_menu.is_none());
    });

    let cell = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph("/h")));
    cell.update(cx, |block, cx| {
        block.set_table_cell_mode(
            crate::components::TableCellPosition { row: 1, column: 0 },
            crate::components::TableColumnAlignment::Default,
        );
        block.selected_range = 2..2;
        block.refresh_slash_menu(cx);
        assert!(block.slash_menu.is_none());
    });
}
