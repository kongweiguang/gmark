// @author kongweiguang

//! Action-backed command palette with background filtering.

use std::time::Duration;

use gpui::*;

use super::{Block, BlockRecord, Editor, render::menu_icon_slot};
use crate::app_menu::menu_action_icon;
use crate::components::{
    BlockEvent, BoldSelection, CodeSelection, EditingCommandCategory, EditingCommandId,
    EditingSelectionContext, EditingViewMode, ItalicSelection, LinkSelection, SetBulletedList,
    SetCodeBlock, SetHeading1, SetHeading2, SetHeading3, SetNumberedList, SetParagraph, SetQuote,
    SetTaskList, StrikethroughSelection, UnderlineSelection,
};
use crate::i18n::I18nStrings;
use crate::theme::Theme;

const FILTER_DEBOUNCE: Duration = Duration::from_millis(20);
const MAX_RESULTS: usize = 100;
const SEARCH_ICON: &str = "icon/ui/search.svg";
const CLOSE_ICON: &str = "icon/ui/close.svg";

fn editing_command_for_action(action: &dyn Action) -> Option<EditingCommandId> {
    if action.as_any().is::<BoldSelection>() {
        Some(EditingCommandId::Bold)
    } else if action.as_any().is::<ItalicSelection>() {
        Some(EditingCommandId::Italic)
    } else if action.as_any().is::<UnderlineSelection>() {
        Some(EditingCommandId::Underline)
    } else if action.as_any().is::<StrikethroughSelection>() {
        Some(EditingCommandId::Strikethrough)
    } else if action.as_any().is::<CodeSelection>() {
        Some(EditingCommandId::InlineCode)
    } else if action.as_any().is::<LinkSelection>() {
        Some(EditingCommandId::Link)
    } else if action.as_any().is::<SetParagraph>() {
        Some(EditingCommandId::Paragraph)
    } else if action.as_any().is::<SetHeading1>() {
        Some(EditingCommandId::Heading1)
    } else if action.as_any().is::<SetHeading2>() {
        Some(EditingCommandId::Heading2)
    } else if action.as_any().is::<SetHeading3>() {
        Some(EditingCommandId::Heading3)
    } else if action.as_any().is::<SetBulletedList>() {
        Some(EditingCommandId::BulletedList)
    } else if action.as_any().is::<SetNumberedList>() {
        Some(EditingCommandId::NumberedList)
    } else if action.as_any().is::<SetTaskList>() {
        Some(EditingCommandId::TaskList)
    } else if action.as_any().is::<SetQuote>() {
        Some(EditingCommandId::Quote)
    } else if action.as_any().is::<SetCodeBlock>() {
        Some(EditingCommandId::CodeBlock)
    } else {
        None
    }
}

struct PaletteCommand {
    label: String,
    shortcut: String,
    icon: Option<&'static str>,
    action: Box<dyn Action>,
}

pub(super) struct CommandPaletteState {
    input: Entity<Block>,
    commands: Vec<PaletteCommand>,
    filtered: Vec<usize>,
    selected: usize,
    generation: u64,
    task: Option<Task<()>>,
}

impl Editor {
    pub(crate) fn on_command_palette_action(
        &mut self,
        _: &crate::components::CommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_menu_bar(cx);
        self.dismiss_contextual_overlays(cx);
        let mut commands = window
            .available_actions(cx)
            .into_iter()
            .filter(|action| self.command_palette_action_available(action.as_ref(), cx))
            .map(|action| {
                let icon = menu_action_icon(action.as_ref());
                PaletteCommand {
                    label: humanize_action_name(action.name()),
                    shortcut: window.keystroke_text_for(action.as_ref()),
                    icon,
                    action,
                }
            })
            .collect::<Vec<_>>();
        commands.sort_by_key(|command| command.label.to_lowercase());
        let input = cx.new(|cx| {
            let mut block = Block::with_record(cx, BlockRecord::paragraph(String::new()));
            block.set_source_raw_mode();
            block
        });
        cx.subscribe(&input, Self::on_command_palette_input_event)
            .detach();
        input.read(cx).focus_handle.focus(window);
        self.command_palette = Some(CommandPaletteState {
            input,
            commands,
            filtered: Vec::new(),
            selected: 0,
            generation: 0,
            task: None,
        });
        self.schedule_command_palette_filter(cx);
        cx.notify();
    }

    fn command_palette_action_available(&self, action: &dyn Action, cx: &App) -> bool {
        let Some(command) = editing_command_for_action(action) else {
            return true;
        };
        let Some(block) = self
            .active_entity_id
            .and_then(|entity_id| self.focusable_entity_by_id(entity_id))
        else {
            return false;
        };
        let block = block.read(cx);
        let mut context = block.editing_command_context();
        context.view_mode = match self.view_mode {
            super::ViewMode::Rendered => EditingViewMode::Rendered,
            super::ViewMode::Source => EditingViewMode::Source,
            super::ViewMode::Split => EditingViewMode::Split,
            super::ViewMode::Preview => EditingViewMode::Preview,
        };
        if context.selection == EditingSelectionContext::AcrossBlocks
            && command.descriptor().category == EditingCommandCategory::Inline
            && !block.editor_selection_supports_inline_commands
        {
            return false;
        }
        command.is_available(context)
    }

    fn on_command_palette_input_event(
        &mut self,
        _: Entity<Block>,
        event: &BlockEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, BlockEvent::Changed) {
            self.schedule_command_palette_filter(cx);
        }
    }

    fn schedule_command_palette_filter(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.command_palette.as_mut() else {
            return;
        };
        state.generation = state.generation.wrapping_add(1);
        state.task = None;
        let generation = state.generation;
        let query = state.input.read(cx).display_text().trim().to_owned();
        let labels = state
            .commands
            .iter()
            .map(|command| command.label.clone())
            .collect::<Vec<_>>();
        state.task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor().timer(FILTER_DEBOUNCE).await;
            let filtered = cx
                .background_spawn(async move { filter_command_labels(&labels, &query) })
                .await;
            let _ = this.update(cx, |editor, cx| {
                let Some(state) = editor.command_palette.as_mut() else {
                    return;
                };
                if state.generation != generation {
                    return;
                }
                state.task = None;
                state.filtered = filtered;
                state.selected = 0;
                cx.notify();
            });
        }));
    }

    pub(super) fn dismiss_command_palette(&mut self) -> bool {
        self.command_palette.take().is_some()
    }

    pub(super) fn handle_command_palette_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.command_palette.as_mut() else {
            return false;
        };
        match event.keystroke.key.as_str() {
            "up" => state.selected = state.selected.saturating_sub(1),
            "down" => {
                state.selected = (state.selected + 1).min(state.filtered.len().saturating_sub(1));
            }
            "enter" => {
                let action = state
                    .filtered
                    .get(state.selected)
                    .and_then(|index| state.commands.get(*index))
                    .map(|command| command.action.boxed_clone());
                self.command_palette = None;
                if let Some(action) = action {
                    window.dispatch_action(action, cx);
                }
            }
            "escape" => self.command_palette = None,
            _ => return false,
        }
        cx.notify();
        true
    }

    pub(super) fn render_command_palette_overlay(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.command_palette.as_ref()?;
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let editor = cx.entity().downgrade();
        let dismiss_editor = editor.clone();
        let close_editor = editor.clone();
        let close_tooltip: SharedString = strings.ui_close.clone().into();
        let empty_message = if state.input.read(cx).display_text().trim().is_empty() {
            strings.command_palette_prompt.clone()
        } else {
            strings.command_palette_no_results.clone()
        };
        Some(
            div()
                .id("command-palette-overlay")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .occlude()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(82.0))
                .bg(c.dialog_backdrop)
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    let _ = dismiss_editor.update(cx, |editor, cx| {
                        editor.command_palette = None;
                        cx.notify();
                    });
                })
                .child(
                    div()
                        .id("command-palette-dialog")
                        .debug_selector(|| "command-palette-dialog".to_owned())
                        .w(px(560.0))
                        .max_w(relative(0.92))
                        .max_h(relative(0.74))
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .bg(c.dialog_surface)
                        .border(px(d.dialog_border_width))
                        .border_color(c.dialog_border)
                        .rounded(px(d.dialog_radius.min(8.0)))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .child(
                            div()
                                .h(px(38.0))
                                .px(px(14.0))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .truncate()
                                        .text_size(px(t.dialog_title_size))
                                        .font_weight(t.dialog_title_weight.to_font_weight())
                                        .text_color(c.dialog_title)
                                        .child(strings.command_palette_title.clone()),
                                )
                                .child(
                                    div()
                                        .id("command-palette-close")
                                        .debug_selector(|| "command-palette-close".to_owned())
                                        .size(px(28.0))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(5.0))
                                        .cursor_pointer()
                                        .hover(|this| this.bg(c.dialog_secondary_button_hover))
                                        .tooltip(move |_window, cx| {
                                            crate::ui::ui_tooltip(close_tooltip.clone(), cx)
                                        })
                                        .child(svg().path(CLOSE_ICON).size(px(15.0)))
                                        .on_click(move |_event, _window, cx| {
                                            let _ = close_editor.update(cx, |editor, cx| {
                                                editor.command_palette = None;
                                                cx.notify();
                                            });
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .id("command-palette-input")
                                .debug_selector(|| "command-palette-input".to_owned())
                                .mx(px(12.0))
                                .mb(px(10.0))
                                .min_h(px(40.0))
                                .px(px(10.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .rounded(px(6.0))
                                .border(px(d.dialog_border_width))
                                .border_color(c.dialog_border)
                                .child(
                                    div()
                                        .id("command-palette-search-icon")
                                        .debug_selector(|| "command-palette-search-icon".to_owned())
                                        .size(px(16.0))
                                        .flex_shrink_0()
                                        .text_color(c.dialog_muted)
                                        .child(svg().path(SEARCH_ICON).size(px(16.0))),
                                )
                                .child(div().flex_1().min_w(px(0.0)).child(state.input.clone())),
                        )
                        .child(
                            div()
                                .id("command-palette-results")
                                .debug_selector(|| "command-palette-results".to_owned())
                                .flex_1()
                                .min_h(px(52.0))
                                .overflow_y_scroll()
                                .px(px(8.0))
                                .pb(px(8.0))
                                .children(state.filtered.is_empty().then(|| {
                                    div()
                                        .px(px(10.0))
                                        .py(px(14.0))
                                        .text_size(px(t.dialog_body_size))
                                        .text_color(c.dialog_muted)
                                        .child(empty_message)
                                }))
                                .children(state.filtered.iter().enumerate().filter_map(
                                    |(row, index)| {
                                        let command = state.commands.get(*index)?;
                                        let action = command.action.boxed_clone();
                                        let editor = editor.clone();
                                        Some(
                                            div()
                                                .id(("command-palette-result", row))
                                                .debug_selector(move || {
                                                    format!("command-palette-result-{row}")
                                                })
                                                .h(px(34.0))
                                                .w_full()
                                                .px(px(10.0))
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .rounded(px(5.0))
                                                .bg(if row == state.selected {
                                                    c.selection
                                                } else {
                                                    hsla(0.0, 0.0, 0.0, 0.0)
                                                })
                                                .hover(|this| {
                                                    this.bg(c.dialog_secondary_button_hover)
                                                })
                                                .cursor_pointer()
                                                .child(
                                                    menu_icon_slot(command.icon, c.dialog_muted)
                                                        .debug_selector(move || {
                                                            format!(
                                                                "command-palette-result-icon-{row}"
                                                            )
                                                        }),
                                                )
                                                .child(
                                                    div()
                                                        .min_w(px(0.0))
                                                        .flex_grow()
                                                        .overflow_hidden()
                                                        .truncate()
                                                        .debug_selector(move || {
                                                            format!(
                                                                "command-palette-result-label-{row}"
                                                            )
                                                        })
                                                        .text_size(px(t.dialog_body_size))
                                                        .text_color(c.text_default)
                                                        .child(command.label.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .flex_shrink_0()
                                                        .max_w(px(160.0))
                                                        .overflow_hidden()
                                                        .truncate()
                                                        .text_right()
                                                        .debug_selector(move || {
                                                            format!(
                                                                "command-palette-result-shortcut-{row}"
                                                            )
                                                        })
                                                        .text_size(px(t.dialog_body_size * 0.86))
                                                        .text_color(c.dialog_muted)
                                                        .child(command.shortcut.clone()),
                                                )
                                                .on_click(move |_event, window, cx| {
                                                    let action = action.boxed_clone();
                                                    let _ = editor.update(cx, |editor, _cx| {
                                                        editor.command_palette = None;
                                                    });
                                                    window.dispatch_action(action, cx);
                                                }),
                                        )
                                    },
                                )),
                        ),
                )
                .into_any_element(),
        )
    }
}

fn humanize_action_name(name: &str) -> String {
    let name = name.rsplit("::").next().unwrap_or(name);
    let mut output = String::with_capacity(name.len() + 8);
    let mut previous_lowercase = false;
    for ch in name.chars() {
        if ch == '_' || ch == '-' {
            if !output.ends_with(' ') {
                output.push(' ');
            }
            previous_lowercase = false;
            continue;
        }
        if ch.is_uppercase() && previous_lowercase {
            output.push(' ');
        }
        output.push(ch);
        previous_lowercase = ch.is_lowercase() || ch.is_ascii_digit();
    }
    output
}

fn filter_command_labels(labels: &[String], query: &str) -> Vec<usize> {
    let mut matches = labels
        .iter()
        .enumerate()
        .filter_map(|(index, label)| {
            let score = command_match_score(label, query)?;
            Some((index, score))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    matches.truncate(MAX_RESULTS);
    matches.into_iter().map(|(index, _)| index).collect()
}

fn command_match_score(label: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let query = query.to_lowercase();
    let label = label.to_lowercase();
    if label.starts_with(&query) {
        return Some(10_000 - i64::try_from(label.len()).unwrap_or(i64::MAX));
    }
    if let Some(index) = label.find(&query) {
        return Some(
            7_500
                - i64::try_from(index).unwrap_or(i64::MAX)
                - i64::try_from(label.len()).unwrap_or(i64::MAX),
        );
    }
    let mut query_chars = query.chars();
    let mut wanted = query_chars.next()?;
    let mut score = 0i64;
    let mut previous = None;
    for (index, ch) in label.chars().enumerate() {
        if ch != wanted {
            continue;
        }
        score += 100;
        if previous == Some(index.saturating_sub(1)) {
            score += 60;
        }
        previous = Some(index);
        if let Some(next) = query_chars.next() {
            wanted = next;
        } else {
            return Some(score - i64::try_from(label.len()).unwrap_or(i64::MAX));
        }
    }
    None
}

#[cfg(test)]
#[path = "../../tests/unit/editor/command_palette.rs"]
mod tests;
