// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn on_copy_capture(
        &mut self,
        _: &Copy,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tsv) = self.selected_table_cells_tsv(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(tsv));
            cx.stop_propagation();
            return;
        }
        let Some(markdown) = self.cross_block_selected_markdown(cx) else {
            cx.propagate();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(markdown));
        cx.stop_propagation();
    }

    pub(in crate::editor) fn on_copy_as_markdown_capture(
        &mut self,
        _: &CopyAsMarkdown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(markdown) = self.cross_block_selected_markdown(cx) else {
            cx.propagate();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(markdown));
        cx.stop_propagation();
    }

    pub(in crate::editor) fn on_cut_capture(
        &mut self,
        _: &Cut,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tsv) = self.selected_table_cells_tsv(cx)
            && let Some(selection) = self.table_cell_rectangle
        {
            cx.write_to_clipboard(ClipboardItem::new_string(tsv));
            self.clear_table_cell_rectangle(selection, cx);
            cx.stop_propagation();
            return;
        }
        let Some(markdown) = self.cross_block_selected_markdown(cx) else {
            cx.propagate();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(markdown));
        self.delete_cross_block_selection(cx);
        cx.stop_propagation();
    }

    pub(in crate::editor) fn on_paste_capture(
        &mut self,
        _: &Paste,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            cx.propagate();
            return;
        };
        if self.paste_table_cells_tsv(&text, cx) {
            cx.stop_propagation();
        } else {
            cx.propagate();
        }
    }

    pub(in crate::editor) fn on_delete_capture(
        &mut self,
        _: &Delete,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(selection) = self.table_cell_rectangle {
            self.clear_table_cell_rectangle(selection, cx);
            cx.stop_propagation();
            return;
        }
        if !self.delete_cross_block_selection(cx) {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
    }

    pub(in crate::editor) fn on_delete_back_capture(
        &mut self,
        _: &DeleteBack,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.delete_cross_block_selection(cx) {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
    }
}
