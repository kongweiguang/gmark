// @author kongweiguang

use super::*;
use crate::i18n::I18nManager;

#[path = "navigation/delete.rs"]
mod delete;
#[path = "navigation/undo.rs"]
mod undo;

impl Editor {
    pub(super) fn jump_to_source_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let source = self.source_document.text();
        let offset = source
            .split_inclusive('\n')
            .take(line.saturating_sub(1))
            .map(str::len)
            .sum::<usize>()
            .min(source.len());
        let selection =
            UndoSelectionSnapshot::collapsed(offset, gmark_document_core::SourceAffinity::Before);
        // 虚拟面可能先滚动再挂载目标 Entity；权威源码选择仍须立即更新，供会话与后续挂载恢复。
        self.last_selection_snapshot = selection;
        if let Some(y) = self
            .virtual_surface
            .as_ref()
            .and_then(|surface| surface.y_for_source_offset(offset))
        {
            self.scroll_handle.set_offset(point(px(0.0), px(-y)));
            cx.notify();
            return;
        }
        self.apply_selection_snapshot_in_current_mode(&selection, cx);
        self.pending_scroll_active_block_into_view = true;
        self.pending_scroll_recheck_after_layout = true;
        cx.notify();
    }

    pub(super) fn open_workspace_search_result(
        &mut self,
        path: PathBuf,
        line: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if workspace_uses_overlay(f32::from(window.viewport_size().width)) {
            self.workspace.is_open = false;
        }
        if self.file_path.as_ref() == Some(&path) {
            self.jump_to_source_line(line, cx);
            return;
        }
        self.workspace.pending_navigation = Some((path.clone(), line));
        self.open_path_in_tab(path, cx);
    }

    pub(in crate::editor) fn apply_pending_workspace_navigation(&mut self, cx: &mut Context<Self>) {
        let Some((path, line)) = self.workspace.pending_navigation.take() else {
            return;
        };
        if self.file_path.as_ref() == Some(&path) {
            self.jump_to_source_line(line, cx);
        }
    }

    pub(in crate::editor) fn take_pending_workspace_navigation(
        &mut self,
    ) -> Option<(PathBuf, usize)> {
        self.workspace.pending_navigation.take()
    }

    pub(in crate::editor) fn restore_pending_workspace_navigation(
        &mut self,
        navigation: Option<(PathBuf, usize)>,
    ) {
        self.workspace.pending_navigation = navigation;
    }

    pub(in crate::editor) fn clear_pending_workspace_navigation(&mut self) {
        self.workspace.pending_navigation = None;
    }

    pub(super) fn toggle_workspace_node(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.workspace.expanded.remove(id) {
            self.workspace.expanded.insert(id.to_string());
        }
        cx.notify();
    }

    pub(super) fn visible_workspace_keyboard_nodes(&self) -> Vec<WorkspaceKeyboardNode> {
        let roots = match self.workspace.active_tab {
            WorkspaceTab::Files => self.workspace.file_tree.as_slice(),
            WorkspaceTab::Outline => self.workspace.outline_tree.as_slice(),
            WorkspaceTab::Search => return Vec::new(),
        };
        let mut nodes = Vec::new();
        collect_visible_keyboard_nodes(roots, &self.workspace.expanded, None, &mut nodes);
        nodes
    }

    pub(super) fn selected_workspace_node_index(
        &self,
        nodes: &[WorkspaceKeyboardNode],
    ) -> Option<usize> {
        nodes
            .iter()
            .position(|node| match (&self.workspace.selected, &node.kind) {
                (
                    Some(WorkspaceSelection::File(selected)),
                    WorkspaceTreeKind::Directory(path) | WorkspaceTreeKind::File(path),
                ) => selected == path,
                (
                    Some(WorkspaceSelection::Outline(selected)),
                    WorkspaceTreeKind::Heading { .. },
                ) => selected == &node.id,
                _ => false,
            })
    }

    pub(super) fn select_workspace_keyboard_node(&mut self, node: &WorkspaceKeyboardNode) {
        self.workspace.selected = match &node.kind {
            WorkspaceTreeKind::Directory(path) | WorkspaceTreeKind::File(path) => {
                Some(WorkspaceSelection::File(path.clone()))
            }
            WorkspaceTreeKind::Heading { .. } => Some(WorkspaceSelection::Outline(node.id.clone())),
        };
    }

    pub(super) fn activate_workspace_keyboard_node(
        &mut self,
        node: WorkspaceKeyboardNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match node.kind {
            WorkspaceTreeKind::Directory(_) => self.toggle_workspace_node(&node.id, cx),
            WorkspaceTreeKind::File(path) => self.open_workspace_file(path, window, cx),
            WorkspaceTreeKind::Heading { line, .. } => {
                self.select_outline_node(node.id, line, window, cx)
            }
        }
    }

    pub(super) fn select_outline_node(
        &mut self,
        id: String,
        line: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace.selected = Some(WorkspaceSelection::Outline(id));
        if workspace_uses_overlay(f32::from(window.viewport_size().width)) {
            self.workspace.is_open = false;
        }
        self.jump_to_source_line(line + 1, cx);
    }

    pub(super) fn open_workspace_file(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace.selected = Some(WorkspaceSelection::File(path.clone()));
        if workspace_uses_overlay(f32::from(window.viewport_size().width)) {
            self.workspace.is_open = false;
        }
        self.open_path_in_tab(path, cx);
    }

    pub(super) fn open_workspace_context_menu(
        &mut self,
        position: Point<Pixels>,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.close_menu_bar(cx);
        // 右键本身就是一次树选择：让菜单关闭后的视觉选中与操作目标一致，
        // 空白区传入的 root 也因此不会继续高亮上一次打开的文件。
        self.workspace.selected = Some(WorkspaceSelection::File(path.clone()));
        self.context_menu = Some(ContextMenuState::Workspace { position, path });
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        self.context_menu_scroll_handle
            .set_offset(point(px(0.0), px(0.0)));
        cx.notify();
    }

    pub(in crate::editor) fn on_workspace_rename_menu(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_workspace_operation_dialog(WorkspaceOperationKind::Rename, window, cx);
    }

    /// 依据已交付的树快照打开文件，避免菜单点击为了判断类型再次访问磁盘。
    pub(in crate::editor) fn on_workspace_open_menu(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ContextMenuState::Workspace { path, .. }) = self.context_menu.take() else {
            return;
        };
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        if self.workspace.snapshot_path_is_file(&path) {
            self.open_workspace_file(path, window, cx);
        } else {
            cx.notify();
        }
    }

    /// 将 Reveal 的 Windows 文件类型查询与 Shell 调用放到后台，避免菜单回调阻塞 UNC/慢盘。
    pub(in crate::editor) fn on_workspace_reveal_menu(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace.file_operation_task.is_some() {
            return;
        }
        let Some(ContextMenuState::Workspace { path, .. }) = self.context_menu.take() else {
            return;
        };
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        let generation = self.workspace.file_operation_generation.wrapping_add(1);
        self.workspace.file_operation_generation = generation;
        self.workspace.operation_error = None;
        self.workspace.file_operation_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    crate::editor::system_file::reveal_in_file_manager(&path)
                })
                .await;
            let _ = this.update(cx, |editor, cx| {
                if editor.workspace.file_operation_generation != generation {
                    return;
                }
                editor.workspace.file_operation_task = None;
                editor.workspace.operation_error = result.err().map(|error| error.to_string());
                // Shell 完成回调不应依赖下一次输入才能显示成功或失败。
                cx.notify();
                cx.refresh_windows();
            });
        }));
        cx.notify();
        cx.refresh_windows();
    }

    pub(in crate::editor) fn on_workspace_copy_path_menu(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_workspace_context_path(false, cx);
    }

    pub(in crate::editor) fn on_workspace_copy_relative_path_menu(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_workspace_context_path(true, cx);
    }

    fn copy_workspace_context_path(&mut self, relative: bool, cx: &mut Context<Self>) {
        let Some(ContextMenuState::Workspace { path, .. }) = self.context_menu.take() else {
            return;
        };
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        let copied = if relative {
            self.workspace
                .root
                .as_ref()
                .and_then(|root| path.strip_prefix(root).ok())
                .map(|path| path.to_string_lossy().replace('\\', "/"))
        } else {
            Some(path.to_string_lossy().into_owned())
        };
        if let Some(copied) = copied {
            cx.write_to_clipboard(ClipboardItem::new_string(copied));
        }
        cx.notify();
    }

    pub(in crate::editor) fn on_workspace_refresh_menu(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = None;
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        self.invalidate_workspace_file_tree();
        self.sync_workspace_file_tree(cx);
    }

    /// 把删除规划及 symlink/边界检查放到后台，避免确认菜单在 UNC 路径上同步卡顿。
    pub(in crate::editor) fn on_workspace_delete_menu(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace.file_operation_task.is_some() {
            return;
        }
        let Some(ContextMenuState::Workspace { path, .. }) = self.context_menu.take() else {
            return;
        };
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        let Some(root) = self.workspace.root.clone() else {
            return;
        };
        let input = cx.new(|cx| {
            let mut block = Block::with_record(cx, BlockRecord::paragraph(String::new()));
            block.set_source_raw_mode();
            block
        });
        self.workspace.operation_dialog = Some(WorkspaceOperationDialog {
            kind: WorkspaceOperationKind::Delete,
            source: path.clone(),
            input,
            plan: None,
            error: None,
            running: true,
        });
        self.workspace.operation_error = None;
        let generation = self.workspace.file_operation_generation.wrapping_add(1);
        self.workspace.file_operation_generation = generation;
        self.workspace.file_operation_task = Some(cx.spawn(async move |this, cx| {
            // 规范化与 symlink 安全检查可能触达 UNC/慢盘，必须离开 GPUI 回调执行。
            let planned = cx
                .background_spawn(async move {
                    super::workspace_file_ops::plan_workspace_delete(&root, &path)
                })
                .await;
            let _ = this.update(cx, |editor, cx| {
                if editor.workspace.file_operation_generation != generation {
                    return;
                }
                editor.workspace.file_operation_task = None;
                match planned {
                    Ok(plan) => {
                        let (_, has_dirty) =
                            editor.workspace_tabs_affected_by_path(&plan.workspace_path);
                        let Some(dialog) = editor.workspace.operation_dialog.as_mut() else {
                            return;
                        };
                        if has_dirty {
                            dialog.error = Some(
                                cx.global::<I18nManager>()
                                    .strings()
                                    .workspace_delete_dirty_error
                                    .clone(),
                            );
                        } else {
                            dialog.plan = Some(WorkspacePendingPlan::Delete(plan));
                        }
                        dialog.running = false;
                    }
                    Err(error) => {
                        let Some(dialog) = editor.workspace.operation_dialog.as_mut() else {
                            return;
                        };
                        dialog.error = Some(error.to_string());
                        dialog.running = false;
                    }
                }
                // 规划结果必须在没有后续输入时也刷新确认按钮或错误提示。
                cx.notify();
                cx.refresh_windows();
            });
        }));
        cx.notify();
        cx.refresh_windows();
    }

    pub(in crate::editor) fn on_workspace_move_menu(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_workspace_operation_dialog(WorkspaceOperationKind::Move, window, cx);
    }

    pub(in crate::editor) fn on_workspace_new_file_menu(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_workspace_operation_dialog(WorkspaceOperationKind::NewFile, window, cx);
    }

    pub(in crate::editor) fn on_workspace_new_folder_menu(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_workspace_operation_dialog(WorkspaceOperationKind::NewFolder, window, cx);
    }

    pub(in crate::editor) fn open_workspace_operation_dialog(
        &mut self,
        kind: WorkspaceOperationKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ContextMenuState::Workspace { path, .. }) = self.context_menu.take() else {
            return;
        };
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        let initial = match kind {
            WorkspaceOperationKind::Rename => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            WorkspaceOperationKind::Move => self
                .workspace
                .root
                .as_ref()
                .and_then(|root| path.strip_prefix(root).ok())
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/"),
            WorkspaceOperationKind::NewFile => "untitled.txt".to_owned(),
            WorkspaceOperationKind::NewFolder => "New Folder".to_owned(),
            WorkspaceOperationKind::Delete => String::new(),
        };
        let input = cx.new(|cx| {
            let mut block = Block::with_record(cx, BlockRecord::paragraph(initial));
            block.set_source_raw_mode();
            block
        });
        self.configure_workspace_operation_input(&input, window, cx);
        self.workspace.operation_dialog = Some(WorkspaceOperationDialog {
            kind,
            source: path,
            input,
            plan: None,
            error: None,
            running: false,
        });
        self.workspace.operation_error = None;
        cx.notify();
    }

    pub(super) fn open_workspace_drop_move_dialog(
        &mut self,
        source: PathBuf,
        target_directory: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(file_name) = source.file_name() else {
            return;
        };
        let destination = target_directory.join(file_name);
        if destination == source {
            return;
        }
        let initial = self
            .workspace
            .root
            .as_ref()
            .and_then(|root| destination.strip_prefix(root).ok())
            .unwrap_or(destination.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let input = cx.new(|cx| {
            let mut block = Block::with_record(cx, BlockRecord::paragraph(initial));
            block.set_source_raw_mode();
            block
        });
        self.configure_workspace_operation_input(&input, window, cx);
        self.context_menu = None;
        self.workspace.operation_dialog = Some(WorkspaceOperationDialog {
            kind: WorkspaceOperationKind::Move,
            source,
            input,
            plan: None,
            error: None,
            running: false,
        });
        cx.notify();
    }

    /// 将操作输入接到宿主动作路由，保证一次键盘确认与按钮确认使用同一条
    /// 规划/执行链，并让 Escape 取消后焦点回到工作区而不是丢到遮罩层。
    fn configure_workspace_operation_input(
        &mut self,
        input: &Entity<Block>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = cx.entity().downgrade();
        input.update(cx, move |input, _cx| {
            input.set_host_submit_enabled(true);
            input.set_host_action_handler(move |action, window, cx| {
                let _ = editor.update(cx, |editor, cx| match action {
                    crate::components::BlockHostAction::Submit(value) => {
                        let has_plan = editor
                            .workspace
                            .operation_dialog
                            .as_ref()
                            .is_some_and(|dialog| dialog.plan.is_some());
                        if has_plan {
                            editor.on_apply_workspace_operation(&ClickEvent::default(), window, cx);
                        } else {
                            // Submit 已携带 Block 在当前更新租约内取得的权威文本；
                            // 直接使用它可避免回读同一实体触发 GPUI double lease panic。
                            editor.review_workspace_operation_value(value.as_ref(), cx);
                        }
                    }
                    crate::components::BlockHostAction::DismissTransientUi => {
                        editor.on_cancel_workspace_operation(&ClickEvent::default(), window, cx)
                    }
                    _ => {}
                });
            });
            input.focus_handle.focus(window);
        });
    }

    pub(in crate::editor) fn on_cancel_workspace_operation(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace.file_operation_generation =
            self.workspace.file_operation_generation.wrapping_add(1);
        self.workspace.file_operation_task = None;
        self.workspace.operation_dialog = None;
        self.ensure_workspace_focus_handle(cx).focus(window);
        cx.notify();
    }

    pub(in crate::editor) fn dismiss_workspace_operation_dialog(&mut self) -> bool {
        let had_dialog = self.workspace.operation_dialog.take().is_some();
        let had_quick_open = self.workspace.quick_open.take().is_some();
        if had_dialog {
            self.workspace.file_operation_generation =
                self.workspace.file_operation_generation.wrapping_add(1);
            self.workspace.file_operation_task = None;
        }
        had_dialog || had_quick_open
    }

    pub(in crate::editor) fn on_review_workspace_operation(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.workspace.operation_dialog.as_ref() else {
            return;
        };
        let value = dialog.input.read(cx).display_text().to_string();
        self.review_workspace_operation_value(&value, cx);
    }

    /// 按钮与 Block 的 Enter 提交共用同一规划入口；显式接收文本可让键盘路径
    /// 在 Block 更新租约结束前安全调度文件操作，而不回读正在更新的输入实体。
    fn review_workspace_operation_value(&mut self, value: &str, cx: &mut Context<Self>) {
        let Some(dialog) = self.workspace.operation_dialog.as_ref() else {
            return;
        };
        if dialog.running || dialog.plan.is_some() {
            return;
        }
        let Some(root) = self.workspace.root.clone() else {
            return;
        };
        let value = value.trim().to_owned();
        if value.is_empty() {
            return;
        }
        let source = dialog.source.clone();
        let operation_kind = dialog.kind;
        let destination = match operation_kind {
            WorkspaceOperationKind::Rename => {
                let candidate = PathBuf::from(&value);
                if candidate.file_name() != Some(candidate.as_os_str()) {
                    if let Some(dialog) = self.workspace.operation_dialog.as_mut() {
                        dialog.error = Some(
                            cx.global::<I18nManager>()
                                .strings()
                                .workspace_rename_filename_only_error
                                .clone(),
                        );
                    }
                    cx.notify();
                    return;
                }
                source
                    .parent()
                    .map(|parent| parent.join(&candidate))
                    .unwrap_or(candidate)
            }
            WorkspaceOperationKind::Move => root.join(PathBuf::from(&value)),
            WorkspaceOperationKind::NewFile | WorkspaceOperationKind::NewFolder => PathBuf::new(),
            WorkspaceOperationKind::Delete => return,
        };
        let creation_parent = if self
            .workspace
            .snapshot_path_is_directory(&source)
            .is_some_and(|is_directory| is_directory)
        {
            source.clone()
        } else {
            source
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.clone())
        };
        let current_file_path = self.file_path.clone();
        let generation = self.workspace.file_operation_generation.wrapping_add(1);
        self.workspace.file_operation_generation = generation;
        if let Some(dialog) = self.workspace.operation_dialog.as_mut() {
            dialog.running = true;
            dialog.error = None;
        }
        self.workspace.file_operation_task =
            Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
                let (plan, current_path) = cx
                    .background_spawn(async move {
                        // 脏文档判断也需要规范化当前文件；把这次 canonicalize 与规划放在同一
                        // worker 中，避免慢盘在 GPUI completion 回调再次阻塞窗口。
                        let current_path = current_file_path.as_ref().map(|path| {
                            dunce::canonicalize(path)
                                .ok()
                                .unwrap_or_else(|| path.clone())
                        });
                        let plan = match operation_kind {
                            WorkspaceOperationKind::Rename | WorkspaceOperationKind::Move => {
                                super::workspace_file_ops::plan_workspace_move(
                                    &root,
                                    &source,
                                    &destination,
                                )
                                .map(WorkspacePendingPlan::Move)
                            }
                            WorkspaceOperationKind::NewFile => {
                                super::workspace_file_ops::plan_workspace_create(
                                    &root,
                                    &creation_parent,
                                    &value,
                                    super::workspace_file_ops::WorkspaceCreateKind::File,
                                )
                                .map(WorkspacePendingPlan::Create)
                            }
                            WorkspaceOperationKind::NewFolder => {
                                super::workspace_file_ops::plan_workspace_create(
                                    &root,
                                    &creation_parent,
                                    &value,
                                    super::workspace_file_ops::WorkspaceCreateKind::Directory,
                                )
                                .map(WorkspacePendingPlan::Create)
                            }
                            WorkspaceOperationKind::Delete => Err(anyhow::anyhow!(
                                "delete plans are created before confirmation"
                            )),
                        };
                        (plan, current_path)
                    })
                    .await;
                let _ = this.update(cx, |editor, cx| {
                    if editor.workspace.file_operation_generation != generation {
                        return;
                    }
                    editor.workspace.file_operation_task = None;
                    let document_dirty = editor.is_document_dirty();
                    let Some(dialog) = editor.workspace.operation_dialog.as_mut() else {
                        return;
                    };
                    dialog.running = false;
                    let mut create_plan_to_execute = None;
                    match plan {
                        Ok(plan) => {
                            let affects_dirty_document = document_dirty
                                && current_path.as_ref().is_some_and(|current| {
                                    matches!(&plan, WorkspacePendingPlan::Move(plan) if
                                    current.starts_with(&plan.source)
                                        || plan.rewrites.iter().any(|rewrite| {
                                            rewrite.before_path == *current
                                        }))
                                });
                            if affects_dirty_document {
                                dialog.error = Some(
                                    cx.global::<crate::i18n::I18nManager>()
                                        .strings()
                                        .workspace_operation_dirty_error
                                        .clone(),
                                );
                            } else if matches!(
                                operation_kind,
                                WorkspaceOperationKind::NewFile | WorkspaceOperationKind::NewFolder
                            ) {
                                if let WorkspacePendingPlan::Create(create_plan) = plan {
                                    dialog.error = None;
                                    create_plan_to_execute = Some(create_plan);
                                } else {
                                    dialog.error = Some(
                                        "workspace create operation returned an invalid plan"
                                            .to_owned(),
                                    );
                                }
                            } else {
                                dialog.plan = Some(plan);
                                dialog.error = None;
                            }
                        }
                        Err(error) => dialog.error = Some(error.to_string()),
                    }
                    if let Some(create_plan) = create_plan_to_execute {
                        // 新建不需要二次 Review/Apply；规划成功后沿用统一后台执行器，
                        // 这样仍保留零字节创建、树增量更新和错误恢复语义。
                        editor.execute_workspace_create_plan(create_plan, false, cx);
                    }
                    cx.notify();
                    cx.refresh_windows();
                });
            }));
        cx.notify();
    }

    pub(in crate::editor) fn on_apply_workspace_operation(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(plan) = self
            .workspace
            .operation_dialog
            .as_ref()
            .and_then(|dialog| dialog.plan.clone())
        else {
            return;
        };
        match plan {
            WorkspacePendingPlan::Move(plan) => self.execute_workspace_move_plan(plan, true, cx),
            WorkspacePendingPlan::Create(plan) => {
                self.execute_workspace_create_plan(plan, false, cx)
            }
            WorkspacePendingPlan::Delete(plan) => self.execute_workspace_delete_plan(plan, cx),
        }
    }
}
