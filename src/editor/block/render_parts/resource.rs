// @author kongweiguang

//! Stable, solid resource-card rendering for the Markdown editor.

use super::*;
use crate::components::{ResourceKind, ResourceLocation, ResourceRecord, ResourceStatus};

impl Block {
    pub(super) fn render_resource_content(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(fallback) = self
            .record
            .resource
            .as_ref()
            .map(|record| record.with_base_dir(self.image_base_dir()))
        else {
            return div().into_any_element();
        };
        let (record, status) = self
            .resource_runtime()
            .map(|runtime| (runtime.record.clone(), runtime.status.clone()))
            .unwrap_or((fallback, ResourceStatus::Loading));
        let c = &theme.colors;
        let wb = &c.workbench;
        let d = &theme.dimensions;
        let strings = cx.global::<crate::i18n::I18nManager>().strings();
        let (kind_icon, kind_label) = match record.kind {
            ResourceKind::File => (
                "icon/ui/file.svg",
                resource_text(strings, "resource_kind_file", "File"),
            ),
            ResourceKind::Video => (
                "icon/ui/play.svg",
                resource_text(strings, "resource_kind_video", "Video"),
            ),
        };
        let target_tooltip = match &record.location {
            ResourceLocation::Local(path) => path.display().to_string(),
            ResourceLocation::Url(url) => url.to_string(),
        };
        let target = match &record.location {
            ResourceLocation::Local(path) => path
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            ResourceLocation::Url(url) => url
                .host_str()
                .map(str::to_owned)
                .unwrap_or_else(|| url.to_string()),
        };
        let retryable = matches!(
            &status,
            ResourceStatus::Missing | ResourceStatus::PermissionDenied | ResourceStatus::OpenFailed
        );
        let primary_label = if retryable {
            resource_text(strings, "resource_retry", "Retry")
        } else {
            match (&record.location, record.kind) {
                (ResourceLocation::Url(_), _) => {
                    resource_text(strings, "resource_open_link", "Open Link")
                }
                (ResourceLocation::Local(_), ResourceKind::Video) => {
                    resource_text(strings, "resource_play", "Play")
                }
                (ResourceLocation::Local(_), ResourceKind::File) => {
                    resource_text(strings, "resource_open", "Open")
                }
            }
        };
        let button_record = record.clone();
        let keyboard_record = record.clone();
        let primary_enabled = !record.is_unsafe_url();
        let primary_tooltip = if primary_enabled {
            primary_label.clone()
        } else {
            resource_text(
                strings,
                "resource_disabled_unsafe_scheme",
                "This address uses a blocked scheme",
            )
        };
        // A truncated card must still expose both the complete author-facing
        // title and the resolved target without widening the content column.
        let tooltip = format!("{}\n{}", record.label, target_tooltip);
        let selected = self.resource_selected;
        let status_text = resource_status_text(&status, &record, strings);
        let status_color = match status {
            ResourceStatus::Missing
            | ResourceStatus::PermissionDenied
            | ResourceStatus::UnsafeScheme
            | ResourceStatus::OpenFailed => wb.danger,
            ResourceStatus::Loading => wb.text_tertiary,
            ResourceStatus::Ready { .. } => wb.text_tertiary,
        };

        div()
            .id(ElementId::Name(
                format!("resource-card-{}", self.record.id).into(),
            ))
            .debug_selector(|| "resource-card".to_owned())
            .w_full()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(d.block_padding_x))
            .py(px(8.0))
            .rounded(px(8.0))
            .border(px(1.0))
            .border_color(if selected {
                wb.accent
            } else {
                wb.border_subtle
            })
            .bg(wb.solid_surface)
            .text_size(px(theme.typography.text_size))
            .text_color(wb.text_primary)
            .cursor_pointer()
            .tab_index(0)
            .track_focus(&self.focus_handle)
            .focus(|this| this.border_color(wb.focus_ring))
            .tooltip(move |_window, cx| crate::ui::ui_tooltip(tooltip.clone(), cx))
            .child(
                div()
                    .id(ElementId::Name(
                        format!("resource-kind-icon-{}", self.record.id).into(),
                    ))
                    .flex_shrink_0()
                    .w(px(32.0))
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .bg(wb.control_surface)
                    .tooltip(move |_window, cx| crate::ui::ui_tooltip(kind_label.clone(), cx))
                    .child(svg().path(kind_icon).size(px(18.0)).text_color(wb.accent)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_grow()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .overflow_hidden()
                            .truncate()
                            .child(SharedString::from(record.label)),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .truncate()
                            .whitespace_nowrap()
                            .text_color(wb.text_tertiary)
                            .child(SharedString::from(target)),
                    ),
            )
            .child(
                div()
                    .id(ElementId::Name(
                        format!("resource-primary-action-{}", self.record.id).into(),
                    ))
                    .flex_shrink_0()
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .bg(wb.control_surface)
                    .text_color(status_color)
                    .debug_selector(|| "resource-primary-action".to_owned())
                    .tooltip({
                        move |_window, cx| crate::ui::ui_tooltip(primary_tooltip.clone(), cx)
                    })
                    .child(SharedString::from(primary_label))
                    .when(primary_enabled, |button| {
                        button
                            .hover(|this| this.bg(wb.control_hover))
                            .cursor_pointer()
                            .on_click(cx.listener(move |block, _event, _window, cx| {
                                if retryable {
                                    block.retry_resource_probe(cx);
                                } else {
                                    block.request_resource_open(&button_record, cx);
                                }
                            }))
                    }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(status_color)
                    .child(SharedString::from(status_text)),
            )
            .on_key_down(
                cx.listener(move |block, event: &KeyDownEvent, _window, cx| {
                    match event.keystroke.key.as_str() {
                        "enter" => {
                            if retryable {
                                block.retry_resource_probe(cx);
                            } else {
                                block.request_resource_open(&keyboard_record, cx);
                            }
                            cx.stop_propagation();
                        }
                        "space" => {
                            block.resource_selected = true;
                            cx.stop_propagation();
                            cx.notify();
                        }
                        _ => {}
                    }
                }),
            )
            .into_any_element()
    }

    pub(crate) fn request_resource_open(
        &mut self,
        record: &ResourceRecord,
        cx: &mut Context<Self>,
    ) {
        if record.is_unsafe_url() {
            return;
        }
        match &record.location {
            ResourceLocation::Local(path) => {
                if crate::resource_io::open_local_resource(path).is_err() {
                    self.mark_resource_open_failed(cx);
                }
            }
            ResourceLocation::Url(url) => cx.emit(BlockEvent::RequestOpenLink {
                prompt_target: record.destination.clone(),
                open_target: url.to_string(),
            }),
        }
    }
}

fn resource_status_text(
    status: &ResourceStatus,
    record: &ResourceRecord,
    strings: &crate::i18n::I18nStrings,
) -> String {
    match status {
        ResourceStatus::Loading => resource_text(strings, "resource_status_loading", "Loading"),
        ResourceStatus::Ready { size } => match size {
            Some(size) => format_size(*size),
            None if record.is_local() => resource_text(strings, "resource_status_ready", "Ready"),
            None => resource_text(strings, "resource_status_online", "Online"),
        },
        ResourceStatus::Missing => {
            resource_text(strings, "resource_status_missing", "File Missing")
        }
        ResourceStatus::PermissionDenied => resource_text(
            strings,
            "resource_status_permission_denied",
            "Permission Denied",
        ),
        ResourceStatus::UnsafeScheme => resource_text(
            strings,
            "resource_status_unsafe_scheme",
            "Unsupported Scheme",
        ),
        ResourceStatus::OpenFailed => {
            resource_text(strings, "resource_status_open_failed", "Open Failed")
        }
    }
}

fn resource_text(strings: &crate::i18n::I18nStrings, key: &str, english_fallback: &str) -> String {
    strings
        .slash_commands
        .get(key)
        .cloned()
        .unwrap_or_else(|| english_fallback.to_owned())
}

fn format_size(size: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = size as f64;
    let mut index = 0usize;
    while value >= 1024.0 && index + 1 < UNITS.len() {
        value /= 1024.0;
        index += 1;
    }
    if index == 0 {
        format!("{} {}", size, UNITS[index])
    } else {
        format!("{value:.1} {}", UNITS[index])
    }
}
