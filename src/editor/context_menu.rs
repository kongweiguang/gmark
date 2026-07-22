// @author kongweiguang

//! Rendered-mode context menus and native table insertion dialog.

use std::time::Duration;

use gpui::*;

use super::{
    Editor, TableAxisSelection, ViewMode,
    render::{
        DialogButtonKind, DialogTitleIcon, clamped_floating_panel_origin,
        compact_menu_panel_height, dialog_actions, dialog_body, dialog_button, dialog_content,
        dialog_panel, dialog_title_with_icon, floating_submenu_x, menu_icon_slot, modal_overlay,
    },
    workspace::WorkspaceOperationKind,
};
use crate::components::{
    BlockKind, BlockRecord, CollapsedCaretAffinity, DismissTransientUi, EditingCommandHistory,
    EditingCommandId, EditingCommandPlan, INSERT_COMMANDS, InlineTextTree, TableAxisKind,
    TableColumnAlignment, TableData, UndoCaptureKind,
};
use crate::i18n::I18nManager;
use crate::theme::Theme;

const MINUS_ICON: &str = "icon/ui/minus.svg";
const PLUS_ICON: &str = "icon/ui/plus.svg";
const CHECK_ICON: &str = "icon/ui/check.svg";
const TABLE_ICON: &str = "icon/ui/table.svg";
const ALIGN_LEFT_ICON: &str = "icon/ui/align-left.svg";
const ALIGN_CENTER_ICON: &str = "icon/ui/align-center.svg";
const ALIGN_RIGHT_ICON: &str = "icon/ui/align-right.svg";
const ARROW_LEFT_ICON: &str = "icon/ui/arrow-left.svg";
const ARROW_RIGHT_ICON: &str = "icon/ui/arrow-right.svg";
const ARROW_UP_ICON: &str = "icon/ui/arrow-up.svg";
const ARROW_DOWN_ICON: &str = "icon/ui/arrow-down.svg";
const TRASH_ICON: &str = "icon/ui/trash.svg";
const COPY_ICON: &str = "icon/ui/copy.svg";

/// Target block position for inserting a native table.
#[derive(Clone, Copy)]
pub(super) enum TableInsertTarget {
    /// Insert the table immediately after the referenced block.
    After(EntityId),
    /// Append the table to the end of the current root list.
    Append,
}

/// Rendered-mode context menu currently open in the editor.
pub(super) enum ContextMenuState {
    /// General block context menu with an insert submenu.
    Insert {
        position: Point<Pixels>,
        target: TableInsertTarget,
        insert_hovered: bool,
        submenu_hovered: bool,
        submenu_open: bool,
    },
    /// Table row or column context menu for an existing native table.
    TableAxis {
        position: Point<Pixels>,
        selection: TableAxisSelection,
    },
    Spelling {
        position: Point<Pixels>,
        entity_id: EntityId,
        diagnostic: crate::spellcheck::SpellingDiagnostic,
    },
    Workspace {
        position: Point<Pixels>,
        path: std::path::PathBuf,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextMenuCommand {
    OpenInsertSubmenu,
    Insert(EditingCommandId),
    SpellingSuggestion(usize),
    AlignColumnLeft,
    AlignColumnCenter,
    AlignColumnRight,
    MoveColumnLeft,
    MoveColumnRight,
    InsertColumnBefore,
    InsertColumnAfter,
    DuplicateColumn,
    DeleteColumn,
    ToggleTableHeaders,
    MoveRowUp,
    MoveRowDown,
    InsertRowBefore,
    InsertRowAfter,
    DuplicateRow,
    DeleteRow,
    DeleteTable,
    WorkspaceNewFile,
    WorkspaceNewFolder,
    WorkspaceRename,
    WorkspaceMove,
    WorkspaceUndo,
    TabTogglePin,
    TabClose,
    TabCloseOthers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ContextMenuCommandEntry {
    command: ContextMenuCommand,
    enabled: bool,
}

#[derive(Default)]
struct ContextMenuCommandModel {
    main: Vec<ContextMenuCommandEntry>,
    submenu: Vec<ContextMenuCommandEntry>,
}

/// State for the table insertion dialog opened from the context menu.
pub(super) struct TableInsertDialogState {
    pub target: TableInsertTarget,
    pub body_rows: usize,
    pub columns: usize,
}

impl Editor {
    fn context_menu_command_model(&self, cx: &App) -> ContextMenuCommandModel {
        let entry = |command, enabled| ContextMenuCommandEntry { command, enabled };
        if let Some((_index, _pinned, can_close_others)) = self.tab_context_menu_info() {
            return ContextMenuCommandModel {
                main: vec![
                    entry(ContextMenuCommand::TabTogglePin, true),
                    entry(ContextMenuCommand::TabClose, true),
                    entry(ContextMenuCommand::TabCloseOthers, can_close_others),
                ],
                submenu: Vec::new(),
            };
        }

        let Some(menu) = self.context_menu.as_ref() else {
            return ContextMenuCommandModel::default();
        };
        match menu {
            ContextMenuState::Insert { .. } => ContextMenuCommandModel {
                main: vec![entry(ContextMenuCommand::OpenInsertSubmenu, true)],
                submenu: INSERT_COMMANDS
                    .into_iter()
                    .map(|command| entry(ContextMenuCommand::Insert(command), true))
                    .collect(),
            },
            ContextMenuState::Spelling { diagnostic, .. } => ContextMenuCommandModel {
                main: diagnostic
                    .replacements
                    .iter()
                    .enumerate()
                    .map(|(index, _)| entry(ContextMenuCommand::SpellingSuggestion(index), true))
                    .collect(),
                submenu: Vec::new(),
            },
            ContextMenuState::TableAxis { selection, .. } => {
                let Some(table) = self
                    .table_block_by_id(selection.table_block_id, cx)
                    .and_then(|block| block.read(cx).record.table.clone())
                else {
                    return ContextMenuCommandModel::default();
                };
                let main = match selection.kind {
                    TableAxisKind::Column => vec![
                        entry(ContextMenuCommand::InsertColumnBefore, true),
                        entry(ContextMenuCommand::InsertColumnAfter, true),
                        entry(ContextMenuCommand::DuplicateColumn, true),
                        entry(ContextMenuCommand::AlignColumnLeft, true),
                        entry(ContextMenuCommand::AlignColumnCenter, true),
                        entry(ContextMenuCommand::AlignColumnRight, true),
                        entry(ContextMenuCommand::MoveColumnLeft, selection.index > 0),
                        entry(
                            ContextMenuCommand::MoveColumnRight,
                            selection.index + 1 < table.column_count(),
                        ),
                        entry(ContextMenuCommand::DeleteColumn, table.column_count() > 1),
                        entry(ContextMenuCommand::DeleteTable, true),
                    ],
                    TableAxisKind::Row if selection.index == 0 => vec![
                        entry(ContextMenuCommand::InsertRowBefore, true),
                        entry(ContextMenuCommand::InsertRowAfter, true),
                        entry(ContextMenuCommand::DuplicateRow, true),
                        entry(ContextMenuCommand::ToggleTableHeaders, true),
                        entry(ContextMenuCommand::MoveRowUp, false),
                        entry(
                            ContextMenuCommand::MoveRowDown,
                            selection.index < table.rows.len(),
                        ),
                        entry(ContextMenuCommand::DeleteRow, !table.rows.is_empty()),
                        entry(ContextMenuCommand::DeleteTable, true),
                    ],
                    TableAxisKind::Row => vec![
                        entry(ContextMenuCommand::InsertRowBefore, true),
                        entry(ContextMenuCommand::InsertRowAfter, true),
                        entry(ContextMenuCommand::DuplicateRow, true),
                        entry(ContextMenuCommand::MoveRowUp, selection.index > 0),
                        entry(
                            ContextMenuCommand::MoveRowDown,
                            selection.index < table.rows.len(),
                        ),
                        entry(ContextMenuCommand::DeleteRow, true),
                        entry(ContextMenuCommand::DeleteTable, true),
                    ],
                };
                ContextMenuCommandModel {
                    main,
                    submenu: Vec::new(),
                }
            }
            ContextMenuState::Workspace { .. } => ContextMenuCommandModel {
                main: vec![
                    entry(ContextMenuCommand::WorkspaceNewFile, true),
                    entry(ContextMenuCommand::WorkspaceNewFolder, true),
                    entry(
                        ContextMenuCommand::WorkspaceRename,
                        !self.workspace_context_target_is_root(),
                    ),
                    entry(
                        ContextMenuCommand::WorkspaceMove,
                        !self.workspace_context_target_is_root(),
                    ),
                    entry(
                        ContextMenuCommand::WorkspaceUndo,
                        self.workspace_can_undo_file_operation(),
                    ),
                ],
                submenu: Vec::new(),
            },
        }
    }

    fn adjacent_context_menu_entry(
        entries: &[ContextMenuCommandEntry],
        current: Option<usize>,
        forward: bool,
    ) -> Option<usize> {
        if entries.is_empty() {
            return None;
        }
        let start = current.unwrap_or(if forward { entries.len() - 1 } else { 0 });
        (1..=entries.len())
            .map(|step| {
                if forward {
                    (start + step) % entries.len()
                } else {
                    (start + entries.len() - (step % entries.len())) % entries.len()
                }
            })
            .find(|index| entries[*index].enabled)
    }

    fn edge_context_menu_entry(entries: &[ContextMenuCommandEntry], first: bool) -> Option<usize> {
        if first {
            entries.iter().position(|entry| entry.enabled)
        } else {
            entries.iter().rposition(|entry| entry.enabled)
        }
    }

    fn scroll_selected_spelling_suggestion_into_view(&self) {
        if matches!(
            self.context_menu.as_ref(),
            Some(ContextMenuState::Spelling { .. })
        ) && let Some(index) = self.context_menu_keyboard_item
        {
            // The diagnostic message is child zero in the tracked panel.
            self.context_menu_scroll_handle.scroll_to_item(index + 1);
        }
    }

    fn insert_context_submenu_open(&self) -> bool {
        matches!(
            self.context_menu,
            Some(ContextMenuState::Insert {
                submenu_open: true,
                ..
            })
        )
    }

    fn set_insert_context_submenu_open(&mut self, open: bool, cx: &mut Context<Self>) {
        let Some(ContextMenuState::Insert { submenu_open, .. }) = self.context_menu.as_mut() else {
            return;
        };
        if *submenu_open != open {
            *submenu_open = open;
            self.context_menu_submenu_close_task = None;
            cx.notify();
        }
    }

    fn execute_context_menu_command(
        &mut self,
        command: ContextMenuCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let click = ClickEvent::default();
        match command {
            ContextMenuCommand::OpenInsertSubmenu => {
                self.set_insert_context_submenu_open(true, cx);
                self.context_menu_keyboard_submenu_item = Some(0);
            }
            ContextMenuCommand::Insert(command) => {
                self.on_context_menu_insert_command(command, &click, window, cx)
            }
            ContextMenuCommand::SpellingSuggestion(index) => {
                self.apply_spelling_suggestion(index, &click, window, cx)
            }
            ContextMenuCommand::AlignColumnLeft => {
                self.on_align_table_column_left(&click, window, cx)
            }
            ContextMenuCommand::AlignColumnCenter => {
                self.on_align_table_column_center(&click, window, cx)
            }
            ContextMenuCommand::AlignColumnRight => {
                self.on_align_table_column_right(&click, window, cx)
            }
            ContextMenuCommand::MoveColumnLeft => {
                self.on_move_table_column_left(&click, window, cx)
            }
            ContextMenuCommand::MoveColumnRight => {
                self.on_move_table_column_right(&click, window, cx)
            }
            ContextMenuCommand::InsertColumnBefore => {
                self.on_insert_table_column_before(&click, window, cx)
            }
            ContextMenuCommand::InsertColumnAfter => {
                self.on_insert_table_column_after(&click, window, cx)
            }
            ContextMenuCommand::DuplicateColumn => {
                self.on_duplicate_table_column(&click, window, cx)
            }
            ContextMenuCommand::DeleteColumn => self.on_delete_table_column(&click, window, cx),
            ContextMenuCommand::ToggleTableHeaders => {
                self.on_toggle_table_headers(&click, window, cx)
            }
            ContextMenuCommand::MoveRowUp => self.on_move_table_row_up(&click, window, cx),
            ContextMenuCommand::MoveRowDown => self.on_move_table_row_down(&click, window, cx),
            ContextMenuCommand::InsertRowBefore => {
                self.on_insert_table_row_before(&click, window, cx)
            }
            ContextMenuCommand::InsertRowAfter => {
                self.on_insert_table_row_after(&click, window, cx)
            }
            ContextMenuCommand::DuplicateRow => self.on_duplicate_table_row(&click, window, cx),
            ContextMenuCommand::DeleteRow => self.on_delete_table_row(&click, window, cx),
            ContextMenuCommand::DeleteTable => self.on_delete_selected_table(&click, window, cx),
            ContextMenuCommand::WorkspaceNewFile => {
                self.open_workspace_operation_dialog(WorkspaceOperationKind::NewFile, window, cx)
            }
            ContextMenuCommand::WorkspaceNewFolder => {
                self.open_workspace_operation_dialog(WorkspaceOperationKind::NewFolder, window, cx)
            }
            ContextMenuCommand::WorkspaceRename => {
                self.open_workspace_operation_dialog(WorkspaceOperationKind::Rename, window, cx)
            }
            ContextMenuCommand::WorkspaceMove => {
                self.open_workspace_operation_dialog(WorkspaceOperationKind::Move, window, cx)
            }
            ContextMenuCommand::WorkspaceUndo => self.undo_workspace_file_operation(cx),
            ContextMenuCommand::TabTogglePin => {
                if let Some((index, _, _)) = self.tab_context_menu_info() {
                    self.dismiss_tab_context_menu();
                    self.toggle_pin_tab(index, cx);
                }
            }
            ContextMenuCommand::TabClose => {
                if let Some((index, _, _)) = self.tab_context_menu_info() {
                    self.dismiss_tab_context_menu();
                    self.request_close_tab_index(index, cx);
                }
            }
            ContextMenuCommand::TabCloseOthers => {
                if let Some((index, _, _)) = self.tab_context_menu_info() {
                    self.dismiss_tab_context_menu();
                    self.request_close_other_tabs(index, cx);
                }
            }
        }
    }

    pub(super) fn handle_context_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.context_menu.is_none() && self.tab_context_menu_info().is_none() {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if !matches!(
            key,
            "up" | "down" | "left" | "right" | "enter" | "escape" | "home" | "end"
        ) {
            return false;
        }
        if key == "escape" {
            self.dismiss_contextual_overlays(cx);
            return true;
        }

        let model = self.context_menu_command_model(cx);
        let submenu_open = self.insert_context_submenu_open();
        match key {
            "up" | "down" => {
                let forward = key == "down";
                if submenu_open {
                    self.context_menu_keyboard_submenu_item = Self::adjacent_context_menu_entry(
                        &model.submenu,
                        self.context_menu_keyboard_submenu_item,
                        forward,
                    );
                } else {
                    self.context_menu_keyboard_item = Self::adjacent_context_menu_entry(
                        &model.main,
                        self.context_menu_keyboard_item,
                        forward,
                    );
                    self.scroll_selected_spelling_suggestion_into_view();
                }
                cx.notify();
            }
            "home" | "end" => {
                let first = key == "home";
                if submenu_open {
                    self.context_menu_keyboard_submenu_item =
                        Self::edge_context_menu_entry(&model.submenu, first);
                } else {
                    self.context_menu_keyboard_item =
                        Self::edge_context_menu_entry(&model.main, first);
                    self.scroll_selected_spelling_suggestion_into_view();
                }
                cx.notify();
            }
            "left" if submenu_open => {
                self.context_menu_keyboard_submenu_item = None;
                self.set_insert_context_submenu_open(false, cx);
            }
            "right" => {
                let command = self
                    .context_menu_keyboard_item
                    .and_then(|index| model.main.get(index))
                    .filter(|entry| entry.enabled)
                    .map(|entry| entry.command);
                if command == Some(ContextMenuCommand::OpenInsertSubmenu) {
                    self.execute_context_menu_command(
                        ContextMenuCommand::OpenInsertSubmenu,
                        window,
                        cx,
                    );
                }
            }
            "enter" => {
                let entry = if submenu_open {
                    self.context_menu_keyboard_submenu_item
                        .and_then(|index| model.submenu.get(index))
                } else {
                    self.context_menu_keyboard_item
                        .and_then(|index| model.main.get(index))
                };
                if let Some(entry) = entry.filter(|entry| entry.enabled) {
                    self.execute_context_menu_command(entry.command, window, cx);
                }
            }
            _ => {}
        }
        true
    }

    pub(super) fn clear_context_menu_keyboard_cursor(&mut self, cx: &mut Context<Self>) {
        let changed = self.context_menu_keyboard_item.take().is_some()
            || self.context_menu_keyboard_submenu_item.take().is_some();
        if changed {
            cx.notify();
        }
    }

    pub(super) fn root_ancestor_entity_id(&self, entity_id: EntityId) -> EntityId {
        let mut current = entity_id;
        while let Some(location) = self.document.find_block_location(current) {
            let Some(parent) = location.parent else {
                break;
            };
            current = parent.entity_id();
        }
        current
    }

    fn open_insert_context_menu(
        &mut self,
        position: Point<Pixels>,
        target: TableInsertTarget,
        cx: &mut Context<Self>,
    ) {
        if self.view_mode != ViewMode::Rendered {
            return;
        }

        self.close_menu_bar(cx);
        self.context_menu_submenu_close_task = None;
        self.context_menu = Some(ContextMenuState::Insert {
            position,
            target,
            insert_hovered: false,
            submenu_hovered: false,
            submenu_open: false,
        });
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        self.context_menu_scroll_handle
            .set_offset(point(px(0.0), px(0.0)));
        cx.notify();
    }

    pub(super) fn open_table_axis_context_menu(
        &mut self,
        position: Point<Pixels>,
        selection: TableAxisSelection,
        cx: &mut Context<Self>,
    ) {
        if self.view_mode != ViewMode::Rendered {
            return;
        }

        self.close_menu_bar(cx);
        self.context_menu_submenu_close_task = None;
        self.context_menu = Some(ContextMenuState::TableAxis {
            position,
            selection,
        });
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        self.context_menu_scroll_handle
            .set_offset(point(px(0.0), px(0.0)));
        cx.notify();
    }

    pub(super) fn close_table_insert_dialog(&mut self, cx: &mut Context<Self>) {
        if self.table_insert_dialog.take().is_some() {
            cx.notify();
        }
    }

    fn close_context_menu(&mut self, cx: &mut Context<Self>) {
        let had_menu = self.context_menu.take().is_some();
        let had_keyboard = self.context_menu_keyboard_item.take().is_some()
            || self.context_menu_keyboard_submenu_item.take().is_some();
        let had_submenu_close = self.context_menu_submenu_close_task.take().is_some();
        if had_menu || had_keyboard || had_submenu_close {
            cx.notify();
        }
    }

    fn dismiss_active_contextual_editing_popovers(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(block) = self
            .active_entity_id
            .and_then(|entity_id| self.focusable_entity_by_id(entity_id))
        else {
            return false;
        };
        let mut changed = false;
        block.update(cx, |block, block_cx| {
            if block.dismiss_contextual_editing_popovers() {
                changed = true;
                block_cx.notify();
            }
        });
        changed
    }

    pub(super) fn dismiss_contextual_overlays(&mut self, cx: &mut Context<Self>) {
        let had_contextual_editing = self.dismiss_active_contextual_editing_popovers(cx);
        let had_menu = self.context_menu.take().is_some();
        let had_keyboard = self.context_menu_keyboard_item.take().is_some()
            || self.context_menu_keyboard_submenu_item.take().is_some();
        let had_dialog = self.table_insert_dialog.take().is_some();
        let had_table_fragment = self.table_fragment_merge.take().is_some();
        let had_diagram_overlay = self.diagram_overlay.take().is_some();
        let had_link_completion = self.workspace_link_completion.take().is_some();
        let had_workspace_dialog = self.dismiss_workspace_operation_dialog();
        let had_command_palette = self.dismiss_command_palette();
        let had_tab_context_menu = self.dismiss_tab_context_menu();
        let had_status_overflow = self.status_bar.format_overflow_open;
        self.status_bar.format_overflow_open = false;
        let had_submenu_close = self.context_menu_submenu_close_task.take().is_some();
        if had_contextual_editing
            || had_menu
            || had_dialog
            || had_table_fragment
            || had_diagram_overlay
            || had_link_completion
            || had_workspace_dialog
            || had_command_palette
            || had_tab_context_menu
            || had_status_overflow
            || had_keyboard
            || had_submenu_close
        {
            cx.notify();
        }
    }
}

#[path = "context_menu_parts/controller.rs"]
mod controller;
#[path = "context_menu_parts/menu_view.rs"]
mod menu_view;
#[path = "context_menu_parts/table_dialog.rs"]
mod table_dialog;

#[cfg(test)]
#[path = "../../tests/unit/editor/context_menu.rs"]
mod tests;
