// @author kongweiguang

//! Bottom status bar: sidebar toggle, mode switch, cursor position,
//! character count, and custom buttons.

use std::collections::{HashMap, HashSet};

use gmark_document::{LineEnding, LineEndingStatus, Revision, SourceFormatSummary};
use gpui::*;
use unicode_segmentation::UnicodeSegmentation;

use super::{Editor, ViewMode};
use crate::i18n::I18nStrings;
use crate::preferences::{StatusBarButton, StatusBarPreferences};
use crate::theme::Theme;

const SIDEBAR_ICON: &str = "icon/ui/panel-left.svg";
const LIVE_MODE_ICON: &str = "icon/ui/live.svg";
const SOURCE_MODE_ICON: &str = "icon/ui/source.svg";
const SPLIT_MODE_ICON: &str = "icon/ui/split.svg";
const PREVIEW_MODE_ICON: &str = "icon/ui/preview.svg";
const MORE_ICON: &str = "icon/ui/more-horizontal.svg";
const RECOVERY_ICON: &str = "icon/ui/refresh.svg";
const CONFLICT_ICON: &str = "icon/ui/triangle-alert.svg";
const TOOLTIP_DELAY: std::time::Duration = std::time::Duration::from_millis(500);
const FORMAT_OVERFLOW_BREAKPOINT: f32 = 900.0;
const METADATA_OVERFLOW_BREAKPOINT: f32 = 760.0;
const ASYNC_CHARACTER_COUNT_THRESHOLD: usize = 1024 * 1024;
const CHARACTER_COUNT_IDLE_DELAY: std::time::Duration = std::time::Duration::from_millis(750);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatusTooltip {
    Sidebar,
    Mode(super::ViewMode),
}

#[derive(Default)]
pub(super) struct StatusBarState {
    pub sidebar_hovered: bool,
    pub mode_hovered: Option<super::ViewMode>,
    custom_button_hovered: Option<String>,
    word_count: Option<(Revision, usize)>,
    word_count_scheduled_revision: Option<Revision>,
    word_count_task: Option<Task<()>>,
    pub(super) format_overflow_open: bool,
    tooltip_hovered: Option<StatusTooltip>,
    tooltip_visible: Option<StatusTooltip>,
    tooltip_task: Option<Task<()>>,
    /// 模式分段跨 render 保持稳定焦点身份，避免鼠标与键盘维护两套选中状态。
    pub(super) mode_focus_handles: Option<[FocusHandle; 4]>,
    pub(super) sidebar_focus_handle: Option<FocusHandle>,
    pub(super) overflow_focus_handle: Option<FocusHandle>,
    pub(super) conflict_focus_handle: Option<FocusHandle>,
    pub(super) custom_button_focus_handles: HashMap<String, FocusHandle>,
}

impl StatusBarState {
    pub(super) fn cached_word_count(&self, revision: Revision) -> Option<usize> {
        self.word_count
            .filter(|(cached_revision, _)| *cached_revision == revision)
            .map(|(_, count)| count)
    }

    pub(super) fn set_word_count(&mut self, revision: Revision, count: usize) {
        self.word_count = Some((revision, count));
        self.word_count_scheduled_revision = None;
        self.word_count_task = None;
    }

    pub(super) fn invalidate_word_count(&mut self) {
        self.word_count_scheduled_revision = None;
        self.word_count_task = None;
    }

    pub(super) fn apply_virtual_text_edit(
        &mut self,
        old_revision: Revision,
        new_revision: Revision,
        old_text: &str,
        new_text: &str,
    ) {
        let Some(total) = self.cached_word_count(old_revision) else {
            self.invalidate_word_count();
            return;
        };
        // virtual edit 替换完整顶层区域，区域两侧是未修改换行；字素簇不跨换行，
        // 因此整区域的新旧差值是精确的，组合音标和 ZWJ emoji 仍按完整区域分段。
        let old_count = count_characters(old_text);
        let new_count = count_characters(new_text);
        self.set_word_count(
            new_revision,
            total.saturating_sub(old_count).saturating_add(new_count),
        );
    }
}

impl Editor {
    fn set_status_tooltip_hover(
        &mut self,
        tooltip: StatusTooltip,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        self.status_bar.tooltip_task = None;
        self.status_bar.tooltip_hovered = hovered.then_some(tooltip);
        self.status_bar.tooltip_visible = None;
        if hovered {
            self.status_bar.tooltip_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(TOOLTIP_DELAY).await;
                let _ = this.update(cx, |editor, cx| {
                    if editor.status_bar.tooltip_hovered == Some(tooltip) {
                        editor.status_bar.tooltip_visible = Some(tooltip);
                        editor.status_bar.tooltip_task = None;
                        cx.notify();
                    }
                });
            }));
        }
        cx.notify();
    }

    pub(super) fn set_status_sidebar_tooltip_hover(
        &mut self,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        self.set_status_tooltip_hover(StatusTooltip::Sidebar, hovered, cx);
    }

    pub(super) fn set_status_mode_tooltip_hover(
        &mut self,
        mode: super::ViewMode,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        self.status_bar.mode_hovered = hovered.then_some(mode);
        self.set_status_tooltip_hover(StatusTooltip::Mode(mode), hovered, cx);
    }

    pub(super) fn render_status_bar(
        &mut self,
        theme: &Theme,
        strings: &I18nStrings,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.focus_mode {
            return None;
        }
        let prefs = self.status_bar_preferences(cx);
        if !prefs.enabled {
            return None;
        }

        let c = &theme.colors;
        let d = &theme.dimensions;
        let custom_button_ids: HashSet<_> = prefs
            .custom_buttons
            .iter()
            .map(|button| button.id.clone())
            .collect();
        self.status_bar
            .custom_button_focus_handles
            .retain(|id, _| custom_button_ids.contains(id));

        let mut left_items: Vec<AnyElement> = Vec::new();

        if prefs.show_sidebar_toggle {
            left_items.push(render_sidebar_toggle(
                &mut self.status_bar,
                self.workspace.is_open,
                theme,
                strings,
                cx,
            ));
        }

        let mut right_items: Vec<AnyElement> = Vec::new();
        let mut overflow_items: Vec<AnyElement> = Vec::new();
        let large_file_status = self
            .source_surface
            .as_ref()
            .map(|view| view.read(cx).status_text());

        if let Some(status) = &large_file_status {
            right_items.push(
                div()
                    .id("status-bar-large-file-status")
                    .debug_selector(|| "status-bar-large-file-status".to_owned())
                    .max_w(px(260.0))
                    .overflow_hidden()
                    .truncate()
                    .text_size(px(11.0))
                    .text_color(c.text_placeholder)
                    .child(status.clone())
                    .into_any_element(),
            );
        }

        if should_render_file_status(self.recovered_session, self.external_file_conflict) {
            right_items.push(render_recovery_status(
                &mut self.status_bar,
                self.external_file_conflict,
                theme,
                strings,
                cx,
            ));
        }

        let viewport_width = viewport_width_for_status(window);
        let (resident_encoding, line_ending) = source_format_labels(
            &self.source_document.source_format_summary(),
            &self.source_encoding,
            strings,
        );
        let encoding = self
            .source_surface
            .as_ref()
            .map_or(resident_encoding, |view| view.read(cx).encoding_label());
        // 左侧只保留侧栏入口；文档状态与模式在右侧，低频源码格式在窄窗口进入 overflow。
        if viewport_width >= FORMAT_OVERFLOW_BREAKPOINT {
            right_items.push(render_source_format_label(encoding.clone(), theme));
            right_items.push(render_source_format_label(line_ending.clone(), theme));
        } else {
            overflow_items.push(render_overflow_text(
                "status-bar-overflow-encoding",
                encoding.clone(),
                theme,
            ));
            overflow_items.push(render_overflow_text(
                "status-bar-overflow-line-ending",
                line_ending.clone(),
                theme,
            ));
        }

        if let Some(large_file) = self.source_surface.disk_view_cloned() {
            let follow_enabled = large_file.read(cx).follow_enabled();
            let follow_view = large_file.clone();
            overflow_items.push(
                render_large_overflow_action(
                    "status-bar-large-follow",
                    if follow_enabled {
                        "Pause log following"
                    } else {
                        "Follow appended content"
                    },
                    follow_enabled,
                    theme,
                )
                .on_click(cx.listener(move |editor, _, _, cx| {
                    follow_view.update(cx, |view, cx| view.toggle_follow(cx));
                    editor.status_bar.format_overflow_open = false;
                    cx.notify();
                }))
                .into_any_element(),
            );
            overflow_items.push(render_overflow_text(
                "status-bar-large-reopen-encoding-label",
                "Reopen with encoding".to_owned(),
                theme,
            ));
            let current_encoding = large_file.read(cx).encoding_label();
            for (id, label, active_label, encoding) in [
                (
                    "status-bar-large-reopen-utf8",
                    "Reopen as UTF-8",
                    "UTF-8",
                    gmark_large_document::TextEncoding::Utf8 { bom: false },
                ),
                (
                    "status-bar-large-reopen-utf8-bom",
                    "Reopen as UTF-8 BOM",
                    "UTF-8 BOM",
                    gmark_large_document::TextEncoding::Utf8 { bom: true },
                ),
                (
                    "status-bar-large-reopen-utf16-le",
                    "Reopen as UTF-16 LE",
                    "UTF-16 LE",
                    gmark_large_document::TextEncoding::Utf16Le,
                ),
                (
                    "status-bar-large-reopen-utf16-be",
                    "Reopen as UTF-16 BE",
                    "UTF-16 BE",
                    gmark_large_document::TextEncoding::Utf16Be,
                ),
                (
                    "status-bar-large-reopen-windows-1252",
                    "Reopen as WINDOWS-1252",
                    "WINDOWS-1252",
                    gmark_large_document::TextEncoding::Legacy("windows-1252".to_owned()),
                ),
                (
                    "status-bar-large-reopen-gbk",
                    "Reopen as GBK",
                    "GBK",
                    gmark_large_document::TextEncoding::Legacy("gbk".to_owned()),
                ),
                (
                    "status-bar-large-reopen-shift-jis",
                    "Reopen as SHIFT_JIS",
                    "SHIFT_JIS",
                    gmark_large_document::TextEncoding::Legacy("shift_jis".to_owned()),
                ),
            ] {
                let encoding_view = large_file.clone();
                overflow_items.push(
                    render_large_overflow_action(
                        id,
                        label,
                        current_encoding == active_label,
                        theme,
                    )
                    .on_click(cx.listener(move |editor, _, window, cx| {
                        let encoding = encoding.clone();
                        encoding_view.update(cx, |view, cx| {
                            view.reopen_with_encoding(encoding, window, cx)
                        });
                        editor.view_mode = super::ViewMode::Source;
                        editor.status_bar.format_overflow_open = false;
                        cx.notify();
                    }))
                    .into_any_element(),
                );
            }
            if large_file.read(cx).has_registered_structure_view() {
                let structure_active = large_file.read(cx).structure_view_active();
                let structured_view = large_file.clone();
                overflow_items.push(
                    render_large_overflow_action(
                        "status-bar-large-structure",
                        if structure_active {
                            "Return to source"
                        } else {
                            "Open structured data view"
                        },
                        structure_active,
                        theme,
                    )
                    .on_click(cx.listener(move |editor, _, _, cx| {
                        structured_view.update(cx, |view, cx| {
                            if structure_active {
                                view.show_source_view(cx);
                            } else {
                                view.show_structure_view(cx);
                            }
                        });
                        // Structure 是同一 Tab 内的文件工具，不冒充 Live/Preview 模式。
                        editor.view_mode = super::ViewMode::Source;
                        editor.status_bar.format_overflow_open = false;
                        cx.notify();
                    }))
                    .into_any_element(),
                );
                let split_active = large_file.read(cx).structured_split_active();
                let split_view = large_file.clone();
                overflow_items.push(
                    render_large_overflow_action(
                        "status-bar-large-structured-split",
                        if split_active {
                            "Close source + structure split"
                        } else {
                            "Open source + structure split"
                        },
                        split_active,
                        theme,
                    )
                    .on_click(cx.listener(move |editor, _, _, cx| {
                        split_view.update(cx, |view, cx| {
                            if split_active {
                                view.show_source_view(cx);
                            } else {
                                view.show_split_view(cx);
                            }
                        });
                        editor.view_mode = super::ViewMode::Source;
                        editor.status_bar.format_overflow_open = false;
                        cx.notify();
                    }))
                    .into_any_element(),
                );
            }
            let endings_visible = large_file.read(cx).line_endings_visible();
            overflow_items.push(
                render_large_overflow_action(
                    "status-bar-large-line-endings",
                    if endings_visible {
                        "Hide line endings"
                    } else {
                        "Show line endings"
                    },
                    endings_visible,
                    theme,
                )
                .on_click(cx.listener(move |editor, _, _, cx| {
                    large_file.update(cx, |view, cx| view.toggle_line_endings(cx));
                    editor.status_bar.format_overflow_open = false;
                    cx.notify();
                }))
                .into_any_element(),
            );
        }

        if prefs.show_cursor_position
            && matches!(
                self.view_mode,
                super::ViewMode::Source | super::ViewMode::Split
            )
        {
            let position = self.source_surface.as_ref().map_or_else(
                || self.compute_source_cursor_position(cx),
                |view| view.read(cx).cursor_position(cx),
            );
            let cursor = render_cursor(position, theme);
            if viewport_width < METADATA_OVERFLOW_BREAKPOINT {
                overflow_items.push(cursor);
            } else {
                right_items.push(cursor);
            }
        }

        if large_file_status.is_none() && prefs.show_word_count {
            let revision = self.source_document.revision();
            let total_count = if let Some(count) = self.status_bar.cached_word_count(revision) {
                Some(count)
            } else if self.source_document.len() < ASYNC_CHARACTER_COUNT_THRESHOLD {
                let text = self.serialized_document_text(cx);
                let count = count_characters(&text);
                self.status_bar.set_word_count(revision, count);
                Some(count)
            } else {
                self.schedule_character_count(revision, cx);
                self.status_bar.word_count.map(|(_, count)| count)
            };
            let selection_count = self
                .selected_markdown_text(cx)
                .as_deref()
                .map(count_characters);
            if let Some(total_count) = total_count {
                let character_count =
                    render_character_count(selection_count, total_count, theme, strings);
                if viewport_width < METADATA_OVERFLOW_BREAKPOINT {
                    overflow_items.push(character_count);
                } else {
                    right_items.push(character_count);
                }
            }
        }

        for button in &prefs.custom_buttons {
            let button = render_custom_button(&mut self.status_bar, button, theme, cx);
            if viewport_width < METADATA_OVERFLOW_BREAKPOINT {
                overflow_items.push(button);
            } else {
                right_items.push(button);
            }
        }

        if !overflow_items.is_empty() {
            right_items.push(render_source_format_overflow_button(
                &mut self.status_bar,
                theme,
                cx,
            ));
        }

        if prefs.show_mode_switch {
            right_items.push(render_mode_switch(
                &mut self.status_bar,
                self.view_mode,
                self.source_surface.is_some(),
                theme,
                strings,
                cx,
            ));
        }

        let bar = div()
            .id("status-bar")
            .debug_selector(|| "status-bar".to_owned())
            .relative()
            .h(px(d.status_bar_height))
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .bg(c.status_bar_background)
            .border_t(px(1.0))
            .border_color(c.dialog_border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(d.status_bar_item_gap))
                    .children(left_items),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(d.status_bar_item_gap))
                    .children(right_items),
            )
            .children(
                (!overflow_items.is_empty() && self.status_bar.format_overflow_open).then(|| {
                    div()
                        .id("status-bar-format-overflow")
                        .debug_selector(|| "status-bar-format-overflow".to_owned())
                        .absolute()
                        .right(px(0.0))
                        .bottom(px(d.status_bar_height + 4.0))
                        .min_w(px(180.0))
                        .occlude()
                        .p(px(10.0))
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .bg(c.dialog_surface)
                        .border(px(d.dialog_border_width))
                        .border_color(c.dialog_border)
                        .rounded(px(6.0))
                        .shadow_lg()
                        .children(overflow_items)
                }),
            )
            .into_any_element();

        Some(bar)
    }

    /// 长文档字符统计只在后台物化 Rope；发布前校验 revision，旧结果不得覆盖新输入。
    fn schedule_character_count(&mut self, revision: Revision, cx: &mut Context<Self>) {
        if self.status_bar.word_count_scheduled_revision == Some(revision) {
            return;
        }
        let snapshot = self.source_document.snapshot();
        self.status_bar.word_count_scheduled_revision = Some(revision);
        self.status_bar.word_count_task =
            Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
                cx.background_executor()
                    .timer(CHARACTER_COUNT_IDLE_DELAY)
                    .await;
                let count = cx
                    .background_spawn(async move { count_characters(&snapshot.text()) })
                    .await;
                let _ = this.update(cx, |editor, cx| {
                    if editor.source_document.revision() != revision
                        || editor.status_bar.word_count_scheduled_revision != Some(revision)
                    {
                        return;
                    }
                    editor.status_bar.set_word_count(revision, count);
                    cx.notify();
                });
            }));
    }

    fn status_bar_preferences(&self, cx: &App) -> StatusBarPreferences {
        crate::preferences::EditorSettings::status_bar_preferences(cx)
    }

    /// Returns (line, col), both 1-based, from the source-mode selection snapshot.
    fn compute_source_cursor_position(&self, cx: &App) -> (usize, usize) {
        let snapshot = self.capture_source_selection_snapshot(cx);
        let cursor_offset =
            super::saturating_source_offset(snapshot.source_selection().head.byte_offset);
        let text = self.document.raw_source_text(cx);
        let clamped = cursor_offset.min(text.len());

        let line = text[..clamped].matches('\n').count() + 1;
        let last_newline = text[..clamped].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = text[last_newline..clamped].graphemes(true).count() + 1;
        (line, col)
    }
}

fn viewport_width_for_status(window: &Window) -> f32 {
    f32::from(window.viewport_size().width)
}

fn source_format_labels(
    format: &SourceFormatSummary,
    source_encoding: &crate::document_io::DocumentEncoding,
    strings: &I18nStrings,
) -> (String, String) {
    let encoding = if !source_encoding.is_utf8() {
        source_encoding.label().to_owned()
    } else if format.utf8_bom {
        strings.status_bar_encoding_utf8_bom.clone()
    } else {
        strings.status_bar_encoding_utf8.clone()
    };
    let line_ending = match format.line_endings {
        LineEndingStatus::None => match format.dominant {
            LineEnding::Lf => "LF".to_owned(),
            LineEnding::CrLf => "CRLF".to_owned(),
            LineEnding::Cr => "CR".to_owned(),
        },
        LineEndingStatus::Uniform(LineEnding::Lf) => "LF".to_owned(),
        LineEndingStatus::Uniform(LineEnding::CrLf) => "CRLF".to_owned(),
        LineEndingStatus::Uniform(LineEnding::Cr) => "CR".to_owned(),
        LineEndingStatus::Mixed => strings.status_bar_line_ending_mixed.clone(),
    };
    (encoding, line_ending)
}

fn render_source_format_label(label: String, theme: &Theme) -> AnyElement {
    div()
        .text_size(px(theme.dimensions.status_bar_text_size))
        .text_color(theme.colors.status_bar_text_dim)
        .child(label)
        .into_any_element()
}
#[cfg(test)]
#[path = "../../tests/unit/editor/status_bar.rs"]
mod tests;
#[path = "status_bar_parts/view.rs"]
mod view;
pub(crate) use view::count_characters;
#[cfg(test)]
use view::normalized_action_id;
use view::{
    render_character_count, render_cursor, render_custom_button, render_large_overflow_action,
    render_mode_switch, render_overflow_text, render_recovery_status, render_sidebar_toggle,
    render_source_format_overflow_button, should_render_file_status,
};
