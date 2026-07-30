// @author kongweiguang

#[test]
fn smooth_scroll_curve_is_bounded_monotonic_and_eases_out() {
    let samples = [0, 20, 60, 100, 140, 220]
        .map(|millis| Editor::smooth_scroll_progress(Duration::from_millis(millis)));
    assert_eq!(samples[0], 0.0);
    assert_eq!(samples[4], 1.0);
    assert_eq!(samples[5], 1.0);
    assert!(samples.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(
        samples[2] > 0.7,
        "ease-out should move decisively without feeling delayed"
    );
}

#[gpui::test]
async fn line_wheel_scrolls_through_intermediate_offsets_before_landing(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let source = "A comfortable scrolling paragraph.\n\n".repeat(120);
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_markdown(cx, source, None));
    visual.simulate_resize(size(px(760.0), px(520.0)));
    redraw(visual);
    let surface_center = point(px(380.0), px(260.0));
    editor.read_with(visual, |editor, _cx| {
        assert!(editor.scroll_handle.max_offset().height > px(0.0));
        assert_eq!(editor.scroll_handle.offset().y, px(0.0));
    });

    visual.simulate_event(gpui::ScrollWheelEvent {
        position: surface_center,
        delta: gpui::ScrollDelta::Lines(point(0.0, -3.0)),
        ..Default::default()
    });
    let target = editor.read_with(visual, |editor, _cx| {
        assert_eq!(
            editor.scroll_handle.offset().y,
            px(0.0),
            "the native line jump must be restored before the frame is painted"
        );
        editor
            .smooth_scroll_animation
            .expect("line wheel starts smooth scroll")
            .target_y
    });
    assert!(target < px(0.0));

    visual.executor().advance_clock(Duration::from_millis(40));
    visual.run_until_parked();
    let intermediate = editor.read_with(visual, |editor, _cx| editor.scroll_handle.offset().y);
    assert!(intermediate < px(0.0));
    assert!(intermediate > target);

    visual.executor().advance_clock(Duration::from_millis(180));
    visual.run_until_parked();
    editor.read_with(visual, |editor, _cx| {
        assert!((f32::from(editor.scroll_handle.offset().y - target)).abs() < 0.01);
        assert!(editor.smooth_scroll_animation.is_none());
        assert!(editor.smooth_scroll_task.is_none());
    });
}

#[gpui::test]
async fn repeated_line_wheels_merge_targets_and_pixel_input_takes_over(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let source = "Continuous wheel input should merge smoothly.\n\n".repeat(120);
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_markdown(cx, source, None));
    visual.simulate_resize(size(px(760.0), px(520.0)));
    redraw(visual);
    let surface_center = point(px(380.0), px(260.0));
    let line_event = gpui::ScrollWheelEvent {
        position: surface_center,
        delta: gpui::ScrollDelta::Lines(point(0.0, -3.0)),
        ..Default::default()
    };

    visual.simulate_event(line_event.clone());
    let first_target = editor.read_with(visual, |editor, _cx| {
        editor
            .smooth_scroll_animation
            .expect("first target")
            .target_y
    });
    visual.executor().advance_clock(Duration::from_millis(32));
    visual.run_until_parked();
    let before_second = editor.read_with(visual, |editor, _cx| editor.scroll_handle.offset().y);

    visual.simulate_event(line_event);
    editor.read_with(visual, |editor, _cx| {
        let animation = editor
            .smooth_scroll_animation
            .expect("second wheel retargets the active animation");
        assert!(animation.target_y < first_target);
        assert!((f32::from(editor.scroll_handle.offset().y - before_second)).abs() < 0.01);
    });

    visual.simulate_event(gpui::ScrollWheelEvent {
        position: surface_center,
        delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-17.0))),
        ..Default::default()
    });
    editor.read_with(visual, |editor, _cx| {
        assert!(editor.smooth_scroll_animation.is_none());
        assert!(editor.smooth_scroll_task.is_none());
        assert!((f32::from(editor.scroll_handle.offset().y - before_second + px(17.0))).abs() < 0.01);
    });
}
