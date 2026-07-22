// @author kongweiguang

//! Non-modal, source-authoritative document find and replace.

use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use gmark_document::{Revision, TextEdit, Transaction};
use gpui::*;
use regex::{Regex, RegexBuilder};

use super::{Block, BlockRecord, Editor, PreparedSplitProjection, UndoSelectionSnapshot, ViewMode};
use crate::components::{BlockEvent, UndoCaptureKind};
use crate::i18n::{I18nManager, I18nStrings};
use crate::theme::Theme;

const FIND_DEBOUNCE: Duration = Duration::from_millis(40);
const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
const MAX_FIND_MATCHES: usize = 20_000;
const FIND_CASE_ICON: &str = "icon/ui/case-sensitive.svg";
const FIND_WORD_ICON: &str = "icon/ui/whole-word.svg";
const FIND_REGEX_ICON: &str = "icon/ui/regex.svg";
const CHEVRON_UP_ICON: &str = "icon/ui/chevron-up.svg";
const CHEVRON_DOWN_ICON: &str = "icon/ui/chevron-down.svg";
const CLOSE_ICON: &str = "icon/ui/close.svg";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct FindOptions {
    pub(super) case_sensitive: bool,
    pub(super) whole_word: bool,
    pub(super) regex: bool,
}

pub(super) struct FindPanelState {
    pub(super) query: Entity<Block>,
    pub(super) replacement: Entity<Block>,
    pub(super) show_replace: bool,
    pub(super) options: FindOptions,
    pub(super) matches: Vec<Range<usize>>,
    pub(super) selected: usize,
    pub(super) error: Option<String>,
    pub(super) truncated: bool,
    pub(super) revision: Revision,
    generation: u64,
    task: Option<Task<()>>,
    tooltip_hovered: Option<&'static str>,
    pub(super) tooltip_visible: Option<&'static str>,
    tooltip_task: Option<Task<()>>,
    keyboard_target: FindKeyboardTarget,
    focus_handle: FocusHandle,
    restore_focus: Option<EntityId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FindKeyboardTarget {
    #[default]
    Query,
    Replacement,
    CaseSensitive,
    WholeWord,
    Regex,
    Previous,
    Next,
    Close,
}

impl FindKeyboardTarget {
    fn is_control(self) -> bool {
        !matches!(self, Self::Query | Self::Replacement)
    }
}

#[path = "find_replace_parts/controller.rs"]
mod controller;

pub(super) struct FindResult {
    pub(super) revision: Revision,
    pub(super) matches: Vec<Range<usize>>,
    pub(super) error: Option<String>,
    pub(super) truncated: bool,
}

impl Editor {
    pub(crate) fn on_find_in_document_action(
        &mut self,
        _: &crate::components::FindInDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(document_host) = self.document_host.clone() {
            document_host.update(cx, |document_host, cx| {
                document_host.on_find_in_document(&crate::components::FindInDocument, window, cx);
            });
            return;
        }
        self.open_find_panel(false, window, cx);
    }

    pub(crate) fn on_replace_in_document_action(
        &mut self,
        _: &crate::components::ReplaceInDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(document_host) = self.document_host.clone() {
            document_host.update(cx, |document_host, cx| {
                document_host.on_find_in_document(&crate::components::FindInDocument, window, cx);
            });
            return;
        }
        self.open_find_panel(true, window, cx);
    }

    pub(crate) fn on_find_next_action(
        &mut self,
        _: &crate::components::FindNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(document_host) = self.document_host.clone() {
            document_host.update(cx, |document_host, cx| {
                document_host.on_find_next(&crate::components::FindNext, window, cx);
            });
            return;
        }
        if self.find_panel.is_none() {
            self.open_find_panel(false, window, cx);
        } else {
            self.navigate_find_match(1, window, cx);
        }
    }

    pub(crate) fn on_find_previous_action(
        &mut self,
        _: &crate::components::FindPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(document_host) = self.document_host.clone() {
            document_host.update(cx, |document_host, cx| {
                document_host.on_find_previous(&crate::components::FindPrevious, window, cx);
            });
            return;
        }
        if self.find_panel.is_none() {
            self.open_find_panel(false, window, cx);
        } else {
            self.navigate_find_match(-1, window, cx);
        }
    }

    fn open_find_panel(&mut self, show_replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menu_bar(cx);
        self.dismiss_contextual_overlays(cx);
        if let Some(state) = self.find_panel.as_mut() {
            state.show_replace |= show_replace;
            if show_replace {
                state.keyboard_target = FindKeyboardTarget::Replacement;
                state.replacement.read(cx).focus_handle.focus(window);
            } else {
                state.keyboard_target = FindKeyboardTarget::Query;
                state.query.read(cx).focus_handle.focus(window);
            }
            cx.notify();
            return;
        }

        let selection = self.capture_source_selection_snapshot(cx);
        let selection_range = selection.range();
        let source = self.source_document.snapshot();
        let initial_query = (!selection_range.is_empty())
            .then(|| source.text_for_range(selection_range).ok())
            .flatten()
            .filter(|text| !text.contains(['\r', '\n']) && text.len() <= 256)
            .unwrap_or_default();
        let strings = cx.global::<I18nManager>().strings_arc();
        let query_placeholder = strings.find_query_placeholder.clone();
        let query = cx.new(|cx| {
            let mut block = Block::with_record(cx, BlockRecord::paragraph(initial_query));
            block.set_source_raw_mode();
            block.set_input_placeholder(query_placeholder);
            block
        });
        let replace_placeholder = strings.find_replace_placeholder.clone();
        let replacement = cx.new(|cx| {
            let mut block = Block::with_record(cx, BlockRecord::paragraph(String::new()));
            block.set_source_raw_mode();
            block.set_input_placeholder(replace_placeholder);
            block
        });
        cx.subscribe(&query, Self::on_find_panel_input_event)
            .detach();
        cx.subscribe(&replacement, Self::on_find_panel_input_event)
            .detach();
        query.read(cx).focus_handle.focus(window);
        let find_focus_handle = cx.focus_handle();
        self.find_panel = Some(FindPanelState {
            query,
            replacement,
            show_replace,
            options: FindOptions::default(),
            matches: Vec::new(),
            selected: 0,
            error: None,
            truncated: false,
            revision: source.revision(),
            generation: 0,
            task: None,
            tooltip_hovered: None,
            tooltip_visible: None,
            tooltip_task: None,
            keyboard_target: FindKeyboardTarget::Query,
            focus_handle: find_focus_handle,
            restore_focus: self.active_entity_id,
        });
        self.schedule_find(cx);
        cx.notify();
    }

    fn set_find_tooltip_hover(&mut self, id: &'static str, hovered: bool, cx: &mut Context<Self>) {
        let Some(state) = self.find_panel.as_mut() else {
            return;
        };
        state.tooltip_task = None;
        state.tooltip_hovered = hovered.then_some(id);
        state.tooltip_visible = None;
        if hovered {
            state.tooltip_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(TOOLTIP_DELAY).await;
                let _ = this.update(cx, |editor, cx| {
                    let Some(state) = editor.find_panel.as_mut() else {
                        return;
                    };
                    if state.tooltip_hovered == Some(id) {
                        state.tooltip_visible = Some(id);
                        state.tooltip_task = None;
                        cx.notify();
                    }
                });
            }));
        }
        cx.notify();
    }

    fn on_find_panel_input_event(
        &mut self,
        input: Entity<Block>,
        event: &BlockEvent,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, BlockEvent::Changed) {
            return;
        }
        let is_query = self
            .find_panel
            .as_ref()
            .is_some_and(|state| state.query.entity_id() == input.entity_id());
        if is_query {
            self.schedule_find(cx);
        } else {
            cx.notify();
        }
    }

    pub(super) fn schedule_find(&mut self, cx: &mut Context<Self>) {
        let anchor = self.capture_source_selection_snapshot(cx).range().end;
        let Some(state) = self.find_panel.as_mut() else {
            return;
        };
        state.generation = state.generation.wrapping_add(1);
        state.task = None;
        state.error = None;
        let generation = state.generation;
        let query = state.query.read(cx).display_text().to_owned();
        if query.is_empty() {
            state.matches.clear();
            state.selected = 0;
            state.truncated = false;
            cx.notify();
            return;
        }
        let options = state.options;
        let snapshot = self.source_document.snapshot();
        state.task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor().timer(FIND_DEBOUNCE).await;
            let result = cx
                .background_spawn(async move {
                    find_matches(&snapshot.text(), &query, options, snapshot.revision())
                })
                .await;
            let _ = this.update(cx, |editor, cx| {
                let current_revision = editor.source_document.revision();
                let stale = {
                    let Some(state) = editor.find_panel.as_mut() else {
                        return;
                    };
                    if state.generation != generation {
                        return;
                    }
                    state.task = None;
                    if result.revision != current_revision {
                        true
                    } else {
                        state.revision = result.revision;
                        state.error = result.error;
                        state.truncated = result.truncated;
                        state.matches = result.matches;
                        state.selected = state
                            .matches
                            .iter()
                            .position(|range| range.start >= anchor)
                            .unwrap_or(0);
                        false
                    }
                };
                if stale {
                    editor.schedule_find(cx);
                    return;
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

fn render_find_tooltip(label: String, theme: &Theme) -> AnyElement {
    div()
        .id("document-find-tooltip")
        .debug_selector(|| "document-find-tooltip".to_owned())
        .absolute()
        .top(px(30.0))
        .left(px(0.0))
        .min_w(px(92.0))
        .h(px(26.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .whitespace_nowrap()
        .rounded(px(5.0))
        .bg(theme.colors.dialog_surface)
        .border(px(theme.dimensions.dialog_border_width))
        .border_color(theme.colors.dialog_border)
        .shadow_md()
        .text_size(px(theme.dimensions.status_bar_text_size))
        .text_color(theme.colors.text_default)
        .child(label)
        .into_any_element()
}

pub(super) fn compile_find_regex(query: &str, options: FindOptions) -> Result<Regex, regex::Error> {
    let pattern = if options.regex {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .unicode(true)
        .build()
}

pub(super) fn find_matches(
    source: &str,
    query: &str,
    options: FindOptions,
    revision: Revision,
) -> FindResult {
    let regex = match compile_find_regex(query, options) {
        Ok(regex) => regex,
        Err(error) => {
            return FindResult {
                revision,
                matches: Vec::new(),
                error: Some(error.to_string()),
                truncated: false,
            };
        }
    };
    let mut matches = Vec::new();
    let mut truncated = false;
    for found in regex.find_iter(source) {
        let range = found.start()..found.end();
        if options.whole_word && !has_word_boundaries(source, &range) {
            continue;
        }
        if matches.len() == MAX_FIND_MATCHES {
            truncated = true;
            break;
        }
        matches.push(range);
    }
    FindResult {
        revision,
        matches,
        error: None,
        truncated,
    }
}

fn has_word_boundaries(source: &str, range: &Range<usize>) -> bool {
    let left_is_word = source[..range.start]
        .chars()
        .next_back()
        .is_some_and(is_word_character);
    let start_is_word = source[range.start..]
        .chars()
        .next()
        .is_some_and(is_word_character);
    let end_is_word = source[..range.end]
        .chars()
        .next_back()
        .is_some_and(is_word_character);
    let right_is_word = source[range.end..]
        .chars()
        .next()
        .is_some_and(is_word_character);
    (!left_is_word || !start_is_word) && (!right_is_word || !end_is_word)
}

fn is_word_character(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

pub(super) fn replacement_for_range(
    regex: &Regex,
    source: &str,
    range: Range<usize>,
    template: &str,
    expand_captures: bool,
) -> Option<String> {
    if !expand_captures {
        return Some(template.to_owned());
    }
    let captures = regex.captures_at(source, range.start)?;
    let matched = captures.get(0)?;
    if matched.start() != range.start || matched.end() != range.end {
        return None;
    }
    let mut replacement = String::new();
    captures.expand(template, &mut replacement);
    Some(replacement)
}
