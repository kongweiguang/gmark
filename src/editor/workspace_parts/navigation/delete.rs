// @author kongweiguang

use super::*;

impl Editor {
    /// 删除由后台 Shell 完成后主动唤醒窗口，避免没有后续输入时旧对话框继续停留。
    pub(super) fn execute_workspace_delete_plan(
        &mut self,
        plan: super::workspace_file_ops::WorkspaceDeletePlan,
        cx: &mut Context<Self>,
    ) {
        if self.workspace.file_operation_task.is_some() {
            return;
        }
        let (_, has_dirty) = self.workspace_tabs_affected_by_path(&plan.workspace_path);
        if has_dirty {
            if let Some(dialog) = self.workspace.operation_dialog.as_mut() {
                dialog.error = Some(
                    cx.global::<I18nManager>()
                        .strings()
                        .workspace_delete_dirty_error
                        .clone(),
                );
            }
            cx.notify();
            return;
        }
        let generation = self.workspace.file_operation_generation.wrapping_add(1);
        self.workspace.file_operation_generation = generation;
        // Windows Shell 可能需要数百毫秒；保留禁用的进度对话框可明确反馈点击已生效，
        // 同时避免磁盘失败前先把仍存在的节点从树中移除。
        if let Some(dialog) = self.workspace.operation_dialog.as_mut() {
            dialog.running = true;
            dialog.error = None;
        }
        self.workspace.operation_error = None;
        cx.notify();
        let worker_plan = plan.clone();
        self.workspace.file_operation_task =
            Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
                let result = cx
                    .background_spawn(async move { worker_plan.execute() })
                    .await;
                let _ = this.update(cx, |editor, cx| {
                    if editor.workspace.file_operation_generation != generation {
                        return;
                    }
                    editor.workspace.file_operation_task = None;
                    match result {
                        Ok(()) => {
                            let tree_updated = editor.workspace.remove_path(&plan.workspace_path);
                            if matches!(
                                editor.workspace.selected.as_ref(),
                                Some(WorkspaceSelection::File(path))
                                    if path.starts_with(&plan.workspace_path)
                            ) {
                                editor.workspace.selected = None;
                            }
                            let tabs_closed = editor
                                .close_tabs_affected_by_deleted_path(&plan.workspace_path, cx);
                            editor.workspace.operation_dialog = None;
                            editor.workspace.operation_error = if tabs_closed {
                                None
                            } else {
                                Some(
                                    cx.global::<I18nManager>()
                                        .strings()
                                        .workspace_delete_completed_dirty_error
                                        .clone(),
                                )
                            };
                            editor.workspace.undo_file_operation = None;
                            editor
                                .workspace
                                .pinned_empty_directories
                                .retain(|path| !path.starts_with(&plan.workspace_path));
                            if !tree_updated {
                                editor.invalidate_workspace_file_tree();
                            }
                            editor.sync_workspace_after_document_path_change(cx);
                        }
                        Err(error) => {
                            if let Some(dialog) = editor.workspace.operation_dialog.as_mut() {
                                dialog.running = false;
                                dialog.error = Some(error.to_string());
                            } else {
                                editor.workspace.operation_error = Some(error.to_string());
                            }
                            editor.invalidate_workspace_file_tree();
                            editor.sync_workspace_after_document_path_change(cx);
                        }
                    }
                    // 原生回收站回调可能跨过模态窗口边界；完成状态必须自己触发重绘，
                    // 否则 UI 只能等下一次鼠标或键盘输入才会显示成功/失败。
                    cx.notify();
                    cx.refresh_windows();
                });
            }));
        cx.notify();
    }
}
