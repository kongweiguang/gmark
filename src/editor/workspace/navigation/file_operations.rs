// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn execute_workspace_create_plan(
        &mut self,
        plan: super::workspace_file_ops::WorkspaceCreatePlan,
        undo: bool,
        cx: &mut Context<Self>,
    ) {
        if self.workspace.file_operation_task.is_some() {
            return;
        }
        if undo && self.file_path.as_ref() == Some(&plan.path) && self.is_document_dirty() {
            self.workspace.operation_error = Some(
                cx.global::<crate::i18n::I18nManager>()
                    .strings()
                    .workspace_operation_dirty_error
                    .clone(),
            );
            cx.notify();
            return;
        }
        let generation = self.workspace.file_operation_generation.wrapping_add(1);
        self.workspace.file_operation_generation = generation;
        if let Some(dialog) = self.workspace.operation_dialog.as_mut() {
            dialog.running = true;
            dialog.error = None;
        }
        self.workspace.operation_error = None;
        let worker_plan = plan.clone();
        self.workspace.file_operation_task =
            Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
                let result = cx
                    .background_spawn(async move {
                        if undo {
                            worker_plan.undo()
                        } else {
                            worker_plan.execute()
                        }
                    })
                    .await;
                let _ = this.update(cx, |editor, cx| {
                    if editor.workspace.file_operation_generation != generation {
                        return;
                    }
                    editor.workspace.file_operation_task = None;
                    match result {
                        Ok(()) => {
                            editor.workspace.operation_dialog = None;
                            editor.workspace.operation_error = None;
                            let tree_updated;
                            if undo {
                                if editor.file_path.as_ref() == Some(&plan.path) {
                                    // 新建的源码文件使用 DocumentHost；撤销后必须同时解除后端，
                                    // 避免已删除文件的分页会话继续占据当前空白标签。
                                    editor.document_host = None;
                                    editor.replace_document_from_markdown(String::new(), None, cx);
                                }
                                editor.workspace.undo_file_operation = None;
                                editor.workspace.pinned_empty_directories.remove(&plan.path);
                                tree_updated = editor.workspace.remove_path(&plan.path);
                            } else {
                                editor.workspace.undo_file_operation =
                                    Some(WorkspaceUndoOperation::Create(plan.clone()));
                                if plan.kind == super::workspace_file_ops::WorkspaceCreateKind::File
                                {
                                    editor.workspace.selected =
                                        Some(WorkspaceSelection::File(plan.path.clone()));
                                    // 所有扩展名都走统一打开策略：Markdown 使用常驻编辑器，
                                    // 代码/数据文件使用安全源码或结构化视图，行为与文件树双击一致。
                                    editor.open_path_in_tab(plan.path.clone(), cx);
                                    crate::app_menu::record_recent_file_from_editor(&plan.path, cx);
                                } else {
                                    editor
                                        .workspace
                                        .pinned_empty_directories
                                        .insert(plan.path.clone());
                                }
                                tree_updated = editor
                                    .workspace
                                    .insert_created_path(&plan.root, &plan.path, plan.kind);
                            }
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
                        }
                    }
                    cx.notify();
                });
            }));
        cx.notify();
    }

    pub(super) fn execute_workspace_move_plan(
        &mut self,
        plan: super::workspace_file_ops::WorkspaceMovePlan,
        from_dialog: bool,
        cx: &mut Context<Self>,
    ) {
        if self.workspace.file_operation_task.is_some() {
            return;
        }
        let active_path = self.file_path.as_ref().and_then(|path| {
            super::workspace_file_ops::canonicalize_workspace_path(path)
                .ok()
                .or_else(|| Some(path.clone()))
        });
        let affects_dirty_document = self.is_document_dirty()
            && active_path.as_ref().is_some_and(|current| {
                current.starts_with(&plan.source)
                    || plan
                        .rewrites
                        .iter()
                        .any(|rewrite| rewrite.before_path == *current)
            });
        if affects_dirty_document {
            let message = cx
                .global::<crate::i18n::I18nManager>()
                .strings()
                .workspace_operation_dirty_error
                .clone();
            if from_dialog {
                if let Some(dialog) = self.workspace.operation_dialog.as_mut() {
                    dialog.error = Some(message);
                }
            } else {
                self.workspace.operation_error = Some(message);
            }
            cx.notify();
            return;
        }
        let selection = self.capture_source_selection_snapshot(cx);
        let view_mode = self.view_mode;
        let generation = self.workspace.file_operation_generation.wrapping_add(1);
        self.workspace.file_operation_generation = generation;
        if let Some(dialog) = self.workspace.operation_dialog.as_mut() {
            dialog.running = true;
            dialog.error = None;
        }
        self.workspace.operation_error = None;
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
                        Ok(()) => editor.finish_workspace_move(
                            &plan,
                            active_path.as_deref(),
                            &selection,
                            view_mode,
                            cx,
                        ),
                        Err(error) => {
                            if from_dialog {
                                if let Some(dialog) = editor.workspace.operation_dialog.as_mut() {
                                    dialog.running = false;
                                    dialog.error = Some(error.to_string());
                                }
                            } else {
                                editor.workspace.operation_error = Some(error.to_string());
                            }
                        }
                    }
                    cx.notify();
                });
            }));
        cx.notify();
    }
}
