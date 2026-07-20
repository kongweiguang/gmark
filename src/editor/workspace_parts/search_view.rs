// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn render_workspace_search(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        editor: &WeakEntity<Editor>,
        cx: &App,
    ) -> AnyElement {
        let c = &theme.colors;
        let t = &theme.typography;
        let input = self.workspace.search_input.clone();
        let toggle = |index: usize,
                      id: &'static str,
                      label: String,
                      icon: &'static str,
                      active: bool,
                      option: fn(&mut WorkspaceSearchOptions) -> &mut bool| {
            let click_editor = editor.clone();
            let hover_editor = editor.clone();
            let keyboard_selected = self.workspace.keyboard_zone
                == WorkspaceKeyboardZone::SearchOptions
                && self.workspace.search_selected == index;
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .relative()
                .w(px(28.0))
                .h(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .border(px(1.0))
                .border_color(if keyboard_selected {
                    c.text_link
                } else {
                    hsla(0.0, 0.0, 0.0, 0.0)
                })
                .bg(if active {
                    c.selection
                } else {
                    hsla(0.0, 0.0, 0.0, 0.0)
                })
                .hover(|this| this.bg(c.dialog_secondary_button_hover))
                .cursor_pointer()
                .text_color(if active {
                    c.text_default
                } else {
                    c.dialog_muted
                })
                .child(
                    svg()
                        .path(icon)
                        .size(px(15.0))
                        .text_color(if active {
                            c.text_default
                        } else {
                            c.dialog_muted
                        })
                        .debug_selector(move || format!("{id}-icon")),
                )
                .children(
                    (self.workspace.tooltip_visible == Some(id))
                        .then(|| render_workspace_tooltip(label, 28.0, theme)),
                )
                .on_hover(move |hovered, _window, cx| {
                    let _ = hover_editor.update(cx, |editor, cx| {
                        editor.set_workspace_tooltip_hover(id, *hovered, cx);
                    });
                })
                .on_click(move |_event, _window, cx| {
                    let _ = click_editor.update(cx, |editor, cx| {
                        editor.toggle_workspace_search_option(option, cx);
                    });
                })
        };

        let status = if self.workspace.search_running {
            Some((
                strings.workspace_search_running.clone(),
                REFRESH_ICON,
                "workspace-search-running-icon",
                c.text_link,
            ))
        } else if let Some(error) = self.workspace.search_error.as_ref() {
            Some((
                error.clone(),
                WARNING_ICON,
                "workspace-search-error-icon",
                c.dialog_danger_button_bg,
            ))
        } else if input.as_ref().is_some_and(|input| {
            !input.read(cx).display_text().is_empty() && self.workspace.search_results.is_empty()
        }) {
            Some((
                strings.workspace_search_no_results.clone(),
                SEARCH_TAB_ICON,
                "workspace-search-empty-icon",
                c.dialog_muted,
            ))
        } else {
            None
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .id("workspace-search-input")
                    .debug_selector(|| "workspace-search-input".to_owned())
                    .min_h(px(34.0))
                    .w_full()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .rounded(px(10.0))
                    .border(px(1.0))
                    .border_color(c.dialog_border)
                    .bg(c.dialog_secondary_button_bg)
                    .child(
                        svg()
                            .path(SEARCH_TAB_ICON)
                            .size(px(14.0))
                            .flex_shrink_0()
                            .text_color(c.dialog_muted)
                            .debug_selector(|| "workspace-search-input-icon".to_owned()),
                    )
                    .child(div().flex_1().min_w(px(0.0)).children(input)),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(toggle(
                        0,
                        "workspace-search-case",
                        strings.find_case_sensitive.clone(),
                        "icon/ui/case-sensitive.svg",
                        self.workspace.search_options.case_sensitive,
                        |options| &mut options.case_sensitive,
                    ))
                    .child(toggle(
                        1,
                        "workspace-search-word",
                        strings.find_whole_word.clone(),
                        "icon/ui/whole-word.svg",
                        self.workspace.search_options.whole_word,
                        |options| &mut options.whole_word,
                    ))
                    .child(toggle(
                        2,
                        "workspace-search-regex",
                        strings.find_regex.clone(),
                        "icon/ui/regex.svg",
                        self.workspace.search_options.regex,
                        |options| &mut options.regex,
                    )),
            )
            .children(status.map(|(message, icon, selector, color)| {
                workspace_status_row(
                    "workspace-search-status",
                    selector,
                    icon,
                    message,
                    color,
                    t.text_size * 0.84,
                )
                .my(px(6.0))
                .into_any_element()
            }))
            .children(
                self.workspace
                    .search_results
                    .iter()
                    .enumerate()
                    .map(|(index, result)| {
                        let editor = editor.clone();
                        let path = result.path.clone();
                        let line = result.line;
                        let keyboard_selected = self.workspace.keyboard_zone
                            == WorkspaceKeyboardZone::SearchResults
                            && self.workspace.search_selected == index;
                        div()
                            .id(SharedString::from(format!(
                                "workspace-search-result-{index}"
                            )))
                            .debug_selector(move || format!("workspace-search-result-{index}"))
                            .w_full()
                            .px(px(6.0))
                            .py(px(5.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .rounded(px(4.0))
                            .bg(if keyboard_selected {
                                c.selection
                            } else {
                                hsla(0.0, 0.0, 0.0, 0.0)
                            })
                            .hover(|this| this.bg(c.dialog_secondary_button_hover))
                            .cursor_pointer()
                            .child(
                                div()
                                    .w_full()
                                    .min_w(px(0.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .text_size(px(t.text_size * 0.8))
                                    .text_color(c.dialog_muted)
                                    .child(
                                        div()
                                            .id(("workspace-search-result-path", index))
                                            .debug_selector(move || {
                                                format!("workspace-search-result-path-{index}")
                                            })
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .truncate()
                                            .child(middle_ellipsis(&result.relative_path, 34)),
                                    )
                                    .child(
                                        div()
                                            .id(("workspace-search-result-location", index))
                                            .debug_selector(move || {
                                                format!("workspace-search-result-location-{index}")
                                            })
                                            .flex_shrink_0()
                                            .child(format!(":{}:{}", result.line, result.column)),
                                    ),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .truncate()
                                    .text_size(px(t.text_size * 0.88))
                                    .text_color(c.text_default)
                                    .child(result.preview.clone()),
                            )
                            .on_click(move |_event, window, cx| {
                                let path = path.clone();
                                let _ = editor.update(cx, |editor, cx| {
                                    editor.open_workspace_search_result(path, line, window, cx);
                                });
                            })
                    }),
            )
            .into_any_element()
    }

    pub(super) fn render_workspace_files_tree(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        editor: &WeakEntity<Editor>,
    ) -> AnyElement {
        if self.workspace.root.is_none() {
            return self.render_workspace_empty_state(
                "workspace-files-empty",
                FILES_TAB_ICON,
                &strings.workspace_no_file_title,
                &strings.workspace_no_file_message,
                Some((strings.menu_open_folder.clone(), editor.clone())),
                theme,
            );
        }

        if let Some(error) = self.workspace.file_error.as_ref() {
            return self.render_workspace_empty_state(
                "workspace-files-error",
                FILES_TAB_ICON,
                &strings.workspace_scan_failed_title,
                error,
                None,
                theme,
            );
        }

        let Some(root) = self.workspace.file_tree.as_ref() else {
            if self.workspace.file_scanning {
                return self.render_workspace_empty_state(
                    "workspace-files-scanning",
                    FILES_TAB_ICON,
                    "",
                    &strings.workspace_scanning_files,
                    None,
                    theme,
                );
            }
            return self.render_workspace_empty_state(
                "workspace-files-empty",
                FILES_TAB_ICON,
                "",
                &strings.workspace_empty_files,
                None,
                theme,
            );
        };
        let c = &theme.colors;
        let t = &theme.typography;

        let tree = div()
            .w_full()
            .flex()
            .flex_col()
            .children(self.render_workspace_nodes(std::slice::from_ref(root), 0, theme, editor));
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .children(self.workspace.operation_error.as_ref().map(|error| {
                workspace_status_row(
                    "workspace-operation-error",
                    "workspace-operation-error-icon",
                    WARNING_ICON,
                    error.clone(),
                    c.dialog_danger_button_bg,
                    t.text_size * 0.82,
                )
                .rounded(px(4.0))
                .bg(c.dialog_secondary_button_bg)
            }))
            .child(tree)
            .into_any_element()
    }

    pub(super) fn render_workspace_outline_tree(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        editor: &WeakEntity<Editor>,
    ) -> AnyElement {
        if self.workspace.outline_tree.is_empty() {
            if self.workspace.outline_running {
                return self.render_workspace_empty_state(
                    "workspace-outline-updating",
                    OUTLINE_TAB_ICON,
                    "",
                    &strings.workspace_outline_updating,
                    None,
                    theme,
                );
            }
            return self.render_workspace_empty_state(
                "workspace-outline-empty",
                OUTLINE_TAB_ICON,
                "",
                &strings.workspace_empty_outline,
                None,
                theme,
            );
        }
        let c = &theme.colors;
        let t = &theme.typography;
        div()
            .w_full()
            .flex()
            .flex_col()
            .children(self.workspace.outline_running.then(|| {
                workspace_status_row(
                    "workspace-outline-progress",
                    "workspace-outline-progress-icon",
                    REFRESH_ICON,
                    strings.workspace_outline_updating.clone(),
                    c.text_link,
                    t.text_size * 0.78,
                )
            }))
            .children(self.render_workspace_nodes(&self.workspace.outline_tree, 0, theme, editor))
            .into_any_element()
    }

    pub(super) fn render_workspace_empty_state(
        &self,
        id: &'static str,
        icon: &'static str,
        title: &str,
        message: &str,
        open_folder_action: Option<(String, WeakEntity<Editor>)>,
        theme: &Theme,
    ) -> AnyElement {
        let c = &theme.colors;
        let t = &theme.typography;
        let has_primary_action = open_folder_action.is_some();
        let title = (!has_primary_action && !title.is_empty()).then(|| {
            div()
                .text_size(px(t.text_size))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.text_default)
                .child(title.to_string())
        });

        let action = open_folder_action.map(|(label, editor)| {
            div()
                .id("workspace-empty-open-folder")
                .debug_selector(|| "workspace-empty-open-folder".to_owned())
                .min_w(px(120.0))
                .max_w(relative(1.0))
                .h(px(30.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(7.0))
                .rounded(px(10.0))
                .border(px(1.0))
                .border_color(c.dialog_border)
                .bg(c.dialog_secondary_button_bg)
                .hover(|this| this.bg(c.chrome_hover))
                .cursor_pointer()
                .text_size(px(t.text_size * 0.86))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.text_default)
                .child(
                    svg()
                        .path(FOLDER_ICON)
                        .size(px(14.0))
                        .flex_shrink_0()
                        .text_color(c.text_default)
                        .debug_selector(|| "workspace-empty-open-folder-icon".to_owned()),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .truncate()
                        .child(label),
                )
                .on_click(move |_event, window, cx| {
                    let _ = editor.update(cx, |editor, cx| {
                        editor.on_open_folder_action(&crate::components::OpenFolder, window, cx);
                    });
                })
        });

        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .px(px(22.0))
            .text_align(TextAlign::Center)
            .children((!has_primary_action).then(|| {
                div()
                    .id(SharedString::from(format!("{id}-icon")))
                    .debug_selector(move || format!("{id}-icon"))
                    .size(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .text_color(c.dialog_muted)
                    .child(
                        svg()
                            .path(icon)
                            .size(px(24.0))
                            .text_color(c.dialog_muted)
                            .debug_selector(move || format!("{id}-icon-svg")),
                    )
            }))
            .children(title)
            .children((!has_primary_action).then(|| {
                div()
                    .text_size(px(t.text_size * 0.9))
                    .line_height(px(t.text_size * t.text_line_height))
                    .text_color(c.dialog_muted)
                    .child(message.to_string())
            }))
            .children(action)
            .into_any_element()
    }

    pub(super) fn render_workspace_nodes(
        &self,
        nodes: &[WorkspaceTreeNode],
        depth: usize,
        theme: &Theme,
        editor: &WeakEntity<Editor>,
    ) -> Vec<AnyElement> {
        let mut elements = Vec::new();
        for node in nodes {
            elements.push(self.render_workspace_node(node, depth, theme, editor));
            if !node.children.is_empty() && self.workspace.expanded.contains(&node.id) {
                elements.extend(self.render_workspace_nodes(
                    &node.children,
                    depth + 1,
                    theme,
                    editor,
                ));
            }
        }
        elements
    }

    pub(super) fn render_workspace_node(
        &self,
        node: &WorkspaceTreeNode,
        depth: usize,
        theme: &Theme,
        editor: &WeakEntity<Editor>,
    ) -> AnyElement {
        let c = &theme.colors;
        let t = &theme.typography;
        let is_expanded = self.workspace.expanded.contains(&node.id);
        let has_children = !node.children.is_empty();
        let selected = match (&self.workspace.selected, &node.kind) {
            (
                Some(WorkspaceSelection::File(selected)),
                WorkspaceTreeKind::Directory(path) | WorkspaceTreeKind::MarkdownFile(path),
            ) => selected == path,
            (Some(WorkspaceSelection::Outline(selected)), _) => selected == &node.id,
            _ => false,
        };
        let node_id = node.id.clone();
        let click_editor = editor.clone();
        let click_kind = node.kind.clone();
        let context_kind = node.kind.clone();
        let context_editor = editor.clone();
        let drag_payload = match &node.kind {
            WorkspaceTreeKind::Directory(path) if self.workspace.root.as_ref() != Some(path) => {
                Some(WorkspaceDragPayload {
                    path: path.clone(),
                    label: node.label.clone(),
                    background: c.dialog_surface,
                    text: c.text_default,
                })
            }
            WorkspaceTreeKind::MarkdownFile(path) => Some(WorkspaceDragPayload {
                path: path.clone(),
                label: node.label.clone(),
                background: c.dialog_surface,
                text: c.text_default,
            }),
            _ => None,
        };
        let drop_target = match &node.kind {
            WorkspaceTreeKind::Directory(path) => Some(path.clone()),
            WorkspaceTreeKind::MarkdownFile(path) => path.parent().map(Path::to_path_buf),
            WorkspaceTreeKind::Heading { .. } => None,
        };
        let drop_editor = editor.clone();
        let drop_background = c.selection;
        let arrow_node_id = node.id.clone();
        let arrow_editor = editor.clone();
        let arrow = has_children.then_some(if is_expanded {
            CHEVRON_DOWN_ICON
        } else {
            CHEVRON_RIGHT_ICON
        });

        let icon = match &node.kind {
            WorkspaceTreeKind::Directory(_) => Some((FOLDER_ICON, c.dialog_muted)),
            WorkspaceTreeKind::MarkdownFile(_) => Some((MARKDOWN_ICON, c.text_link)),
            WorkspaceTreeKind::Heading { .. } => None,
        };

        let label_color = if selected {
            c.text_default
        } else {
            c.dialog_muted
        };
        let label_budget = 28usize.saturating_sub(depth.saturating_mul(2)).max(12);
        let display_label = middle_ellipsis(&node.label, label_budget);

        let mut arrow_el = div()
            .w(px(14.0))
            .h(px(18.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .text_color(c.dialog_muted)
            .children(arrow.map(|path| svg().path(path).size(px(14.0)).text_color(c.dialog_muted)));
        if has_children {
            arrow_el = arrow_el.cursor_pointer().on_mouse_down(
                MouseButton::Left,
                move |_event, _window, cx| {
                    let _ = arrow_editor.update(cx, |editor, cx| {
                        editor.toggle_workspace_node(&arrow_node_id, cx);
                    });
                    cx.stop_propagation();
                },
            );
        }

        let row = div()
            .id(("workspace-node", stable_node_hash(&node.id)))
            .h(px(WORKSPACE_NODE_HEIGHT))
            .w_full()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap(px(6.0))
            .pl(px(8.0 + depth as f32 * WORKSPACE_NODE_INDENT))
            .pr(px(8.0))
            .rounded(px(6.0))
            .bg(if selected {
                c.selection
            } else {
                hsla(0.0, 0.0, 0.0, 0.0)
            })
            .hover(|this| this.bg(c.chrome_hover))
            .cursor_pointer()
            .child(arrow_el)
            .children(icon.map(|(path, color)| {
                svg()
                    .path(path)
                    .size(px(16.0))
                    .flex_shrink_0()
                    .text_color(color)
                    .into_any_element()
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .text_size(px(t.text_size * 0.9))
                    .line_height(px(t.text_size * t.text_line_height))
                    .text_color(label_color)
                    .child(display_label),
            )
            .on_click(move |_event, window, cx| {
                let node_id = node_id.clone();
                let click_kind = click_kind.clone();
                let _ = click_editor.update(cx, |editor, cx| match click_kind {
                    WorkspaceTreeKind::Directory(path) => {
                        editor.workspace.selected = Some(WorkspaceSelection::File(path));
                        editor.toggle_workspace_node(&node_id, cx);
                    }
                    WorkspaceTreeKind::MarkdownFile(path) => {
                        editor.open_workspace_file(path, window, cx);
                    }
                    WorkspaceTreeKind::Heading { line, .. } => {
                        editor.select_outline_node(node_id, line, window, cx)
                    }
                });
            })
            .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                let path = match &context_kind {
                    WorkspaceTreeKind::Directory(path) | WorkspaceTreeKind::MarkdownFile(path) => {
                        path.clone()
                    }
                    WorkspaceTreeKind::Heading { .. } => return,
                };
                let _ = context_editor.update(cx, |editor, cx| {
                    editor.open_workspace_context_menu(event.position, path, cx);
                });
                cx.stop_propagation();
            });
        let row = if let Some(payload) = drag_payload {
            row.cursor_move()
                .on_drag(payload, |payload, position, _, cx| {
                    let payload = payload.clone();
                    cx.new(|_| WorkspaceDragPreview { payload, position })
                })
        } else {
            row
        };
        let row = if let Some(target_directory) = drop_target {
            row.drag_over::<WorkspaceDragPayload>(move |style, _, _, _| style.bg(drop_background))
                .on_drop(move |payload: &WorkspaceDragPayload, window, cx| {
                    let source = payload.path.clone();
                    let target_directory = target_directory.clone();
                    let _ = drop_editor.update(cx, |editor, cx| {
                        editor.open_workspace_drop_move_dialog(
                            source,
                            target_directory,
                            window,
                            cx,
                        );
                    });
                })
        } else {
            row
        };
        row.into_any_element()
    }
}
