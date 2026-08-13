// @author kongweiguang

fn flush_split_projection(cx: &mut gpui::VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(30));
    cx.run_until_parked();
    redraw(cx);
}

fn activate_visual_window(cx: &mut VisualTestContext) -> AnyWindowHandle {
    cx.update(|window, _cx| window.activate_window());
    cx.run_until_parked();
    cx.cx
        .update(|cx| cx.active_window().expect("window should be active"))
}

#[test]
fn centered_column_ratio_stays_full_before_shrink_start() {
    let theme = Theme::default_theme();
    assert_eq!(
        crate::ui::centered_column_ratio(900.0, &theme.dimensions),
        1.0
    );
    assert_eq!(
        crate::ui::centered_column_ratio(theme.dimensions.centered_shrink_start, &theme.dimensions,),
        1.0
    );
}

#[test]
fn centered_column_ratio_reaches_new_minimum() {
    let theme = Theme::default_theme();
    let ratio =
        crate::ui::centered_column_ratio(theme.dimensions.centered_shrink_end, &theme.dimensions);
    assert!((ratio - 0.58).abs() < f32::EPSILON);
}

#[test]
fn centered_column_width_caps_wide_viewports_and_yields_to_compact_space() {
    let mut theme = Theme::default_theme();
    theme.dimensions.centered_max_width = 880.0;
    assert_eq!(
        crate::ui::centered_column_width(1600.0, &theme.dimensions),
        880.0
    );
    assert_eq!(
        crate::ui::centered_column_width(720.0, &theme.dimensions),
        720.0 - theme.dimensions.editor_padding * 2.0
    );
}

#[test]
/// 锁定四种模式的 Source 基准，防止后续为阅读视图单独加回额外顶距。
fn document_modes_share_source_top_padding_and_keep_typewriter_target() {
    let theme = Theme::default_theme();
    let top_padding = |view_mode, typewriter_mode, viewport_height| {
        super::render::editor_top_padding(
            view_mode,
            typewriter_mode,
            viewport_height,
            &theme.dimensions,
        )
    };
    for view_mode in [
        ViewMode::Source,
        ViewMode::Rendered,
        ViewMode::Preview,
        ViewMode::Split,
    ] {
        assert_eq!(
            top_padding(view_mode, false, 700.0),
            theme.dimensions.editor_padding
        );
    }
    assert_eq!(top_padding(ViewMode::Rendered, true, 700.0), 315.0);
    assert_eq!(
        top_padding(ViewMode::Preview, true, 700.0),
        theme.dimensions.editor_padding
    );
    assert_eq!(top_padding(ViewMode::Rendered, true, 80.0), 48.0);
    assert_eq!(
        super::render::editor_bottom_padding(700.0, &theme.dimensions),
        theme.dimensions.editor_padding
            + (theme.dimensions.block_min_height * 0.75).max(16.0)
            + 350.0
    );
}

#[test]
fn scrollbar_geometry_and_inverse_mapping_stay_aligned() {
    let geometry = Editor::scrollbar_geometry(400.0, 600.0, 300.0);
    assert_eq!(geometry.track_height, 400.0);
    assert!(geometry.thumb_height >= 28.0);
    assert!((geometry.thumb_top - (400.0 - geometry.thumb_height) * 0.5).abs() < 0.001);

    let scroll_y = Editor::scroll_offset_for_thumb_top(
        geometry.thumb_top,
        geometry.track_height,
        geometry.thumb_height,
        geometry.max_scroll_y,
    );
    assert!((scroll_y - 300.0).abs() < 0.001);
}

#[test]
fn scrollbar_offset_mapping_clamps_to_track_bounds() {
    let geometry = Editor::scrollbar_geometry(300.0, 450.0, 0.0);
    assert_eq!(
        Editor::scroll_offset_for_thumb_top(
            -25.0,
            geometry.track_height,
            geometry.thumb_height,
            geometry.max_scroll_y,
        ),
        0.0
    );
    assert_eq!(
        Editor::scroll_offset_for_thumb_top(
            999.0,
            geometry.track_height,
            geometry.thumb_height,
            geometry.max_scroll_y,
        ),
        geometry.max_scroll_y
    );
}
