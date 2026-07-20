// @author kongweiguang

use gpui::{AppContext, TestAppContext};

use crate::components::{BlockKind, BlockRecord};
use crate::editor::Editor;

#[gpui::test]
async fn snapshot_tracks_nested_visible_order(cx: &mut TestAppContext) {
    let editor =
        cx.new(|cx| Editor::from_markdown(cx, "- a\n  - b\n    - c\n- d".to_string(), None));

    editor.update(cx, |editor, _cx| {
        let visible = editor.document.visible_blocks().to_vec();
        let a = visible[0].entity.clone();
        let b = visible[1].entity.clone();
        let c = visible[2].entity.clone();
        let d = visible[3].entity.clone();

        assert_eq!(
            editor.document.visible_index_for_entity_id(a.entity_id()),
            Some(0)
        );
        assert_eq!(
            editor.document.visible_index_for_entity_id(b.entity_id()),
            Some(1)
        );
        assert_eq!(
            editor.document.visible_index_for_entity_id(c.entity_id()),
            Some(2)
        );
        assert_eq!(
            editor.document.visible_index_for_entity_id(d.entity_id()),
            Some(3)
        );

        let c_location = editor
            .document
            .find_block_location(c.entity_id())
            .expect("location");
        assert_eq!(
            c_location.parent.expect("nested parent").entity_id(),
            b.entity_id()
        );
        assert_eq!(c_location.index, 0);

        assert_eq!(
            editor
                .document
                .last_visible_descendant(a.entity_id())
                .expect("descendant")
                .entity_id(),
            c.entity_id()
        );
    });
}

#[gpui::test]
async fn rebuild_hoists_children_from_leaf_blocks(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

    editor.update(cx, |editor, cx| {
        let root = editor.document.first_root().expect("root").clone();
        let child = Editor::new_block(cx, BlockRecord::paragraph("child"));

        root.update(cx, {
            let child = child.clone();
            move |root, _cx| {
                root.children.push(child.clone());
            }
        });

        editor.document.rebuild_metadata_and_snapshot(cx);

        assert!(root.read(cx).children.is_empty());
        let visible_ids = editor
            .document
            .visible_blocks()
            .iter()
            .map(|visible| visible.entity.entity_id())
            .collect::<Vec<_>>();
        assert_eq!(visible_ids, vec![root.entity_id(), child.entity_id()]);

        let location = editor
            .document
            .find_block_location(child.entity_id())
            .expect("child location");
        assert!(location.parent.is_none());
        assert_eq!(location.index, 1);
    });
}

#[gpui::test]
async fn code_block_language_edit_serializes_to_opening_fence(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "```rust\nfn main() {}\n```".into(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("code block").clone();
        block.update(cx, |block, cx| {
            let range = 0..block.code_language_text().len();
            block.replace_code_language_text_in_range(range, "unknown-lang", None, false, cx);
        });

        assert_eq!(
            editor.document.markdown_text(cx),
            "```unknown-lang\nfn main() {}\n```"
        );
    });
}

#[gpui::test]
async fn code_block_language_with_backtick_round_trips_with_tilde_fence(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "```rust\nbody\n```".into(), None));

    let markdown = editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("code block").clone();
        block.update(cx, |block, cx| {
            let range = 0..block.code_language_text().len();
            block.replace_code_language_text_in_range(range, "we`rd", None, false, cx);
        });
        editor.document.markdown_text(cx)
    });

    assert_eq!(markdown, "~~~we`rd\nbody\n~~~");

    let round_tripped = cx.new(|cx| Editor::from_markdown(cx, markdown, None));
    round_tripped.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("code block");
        assert_eq!(block.read(cx).code_language_text(), "we`rd");
        assert!(matches!(block.read(cx).kind(), BlockKind::CodeBlock { .. }));
    });
}

#[gpui::test]
async fn structure_mutation_rebuilds_snapshot_after_relocation(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "- a\n- b\n- c".to_string(), None));

    editor.update(cx, |editor, cx| {
        let visible = editor.document.visible_blocks().to_vec();
        let a = visible[0].entity.clone();
        let b = visible[1].entity.clone();
        let c = visible[2].entity.clone();

        editor.document.with_structure_mutation(cx, |document, cx| {
            let moved = document
                .remove_block_by_id_raw(c.entity_id(), cx)
                .expect("remove c")
                .0;
            document.insert_blocks_at_raw(
                Some(a.clone()),
                a.read(cx).children.len(),
                vec![moved],
                cx,
            );
        });

        assert_eq!(
            editor.document.visible_index_for_entity_id(a.entity_id()),
            Some(0)
        );
        assert_eq!(
            editor.document.visible_index_for_entity_id(c.entity_id()),
            Some(1)
        );
        assert_eq!(
            editor.document.visible_index_for_entity_id(b.entity_id()),
            Some(2)
        );

        let c_location = editor
            .document
            .find_block_location(c.entity_id())
            .expect("c location");
        assert_eq!(
            c_location.parent.expect("nested parent").entity_id(),
            a.entity_id()
        );
        assert_eq!(c_location.index, 0);

        assert_eq!(
            editor
                .document
                .last_visible_descendant(a.entity_id())
                .expect("descendant")
                .entity_id(),
            c.entity_id()
        );
    });
}
