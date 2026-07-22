// @author kongweiguang

//! Explicit conversion gate for non-UTF-8 source files.

use gpui::*;

use super::*;

impl Editor {
    pub(super) fn request_encoding_conversion(&mut self, cx: &mut Context<Self>) {
        self.show_encoding_conversion_dialog = true;
        self.close_menu_bar(cx);
        self.dismiss_contextual_overlays(cx);
        cx.notify();
    }

    pub(crate) fn on_convert_encoding_to_utf8(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.document_host.clone() {
            view.update(cx, |view, cx| view.convert_source_encoding_to_utf8(cx));
        }
        self.source_encoding = crate::document_io::DocumentEncoding::Utf8;
        self.show_encoding_conversion_dialog = false;
        self.set_view_mode(ViewMode::Rendered, cx);
        // 转换改变下一次保存的字节编码，即使规范化文本不变也必须进入恢复与保存流程。
        self.mark_source_format_dirty(cx);
    }

    pub(crate) fn on_keep_legacy_encoding_read_only(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_encoding_conversion_dialog = false;
        cx.notify();
    }
}
