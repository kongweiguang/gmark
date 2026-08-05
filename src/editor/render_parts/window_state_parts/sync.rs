// @author kongweiguang

use super::super::*;

impl Editor {
    pub(in crate::editor) fn sync_window_edited_state(&mut self, window: &mut Window) {
        if self.pending_window_edited {
            self.pending_window_edited = false;
            window.set_window_edited(true);
        }
    }

    pub(in crate::editor) fn sync_scroll_viewport(
        &mut self,
        viewport_size: Size<Pixels>,
        cx: &mut Context<Self>,
    ) {
        match self.last_scroll_viewport_size {
            Some(previous) if Self::viewport_size_changed(previous, viewport_size) => {
                self.last_scroll_viewport_size = Some(viewport_size);
                self.request_active_block_scroll_into_view(cx);
            }
            Some(_) => {}
            None => {
                self.last_scroll_viewport_size = Some(viewport_size);
            }
        }
    }

    pub(in crate::editor) fn sync_window_title(
        &mut self,
        window: &mut Window,
        strings: &I18nStrings,
    ) {
        if self.pending_window_title_refresh {
            self.pending_window_title_refresh = false;
            let title =
                Self::window_title(self.file_path.as_deref(), self.is_document_dirty(), strings);
            window.set_window_title(&title);
        }
    }
}
