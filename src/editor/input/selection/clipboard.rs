// @author kongweiguang

use super::*;

fn write_rich_system_clipboard(html: &str, plain_text: &str) -> bool {
    let Ok(mut clipboard) = arboard::Clipboard::new() else {
        return false;
    };
    clipboard.set_html(html, Some(plain_text)).is_ok()
}

fn table_tsv_html(tsv: &str) -> String {
    let rows = tsv
        .split('\n')
        .map(|row| {
            let cells = row
                .split('\t')
                .map(|cell| format!("<td>{}</td>", gmark_markdown::escape_html(cell)))
                .collect::<String>();
            format!("<tr>{cells}</tr>")
        })
        .collect::<String>();
    format!("<table><tbody>{rows}</tbody></table>")
}

impl Editor {
    pub(in crate::editor) fn on_copy_capture(
        &mut self,
        _: &Copy,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Source and Split keep the source editor's plain-text clipboard
        // contract.  Rendered HTML is only synthesized for a semantic
        // selection in the rendered/preview projections, where the visible
        // Markdown value is the user-facing text.
        if matches!(self.view_mode, ViewMode::Source | ViewMode::Split) {
            cx.propagate();
            return;
        }
        if let Some(tsv) = self.selected_table_cells_tsv(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(tsv.clone()));
            let _ = write_rich_system_clipboard(&table_tsv_html(&tsv), &tsv);
            cx.stop_propagation();
            return;
        }
        let Some(markdown) = self.cross_block_selected_markdown(cx) else {
            cx.propagate();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(markdown.clone()));
        let theme = cx.global::<crate::theme::ThemeManager>().current();
        let base_dir = self.file_path.as_deref().and_then(std::path::Path::parent);
        let html = crate::adapters::export::render_clipboard_fragment_with_base_dir(
            &markdown, theme, base_dir,
        );
        let _ = write_rich_system_clipboard(&html, &markdown);
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
        if matches!(self.view_mode, ViewMode::Source | ViewMode::Split) {
            cx.propagate();
            return;
        }
        if let Some(tsv) = self.selected_table_cells_tsv(cx)
            && let Some(selection) = self.table_cell_rectangle
        {
            cx.write_to_clipboard(ClipboardItem::new_string(tsv.clone()));
            let _ = write_rich_system_clipboard(&table_tsv_html(&tsv), &tsv);
            self.clear_table_cell_rectangle(selection, cx);
            cx.stop_propagation();
            return;
        }
        let Some(markdown) = self.cross_block_selected_markdown(cx) else {
            cx.propagate();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(markdown.clone()));
        let theme = cx.global::<crate::theme::ThemeManager>().current();
        let base_dir = self.file_path.as_deref().and_then(std::path::Path::parent);
        let html = crate::adapters::export::render_clipboard_fragment_with_base_dir(
            &markdown, theme, base_dir,
        );
        let _ = write_rich_system_clipboard(&html, &markdown);
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
