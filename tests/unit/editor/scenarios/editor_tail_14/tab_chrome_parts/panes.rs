// @author kongweiguang

async fn zed_style_pane_controls_keep_recursive_splits_visible_and_closable(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "# panes\n\nBody\n".to_owned(), None)
    });
    visual.simulate_resize(size(px(1080.0), px(720.0)));
    redraw(visual);

    assert!(
        visual.debug_bounds("document-toolbar-action-4").is_none(),
        "global Tab must not render the ZoomPane/maximize action"
    );
    assert!(visual.debug_bounds("document-tab-open-bottom-0").is_none());

    // Existing global tabs are part of the compatibility contract. Creating
    // two before the first split verifies that pane promotion migrates them into
    // the root pane instead of dropping it or leaving a second global strip.
    editor.update(visual, |editor, cx| {
        assert!(editor.new_untitled_tab(cx));
        assert!(editor.new_untitled_tab(cx));
        assert_eq!(editor.tabs.records.len(), 3);
    });
    visual.run_until_parked();
    redraw(visual);

    // The first split remains reachable from the global trailing controls.
    let global_split = visual
        .debug_bounds("document-toolbar-action-0")
        .expect("global split control");
    visual.simulate_click(global_split.center(), Modifiers::default());
    redraw(visual);
    for selector in [
        "pane-split-right",
        "pane-split-left",
        "pane-split-up",
        "pane-split-down",
        "pane-close-current",
    ] {
        assert!(
            visual.debug_bounds(selector).is_some(),
            "split popover must expose {selector}"
        );
    }
    let split_right = visual
        .debug_bounds("pane-split-right")
        .expect("initial right split action");
    visual.simulate_click(split_right.center(), Modifiers::default());
    visual.run_until_parked();
    for _ in 0..3 {
        redraw(visual);
    }

    let editor_content = visual.debug_bounds("editor-content").unwrap();
    let initial_shell = visual
        .debug_bounds("pane-shell")
        .expect("split panes expose pane shells");
    let initial_header = visual
        .debug_bounds("pane-tab-bar")
        .expect("every pane exposes a fixed-height local tab bar");
    let initial_header_height = initial_header.size.height;
    assert!(f32::from(initial_header_height) > 0.0);
    assert!(initial_header.top() >= initial_shell.top());
    assert!(initial_header.bottom() <= initial_shell.bottom());
    assert!(initial_header.bottom() <= editor_content.bottom());
    for selector in ["pane-tab-active", "pane-tab-new", "pane-tab-split"] {
        let control = visual
            .debug_bounds(selector)
            .expect("focused pane exposes local tab controls");
        assert!(control.top() >= initial_header.top());
        assert!(control.bottom() <= initial_header.bottom());
    }
    let active_pane_tab = visual.debug_bounds("pane-tab-active").unwrap();
    assert_eq!(active_pane_tab.bottom(), initial_header.bottom());
    assert_eq!(active_pane_tab.top(), initial_header.top());
    assert_eq!(active_pane_tab.size.height, initial_header.size.height);
    let left_shoulder = visual
        .debug_bounds("pane-tab-active-bottom-curve-left")
        .expect("active pane tabs keep the left 8px shoulder");
    let right_shoulder = visual
        .debug_bounds("pane-tab-active-bottom-curve-right")
        .expect("active pane tabs keep the right 8px shoulder");
    for shoulder in [left_shoulder, right_shoulder] {
        assert_eq!(shoulder.size, size(px(8.0), px(8.0)));
        assert_eq!(shoulder.bottom(), initial_header.bottom());
        assert!(shoulder.top() >= initial_header.top());
    }
    assert_eq!(left_shoulder.right(), active_pane_tab.left());
    assert_eq!(right_shoulder.left(), active_pane_tab.right());
    let inactive_separator = visual
        .debug_bounds("pane-tab-inactive-separator-2")
        .expect("two adjacent inactive pane tabs keep a short separator");
    assert_eq!(inactive_separator.size, size(px(1.0), px(16.0)));
    assert_eq!(inactive_separator.center().y, initial_header.center().y);
    assert!(
        visual
            .debug_bounds("pane-tab-inactive-separator-1")
            .is_none(),
        "the active pane tab shoulder replaces a straight adjacent separator"
    );
    let divider = visual
        .debug_bounds("pane-divider")
        .expect("horizontal pane split exposes its divider hit area");
    let divider_tab_fill = visual
        .debug_bounds("pane-divider-tab-bar-fill")
        .expect("divider hit area continues the pane tab-bar surface");
    assert_eq!(divider_tab_fill.left(), divider.left());
    assert_eq!(divider_tab_fill.right(), divider.right());
    assert_eq!(divider_tab_fill.top(), divider.top());
    assert_eq!(divider_tab_fill.size.height, initial_header_height);
    assert_eq!(divider_tab_fill.bottom(), initial_header.bottom());
    editor.read_with(visual, |editor, cx| {
        let workspace = editor
            .pane_workspace
            .as_ref()
            .expect("pane workspace after split")
            .read(cx);
        assert_eq!(workspace.workspace().pane_count(), 2);
        assert!(
            workspace
                .workspace()
                .pane_states()
                .any(|(_, pane)| pane.tabs().len() == 3),
            "the pre-split legacy tabs must migrate into one pane-local tab bar"
        );
        assert_eq!(editor.tabs.records.len(), 1);
    });
    let cleared_global_strip = visual
        .debug_bounds("document-tab-strip-cleared")
        .expect("zero-height invalidation node after pane promotion");
    assert_eq!(cleared_global_strip.size.height, px(0.0));
    assert_eq!(initial_shell.top(), editor_content.top());
    editor.read_with(visual, |editor, _cx| {
        assert_eq!(editor.tab_strip_height(), 0.0);
    });
    assert!(visual.debug_bounds("pane-tab-inactive").is_some());

    // The pane-local + must add a second tab to the focused pane instead of
    // creating an invisible legacy/global tab.
    let pane_new = visual
        .debug_bounds("pane-tab-new")
        .expect("focused pane exposes local new-tab control");
    visual.simulate_click(pane_new.center(), Modifiers::default());
    redraw(visual);
    let new_markdown = visual
        .debug_bounds("new-tab-markdown")
        .expect("pane-local new-tab menu");
    visual.simulate_click(new_markdown.center(), Modifiers::default());
    visual.run_until_parked();
    for _ in 0..2 {
        redraw(visual);
    }
    editor.read_with(visual, |editor, cx| {
        let workspace = editor.pane_workspace.as_ref().unwrap().read(cx);
        let focused = workspace.workspace().focused_pane();
        let pane = workspace.workspace().pane(focused).unwrap();
        assert_eq!(pane.tabs().len(), 2);
        assert_eq!(
            pane.active_tab().unwrap().view().display_title(),
            "Untitled.md"
        );
        assert_eq!(editor.tabs.records.len(), 1);
    });
    assert!(visual.debug_bounds("pane-tab-inactive").is_some());

    // Clicking a pane before opening its local split control must target that
    // pane, while opening/closing the menu must not move its shell bounds.
    let click_left_pane = point(
        editor_content.left() + editor_content.size.width * 0.25,
        editor_content.top() + px(120.0),
    );
    visual.simulate_click(click_left_pane, Modifiers::default());
    visual.run_until_parked();
    redraw(visual);
    let shell_before_menu = visual.debug_bounds("pane-shell").unwrap();
    assert_eq!(
        shell_before_menu, initial_shell,
        "focusing a pane must not move or resize its pane-view shell"
    );
    let pane_split = visual
        .debug_bounds("pane-tab-split")
        .expect("clicked pane exposes local split control");
    visual.simulate_click(pane_split.center(), Modifiers::default());
    redraw(visual);
    for selector in [
        "pane-split-right",
        "pane-split-left",
        "pane-split-up",
        "pane-split-down",
        "pane-close-current",
    ] {
        assert!(
            visual.debug_bounds(selector).is_some(),
            "pane-local split menu must expose {selector}"
        );
    }
    visual.run_until_parked();
    redraw(visual);
    visual.update(|window, cx| {
        assert!(
            editor.read(cx).split_pane_menu_is_focused_for_test(window),
            "split menu must own keyboard focus before handling Escape"
        );
    });
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_dismiss_transient_ui(&crate::components::DismissTransientUi, window, cx);
        });
    });
    assert!(!editor.read_with(visual, |editor, _cx| {
        editor.has_new_or_split_menu_for_test()
    }));
    visual.run_until_parked();
    for _ in 0..2 {
        redraw(visual);
    }
    assert_eq!(
        visual.debug_bounds("pane-shell").unwrap(),
        shell_before_menu,
        "dismissing the menu must not move or resize its pane-view shell"
    );
    let pane_split = visual
        .debug_bounds("pane-tab-split")
        .expect("pane-local split control after Escape");
    visual.simulate_click(pane_split.center(), Modifiers::default());
    redraw(visual);
    assert_eq!(
        visual.debug_bounds("pane-shell").unwrap(),
        shell_before_menu
    );

    let ancestor_ratio_before_down = editor.read_with(visual, |editor, cx| {
        editor
            .pane_workspace
            .as_ref()
            .unwrap()
            .read(cx)
            .workspace()
            .root()
            .ratio()
            .unwrap()
    });

    let split_down = visual
        .debug_bounds("pane-split-down")
        .expect("downward split action");
    visual.simulate_click(split_down.center(), Modifiers::default());
    visual.run_until_parked();
    for _ in 0..3 {
        redraw(visual);
    }
    let header_after_down = visual.debug_bounds("pane-tab-bar").unwrap();
    assert_eq!(header_after_down.size.height, initial_header_height);
    let ancestor_ratio_after_down = editor.read_with(visual, |editor, cx| {
        editor
            .pane_workspace
            .as_ref()
            .unwrap()
            .read(cx)
            .workspace()
            .root()
            .ratio()
            .unwrap()
    });
    assert_eq!(
        ancestor_ratio_after_down, ancestor_ratio_before_down,
        "splitting one pane must not resize its ancestor split"
    );

    let pane_split = visual
        .debug_bounds("pane-tab-split")
        .expect("newly focused pane exposes local split control");
    visual.simulate_click(pane_split.center(), Modifiers::default());
    redraw(visual);
    let split_left = visual
        .debug_bounds("pane-split-left")
        .expect("leftward split action");
    visual.simulate_click(split_left.center(), Modifiers::default());
    visual.run_until_parked();
    for _ in 0..3 {
        redraw(visual);
    }
    let final_header = visual
        .debug_bounds("pane-tab-bar")
        .expect("all visible panes retain local tab headers");
    assert_eq!(final_header.size.height, initial_header_height);
    for selector in ["pane-tab-active", "pane-tab-new", "pane-tab-split"] {
        assert!(visual.debug_bounds(selector).is_some());
    }

    editor.read_with(visual, |editor, cx| {
        let workspace = editor.pane_workspace.as_ref().unwrap().read(cx);
        assert_eq!(workspace.workspace().pane_count(), 4);
        assert_eq!(workspace.layout().visible_count(), 4);
        assert_eq!(workspace.layout().hidden_count(), 0);
        assert_eq!(workspace.layout().dividers().len(), 3);
        assert_eq!(
            workspace.viewport().width,
            f32::from(editor_content.size.width)
        );
        assert_eq!(
            workspace.viewport().height,
            f32::from(editor_content.size.height)
        );
    });
    assert!(visual.debug_bounds("pane-hidden-switcher").is_none());

    let pane_split = visual
        .debug_bounds("pane-tab-split")
        .expect("pane-local split control before close");
    visual.simulate_click(pane_split.center(), Modifiers::default());
    redraw(visual);
    let close = visual
        .debug_bounds("pane-close-current")
        .expect("close current pane item");
    visual.simulate_click(close.center(), Modifiers::default());
    visual.run_until_parked();
    for _ in 0..2 {
        redraw(visual);
    }
    editor.read_with(visual, |editor, cx| {
        let workspace = editor.pane_workspace.as_ref().unwrap().read(cx);
        assert_eq!(workspace.workspace().pane_count(), 3);
        assert_eq!(workspace.layout().visible_count(), 3);
    });
}
