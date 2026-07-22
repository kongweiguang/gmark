// @author kongweiguang

    #[gpui::test]
    async fn preferences_switches_share_stable_mouse_and_keyboard_focus(cx: &mut TestAppContext) {
        init_preferences_test_app(cx);
        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "Preferences".into(),
            )
        });
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_resize(size(px(720.0), px(520.0)));
        visual.update(|window, cx| window.draw(cx).clear());

        handle
            .update(&mut visual, |preferences, window, _cx| {
                preferences.switch_focus_handles[PreferencesSwitch::SpellCheck.index()]
                    .focus(window);
            })
            .unwrap();
        visual.simulate_keystrokes("space");
        visual.run_until_parked();
        handle
            .update(&mut visual, |preferences, window, cx| {
                assert!(!preferences.spell_check);
                assert!(preferences.switch_focus_handles[0].is_focused(window));
                assert!(preferences.has_unsaved_changes());
                preferences.select_nav(PreferencesNav::Editor, cx);
                preferences.switch_focus_handles[PreferencesSwitch::AutoPairBrackets.index()]
                    .focus(window);
            })
            .unwrap();
        visual.update(|window, cx| window.draw(cx).clear());

        visual.simulate_keystrokes("space");
        visual.run_until_parked();
        handle
            .update(&mut visual, |preferences, window, _cx| {
                assert!(!preferences.auto_pair_brackets);
                assert!(
                    preferences.switch_focus_handles[PreferencesSwitch::AutoPairBrackets.index()]
                        .is_focused(window)
                );
                preferences.switch_focus_handles[PreferencesSwitch::AutoPairMarkdown.index()]
                    .focus(window);
            })
            .unwrap();
        visual.simulate_keystrokes("enter");
        visual.run_until_parked();
        handle
            .update(&mut visual, |preferences, window, _cx| {
                assert!(!preferences.auto_pair_markdown);
                assert!(
                    preferences.switch_focus_handles[PreferencesSwitch::AutoPairMarkdown.index()]
                        .is_focused(window)
                );
                preferences.switch_focus_handles[PreferencesSwitch::WorkspaceSidebarRight.index()]
                    .focus(window);
            })
            .unwrap();

        visual.simulate_keystrokes("space");
        visual.run_until_parked();
        handle
            .update(&mut visual, |preferences, window, _cx| {
                assert_eq!(
                    preferences.workspace_sidebar_position,
                    WorkspaceSidebarPosition::Right
                );
                assert!(
                    preferences.switch_focus_handles
                        [PreferencesSwitch::WorkspaceSidebarRight.index()]
                    .is_focused(window)
                );
                preferences.switch_focus_handles[PreferencesSwitch::ShowTabBarActions.index()]
                    .focus(window);
            })
            .unwrap();
        visual.update(|window, cx| window.draw(cx).clear());

        let tab_actions = visual
            .debug_bounds(PreferencesSwitch::ShowTabBarActions.id())
            .expect("tab-bar actions switch should render");
        assert_eq!(tab_actions.size, size(px(36.0), px(20.0)));
        for preference in [
            PreferencesSwitch::AutoPairBrackets,
            PreferencesSwitch::AutoPairMarkdown,
        ] {
            let bounds = visual
                .debug_bounds(preference.id())
                .unwrap_or_else(|| panic!("missing {}", preference.id()));
            assert_eq!(bounds.size, size(px(36.0), px(20.0)));
        }
        visual.simulate_keystrokes("enter");
        visual.run_until_parked();
        handle
            .update(&mut visual, |preferences, window, cx| {
                assert!(preferences.show_tab_bar_actions);
                assert!(
                    preferences.switch_focus_handles[PreferencesSwitch::ShowTabBarActions.index()]
                        .is_focused(window)
                );
                preferences.select_nav(PreferencesNav::StatusBar, cx);
                preferences.switch_focus_handles[PreferencesSwitch::StatusBarEnabled.index()]
                    .focus(window);
            })
            .unwrap();
        visual.update(|window, cx| window.draw(cx).clear());

        visual.simulate_keystrokes("enter");
        visual.run_until_parked();
        handle
            .update(&mut visual, |preferences, window, _cx| {
                assert!(!preferences.status_bar_enabled);
                assert!(
                    preferences.switch_focus_handles[PreferencesSwitch::StatusBarEnabled.index()]
                        .is_focused(window)
                );
            })
            .unwrap();

        let word_count = visual
            .debug_bounds(PreferencesSwitch::StatusBarWordCount.id())
            .expect("word-count switch should render");
        assert_eq!(word_count.size, size(px(36.0), px(20.0)));
        visual.simulate_click(word_count.center(), Modifiers::default());
        visual.run_until_parked();
        handle
            .update(&mut visual, |preferences, window, _cx| {
                assert!(!preferences.status_bar_show_word_count);
                assert!(
                    preferences.switch_focus_handles[PreferencesSwitch::StatusBarWordCount.index()]
                        .is_focused(window)
                );
            })
            .unwrap();

        for viewport in [size(px(720.0), px(520.0)), size(px(980.0), px(700.0))] {
            visual.simulate_resize(viewport);
            visual.update(|window, cx| {
                assert_eq!(window.scale_factor(), 2.0);
                window.draw(cx).clear();
            });
            visual.run_until_parked();
            let page = visual.debug_bounds("preferences-page-scroll").unwrap();
            for preference in [
                PreferencesSwitch::StatusBarEnabled,
                PreferencesSwitch::StatusBarWordCount,
                PreferencesSwitch::StatusBarCursorPosition,
                PreferencesSwitch::StatusBarSidebarToggle,
                PreferencesSwitch::StatusBarModeSwitch,
            ] {
                let bounds = visual
                    .debug_bounds(preference.id())
                    .unwrap_or_else(|| panic!("missing {}", preference.id()));
                assert_eq!(bounds.size, size(px(36.0), px(20.0)));
                assert!(bounds.left() >= page.left());
                assert!(bounds.right() <= page.right());
                assert!(bounds.top() >= page.top());
                assert!(bounds.bottom() <= page.bottom());
            }
        }
    }

    #[gpui::test]
    async fn preferences_search_matches_unicode_categories_and_shortcuts(cx: &mut TestAppContext) {
        cx.update(|cx| {
            I18nManager::init_with_language_id(cx, "zh-CN");
            ThemeManager::init_with_theme_id(cx, "gmark");
            crate::components::init(cx);
            EditorSettings::init(cx, true, AutoSavePreference::Off, true);
        });
        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "偏好设置".into(),
            )
        });
        cx.run_until_parked();

        handle
            .update(cx, |preferences, _window, cx| {
                let input = preferences.search_input.clone();
                input.update(cx, |input, cx| {
                    input.replace_text_in_visible_range(0..0, "状态栏", None, false, cx);
                });
                let strings = cx.global::<I18nManager>().strings();
                let results = preferences.preference_search_results(strings, cx);
                assert_eq!(results.len(), 5);
                assert!(
                    results
                        .iter()
                        .all(|result| result.nav == PreferencesNav::StatusBar)
                );

                let input = preferences.search_input.clone();
                input.update(cx, |input, cx| {
                    let len = input.visible_len();
                    input.replace_text_in_visible_range(0..len, "编辑器", None, false, cx);
                });
                let strings = cx.global::<I18nManager>().strings();
                let results = preferences.preference_search_results(strings, cx);
                assert_eq!(results.len(), 8);
                assert!(
                    results
                        .iter()
                        .all(|result| result.nav == PreferencesNav::Editor)
                );

                let input = preferences.search_input.clone();
                input.update(cx, |input, cx| {
                    let len = input.visible_len();
                    input.replace_text_in_visible_range(0..len, "侧栏", None, false, cx);
                });
                let strings = cx.global::<I18nManager>().strings();
                let results = preferences.preference_search_results(strings, cx);
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].nav, PreferencesNav::Editor);

                let input = preferences.search_input.clone();
                input.update(cx, |input, cx| {
                    let len = input.visible_len();
                    input.replace_text_in_visible_range(0..len, "快捷操作", None, false, cx);
                });
                let strings = cx.global::<I18nManager>().strings();
                let results = preferences.preference_search_results(strings, cx);
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].nav, PreferencesNav::Editor);

                let input = preferences.search_input.clone();
                input.update(cx, |input, cx| {
                    let len = input.visible_len();
                    input.replace_text_in_visible_range(0..len, "保存", None, false, cx);
                });
                let strings = cx.global::<I18nManager>().strings();
                let results = preferences.preference_search_results(strings, cx);
                assert!(
                    results
                        .iter()
                        .any(|result| result.nav == PreferencesNav::Shortcuts)
                );
            })
            .expect("preferences window should be updateable");
    }

    #[gpui::test]
    async fn preferences_search_navigates_and_stays_inside_compact_windows(
        cx: &mut TestAppContext,
    ) {
        init_preferences_test_app(cx);
        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "Preferences".into(),
            )
        });
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(handle.into(), cx);

        for viewport in [size(px(720.0), px(520.0)), size(px(980.0), px(700.0))] {
            visual.simulate_resize(viewport);
            handle
                .update(&mut visual, |preferences, window, cx| {
                    let input = preferences.search_input.clone();
                    input.update(cx, |input, cx| {
                        let len = input.visible_len();
                        input.replace_text_in_visible_range(0..len, "theme", None, false, cx);
                    });
                    input.read(cx).focus_handle.focus(window);
                })
                .unwrap();
            visual.update(|window, cx| window.draw(cx).clear());
            visual.run_until_parked();

            let content = visual.debug_bounds("preferences-content").unwrap();
            let search = visual.debug_bounds("preferences-search-input").unwrap();
            let clear = visual.debug_bounds("preferences-search-clear").unwrap();
            let results = visual.debug_bounds("preferences-search-results").unwrap();
            let first = visual.debug_bounds("preferences-search-result-0").unwrap();
            for (name, bounds) in [
                ("search", search),
                ("clear", clear),
                ("results", results),
                ("first result", first),
            ] {
                assert!(bounds.left() >= content.left(), "{name} escaped left");
                assert!(bounds.right() <= content.right(), "{name} escaped right");
                assert!(bounds.top() >= content.top(), "{name} escaped top");
                assert!(bounds.bottom() <= content.bottom(), "{name} escaped bottom");
            }
            assert_eq!(f32::from(first.size.height), 40.0);

            visual.simulate_click(first.center(), Modifiers::default());
            visual.run_until_parked();
            handle
                .update(&mut visual, |preferences, _window, cx| {
                    assert_eq!(preferences.nav, PreferencesNav::Theme);
                    assert!(preferences.search_query(cx).is_empty());
                })
                .unwrap();
        }

        handle
            .update(&mut visual, |preferences, window, cx| {
                let input = preferences.search_input.clone();
                input.update(cx, |input, cx| {
                    input.replace_text_in_visible_range(0..0, "status", None, false, cx);
                });
                input.read(cx).focus_handle.focus(window);
            })
            .unwrap();
        visual.run_until_parked();
        handle
            .update(&mut visual, |preferences, window, cx| {
                let down = KeyDownEvent {
                    keystroke: Keystroke::parse("down").expect("valid down keystroke"),
                    is_held: false,
                };
                preferences.capture_shortcut_key(&down, window, cx);
                assert_eq!(preferences.search_selected, 1);
                let escape = KeyDownEvent {
                    keystroke: Keystroke::parse("escape").expect("valid escape keystroke"),
                    is_held: false,
                };
                preferences.capture_shortcut_key(&escape, window, cx);
                assert!(preferences.search_query(cx).is_empty());
                assert_eq!(preferences.search_selected, 0);
            })
            .unwrap();

        handle
            .update(&mut visual, |preferences, window, cx| {
                let input = preferences.search_input.clone();
                input.update(cx, |input, cx| {
                    input.replace_text_in_visible_range(0..0, "image", None, false, cx);
                });
                input.read(cx).focus_handle.focus(window);
            })
            .unwrap();
        visual.run_until_parked();
        handle
            .update(&mut visual, |preferences, window, cx| {
                let enter = KeyDownEvent {
                    keystroke: Keystroke::parse("enter").expect("valid enter keystroke"),
                    is_held: false,
                };
                preferences.capture_shortcut_key(&enter, window, cx);
                assert_eq!(preferences.nav, PreferencesNav::Image);
                assert!(preferences.search_query(cx).is_empty());
            })
            .unwrap();

        handle
            .update(&mut visual, |preferences, _window, cx| {
                let input = preferences.search_input.clone();
                input.update(cx, |input, cx| {
                    input.replace_text_in_visible_range(
                        0..0,
                        "definitely-missing-setting",
                        None,
                        false,
                        cx,
                    );
                });
            })
            .unwrap();
        visual.update(|window, cx| window.draw(cx).clear());
        visual.run_until_parked();
        let content = visual.debug_bounds("preferences-content").unwrap();
        let empty = visual
            .debug_bounds("preferences-search-no-results")
            .unwrap();
        assert!(empty.left() >= content.left());
        assert!(empty.right() <= content.right());
        assert!(empty.top() >= content.top());
        assert!(empty.bottom() <= content.bottom());
    }

    #[gpui::test]
    async fn theme_selection_previews_live_and_unsaved_preview_restores(cx: &mut TestAppContext) {
        init_preferences_test_app(cx);
        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "Preferences".into(),
            )
        });
        cx.run_until_parked();

        handle
            .update(cx, |preferences, _window, cx| {
                preferences.nav = PreferencesNav::Theme;
                preferences.preview_theme("gmark-light".into(), cx);
                assert_eq!(preferences.selected_theme_id, "gmark-light");
                assert!(preferences.has_unsaved_changes());
                assert_eq!(
                    cx.global::<ThemeManager>().current_theme_id(),
                    "gmark-light"
                );
                preferences.theme_dropdown_open = true;
                cx.notify();
            })
            .expect("preferences window should be updateable");
        cx.run_until_parked();

        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_resize(size(px(720.0), px(520.0)));
        visual.update(|window, cx| window.draw(cx).clear());
        visual.run_until_parked();
        let dropdown = visual.debug_bounds("preferences-theme-dropdown").unwrap();
        let swatch = visual.debug_bounds("preferences-theme-swatch").unwrap();
        let selected_icon = visual
            .debug_bounds("preferences-theme-selected-icon")
            .unwrap();
        assert_eq!(f32::from(dropdown.size.width), 280.0);
        assert_eq!(f32::from(dropdown.size.height), 32.0);
        assert_eq!(f32::from(swatch.size.width), 42.0);
        assert_eq!(f32::from(swatch.size.height), 16.0);
        assert!(swatch.left() >= dropdown.left());
        assert!(swatch.right() <= dropdown.right());
        assert!(swatch.top() >= dropdown.top());
        assert!(swatch.bottom() <= dropdown.bottom());
        assert_eq!(f32::from(selected_icon.size.width), 16.0);
        assert_eq!(f32::from(selected_icon.size.height), 16.0);
        assert!(selected_icon.left() >= dropdown.left());
        assert!(selected_icon.right() <= dropdown.right());
        for (row_selector, icon_selector) in [
            (
                "preferences-theme-option-0",
                "preferences-theme-option-icon-0",
            ),
            (
                "preferences-theme-option-1",
                "preferences-theme-option-icon-1",
            ),
            (
                "preferences-theme-option-2",
                "preferences-theme-option-icon-2",
            ),
        ] {
            let row = visual.debug_bounds(row_selector).unwrap();
            let icon = visual.debug_bounds(icon_selector).unwrap();
            assert_eq!(f32::from(icon.size.width), 16.0);
            assert_eq!(f32::from(icon.size.height), 16.0);
            assert!(icon.left() >= row.left());
            assert!(icon.right() <= row.right());
            assert!(icon.top() >= row.top());
            assert!(icon.bottom() <= row.bottom());
        }

        // 第一项与下一行语言控件在几何上重叠；菜单必须最后绘制并接收点击。
        let first_theme_option = visual
            .debug_bounds("preferences-theme-option-0")
            .unwrap();
        visual.simulate_click(first_theme_option.center(), Modifiers::default());
        visual.run_until_parked();
        handle
            .update(&mut visual, |preferences, _window, _cx| {
                assert_eq!(preferences.selected_theme_id, "system");
                assert!(!preferences.theme_dropdown_open);
                assert!(!preferences.language_dropdown_open);
            })
            .expect("preferences window should be readable");

        handle
            .update(&mut visual, |preferences, _window, cx| {
                preferences.restore_saved_theme(cx);
                assert_eq!(preferences.selected_theme_id, "gmark");
                assert!(!preferences.has_unsaved_changes());
                assert_eq!(cx.global::<ThemeManager>().current_theme_id(), "gmark");
            })
            .expect("preferences window should be updateable");
    }

    #[gpui::test]
    async fn follow_system_preview_keeps_selection_identity_and_restores(cx: &mut TestAppContext) {
        init_preferences_test_app(cx);
        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "Preferences".into(),
            )
        });
        cx.run_until_parked();

        handle
            .update(cx, |preferences, _window, cx| {
                preferences.preview_theme("system".into(), cx);
                assert_eq!(preferences.selected_theme_id, "system");
                assert_eq!(cx.global::<ThemeManager>().selected_theme_id(), "system");
                assert_eq!(
                    cx.global::<ThemeManager>().current_theme_id(),
                    "gmark-light"
                );

                preferences.restore_saved_theme(cx);
                assert_eq!(preferences.selected_theme_id, "gmark");
                assert_eq!(cx.global::<ThemeManager>().selected_theme_id(), "gmark");
                assert_eq!(cx.global::<ThemeManager>().current_theme_id(), "gmark");
            })
            .expect("preferences window should be updateable");
    }

    #[gpui::test]
    async fn editor_typography_steppers_preview_and_restore_in_compact_window(
        cx: &mut TestAppContext,
    ) {
        init_preferences_test_app(cx);
        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "Preferences".into(),
            )
        });
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_resize(size(px(720.0), px(520.0)));
        handle
            .update(&mut visual, |preferences, _window, cx| {
                preferences.nav = PreferencesNav::Editor;
                assert!(preferences.search_input.read(cx).compact_source_host());
                assert_eq!(preferences.font_options.first().map(String::as_str), Some(""));
                assert_eq!(
                    preferences.dropdown_current_index(PreferencesDropdown::Font),
                    0
                );
                assert!(preferences.font_options.len() > 1);
                cx.notify();
            })
            .unwrap();
        visual.update(|window, cx| window.draw(cx).clear());
        visual.run_until_parked();

        let page = visual.debug_bounds("preferences-page-scroll").unwrap();
        for selector in [
            "preferences-editor-font-family",
            "preferences-editor-font-size",
            "preferences-editor-font-size-decrease",
            "preferences-editor-font-size-increase",
            "preferences-editor-line-height",
            "preferences-editor-line-height-decrease",
            "preferences-editor-line-height-increase",
            "preferences-editor-content-width",
            "preferences-editor-content-width-decrease",
            "preferences-editor-content-width-increase",
            "preferences-workspace-sidebar-right",
        ] {
            let bounds = visual.debug_bounds(selector).unwrap();
            assert!(bounds.left() >= page.left(), "{selector} escaped left");
            assert!(bounds.right() <= page.right(), "{selector} escaped right");
            assert!(bounds.top() >= page.top(), "{selector} escaped top");
            assert!(
                bounds.bottom() <= page.bottom(),
                "{selector} escaped bottom"
            );
        }

        let font_dropdown = visual
            .debug_bounds("preferences-editor-font-family")
            .unwrap();
        visual.simulate_click(font_dropdown.center(), Modifiers::default());
        visual.update(|window, cx| window.draw(cx).clear());
        visual.run_until_parked();
        let font_list = visual.debug_bounds("preferences-editor-font-list").unwrap();
        assert!(font_list.size.height <= px(260.0));
        assert!(
            visual
                .debug_bounds("preferences-editor-font-option-0")
                .is_some()
        );
        visual.simulate_keystrokes("down enter");
        visual.run_until_parked();

        let font_increase = visual
            .debug_bounds("preferences-editor-font-size-increase")
            .unwrap();
        visual.simulate_click(font_increase.center(), Modifiers::default());
        let line_height_increase = visual
            .debug_bounds("preferences-editor-line-height-increase")
            .unwrap();
        visual.simulate_click(line_height_increase.center(), Modifiers::default());
        let content_width_increase = visual
            .debug_bounds("preferences-editor-content-width-increase")
            .unwrap();
        visual.simulate_click(content_width_increase.center(), Modifiers::default());
        visual.run_until_parked();

        handle
            .update(&mut visual, |preferences, _window, cx| {
                assert_eq!(preferences.editor_font_size, 17);
                assert_eq!(preferences.editor_line_height_percent, 165);
                assert_eq!(preferences.editor_content_width, 1240);
                assert_eq!(preferences.editor_font_family, preferences.font_options[1]);
                assert!(preferences.has_unsaved_changes());
                assert_eq!(
                    cx.global::<ThemeManager>().current().typography.text_size,
                    17.0
                );
                assert_eq!(
                    cx.global::<ThemeManager>()
                        .current()
                        .typography
                        .text_line_height,
                    1.65
                );
                assert_eq!(
                    cx.global::<ThemeManager>()
                        .current()
                        .dimensions
                        .centered_max_width,
                    1240.0
                );
                assert_eq!(
                    EditorSettings::editor_font_family(cx),
                    preferences.font_options[1]
                );

                preferences.restore_saved_theme(cx);
                assert_eq!(preferences.editor_font_size, 16);
                assert_eq!(preferences.editor_line_height_percent, 160);
                assert_eq!(preferences.editor_content_width, 1200);
                assert!(preferences.editor_font_family.is_empty());
                assert!(!preferences.has_unsaved_changes());
                assert_eq!(
                    cx.global::<ThemeManager>().current().typography.text_size,
                    16.0
                );
                assert_eq!(
                    cx.global::<ThemeManager>()
                        .current()
                        .typography
                        .text_line_height,
                    1.6
                );
                assert_eq!(
                    cx.global::<ThemeManager>()
                        .current()
                        .dimensions
                        .centered_max_width,
                    1200.0
                );
                assert!(EditorSettings::editor_font_family(cx).is_empty());
                assert_eq!(
                    preferences.dropdown_current_index(PreferencesDropdown::Font),
                    0
                );
            })
            .unwrap();
    }

    #[gpui::test]
    async fn preferences_dirty_state_tracks_draft_changes(cx: &mut TestAppContext) {
        init_preferences_test_app(cx);

        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "Preferences".into(),
            )
        });
        cx.run_until_parked();

        handle
            .update(cx, |preferences, _window, _cx| {
                assert!(!preferences.has_unsaved_changes());
                preferences.startup_open = StartupOpenPreference::LastOpenedFile;
                assert!(preferences.has_unsaved_changes());
                preferences.startup_open = StartupOpenPreference::NewFile;
                assert!(!preferences.has_unsaved_changes());

                preferences.image_paste_behavior = ImagePasteBehavior::CopyToAssetsFolder;
                assert!(preferences.has_unsaved_changes());
                preferences.image_paste_behavior = ImagePasteBehavior::None;
                assert!(!preferences.has_unsaved_changes());

                preferences
                    .keybindings
                    .insert("save_document".into(), vec!["ctrl-alt-s".into()]);
                assert!(preferences.has_unsaved_changes());
            })
            .expect("preferences window should be updateable");
    }

    #[gpui::test]
    async fn applying_saved_preferences_keeps_window_open_and_focused(cx: &mut TestAppContext) {
        init_preferences_test_app(cx);

        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "Preferences".into(),
            )
        });
        cx.run_until_parked();

        handle
            .update(cx, |preferences, window, cx| {
                preferences.startup_open = StartupOpenPreference::LastOpenedFile;
                assert!(preferences.has_unsaved_changes());
                let saved = AppPreferences {
                    startup_open: StartupOpenPreference::LastOpenedFile,
                    workspace_sidebar_position: WorkspaceSidebarPosition::Right,
                    ..AppPreferences::default()
                };
                preferences.apply_saved_preferences(saved, window, cx);
            })
            .expect("preferences window should be updateable");
        cx.run_until_parked();

        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
        assert_eq!(
            cx.update(|cx| EditorSettings::workspace_sidebar_position(cx)),
            WorkspaceSidebarPosition::Right
        );
        let active_window = cx.update(|cx| cx.active_window().expect("window should be active"));
        assert_eq!(active_window.window_id(), handle.window_id());
        assert!(
            handle
                .update(cx, |preferences, window, _cx| preferences
                    .focus_handle
                    .is_focused(window))
                .expect("preferences window should remain updateable")
        );
        assert!(
            !handle
                .update(cx, |preferences, _window, _cx| preferences
                    .has_unsaved_changes())
                .expect("preferences window should remain updateable")
        );
    }
