// @author kongweiguang

//! Incomplete GFM table-row paste detection and confirmed merge transactions.

use super::*;
use crate::components::parse_table_fragment_rows;
use crate::i18n::I18nStrings;
use crate::theme::Theme;

impl Editor {
    pub(super) fn render_table_fragment_merge_prompt(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.table_fragment_merge.as_ref()?;
        let editor = cx.entity().downgrade();
        let buttons = state
            .targets
            .iter()
            .enumerate()
            .map(|(index, target)| {
                let label = match target.direction {
                    TableFragmentMergeDirection::IntoPrevious => {
                        strings.large_document_text("table_fragment_merge_previous")
                    }
                    TableFragmentMergeDirection::IntoNext => {
                        strings.large_document_text("table_fragment_merge_next")
                    }
                };
                let editor = editor.clone();
                div()
                    .id(SharedString::from(format!("table-fragment-merge-{index}")))
                    .debug_selector(move || format!("table-fragment-merge-{index}"))
                    .h(px(theme.dimensions.dialog_button_height))
                    .px(px(theme.dimensions.dialog_button_padding_x))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .bg(theme.colors.dialog_primary_button_bg)
                    .text_color(theme.colors.dialog_primary_button_text)
                    .hover(|this| this.bg(theme.colors.dialog_primary_button_hover))
                    .cursor_pointer()
                    .child(label.to_owned())
                    .on_click(move |_, _window, cx| {
                        let _ = editor.update(cx, |editor, cx| {
                            editor.confirm_table_fragment_merge(index, cx)
                        });
                    })
            })
            .collect::<Vec<_>>();
        let dismiss_editor = cx.entity().downgrade();

        Some(
            div()
                .id("table-fragment-merge-prompt")
                .debug_selector(|| "table-fragment-merge-prompt".to_owned())
                .absolute()
                .right(px(18.0))
                .bottom(px(48.0))
                .max_w(px(560.0))
                .p(px(12.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(theme.dimensions.dialog_radius))
                .border(px(theme.dimensions.dialog_border_width))
                .border_color(theme.colors.dialog_border)
                .bg(theme.colors.dialog_surface)
                .shadow_lg()
                .text_color(theme.colors.dialog_body)
                .child(
                    div().flex_1().min_w(px(120.0)).child(
                        strings
                            .large_document_text("table_fragment_prompt")
                            .to_owned(),
                    ),
                )
                .children(buttons)
                .child(
                    div()
                        .id("table-fragment-merge-cancel")
                        .debug_selector(|| "table-fragment-merge-cancel".to_owned())
                        .h(px(theme.dimensions.dialog_button_height))
                        .px(px(theme.dimensions.dialog_button_padding_x))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.0))
                        .border(px(theme.dimensions.dialog_border_width))
                        .border_color(theme.colors.dialog_border)
                        .bg(theme.colors.dialog_secondary_button_bg)
                        .text_color(theme.colors.dialog_secondary_button_text)
                        .hover(|this| this.bg(theme.colors.dialog_secondary_button_hover))
                        .cursor_pointer()
                        .child(strings.large_document_text("cancel").to_owned())
                        .on_click(move |_, _window, cx| {
                            let _ = dismiss_editor
                                .update(cx, |editor, cx| editor.dismiss_table_fragment_merge(cx));
                        }),
                )
                .into_any_element(),
        )
    }

    /// 只在普通驻留文档、同一父级的直接相邻表格上建立候选；大文档投影没有
    /// 同时稳定映射目标与片段时按普通粘贴处理，避免把视图实体误当源码身份。
    pub(super) fn table_fragment_targets_for_paste(
        &self,
        parent: Option<&Entity<Block>>,
        insertion_index: usize,
        lines: &[String],
        cx: &App,
    ) -> Vec<TableFragmentMergeTarget> {
        if self.virtual_surface.is_some() {
            return Vec::new();
        }

        let siblings = parent
            .map(|parent| parent.read(cx).children.clone())
            .unwrap_or_else(|| self.document.root_blocks().to_vec());
        let mut targets = Vec::with_capacity(2);
        let mut push_target =
            |table: Option<&Entity<Block>>, direction: TableFragmentMergeDirection| {
                let Some(table) = table else { return };
                let block = table.read(cx);
                if block.kind() != BlockKind::Table {
                    return;
                }
                let Some(table_data) = block.record.table.as_ref() else {
                    return;
                };
                let Some(rows) = parse_table_fragment_rows(lines, table_data.column_count()) else {
                    return;
                };
                targets.push(TableFragmentMergeTarget {
                    table_id: table.entity_id(),
                    direction,
                    rows,
                });
            };

        push_target(
            insertion_index
                .checked_sub(1)
                .and_then(|index| siblings.get(index)),
            TableFragmentMergeDirection::IntoPrevious,
        );
        push_target(
            siblings.get(insertion_index.saturating_add(1)),
            TableFragmentMergeDirection::IntoNext,
        );
        targets
    }

    pub(super) fn install_table_fragment_merge_candidate(
        &mut self,
        parent: Option<&Entity<Block>>,
        fragment_ids: Vec<EntityId>,
        targets: Vec<TableFragmentMergeTarget>,
        cx: &mut Context<Self>,
    ) {
        self.table_fragment_merge =
            (!fragment_ids.is_empty() && !targets.is_empty()).then(|| TableFragmentMergeState {
                base_revision: self.source_document.revision(),
                parent_id: parent.map(Entity::entity_id),
                fragment_ids,
                targets,
            });
        cx.notify();
    }

    pub(super) fn dismiss_table_fragment_merge(&mut self, cx: &mut Context<Self>) {
        if self.table_fragment_merge.take().is_some() {
            cx.notify();
        }
    }

    /// 确认前重新验证 revision、父级、相邻关系、目标身份和列宽。失败只关闭建议，
    /// 原始粘贴块保持不变；成功的表格替换与片段删除由一个不可合并撤销快照覆盖。
    pub(super) fn confirm_table_fragment_merge(
        &mut self,
        target_index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.table_fragment_merge.clone() else {
            return;
        };
        let Some(target) = state.targets.get(target_index).cloned() else {
            self.dismiss_table_fragment_merge(cx);
            return;
        };
        if self.source_document.revision() != state.base_revision {
            self.dismiss_table_fragment_merge(cx);
            return;
        }

        let Some(table_block) = self.document.block_entity_by_id(target.table_id) else {
            self.dismiss_table_fragment_merge(cx);
            return;
        };
        let Some(table_location) = self.document.find_block_location(target.table_id) else {
            self.dismiss_table_fragment_merge(cx);
            return;
        };
        let table_parent_id = table_location.parent.as_ref().map(Entity::entity_id);
        if table_parent_id != state.parent_id || table_block.read(cx).kind() != BlockKind::Table {
            self.dismiss_table_fragment_merge(cx);
            return;
        }

        let mut fragment_locations = Vec::with_capacity(state.fragment_ids.len());
        for fragment_id in &state.fragment_ids {
            let Some(location) = self.document.find_block_location(*fragment_id) else {
                self.dismiss_table_fragment_merge(cx);
                return;
            };
            if location.parent.as_ref().map(Entity::entity_id) != state.parent_id {
                self.dismiss_table_fragment_merge(cx);
                return;
            }
            fragment_locations.push(location.index);
        }
        fragment_locations.sort_unstable();
        let contiguous = fragment_locations
            .windows(2)
            .all(|pair| pair[1] == pair[0].saturating_add(1));
        let adjacent = match target.direction {
            TableFragmentMergeDirection::IntoPrevious => fragment_locations
                .first()
                .is_some_and(|first| table_location.index.saturating_add(1) == *first),
            TableFragmentMergeDirection::IntoNext => fragment_locations
                .last()
                .is_some_and(|last| last.saturating_add(1) == table_location.index),
        };
        let Some(mut table) = table_block.read(cx).record.table.clone() else {
            self.dismiss_table_fragment_merge(cx);
            return;
        };
        if !contiguous
            || !adjacent
            || target.rows.is_empty()
            || target
                .rows
                .iter()
                .any(|row| row.len() != table.column_count())
        {
            self.dismiss_table_fragment_merge(cx);
            return;
        }

        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        match target.direction {
            TableFragmentMergeDirection::IntoPrevious => table.rows.extend(target.rows),
            TableFragmentMergeDirection::IntoNext => {
                let mut rows = target.rows;
                rows.append(&mut table.rows);
                table.rows = rows;
            }
        }
        table_block.update(cx, move |block, _cx| block.record.table = Some(table));
        self.document.with_structure_mutation(cx, |document, cx| {
            for fragment_id in state.fragment_ids {
                let _ = document.remove_block_by_id_raw(fragment_id, cx);
            }
        });
        self.table_fragment_merge = None;
        self.rebuild_table_runtimes(cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        if let Some(cell) = table_block
            .read(cx)
            .table_runtime
            .as_ref()
            .and_then(|runtime| match target.direction {
                TableFragmentMergeDirection::IntoPrevious => {
                    runtime.rows.last().and_then(|row| row.first()).cloned()
                }
                TableFragmentMergeDirection::IntoNext => {
                    runtime.rows.first().and_then(|row| row.first()).cloned()
                }
            })
        {
            self.focus_block(cell.entity_id());
        }
        cx.notify();
    }
}
