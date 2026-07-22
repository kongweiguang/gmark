// @author kongweiguang

//! Workspace Markdown-link completion triggered by transient `[[query` text.

use std::collections::HashMap;
use std::path::{Component, Path};

use super::*;
use crate::i18n::I18nStrings;
use crate::theme::Theme;

const WORKSPACE_LINK_MAX_RESULTS: usize = 50;

impl Editor {
    pub(super) fn refresh_workspace_link_completion(
        &mut self,
        block: &Entity<Block>,
        cx: &mut Context<Self>,
    ) {
        self.workspace_link_completion = None;
        if self.view_mode != ViewMode::Rendered || self.virtual_surface.is_some() {
            return;
        }
        let Some(current_file) = self.file_path.as_ref() else {
            return;
        };
        let block_ref = block.read(cx);
        if !workspace_link_block_kind(&block_ref.kind()) || !block_ref.selected_range.is_empty() {
            return;
        }
        let caret = block_ref.current_to_clean_offset(block_ref.selected_range.end);
        let text = block_ref.record.title.visible_text();
        if block_ref
            .record
            .title
            .attributes_for_insertion_at(caret)
            .style
            .code
        {
            return;
        }
        let Some((trigger_range, query)) = detect_workspace_link_trigger(&text, caret) else {
            return;
        };
        let Some((root, paths)) = self.workspace.markdown_snapshot() else {
            return;
        };
        if !current_file.starts_with(&root) {
            return;
        }
        let candidates = rank_workspace_link_candidates(&root, paths, current_file, &query);
        if candidates.is_empty() {
            return;
        }
        self.workspace_link_completion = Some(WorkspaceLinkCompletionState {
            block_id: block.entity_id(),
            base_revision: self.source_document.revision(),
            trigger_range,
            selected: 0,
            candidates,
        });
        cx.notify();
    }

    pub(super) fn handle_workspace_link_completion_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.workspace_link_completion.as_mut() else {
            return false;
        };
        match event.keystroke.key.as_str() {
            "up" => state.selected = state.selected.saturating_sub(1),
            "down" => {
                state.selected = (state.selected + 1).min(state.candidates.len().saturating_sub(1));
            }
            "enter" | "tab" => {
                let selected = state.selected;
                self.accept_workspace_link_completion(selected, cx);
            }
            "escape" => self.workspace_link_completion = None,
            _ => return false,
        }
        cx.notify();
        true
    }

    pub(super) fn handle_diagram_overlay_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.diagram_overlay.is_some() && event.keystroke.key == "escape" {
            self.close_diagram_overlay(cx);
            return true;
        }
        false
    }

    pub(super) fn accept_workspace_link_completion(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.workspace_link_completion.clone() else {
            return;
        };
        if self.source_document.revision() != state.base_revision {
            self.workspace_link_completion = None;
            return;
        }
        let Some(candidate) = state.candidates.get(index) else {
            self.workspace_link_completion = None;
            return;
        };
        let Some(current_file) = self.file_path.as_ref() else {
            self.workspace_link_completion = None;
            return;
        };
        let Some(current_dir) = current_file.parent() else {
            self.workspace_link_completion = None;
            return;
        };
        let Some(target) = relative_markdown_path(current_dir, &candidate.path) else {
            self.workspace_link_completion = None;
            return;
        };
        let escaped_target = if target.contains([' ', '(', ')']) {
            format!("<{target}>")
        } else {
            target
        };
        let escaped_title = candidate
            .title
            .replace('\\', "\\\\")
            .replace('[', "\\[")
            .replace(']', "\\]");
        let markdown = format!("[{escaped_title}]({escaped_target})");
        let Some(block) = self.document.block_entity_by_id(state.block_id) else {
            self.workspace_link_completion = None;
            return;
        };

        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        block.update(cx, |block, cx| {
            block.replace_text_in_visible_range(
                state.trigger_range.clone(),
                &markdown,
                None,
                false,
                cx,
            );
        });
        self.workspace_link_completion = None;
    }

    pub(super) fn render_workspace_link_completion(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.workspace_link_completion.as_ref()?;
        if self.source_document.revision() != state.base_revision {
            return None;
        }
        let editor = cx.entity().downgrade();
        let rows = state
            .candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                let selected = index == state.selected;
                let label = candidate.title.clone();
                let detail = candidate
                    .disambiguate
                    .then(|| candidate.relative_workspace_path.clone());
                let editor = editor.clone();
                div()
                    .id(("workspace-link-candidate", index))
                    .debug_selector(move || format!("workspace-link-candidate-{index}"))
                    .w_full()
                    .px(px(10.0))
                    .py(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .bg(if selected {
                        theme.colors.chrome_hover
                    } else {
                        theme.colors.dialog_surface
                    })
                    .cursor_pointer()
                    .child(div().flex_1().min_w(px(0.0)).child(label))
                    .children(detail.map(|detail| {
                        div()
                            .text_color(theme.colors.dialog_muted)
                            .text_size(px(theme.typography.code_size))
                            .child(detail)
                    }))
                    .on_click(move |_, _, cx| {
                        let _ = editor.update(cx, |editor, cx| {
                            editor.accept_workspace_link_completion(index, cx)
                        });
                    })
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        Some(
            div()
                .id("workspace-link-completion")
                .debug_selector(|| "workspace-link-completion".to_owned())
                .absolute()
                .right(px(18.0))
                .bottom(px(48.0))
                .w(px(420.0))
                .max_h(px(360.0))
                .overflow_y_scroll()
                .rounded(px(theme.dimensions.dialog_radius))
                .border(px(theme.dimensions.dialog_border_width))
                .border_color(theme.colors.dialog_border)
                .bg(theme.colors.dialog_surface)
                .shadow_lg()
                .text_color(theme.colors.dialog_body)
                .children(rows)
                .tooltip({
                    let text: SharedString = strings
                        .large_document_text("workspace_link_completion")
                        .to_owned()
                        .into();
                    move |_window, cx| crate::ui::ui_tooltip(text.clone(), cx)
                })
                .into_any_element(),
        )
    }
}

fn workspace_link_block_kind(kind: &BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Paragraph
            | BlockKind::Heading { .. }
            | BlockKind::BulletedListItem
            | BlockKind::NumberedListItem
            | BlockKind::TaskListItem { .. }
            | BlockKind::Quote
            | BlockKind::Callout(_)
            | BlockKind::FootnoteDefinition
    )
}

fn detect_workspace_link_trigger(
    text: &str,
    caret: usize,
) -> Option<(std::ops::Range<usize>, String)> {
    if caret > text.len() || !text.is_char_boundary(caret) {
        return None;
    }
    let prefix = &text[..caret];
    let start = prefix.rfind("[[")?;
    let escape_count = prefix.as_bytes()[..start]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    if escape_count % 2 == 1 {
        return None;
    }
    let query = &prefix[start + 2..];
    if query.contains(['\n', '[', ']']) || inside_inline_code(prefix, start) {
        return None;
    }
    Some((start..caret, query.to_owned()))
}

fn inside_inline_code(text: &str, offset: usize) -> bool {
    let mut run = 0usize;
    let mut open: Option<usize> = None;
    for ch in text[..offset].chars() {
        if ch == '`' {
            run += 1;
        } else if run > 0 {
            open = if open == Some(run) { None } else { Some(run) };
            run = 0;
        }
    }
    if run > 0 {
        open = if open == Some(run) { None } else { Some(run) };
    }
    open.is_some()
}

fn rank_workspace_link_candidates(
    root: &Path,
    paths: Vec<PathBuf>,
    current_file: &Path,
    query: &str,
) -> Vec<WorkspaceLinkCandidate> {
    let query_lower = query.to_lowercase();
    let mut ranked = paths
        .into_iter()
        .filter(|path| path != current_file)
        .filter_map(|path| {
            let title = path.file_stem()?.to_string_lossy().to_string();
            let relative = path
                .strip_prefix(root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            let title_lower = title.to_lowercase();
            let (group, score) = if title_lower.starts_with(&query_lower) {
                (0u8, -(title.len() as i64))
            } else if let Some(score) = super::workspace::subsequence_score(&title, query) {
                (1, -score)
            } else {
                let score = super::workspace::subsequence_score(&relative, query)?;
                (2, -score)
            };
            Some((group, score, relative, path, title))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        (left.0, left.1, left.2.to_lowercase()).cmp(&(right.0, right.1, right.2.to_lowercase()))
    });
    ranked.truncate(WORKSPACE_LINK_MAX_RESULTS);
    let counts = ranked.iter().fold(HashMap::new(), |mut counts, item| {
        *counts.entry(item.4.to_lowercase()).or_insert(0usize) += 1;
        counts
    });
    ranked
        .into_iter()
        .map(
            |(_, _, relative_workspace_path, path, title)| WorkspaceLinkCandidate {
                disambiguate: counts.get(&title.to_lowercase()).copied().unwrap_or(0) > 1,
                path,
                relative_workspace_path,
                title,
            },
        )
        .collect()
}

fn relative_markdown_path(from_dir: &Path, target: &Path) -> Option<String> {
    let from = from_dir.components().collect::<Vec<_>>();
    let to = target.components().collect::<Vec<_>>();
    let mut common = 0usize;
    while common < from.len() && common < to.len() && component_eq(from[common], to[common]) {
        common += 1;
    }
    if common == 0 {
        return None;
    }
    let mut parts = vec!["..".to_owned(); from.len().saturating_sub(common)];
    parts.extend(to[common..].iter().filter_map(|component| match component {
        Component::Normal(value) => Some(value.to_string_lossy().to_string()),
        _ => None,
    }));
    Some(parts.join("/"))
}

fn component_eq(left: Component<'_>, right: Component<'_>) -> bool {
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        detect_workspace_link_trigger, rank_workspace_link_candidates, relative_markdown_path,
    };

    #[test]
    fn trigger_rejects_escaped_and_inline_code_markers() {
        assert_eq!(
            detect_workspace_link_trigger("See [[guide", 11),
            Some((4..11, "guide".to_owned()))
        );
        assert!(detect_workspace_link_trigger(r"See \[[guide", 12).is_none());
        assert!(detect_workspace_link_trigger("`[[guide`", 8).is_none());
        assert!(detect_workspace_link_trigger("[[guide\n", 8).is_none());
    }

    #[test]
    fn relative_paths_use_forward_slashes_and_parent_segments() {
        let from = Path::new(r"C:\workspace\notes\daily");
        let target = Path::new(r"C:\workspace\guides\Start Here.md");
        assert_eq!(
            relative_markdown_path(from, target).as_deref(),
            Some("../../guides/Start Here.md")
        );
    }

    #[test]
    fn ranking_prefers_stem_prefix_then_stem_fuzzy_then_path_fuzzy() {
        let root = Path::new(r"C:\workspace");
        let current = root.join("current.md");
        let candidates = rank_workspace_link_candidates(
            root,
            vec![
                root.join("other").join("query-path.md"),
                root.join("Quick Guide.md"),
                root.join("q-u-e-r-y.md"),
                current.clone(),
            ],
            &current,
            "qu",
        );
        assert_eq!(candidates[0].title, "Quick Guide");
        assert!(candidates.iter().all(|candidate| candidate.path != current));
    }

    #[test]
    fn duplicate_titles_request_relative_path_disambiguation() {
        let root = Path::new(r"C:\workspace");
        let candidates = rank_workspace_link_candidates(
            root,
            vec![
                root.join("a").join("Guide.md"),
                root.join("b").join("Guide.md"),
            ],
            &root.join("current.md"),
            "guide",
        );
        assert_eq!(candidates.len(), 2);
        assert!(candidates.iter().all(|candidate| candidate.disambiguate));
    }
}
