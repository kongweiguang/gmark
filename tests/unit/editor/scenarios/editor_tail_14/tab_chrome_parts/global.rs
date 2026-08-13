// @author kongweiguang

#[gpui::test]
async fn tab_strip_keeps_only_optional_document_actions(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    cx.update(|cx| {
        crate::config::EditorSettings::init(cx, true, crate::config::AutoSavePreference::Off, true);
        crate::config::EditorSettings::set_show_tab_bar_actions_for_test(cx, true);
    });
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "# gmark\n\nBody\n".to_owned(), None)
    });
    let source = editor.read_with(visual, |editor, _cx| editor.source_document.text());
    let revision = editor.read_with(visual, |editor, _cx| editor.source_document.revision());

    for viewport in [size(px(720.0), px(520.0)), size(px(1180.0), px(780.0))] {
        visual.simulate_resize(viewport);
        redraw(visual);
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
        let toolbar = visual.debug_bounds("document-tab-strip").unwrap();
        let content = visual.debug_bounds("editor-content").unwrap();
        let tab_scroll = visual.debug_bounds("document-tab-scroll").unwrap();
        let active_tab = visual.debug_bounds("document-tab-0").unwrap();
        let new_tab = visual.debug_bounds("document-new-tab").unwrap();
        let trailing_tools = visual.debug_bounds("document-tab-trailing-tools").unwrap();

        assert_eq!(f32::from(toolbar.size.height), 36.0);
        assert_eq!(toolbar.bottom(), content.top());
        assert_eq!(trailing_tools.right(), toolbar.right());
        assert_eq!(tab_scroll.left(), toolbar.left());
        assert!(active_tab.left() >= tab_scroll.left());
        assert!(active_tab.right() <= tab_scroll.right());
        assert!(
            visual.debug_bounds("document-tab-open-bottom-0").is_none(),
            "active document tabs must not extend a line below the tab strip"
        );
        assert_eq!(active_tab.bottom(), toolbar.bottom());
        assert_eq!(active_tab.top(), toolbar.top());
        assert_eq!(active_tab.size.height, toolbar.size.height);
        let left_shoulder = visual
            .debug_bounds("document-tab-active-bottom-curve-left-0")
            .expect("active document tabs keep the left 8px shoulder");
        let right_shoulder = visual
            .debug_bounds("document-tab-active-bottom-curve-right-0")
            .expect("active document tabs keep the right 8px shoulder");
        for shoulder in [left_shoulder, right_shoulder] {
            assert_eq!(shoulder.size, size(px(8.0), px(8.0)));
            assert_eq!(shoulder.bottom(), toolbar.bottom());
            assert!(shoulder.top() >= toolbar.top());
        }
        assert_eq!(left_shoulder.right(), active_tab.left());
        assert_eq!(right_shoulder.left(), active_tab.right());
        assert!(new_tab.left() >= trailing_tools.left());
        assert!(new_tab.right() <= trailing_tools.right());
        for selector in [
            "document-toolbar-action-0",
            "document-toolbar-action-1",
            "document-toolbar-action-2",
            "document-toolbar-action-3",
        ] {
            let action = visual.debug_bounds(selector).unwrap();
            assert_eq!(action.size, size(px(28.0), px(28.0)));
            assert!(action.left() >= toolbar.left());
            assert!(action.right() <= toolbar.right());
            assert!(action.top() >= toolbar.top());
            assert!(action.bottom() <= toolbar.bottom());
        }
        for selector in [
            "document-toolbar-action-0",
            "document-toolbar-action-1",
            "document-toolbar-action-2",
            "document-toolbar-action-3",
        ] {
            let action = visual.debug_bounds(selector).unwrap();
            assert!(action.left() >= trailing_tools.left());
            assert!(action.right() <= trailing_tools.right());
        }
    }

    let find = visual.debug_bounds("document-toolbar-action-2").unwrap();
    visual.simulate_click(find.center(), Modifiers::default());
    visual.run_until_parked();
    assert!(editor.read_with(visual, |editor, _cx| editor.find_panel.is_some()));
    assert_eq!(
        editor.read_with(visual, |editor, _cx| editor.source_document.text()),
        source
    );
    assert_eq!(
        editor.read_with(visual, |editor, _cx| editor.source_document.revision()),
        revision
    );
    assert!(!editor.read_with(visual, |editor, _cx| editor.document_dirty));

    editor.update(visual, |editor, cx| {
        assert!(editor.new_untitled_tab(cx));
        assert!(editor.new_untitled_tab(cx));
    });
    visual.run_until_parked();
    redraw(visual);
    let strip = visual.debug_bounds("document-tab-strip").unwrap();
    let first_tab = visual.debug_bounds("document-tab-0").unwrap();
    let second_tab = visual.debug_bounds("document-tab-1").unwrap();
    let separator = visual
        .debug_bounds("document-tab-inactive-separator-1")
        .expect("two adjacent inactive document tabs keep a short separator");
    assert_eq!(separator.size, size(px(1.0), px(16.0)));
    assert_eq!(second_tab.left() - first_tab.right(), px(4.0));
    assert_eq!(separator.left(), second_tab.left() - px(2.0));
    assert_eq!(separator.center().y, strip.center().y);
    assert!(
        visual
            .debug_bounds("document-tab-inactive-separator-2")
            .is_none(),
        "the active tab shoulder replaces a straight adjacent separator"
    );
}

#[gpui::test]
async fn tab_strip_defaults_to_clean_document_chrome(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (_editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "# gmark\n".to_owned(), None));
    visual.simulate_resize(size(px(720.0), px(520.0)));
    redraw(visual);

    assert!(visual.debug_bounds("document-tab-0").is_some());
    assert!(visual.debug_bounds("document-new-tab").is_some());
    assert!(visual.debug_bounds("document-toolbar-action-0").is_some());
    assert!(visual.debug_bounds("document-toolbar-action-1").is_none());
    assert!(visual.debug_bounds("document-toolbar-action-2").is_none());
    assert!(visual.debug_bounds("document-toolbar-action-3").is_none());
    assert!(
        visual.debug_bounds("document-toolbar-action-4").is_none(),
        "ZoomPane must not be rendered in the global document tab strip"
    );
    assert!(visual.debug_bounds("document-tab-open-bottom-0").is_none());
    assert!(visual.debug_bounds("document-tab-leading-tools").is_none());
    assert!(visual.debug_bounds("document-tab-trailing-tools").is_some());
}
