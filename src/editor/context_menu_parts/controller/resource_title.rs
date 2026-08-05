// @author kongweiguang

use super::*;
use crate::theme::workbench::SurfaceKind;
use crate::ui::visual_preferences::VisualPreferencesManager;

impl Editor {
    pub(in crate::editor) fn open_resource_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.resource_context_block(cx) else {
            return;
        };
        let Some(record) = self.resource_context_record(cx) else {
            return;
        };
        self.close_context_menu(cx);
        block.update(cx, |block, cx| {
            block.request_resource_open(&record, cx);
        });
    }

    pub(in crate::editor) fn reveal_resource_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let block = self.resource_context_block(cx);
        let path = self
            .resource_context_record(cx)
            .as_ref()
            .and_then(ResourceRecord::local_path)
            .map(std::path::Path::to_path_buf);
        self.close_context_menu(cx);
        if let Some(path) = path
            && let Err(error) = crate::resource_io::reveal_local_resource(&path)
        {
            if let Some(block) = block {
                block.update(cx, |block, cx| block.mark_resource_open_failed(cx));
            }
            eprintln!("failed to reveal resource '{}': {error}", path.display());
        }
    }

    pub(in crate::editor) fn edit_resource_title_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.resource_context_block(cx) else {
            return;
        };
        let Some(previous) = self.resource_context_record(cx) else {
            return;
        };
        self.close_context_menu(cx);
        let label = previous.label.clone();
        let input = cx.new(|cx| {
            let mut input =
                crate::components::Block::with_record(cx, BlockRecord::paragraph(label));
            input.set_source_raw_mode();
            input
        });
        input.read(cx).focus_handle.focus(window);
        self.resource_title_dialog = Some(crate::editor::ResourceTitleDialogState {
            entity_id: block.entity_id(),
            previous,
            input,
        });
        cx.notify();
    }

    pub(in crate::editor) fn cancel_resource_title_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.resource_title_dialog.take() {
            if self.focusable_entity_by_id(dialog.entity_id).is_some() {
                self.focus_block(dialog.entity_id);
            }
            cx.notify();
        }
    }

    pub(in crate::editor) fn confirm_resource_title_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.resource_title_dialog.take() else {
            return;
        };
        let label = dialog.input.read(cx).display_text().to_owned();
        let Some(block) = self.focusable_entity_by_id(dialog.entity_id) else {
            cx.notify();
            return;
        };
        let destination = if dialog.previous.is_local() {
            dialog.previous.destination.replace('\\', "/")
        } else {
            dialog.previous.destination.clone()
        };
        let base_dir = self.image_base_dir();
        let record = ResourceRecord::from_parts(
            label,
            destination,
            dialog.previous.explicit_kind,
            base_dir.as_deref(),
        );
        let markdown = record.to_markdown();

        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        let title = InlineTextTree::from_markdown(&markdown);
        let cursor = title.visible_len();
        Self::set_block_title_and_kind(&block, block.read(cx).kind(), title, cursor, cx);
        self.rebuild_image_runtimes(cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        self.focus_block(dialog.entity_id);
        cx.notify();
    }

    pub(in crate::editor) fn on_resource_title_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "enter" => {
                self.confirm_resource_title_dialog(cx);
                cx.stop_propagation();
            }
            "escape" => {
                self.cancel_resource_title_dialog(cx);
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    pub(in crate::editor) fn on_confirm_resource_title_dialog(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm_resource_title_dialog(cx);
    }

    pub(in crate::editor) fn on_cancel_resource_title_dialog(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_resource_title_dialog(cx);
    }

    pub(in crate::editor) fn render_resource_title_dialog_overlay(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.resource_title_dialog.as_ref()?;
        let strings = cx.global::<I18nManager>().strings().clone();
        let title = strings
            .slash_commands
            .get("resource_edit_title")
            .cloned()
            .unwrap_or_else(|| "Edit Resource Title".to_owned());
        let c = &theme.colors;
        let visual_preferences = cx
            .try_global::<VisualPreferencesManager>()
            .map(VisualPreferencesManager::current)
            .unwrap_or_default();
        let palette = &c.workbench;
        let solid_material = palette.material(SurfaceKind::Solid, visual_preferences);
        let d = &theme.dimensions;
        let t = &theme.typography;
        Some(
            modal_overlay("resource-title-dialog-overlay", theme)
                .capture_key_down(cx.listener(Self::on_resource_title_dialog_key_down))
                .child(
                    dialog_panel("resource-title-dialog", d.dialog_width.min(480.0), theme)
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation()
                        })
                        .child(
                            crate::editor::render::dialog_content(
                                "resource-title-dialog-content",
                                theme,
                            )
                            .child(dialog_title_with_icon(
                                "resource-title-dialog-title",
                                title,
                                DialogTitleIcon::Files,
                                theme,
                            ))
                            .child(
                                div()
                                    .text_size(px(t.dialog_body_size))
                                    .text_color(palette.text_primary)
                                    .child(
                                        strings
                                            .slash_commands
                                            .get("resource_title_field")
                                            .cloned()
                                            .unwrap_or_else(|| "Title".to_owned()),
                                    ),
                            )
                            .child(
                                div()
                                    .id("resource-title-dialog-input")
                                    .debug_selector(|| "resource-title-dialog-input".to_owned())
                                    .min_h(px(38.0))
                                    .w_full()
                                    .px(px(8.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.0))
                                    .border(px(d.dialog_border_width))
                                    .border_color(solid_material.border)
                                    .bg(solid_material.background)
                                    .child(dialog.input.clone()),
                            ),
                        )
                        .child(
                            dialog_actions(theme)
                                .child(
                                    dialog_button(
                                        "cancel-resource-title-dialog",
                                        strings.open_link_cancel.clone(),
                                        DialogButtonKind::Secondary,
                                        theme,
                                    )
                                    .on_click(cx.listener(Self::on_cancel_resource_title_dialog)),
                                )
                                .child(
                                    dialog_button(
                                        "confirm-resource-title-dialog",
                                        strings.info_dialog_ok.clone(),
                                        DialogButtonKind::Primary,
                                        theme,
                                    )
                                    .on_click(cx.listener(Self::on_confirm_resource_title_dialog)),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
