// @author kongweiguang

//! Source-surface context menus and hit testing.

use super::*;

impl DocumentHost {
    pub(super) fn open_source_context_menu(
        &mut self,
        line: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.source_row_blocks.get(&line).cloned() else {
            return;
        };
        let Some(row) = self.displayed_screen_lines.row(line) else {
            return;
        };
        let local = block.read(cx).index_for_mouse_position(event.position);
        let offset = row
            .content_range
            .start
            .saturating_add(local.min(row.text.len()) as u64);
        let existing = self
            .document
            .as_ref()
            .map(DocumentSession::source_selection)
            .unwrap_or_default();
        if existing.range().is_empty() || !existing.range().contains(&offset) {
            let caret = SourceAnchor::new(offset, SourceAffinity::After);
            self.set_source_selection(
                SourceSelection {
                    anchor: caret,
                    head: caret,
                },
                cx,
            );
        }
        // 右键菜单命令由 DocumentHost 统一执行；不能继续让行内 Block 持有编辑权，
        // 否则在已有选区内右击时，剪切、粘贴和全选都会被宿主的行内编辑保护提前拒绝。
        self.active_edit = None;
        self.sync_source_selection_visuals(cx);
        let host_origin = self
            .document_host_bounds
            .lock()
            .ok()
            .and_then(|bounds| *bounds)
            .map(|bounds| bounds.origin)
            .unwrap_or_default();
        self.source_context_menu = Some(point(
            event.position.x - host_origin.x,
            event.position.y - host_origin.y,
        ));
        let menu_focus = self.source_context_menu_focus_handle.clone();
        // 菜单的 focus node 要到下一帧才进入 dispatch tree；延后聚焦可确保
        // Escape action 命中菜单自身，而不是沿用右键前的行内 Block 路径。
        window.defer(cx, move |window, _cx| menu_focus.focus(window));
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn dismiss_source_context_menu_on_mouse_down(
        &mut self,
        _: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.source_context_menu.take().is_some() {
            self.focus_handle.focus(window);
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(super) fn capture_source_surface_mouse_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Right {
            if let Some((line, _)) = self.source_block_at_point(event.position, cx) {
                self.open_source_context_menu(line, event, window, cx);
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    pub(super) fn run_source_context_command(
        &mut self,
        command: SourceContextCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.source_context_menu = None;
        match command {
            SourceContextCommand::Copy => self.on_copy(&Copy, window, cx),
            SourceContextCommand::Cut => self.on_cut(&Cut, window, cx),
            SourceContextCommand::Paste => self.on_paste(&Paste, window, cx),
            SourceContextCommand::SelectAll => self.on_select_all(&SelectAll, window, cx),
            SourceContextCommand::ExportSelection => {
                self.on_export_selection(&ExportSelection, window, cx)
            }
            SourceContextCommand::ExportSelectionUtf8 => self.export_selection_as_utf8(window, cx),
            SourceContextCommand::FormatDocument => {
                self.on_format_document(&FormatDocument, window, cx)
            }
            SourceContextCommand::FormatSelection => {
                self.on_format_selection(&FormatSelection, window, cx)
            }
        }
        cx.notify();
    }

    pub(super) fn on_source_surface_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape" && self.source_context_menu.take().is_some() {
            self.focus_handle.focus(window);
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(super) fn source_block_at_point(
        &self,
        position: gpui::Point<gpui::Pixels>,
        cx: &App,
    ) -> Option<(usize, Entity<Block>)> {
        let mut nearest = None;
        for (line, block) in &self.source_row_blocks {
            let Some(bounds) = block.read(cx).last_bounds else {
                continue;
            };
            if position.y >= bounds.top() && position.y <= bounds.bottom() {
                return Some((*line, block.clone()));
            }
            let distance = if position.y < bounds.top() {
                f32::from(bounds.top() - position.y)
            } else {
                f32::from(position.y - bounds.bottom())
            };
            if nearest
                .as_ref()
                .is_none_or(|(_, _, best_distance)| distance < *best_distance)
            {
                nearest = Some((*line, block.clone(), distance));
            }
        }
        nearest.map(|(line, block, _)| (line, block))
    }
}
