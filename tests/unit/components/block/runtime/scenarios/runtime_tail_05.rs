// @author kongweiguang

use crate::components::UndoCaptureKind;
use gpui::{KeyDownEvent, Keystroke};
use std::sync::atomic::{AtomicUsize, Ordering};

fn math_key_event(key: &str) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke::parse(key).expect("valid math key"),
        is_held: false,
    }
}

#[gpui::test]
async fn block_math_structure_focus_and_escape_preserve_live_source(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::math("$$\nx^2\n$$")));

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.focus_handle.focus(window);
            assert!(!block.sync_math_edit_focus(true, window, block_cx));
            assert!(block.math_edit_session.is_some());
            assert!(block.math_structure_focus_handle.is_focused(window));

            assert!(block.execute_math_command_live(
                gmark_math_edit::MathEditCommand::InsertText("+1".to_owned()),
                UndoCaptureKind::CoalescibleText,
                block_cx,
            ));

            block.on_math_structure_key_down(&math_key_event("escape"), window, block_cx);
            assert!(block.math_edit_session.is_none());
            assert!(block.focus_handle.is_focused(window));
            assert_eq!(block.record.raw_fallback.as_deref(), Some("$$\n+1x^2\n$$"));
        });
    });
}

#[gpui::test]
async fn block_math_blur_ends_the_session_and_preserves_live_source(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::math("$$\nx^2\n$$")));

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.focus_handle.focus(window);
            block.sync_math_edit_focus(true, window, block_cx);
            assert!(block.execute_math_command_live(
                gmark_math_edit::MathEditCommand::InsertText("+1".to_owned()),
                UndoCaptureKind::CoalescibleText,
                block_cx,
            ));

            assert!(!block.sync_math_edit_focus(false, window, block_cx));
            assert!(block.math_edit_session.is_none());
            assert_eq!(block.record.raw_fallback.as_deref(), Some("$$\n+1x^2\n$$"));
        });
    });
}

/// Ensures render-driven blur synchronization stays quiet without an active session while
/// preserving one invalidation for the real transition out of structured formula editing.
#[gpui::test]
async fn block_math_focus_sync_is_idempotent_without_an_active_session(cx: &mut TestAppContext) {
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::math("$$\nx^2\n$$")));
    let notifications = Arc::new(AtomicUsize::new(0));
    let observed_notifications = notifications.clone();
    let _subscription = cx.update(|cx| {
        cx.observe(&block, move |_, _| {
            observed_notifications.fetch_add(1, Ordering::SeqCst);
        })
    });
    let visual = cx.add_empty_window();

    visual.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.sync_math_edit_focus(false, window, block_cx);
            block.sync_math_edit_focus(false, window, block_cx);
        });
    });
    assert_eq!(notifications.load(Ordering::SeqCst), 0);

    visual.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.sync_math_edit_focus(true, window, block_cx);
            assert!(block.math_edit_session.is_some());
            block.sync_math_edit_focus(false, window, block_cx);
        });
    });
    assert_eq!(notifications.load(Ordering::SeqCst), 1);

    visual.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.sync_math_edit_focus(false, window, block_cx);
        });
    });
    assert_eq!(notifications.load(Ordering::SeqCst), 1);
}

#[gpui::test]
async fn math_source_surface_edits_only_the_formula_body_live(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::math("$$\nx^2\n$$")));

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.focus_handle.focus(window);
            block.sync_math_edit_focus(true, window, block_cx);
            block.math_source_focus_handle.focus(window);
            assert_eq!(block.math_source_text(), "x^2");

            assert!(block.replace_math_source_text_in_range(
                0..3,
                "y+1",
                None,
                false,
                UndoCaptureKind::CoalescibleText,
                block_cx,
            ));
            assert_eq!(block.record.raw_fallback.as_deref(), Some("$$\ny+1\n$$"));
            assert_eq!(block.math_source_selection().0, 3..3);
        });
    });
}

#[gpui::test]
async fn inline_math_source_growth_and_shrink_keep_the_active_range_aligned(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph("a $x$ b")));

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.focus_handle.focus(window);
            block.begin_inline_math_edit("$x$", 2..5, window);
            block.math_source_focus_handle.focus(window);
            assert_eq!(block.math_source_text(), "x");
            assert!(block.replace_math_source_text_in_range(
                0..1,
                "yz",
                None,
                false,
                UndoCaptureKind::CoalescibleText,
                block_cx,
            ));
            assert_eq!(block.math_source_text(), "yz");
            assert_eq!(block.math_edit_inline_range, Some(2..6));

            assert!(block.replace_math_source_text_in_range(
                0..1,
                "",
                None,
                false,
                UndoCaptureKind::CoalescibleText,
                block_cx,
            ));
            assert_eq!(block.math_source_text(), "z");
            assert_eq!(block.math_edit_inline_range, Some(2..5));
        });
    });
}

#[gpui::test]
async fn math_source_drag_selects_the_latex_body(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::math("$$\nxy\n$$")));
    let drag = gpui::MouseMoveEvent {
        position: gpui::point(gpui::px(200.0), gpui::px(20.0)),
        pressed_button: Some(gpui::MouseButton::Left),
        modifiers: gpui::Modifiers::default(),
    };

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.focus_handle.focus(window);
            block.sync_math_edit_focus(true, window, block_cx);
            block.math_source_focus_handle.focus(window);
            block.set_math_source_selection(0..0, false);
            block.math_source_is_selecting = true;

            block.on_math_source_mouse_move(&drag, window, block_cx);

            assert!(block.math_source_is_selecting);
            assert_eq!(block.math_source_selection(), (0..2, false));
            assert_eq!(block.math_source_text(), "xy");
        });
    });
}

#[gpui::test]
async fn empty_display_math_source_inserts_between_the_preserved_lines(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::math("$$\n\n$$")));

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.focus_handle.focus(window);
            block.sync_math_edit_focus(true, window, block_cx);
            block.math_source_focus_handle.focus(window);
            assert_eq!(block.math_source_text(), "");

            assert!(block.replace_math_source_text_in_range(
                0..0,
                "x",
                None,
                false,
                UndoCaptureKind::CoalescibleText,
                block_cx,
            ));

            assert_eq!(block.record.raw_fallback.as_deref(), Some("$$\nx\n$$"));
            assert_eq!(block.math_source_text(), "x");
        });
    });
}

#[gpui::test]
async fn math_delete_actions_reach_source_and_two_dimensional_focus(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::math("$$\nxy\n$$")));

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.focus_handle.focus(window);
            block.sync_math_edit_focus(true, window, block_cx);
            let cursor = gmark_math_edit::MathCursor2D::at(
                block
                    .math_edit_session
                    .as_ref()
                    .expect("session")
                    .document(),
                gmark_math_edit::MathSlot::root(),
                2,
            )
            .expect("root cursor");
            block
                .math_edit_session
                .as_mut()
                .expect("session")
                .editor_mut()
                .set_cursor(cursor)
                .expect("set cursor");
            block.math_structure_focus_handle.focus(window);
            block.on_math_structure_delete_back(&DeleteBack, window, block_cx);
            assert_eq!(block.record.raw_fallback.as_deref(), Some("$$\nx\n$$"));

            block.math_source_focus_handle.focus(window);
            let source = block.math_source_text();
            let after_x = source.find('x').expect("x") + 1;
            block.set_math_source_selection(after_x..after_x, false);
            block.on_math_source_delete_back(&DeleteBack, window, block_cx);
            assert_eq!(block.record.raw_fallback.as_deref(), Some("$$\n\n$$"));
        });
    });
}
