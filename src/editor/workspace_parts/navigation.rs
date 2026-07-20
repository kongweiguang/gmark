// @author kongweiguang

use super::*;

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
            UndoSelectionSnapshot::collapsed(offset, gmark_large_document::SourceAffinity::Before);
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
        self.open_path_in_tab(path, window, cx);
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
                    WorkspaceTreeKind::Directory(path) | WorkspaceTreeKind::MarkdownFile(path),
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
            WorkspaceTreeKind::Directory(path) | WorkspaceTreeKind::MarkdownFile(path) => {
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
            WorkspaceTreeKind::MarkdownFile(path) => self.open_workspace_file(path, window, cx),
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
        self.open_path_in_tab(path, window, cx);
    }

    pub(super) fn open_workspace_context_menu(
        &mut self,
        position: Point<Pixels>,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.close_menu_bar(cx);
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
            WorkspaceOperationKind::NewFile => "untitled.md".to_owned(),
            WorkspaceOperationKind::NewFolder => "New Folder".to_owned(),
        };
        let input = cx.new(|cx| {
            let mut block = Block::with_record(cx, BlockRecord::paragraph(initial));
            block.set_source_raw_mode();
            block
        });
        input.read(cx).focus_handle.focus(window);
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
        input.read(cx).focus_handle.focus(window);
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

    pub(in crate::editor) fn on_cancel_workspace_operation(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace.file_operation_generation =
            self.workspace.file_operation_generation.wrapping_add(1);
        self.workspace.file_operation_task = None;
        self.workspace.operation_dialog = None;
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
        if dialog.running || dialog.plan.is_some() {
            return;
        }
        let Some(root) = self.workspace.root.clone() else {
            return;
        };
        let value = dialog.input.read(cx).display_text().trim().to_owned();
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
                        dialog.error = Some("A rename must contain only a file name.".to_owned());
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
        };
        let creation_parent = if source.is_dir() {
            source.clone()
        } else {
            source
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.clone())
        };
        let generation = self.workspace.file_operation_generation.wrapping_add(1);
        self.workspace.file_operation_generation = generation;
        if let Some(dialog) = self.workspace.operation_dialog.as_mut() {
            dialog.running = true;
            dialog.error = None;
        }
        self.workspace.file_operation_task =
            Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
                let plan = cx
                    .background_spawn(async move {
                        match operation_kind {
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
                                    super::workspace_file_ops::WorkspaceCreateKind::MarkdownFile,
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
                        }
                    })
                    .await;
                let _ = this.update(cx, |editor, cx| {
                    if editor.workspace.file_operation_generation != generation {
                        return;
                    }
                    editor.workspace.file_operation_task = None;
                    let current_path = editor.file_path.as_ref().and_then(|path| {
                        super::workspace_file_ops::canonicalize_workspace_path(path)
                            .ok()
                            .or_else(|| Some(path.clone()))
                    });
                    let Some(dialog) = editor.workspace.operation_dialog.as_mut() else {
                        return;
                    };
                    dialog.running = false;
                    match plan {
                        Ok(plan) => {
                            let affects_dirty_document = editor.document_dirty
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
                            } else {
                                dialog.plan = Some(plan);
                                dialog.error = None;
                            }
                        }
                        Err(error) => dialog.error = Some(error.to_string()),
                    }
                    cx.notify();
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
        }
    }

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

    pub(super) fn execute_workspace_create_plan(
        &mut self,
        plan: super::workspace_file_ops::WorkspaceCreatePlan,
        undo: bool,
        cx: &mut Context<Self>,
    ) {
        if self.workspace.file_operation_task.is_some() {
            return;
        }
        if undo && self.file_path.as_ref() == Some(&plan.path) && self.document_dirty {
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
                            if undo {
                                if editor.file_path.as_ref() == Some(&plan.path) {
                                    editor.replace_document_from_markdown(String::new(), None, cx);
                                }
                                editor.workspace.undo_file_operation = None;
                                editor.workspace.pinned_empty_directories.remove(&plan.path);
                            } else {
                                editor.workspace.undo_file_operation =
                                    Some(WorkspaceUndoOperation::Create(plan.clone()));
                                if plan.kind
                                    == super::workspace_file_ops::WorkspaceCreateKind::MarkdownFile
                                {
                                    if editor.document_dirty {
                                        editor.workspace.selected =
                                            Some(WorkspaceSelection::File(plan.path.clone()));
                                    } else {
                                        editor.replace_document_from_markdown(
                                            String::new(),
                                            Some(plan.path.clone()),
                                            cx,
                                        );
                                        crate::app_menu::record_recent_file_from_editor(
                                            &plan.path, cx,
                                        );
                                    }
                                } else {
                                    editor
                                        .workspace
                                        .pinned_empty_directories
                                        .insert(plan.path.clone());
                                }
                            }
                            editor.invalidate_workspace_file_tree();
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
        let affects_dirty_document = self.document_dirty
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
