// @author kongweiguang

use super::*;
use crate::i18n::I18nManager;

impl Editor {
    pub(super) fn handle_workspace_search_input_key(
        &mut self,
        key: &str,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "escape" => {
                self.workspace.keyboard_zone = WorkspaceKeyboardZone::Tabs;
                self.ensure_workspace_focus_handle(cx).focus(window);
            }
            "tab" => {
                self.workspace.keyboard_zone = if shift {
                    WorkspaceKeyboardZone::Tabs
                } else {
                    WorkspaceKeyboardZone::SearchOptions
                };
                self.ensure_workspace_focus_handle(cx).focus(window);
            }
            "up" | "down" => {
                if self.workspace.search_results.is_empty() {
                    return false;
                }
                self.workspace.search_selected = if key == "up" {
                    self.workspace.search_results.len() - 1
                } else {
                    0
                };
                self.workspace.keyboard_zone = WorkspaceKeyboardZone::SearchResults;
                self.ensure_workspace_focus_handle(cx).focus(window);
                self.ensure_workspace_keyboard_item_visible(
                    self.workspace.search_selected,
                    58.0,
                    80.0,
                );
            }
            "enter" => {
                self.open_selected_workspace_search_result(window, cx);
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    pub(super) fn handle_workspace_tabs_key(
        &mut self,
        key: &str,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let index = match self.workspace.active_tab {
            WorkspaceTab::Files => 0usize,
            WorkspaceTab::Outline => 1,
            WorkspaceTab::Search => 2,
        };
        let next = match key {
            "left" => Some(index.saturating_sub(1)),
            "right" => Some((index + 1).min(2)),
            "home" => Some(0),
            "end" => Some(2),
            "tab" if shift => {
                self.enter_workspace_body(window, cx, true);
                return true;
            }
            "tab" | "down" | "enter" => {
                self.enter_workspace_body(window, cx, false);
                return true;
            }
            "escape" => {
                self.workspace.is_open = false;
                self.focus_editor_after_workspace(window, cx);
                return true;
            }
            _ => return false,
        };
        let tab = match next.expect("tab navigation must produce an index") {
            0 => WorkspaceTab::Files,
            1 => WorkspaceTab::Outline,
            _ => WorkspaceTab::Search,
        };
        self.set_workspace_tab(tab, cx);
        true
    }

    pub(super) fn enter_workspace_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        from_end: bool,
    ) {
        if self.workspace.active_tab == WorkspaceTab::Search {
            if from_end && !self.workspace.search_results.is_empty() {
                self.workspace.keyboard_zone = WorkspaceKeyboardZone::SearchResults;
                self.workspace.search_selected = self.workspace.search_results.len() - 1;
                return;
            }
            self.ensure_workspace_search_input(cx)
                .read(cx)
                .focus_handle
                .focus(window);
            return;
        }
        self.workspace.keyboard_zone = WorkspaceKeyboardZone::Body;
        let nodes = self.visible_workspace_keyboard_nodes();
        if self.selected_workspace_node_index(&nodes).is_none()
            && let Some(node) = if from_end {
                nodes.last()
            } else {
                nodes.first()
            }
        {
            self.select_workspace_keyboard_node(node);
        }
    }

    pub(super) fn handle_workspace_body_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let nodes = self.visible_workspace_keyboard_nodes();
        if nodes.is_empty() {
            return match key {
                "tab" => {
                    self.workspace.keyboard_zone = WorkspaceKeyboardZone::Tabs;
                    true
                }
                "escape" => {
                    self.workspace.is_open = false;
                    self.focus_editor_after_workspace(window, cx);
                    true
                }
                _ => false,
            };
        }
        let current = self.selected_workspace_node_index(&nodes).unwrap_or(0);
        match key {
            "up" => self.select_workspace_keyboard_node(&nodes[current.saturating_sub(1)]),
            "down" => self.select_workspace_keyboard_node(
                &nodes[(current + 1).min(nodes.len().saturating_sub(1))],
            ),
            "home" => self.select_workspace_keyboard_node(&nodes[0]),
            "end" => self.select_workspace_keyboard_node(&nodes[nodes.len() - 1]),
            "right" => {
                let node = &nodes[current];
                if node.has_children && !self.workspace.expanded.contains(&node.id) {
                    self.workspace.expanded.insert(node.id.clone());
                } else if let Some(child) = nodes
                    .iter()
                    .skip(current + 1)
                    .find(|candidate| candidate.parent_id.as_ref() == Some(&node.id))
                {
                    self.select_workspace_keyboard_node(child);
                }
            }
            "left" => {
                let node = &nodes[current];
                if node.has_children && self.workspace.expanded.remove(&node.id) {
                    return true;
                }
                if let Some(parent_id) = node.parent_id.as_ref()
                    && let Some(parent) = nodes.iter().find(|node| &node.id == parent_id)
                {
                    self.select_workspace_keyboard_node(parent);
                }
            }
            "enter" | "space" => {
                self.activate_workspace_keyboard_node(nodes[current].clone(), window, cx)
            }
            "tab" => self.workspace.keyboard_zone = WorkspaceKeyboardZone::Tabs,
            "escape" => {
                self.workspace.is_open = false;
                self.focus_editor_after_workspace(window, cx);
            }
            _ => return false,
        }
        let refreshed = self.visible_workspace_keyboard_nodes();
        if let Some(index) = self.selected_workspace_node_index(&refreshed) {
            self.ensure_workspace_keyboard_item_visible(index, WORKSPACE_NODE_HEIGHT, 0.0);
        }
        true
    }

    pub(super) fn handle_workspace_search_options_key(
        &mut self,
        key: &str,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let selected = self.workspace.search_selected.min(2);
        match key {
            "left" | "up" => self.workspace.search_selected = selected.saturating_sub(1),
            "right" | "down" => self.workspace.search_selected = (selected + 1).min(2),
            "home" => self.workspace.search_selected = 0,
            "end" => self.workspace.search_selected = 2,
            "enter" | "space" => {
                let option: fn(&mut WorkspaceSearchOptions) -> &mut bool = match selected {
                    0 => |options| &mut options.case_sensitive,
                    1 => |options| &mut options.whole_word,
                    _ => |options| &mut options.regex,
                };
                self.toggle_workspace_search_option(option, cx);
            }
            "tab" if shift => {
                self.ensure_workspace_search_input(cx)
                    .read(cx)
                    .focus_handle
                    .focus(window);
            }
            "tab" => {
                self.workspace.keyboard_zone = if self.workspace.search_results.is_empty() {
                    WorkspaceKeyboardZone::Tabs
                } else {
                    self.workspace.search_selected = 0;
                    self.ensure_workspace_keyboard_item_visible(0, 58.0, 80.0);
                    WorkspaceKeyboardZone::SearchResults
                };
            }
            "escape" => self.workspace.keyboard_zone = WorkspaceKeyboardZone::Tabs,
            _ => return false,
        }
        true
    }

    pub(super) fn handle_workspace_search_results_key(
        &mut self,
        key: &str,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let last = self.workspace.search_results.len().saturating_sub(1);
        match key {
            "up" => {
                self.workspace.search_selected = self.workspace.search_selected.saturating_sub(1)
            }
            "down" => {
                self.workspace.search_selected = (self.workspace.search_selected + 1).min(last)
            }
            "home" => self.workspace.search_selected = 0,
            "end" => self.workspace.search_selected = last,
            "enter" | "space" => self.open_selected_workspace_search_result(window, cx),
            "tab" if shift => self.workspace.keyboard_zone = WorkspaceKeyboardZone::SearchOptions,
            "tab" => self.workspace.keyboard_zone = WorkspaceKeyboardZone::Tabs,
            "escape" => {
                self.ensure_workspace_search_input(cx)
                    .read(cx)
                    .focus_handle
                    .focus(window);
            }
            _ => return false,
        }
        if self.workspace.keyboard_zone == WorkspaceKeyboardZone::SearchResults {
            self.ensure_workspace_keyboard_item_visible(self.workspace.search_selected, 58.0, 80.0);
        }
        true
    }

    pub(super) fn ensure_workspace_keyboard_item_visible(
        &self,
        index: usize,
        row_height: f32,
        top_padding: f32,
    ) {
        let viewport_height = f32::from(self.workspace.panel_scroll.bounds().size.height);
        if viewport_height <= 0.0 {
            return;
        }
        let current_top = -f32::from(self.workspace.panel_scroll.offset().y);
        let item_top = top_padding + index as f32 * row_height;
        let item_bottom = item_top + row_height;
        let target_top = if item_top < current_top {
            item_top
        } else if item_bottom > current_top + viewport_height {
            item_bottom - viewport_height
        } else {
            current_top
        };
        let max_offset = f32::from(self.workspace.panel_scroll.max_offset().height).max(0.0);
        self.workspace
            .panel_scroll
            .set_offset(point(px(0.0), px(-target_top.clamp(0.0, max_offset))));
    }

    pub(super) fn open_selected_workspace_search_result(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self
            .workspace
            .search_results
            .get(self.workspace.search_selected)
            .cloned();
        if let Some(result) = result {
            self.open_workspace_search_result(result.path, result.line, window, cx);
        }
    }

    pub(in crate::editor) fn sync_workspace_after_document_path_change(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.workspace.search_generation = self.workspace.search_generation.wrapping_add(1);
        self.workspace.search_task = None;
        self.workspace.search_running = false;
        self.workspace.search_results.clear();
        self.workspace.search_error = None;
        let next_root = self.workspace_root_for_current_file();
        if self.workspace.root != next_root {
            self.invalidate_workspace_file_tree();
        }
        self.workspace.outline_source = None;
        self.workspace.outline_revision = None;
        self.workspace.outline_requested_source = None;
        self.workspace.outline_requested_revision = None;
        self.workspace.outline_generation = self.workspace.outline_generation.wrapping_add(1);
        self.workspace.outline_task = None;
        self.workspace.outline_running = false;
        if self.workspace.is_open {
            self.sync_workspace_models(cx);
            if self.workspace.active_tab == WorkspaceTab::Search {
                self.schedule_workspace_search(cx);
            }
        }
    }

    pub(super) fn sync_workspace_models(&mut self, cx: &mut Context<Self>) {
        self.sync_workspace_file_tree(cx);
        self.sync_workspace_outline(cx);
    }

    pub(super) fn workspace_root_for_current_file(&self) -> Option<PathBuf> {
        // “打开文件”只建立文档会话；只有用户明确打开文件夹或恢复工作区时才展示目录树。
        self.workspace.explicit_root.clone()
    }

    pub(super) fn invalidate_workspace_file_tree(&mut self) {
        self.workspace.file_scan_generation = self.workspace.file_scan_generation.wrapping_add(1);
        self.workspace.file_scan_task = None;
        self.workspace.file_scanning = false;
        self.workspace.root = None;
        self.workspace.file_tree = None;
        self.workspace.file_error = None;
    }

    pub(in crate::editor) fn explicit_workspace_root(&self) -> Option<PathBuf> {
        self.workspace.explicit_root.clone()
    }

    pub(in crate::editor) fn restore_explicit_workspace_root(
        &mut self,
        root: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.set_explicit_workspace_root(root, cx);
    }

    pub(crate) fn on_open_folder_action(
        &mut self,
        _: &crate::components::OpenFolder,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prompt_title = cx
            .global::<crate::i18n::I18nManager>()
            .strings()
            .open_workspace_folder_prompt
            .clone();
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(prompt_title.into()),
        });
        cx.spawn(async move |this: WeakEntity<Self>, cx| match prompt.await {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update(cx, |editor, cx| {
                        editor.set_explicit_workspace_root(path, cx);
                    });
                }
            }
            Ok(Err(error)) => {
                let _ = this.update(cx, |editor, cx| {
                    editor.workspace.file_error = Some(error.to_string());
                    editor.workspace.is_open = true;
                    cx.notify();
                });
            }
            Ok(Ok(None)) | Err(_) => {}
        })
        .detach();
    }

    pub(super) fn set_explicit_workspace_root(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        match dunce::canonicalize(&root) {
            Ok(root) if root.is_dir() => {
                self.workspace.explicit_root = Some(root);
                self.workspace.is_open = true;
                self.sync_workspace_after_document_path_change(cx);
                self.schedule_workspace_session_save(cx);
            }
            Ok(_) => {
                self.workspace.file_error = Some(format!(
                    "workspace root is not a directory: '{}'",
                    root.display()
                ));
                self.workspace.is_open = true;
            }
            Err(error) => {
                self.workspace.file_error = Some(error.to_string());
                self.workspace.is_open = true;
            }
        }
        cx.notify();
    }

    pub(super) fn sync_workspace_file_tree(&mut self, cx: &mut Context<Self>) {
        let next_root = self.workspace_root_for_current_file();
        if self.workspace.root == next_root
            && (self.workspace.file_tree.is_some()
                || self.workspace.file_scan_task.is_some()
                || self.workspace.file_error.is_some())
        {
            // 模型版本未变化时，选择权属于用户；每帧回写当前文档会破坏键盘和目录选择。
            return;
        }

        self.workspace.root = next_root.clone();
        self.workspace.file_tree = None;
        self.workspace.file_error = None;
        self.workspace.file_scan_task = None;
        self.workspace.file_scan_generation = self.workspace.file_scan_generation.wrapping_add(1);
        self.workspace.file_scanning = false;

        let Some(root) = next_root else {
            self.workspace.selected = None;
            return;
        };

        // Validate the root path
        if root.as_os_str().is_empty() {
            self.workspace.file_error = Some(
                cx.global::<I18nManager>()
                    .strings()
                    .workspace_invalid_path_error
                    .clone(),
            );
            self.workspace.selected = None;
            return;
        }

        self.workspace.file_scanning = true;
        let generation = self.workspace.file_scan_generation;
        self.workspace.file_scan_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let scan_root = root.clone();
            let result = cx
                .background_spawn(async move { scan_workspace_dir(&scan_root) })
                .await;
            let _ = this.update(cx, |editor, cx| {
                if editor.workspace.file_scan_generation != generation
                    || editor.workspace.root.as_ref() != Some(&root)
                {
                    return;
                }
                editor.workspace.file_scan_task = None;
                editor.workspace.file_scanning = false;
                match result {
                    Ok(mut tree) => {
                        editor
                            .workspace
                            .pinned_empty_directories
                            .retain(|path| path.is_dir() && path.starts_with(&root));
                        for directory in &editor.workspace.pinned_empty_directories {
                            insert_workspace_directory(&mut tree, &root, directory);
                        }
                        sort_workspace_tree(&mut tree);
                        editor.workspace.expanded.insert(tree.id.clone());
                        editor.workspace.file_tree = Some(tree);
                        editor.workspace.file_error = None;
                        editor.workspace.selected = editor
                            .file_path
                            .as_ref()
                            .map(|path| WorkspaceSelection::File(path.clone()));
                        if editor.workspace.quick_open.is_some() {
                            editor.schedule_quick_open(cx);
                        }
                    }
                    Err(error) => {
                        editor.workspace.file_tree = None;
                        editor.workspace.file_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(super) fn sync_workspace_outline(&mut self, cx: &mut Context<Self>) {
        let revision = (self.document_epoch, self.source_document.revision());
        if self.workspace.outline_revision == Some(revision)
            || self.workspace.outline_requested_revision == Some(revision)
        {
            return;
        }
        let source = self.serialized_document_text(cx);
        if self.workspace.outline_source.as_deref() == Some(source.as_str())
            || self.workspace.outline_requested_source.as_deref() == Some(source.as_str())
        {
            return;
        }
        self.workspace.outline_generation = self.workspace.outline_generation.wrapping_add(1);
        let generation = self.workspace.outline_generation;
        self.workspace.outline_task = None;
        self.workspace.outline_requested_source = Some(source.clone());
        self.workspace.outline_requested_revision = Some(revision);
        self.workspace.outline_running = true;
        self.workspace.outline_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let worker_source = source.clone();
            let outline = cx
                .background_spawn(async move { build_outline_tree(&worker_source) })
                .await;
            let _ = this.update(cx, |editor, cx| {
                if editor.workspace.outline_generation != generation
                    || editor.workspace.outline_requested_revision != Some(revision)
                    || editor.workspace.outline_requested_source.as_deref() != Some(source.as_str())
                {
                    return;
                }
                editor.workspace.outline_task = None;
                editor.workspace.outline_running = false;
                editor.workspace.outline_requested_source = None;
                editor.workspace.outline_requested_revision = None;
                prune_outline_state(&mut editor.workspace, &outline);
                editor.workspace.outline_tree = outline;
                editor.workspace.outline_source = Some(source);
                editor.workspace.outline_revision = Some(revision);
                cx.notify();
            });
        }));
    }

    pub(super) fn set_workspace_tab(&mut self, tab: WorkspaceTab, cx: &mut Context<Self>) {
        if self.workspace.active_tab != tab {
            self.workspace.active_tab = tab;
            self.sync_workspace_models(cx);
            if tab == WorkspaceTab::Search {
                self.ensure_workspace_search_input(cx);
                self.schedule_workspace_search(cx);
            }
            cx.notify();
        }
    }

    pub(super) fn on_workspace_tab_key_down(
        &mut self,
        tab: WorkspaceTab,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            self.set_workspace_tab(tab, cx);
            cx.stop_propagation();
        }
    }

    pub(super) fn ensure_workspace_search_input(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Entity<Block> {
        let placeholder = cx
            .global::<crate::i18n::I18nManager>()
            .strings()
            .workspace_search_prompt
            .clone();
        if let Some(input) = self.workspace.search_input.as_ref() {
            input.update(cx, |input, _cx| {
                input.set_input_placeholder(placeholder);
            });
            return input.clone();
        }
        let input = cx.new(move |cx| {
            let mut block = Block::with_record(cx, BlockRecord::paragraph(String::new()));
            block.set_source_raw_mode();
            block.set_input_placeholder(placeholder);
            block
        });
        cx.subscribe(&input, Self::on_workspace_search_input_event)
            .detach();
        self.workspace.search_input = Some(input.clone());
        input
    }

    pub(super) fn on_workspace_search_input_event(
        &mut self,
        _input: Entity<Block>,
        event: &BlockEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, BlockEvent::Changed) {
            self.schedule_workspace_search(cx);
        }
    }

    pub(super) fn toggle_workspace_search_option(
        &mut self,
        option: fn(&mut WorkspaceSearchOptions) -> &mut bool,
        cx: &mut Context<Self>,
    ) {
        let value = option(&mut self.workspace.search_options);
        *value = !*value;
        self.schedule_workspace_search(cx);
    }

    pub(super) fn schedule_workspace_search(&mut self, cx: &mut Context<Self>) {
        self.workspace.search_generation = self.workspace.search_generation.wrapping_add(1);
        self.workspace.search_task = None;
        self.workspace.search_error = None;
        let query = self
            .workspace
            .search_input
            .as_ref()
            .map(|input| input.read(cx).display_text().trim().to_owned())
            .unwrap_or_default();
        let Some(root) = self.workspace.root.clone() else {
            self.workspace.search_results.clear();
            self.workspace.search_running = false;
            return;
        };
        if query.is_empty() {
            self.workspace.search_results.clear();
            self.workspace.search_selected = 0;
            self.workspace.search_running = false;
            cx.notify();
            return;
        }

        self.workspace.search_running = true;
        let generation = self.workspace.search_generation;
        let options = self.workspace.search_options;
        self.workspace.search_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            let result = cx
                .background_spawn(async move { search_workspace(&root, &query, options) })
                .await;
            let _ = this.update(cx, |editor, cx| {
                if editor.workspace.search_generation != generation {
                    return;
                }
                editor.workspace.search_task = None;
                editor.workspace.search_running = false;
                match result {
                    Ok(results) => {
                        editor.workspace.search_results = results;
                        editor.workspace.search_selected = editor
                            .workspace
                            .search_selected
                            .min(editor.workspace.search_results.len().saturating_sub(1));
                        editor.workspace.search_error = None;
                    }
                    Err(error) => {
                        editor.workspace.search_results.clear();
                        editor.workspace.search_error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
