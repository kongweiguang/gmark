// @author kongweiguang

#[gpui::test]
async fn status_bar_line_ending_picker_normalizes_the_document(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        let mut editor = Editor::from_markdown(cx, "alpha\nbeta\n".to_owned(), None);
        editor.set_view_mode(ViewMode::Source, cx);
        editor
    });
    visual.simulate_resize(size(px(960.0), px(640.0)));
    redraw(visual);

    let picker = visual
        .debug_bounds("status-bar-line-ending-button")
        .expect("line-ending picker");
    visual.simulate_click(picker.center(), Modifiers::default());
    redraw(visual);
    let menu = visual
        .debug_bounds("status-bar-line-ending-menu")
        .expect("line-ending menu");
    assert_eq!(f32::from(menu.size.width), 92.0);
    assert_eq!(menu.right(), picker.right());

    let crlf = visual
        .debug_bounds("status-bar-line-ending-crlf")
        .expect("CRLF menu item");
    visual.simulate_click(crlf.center(), Modifiers::default());
    visual.run_until_parked();
    redraw(visual);

    assert_eq!(
        editor.read_with(visual, |editor, _cx| editor
            .source_document
            .serialized_bytes()),
        b"alpha\r\nbeta\r\n"
    );
    assert!(editor.read_with(visual, |editor, _cx| !editor
        .status_bar
        .line_ending_menu_open));
}
