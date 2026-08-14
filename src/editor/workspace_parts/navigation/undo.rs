// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn on_workspace_undo_file_operation(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.undo_workspace_file_operation(cx);
    }

    pub(in crate::editor) fn undo_workspace_file_operation(&mut self, cx: &mut Context<Self>) {
        self.context_menu = None;
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        let Some(operation) = self.workspace.undo_file_operation.clone() else {
            return;
        };
        match operation {
            WorkspaceUndoOperation::Move(plan) => self.execute_workspace_move_plan(plan, false, cx),
            WorkspaceUndoOperation::Create(plan) => {
                self.execute_workspace_create_plan(plan, true, cx)
            }
        }
    }
}
