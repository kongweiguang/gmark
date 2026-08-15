// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn render_workspace_operation_dialog_overlay(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.workspace.operation_dialog.as_ref()?;
        if dialog.kind == WorkspaceOperationKind::Delete {
            return self.render_workspace_delete_dialog_overlay(theme, strings, cx);
        }
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let title = match dialog.kind {
            WorkspaceOperationKind::Rename => strings.workspace_rename_title.clone(),
            WorkspaceOperationKind::Move => strings.workspace_move_title.clone(),
            WorkspaceOperationKind::NewFile => strings.workspace_new_file_title.clone(),
            WorkspaceOperationKind::NewFolder => strings.workspace_new_folder_title.clone(),
            WorkspaceOperationKind::Delete => strings.workspace_delete_title.clone(),
        };
        let status = if dialog.running {
            Some((
                strings.workspace_operation_busy.clone(),
                REFRESH_ICON,
                "workspace-operation-status-progress-icon",
                c.text_link,
            ))
        } else if let Some(error) = dialog.error.as_ref() {
            Some((
                error.clone(),
                WARNING_ICON,
                "workspace-operation-status-error-icon",
                c.dialog_danger_button_bg,
            ))
        } else {
            dialog.plan.as_ref().map(|plan| match plan {
                WorkspacePendingPlan::Move(plan) => (
                    strings
                        .workspace_operation_affected_template
                        .replace("{count}", &plan.rewrites.len().to_string()),
                    CHECK_ICON,
                    "workspace-operation-status-ready-icon",
                    c.dialog_muted,
                ),
                WorkspacePendingPlan::Create(plan) => (
                    plan.path.display().to_string(),
                    CHECK_ICON,
                    "workspace-operation-status-ready-icon",
                    c.dialog_muted,
                ),
                WorkspacePendingPlan::Delete(plan) => (
                    plan.path.display().to_string(),
                    CHECK_ICON,
                    "workspace-operation-status-ready-icon",
                    c.dialog_muted,
                ),
            })
        };
        let primary_label = match dialog.kind {
            WorkspaceOperationKind::NewFile => strings.workspace_new_file.clone(),
            WorkspaceOperationKind::NewFolder => strings.workspace_new_folder.clone(),
            WorkspaceOperationKind::Rename
            | WorkspaceOperationKind::Move
            | WorkspaceOperationKind::Delete => {
                if dialog.plan.is_some() {
                    strings.workspace_apply_operation.clone()
                } else {
                    strings.workspace_review_operation.clone()
                }
            }
        };
        let primary_handler = if dialog.plan.is_some() {
            Self::on_apply_workspace_operation
                as fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>)
        } else {
            Self::on_review_workspace_operation
                as fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>)
        };
        let enabled = !dialog.running;
        let primary = dialog_button(
            "confirm-workspace-operation",
            primary_label,
            if enabled {
                DialogButtonKind::Primary
            } else {
                DialogButtonKind::Secondary
            },
            theme,
        );
        let primary = if enabled {
            primary.on_click(cx.listener(primary_handler))
        } else {
            primary.opacity(0.62)
        };

        Some(
            modal_overlay("workspace-operation-dialog-overlay", theme)
                .child(
                    dialog_panel(
                        "workspace-operation-dialog",
                        d.dialog_width.min(520.0),
                        theme,
                    )
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation()
                    })
                    .child(
                        dialog_content("workspace-operation-dialog-content", theme)
                            .child(dialog_title_with_icon(
                                "workspace-operation-title",
                                title,
                                DialogTitleIcon::Files,
                                theme,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .text_size(px(t.dialog_body_size))
                                            .text_color(c.dialog_body)
                                            .child(strings.workspace_destination_label.clone()),
                                    )
                                    .child(
                                        div()
                                            .id("workspace-operation-destination-input")
                                            .debug_selector(|| {
                                                "workspace-operation-destination-input".to_owned()
                                            })
                                            .min_h(px(38.0))
                                            .w_full()
                                            .px(px(8.0))
                                            .flex()
                                            .items_center()
                                            .rounded(px(6.0))
                                            .border(px(d.dialog_border_width))
                                            .border_color(c.dialog_border)
                                            .child(dialog.input.clone()),
                                    ),
                            )
                            .children(status.map(|(message, icon, selector, color)| {
                                workspace_status_row(
                                    "workspace-operation-status",
                                    selector,
                                    icon,
                                    message,
                                    color,
                                    t.dialog_body_size,
                                )
                                .into_any_element()
                            })),
                    )
                    .child(
                        dialog_actions(theme)
                            .child(
                                dialog_button(
                                    "cancel-workspace-operation",
                                    strings.open_link_cancel.clone(),
                                    DialogButtonKind::Secondary,
                                    theme,
                                )
                                .on_click(cx.listener(Self::on_cancel_workspace_operation)),
                            )
                            .child(primary),
                    ),
                )
                .into_any_element(),
        )
    }

    /// 删除确认复用标准正文和操作区，避免自定义 flex 内容再次引入底部空白。
    fn render_workspace_delete_dialog_overlay(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.workspace.operation_dialog.as_ref()?;
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let enabled = dialog.plan.is_some() && !dialog.running;
        let confirm = dialog_button(
            "confirm-workspace-delete",
            strings.workspace_delete_confirm.clone(),
            if enabled {
                DialogButtonKind::Danger
            } else {
                DialogButtonKind::Secondary
            },
            theme,
        );
        let confirm = if enabled {
            confirm.on_click(cx.listener(Self::on_apply_workspace_operation))
        } else {
            confirm.opacity(0.62)
        };
        let path = dialog.source.display().to_string();
        let status = if dialog.running {
            Some((
                strings.workspace_operation_busy.clone(),
                REFRESH_ICON,
                "workspace-delete-status-progress-icon",
                c.text_link,
            ))
        } else {
            dialog.error.as_ref().map(|error| {
                (
                    error.clone(),
                    WARNING_ICON,
                    "workspace-delete-status-error-icon",
                    c.dialog_danger_button_bg,
                )
            })
        };

        Some(
            modal_overlay("workspace-delete-dialog-overlay", theme)
                .child(
                    dialog_panel("workspace-delete-dialog", d.dialog_width.min(520.0), theme)
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation()
                        })
                        .child(
                            dialog_content("workspace-delete-dialog-content", theme)
                                .child(dialog_title_with_icon(
                                    "workspace-delete-title",
                                    strings.workspace_delete_title.clone(),
                                    DialogTitleIcon::Warning,
                                    theme,
                                ))
                                .child(dialog_body(
                                    strings
                                        .workspace_delete_message_template
                                        .replace("{path}", &path),
                                    theme,
                                ))
                                .child(
                                    div()
                                        .id("workspace-delete-target")
                                        .debug_selector(|| "workspace-delete-target".to_owned())
                                        .w_full()
                                        // 单行路径使用确定的 BorderBox 高度，避免 Windows 2× DPI
                                        // 字体测量产生半逻辑像素并溢出滚动正文边界。
                                        .h(px(40.0))
                                        .px(px(10.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.0))
                                        .border(px(d.dialog_border_width))
                                        .border_color(c.dialog_border)
                                        .bg(c.dialog_secondary_button_bg)
                                        .text_size(px(t.dialog_body_size))
                                        .text_color(c.dialog_body)
                                        .overflow_hidden()
                                        .truncate()
                                        .child(path),
                                )
                                .children(status.map(|(message, icon, selector, color)| {
                                    workspace_status_row(
                                        "workspace-delete-status",
                                        selector,
                                        icon,
                                        message,
                                        color,
                                        t.dialog_body_size,
                                    )
                                    .into_any_element()
                                })),
                        )
                        .child(
                            compact_dialog_actions(theme)
                                .id("workspace-delete-actions")
                                .debug_selector(|| "workspace-delete-actions".to_owned())
                                .child(
                                    dialog_button(
                                        "cancel-workspace-delete",
                                        strings.open_link_cancel.clone(),
                                        DialogButtonKind::Secondary,
                                        theme,
                                    )
                                    .on_click(cx.listener(Self::on_cancel_workspace_operation)),
                                )
                                .child(confirm),
                        ),
                )
                .into_any_element(),
        )
    }
}
