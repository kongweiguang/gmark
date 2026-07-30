// @author kongweiguang

//! Rendered standalone image runtime state.

use super::*;

impl Block {
    pub(crate) fn image_runtime(&self) -> Option<&ImageRuntime> {
        self.image_runtime.as_ref()
    }

    pub(super) fn can_present_as_image(&self) -> bool {
        self.is_table_cell()
            || matches!(
                self.kind(),
                BlockKind::Paragraph
                    | BlockKind::BulletedListItem
                    | BlockKind::NumberedListItem
                    | BlockKind::TaskListItem { .. }
            )
    }

    /// Whether this block's text is a lone image that renders as a
    /// self-contained image widget. Unlike `showing_rendered_image`, this is
    /// derived from the title text rather than the computed runtime, so it is
    /// valid before image runtimes are (re)built.
    pub(crate) fn renders_as_standalone_image(&self) -> bool {
        self.can_present_as_image() && self.standalone_image_markdown_for_runtime().is_some()
    }

    pub(super) fn compute_image_runtime(
        &self,
        base_dir: Option<&Path>,
        syntax: ImageSyntax,
    ) -> Option<ImageRuntime> {
        let resolved_target = syntax.resolve_target(&self.image_reference_definitions)?;
        self.can_present_as_image().then(|| ImageRuntime {
            alt: syntax.alt.clone(),
            src: resolved_target.src.clone(),
            title: resolved_target.title.clone(),
            width_percent: syntax.width_percent,
            resolved_source: resolve_image_source(&resolved_target.src, base_dir),
        })
    }

    pub(crate) fn image_runtime_for_syntax(&self, syntax: ImageSyntax) -> Option<ImageRuntime> {
        self.compute_image_runtime(self.image_base_dir.as_deref(), syntax)
    }

    pub(crate) fn image_base_dir(&self) -> Option<&Path> {
        self.image_base_dir.as_deref()
    }

    pub(super) fn sync_image_runtime(&mut self) {
        let next_runtime = if self.can_present_as_image() {
            self.standalone_image_markdown_for_runtime()
                .and_then(|markdown| parse_standalone_image(&markdown))
                .and_then(|syntax| {
                    self.compute_image_runtime(self.image_base_dir.as_deref(), syntax)
                })
        } else {
            None
        };

        if next_runtime.is_none() {
            self.image_edit_expanded = false;
            self.image_expand_requested = false;
        }
        self.image_runtime = next_runtime;
    }

    fn standalone_image_markdown_for_runtime(&self) -> Option<String> {
        let visible = self.record.title.visible_text();
        if parse_standalone_image(&visible).is_some() {
            return Some(visible);
        }

        let serialized = self.record.title.serialize_markdown();
        parse_standalone_image(&serialized)
            .is_some()
            .then_some(serialized)
    }

    pub(crate) fn request_image_edit_expansion(&mut self) {
        if self.image_runtime.is_some() {
            self.image_expand_requested = true;
        }
    }

    pub(super) fn consume_requested_image_edit_expansion(&mut self) -> bool {
        if self.image_runtime.is_some() && self.image_expand_requested && !self.image_edit_expanded
        {
            self.image_expand_requested = false;
            self.image_edit_expanded = true;
            self.clear_inline_projection();
            self.assign_collapsed_selection_offset(
                self.visible_len(),
                CollapsedCaretAffinity::Default,
                None,
            );
            self.cursor_blink_epoch = Instant::now();
            self.clear_vertical_motion();
            return true;
        }

        false
    }

    pub(crate) fn sync_image_focus_state(&mut self, focused: bool) -> bool {
        if self.image_runtime.is_none() {
            let had_image_state = self.image_edit_expanded
                || self.image_expand_requested
                || self.image_selected
                || self.image_resize_session.is_some()
                || self.image_preview_width_percent.is_some();
            if had_image_state {
                self.image_edit_expanded = false;
                self.image_expand_requested = false;
                self.image_selected = false;
                self.image_resize_session = None;
                self.image_preview_width_percent = None;
                self.clear_inline_projection();
                return true;
            }
            return false;
        }

        if focused {
            return self.consume_requested_image_edit_expansion();
        }

        self.image_selected = false;
        self.image_resize_session = None;
        self.image_preview_width_percent = None;

        if self.image_edit_expanded {
            self.image_edit_expanded = false;
            self.clear_inline_projection();
            return true;
        }

        false
    }

    pub(crate) fn showing_rendered_image(&self) -> bool {
        self.image_runtime.is_some() && !self.is_source_raw_mode() && !self.image_edit_expanded
    }

    pub(crate) fn select_rendered_image(&mut self, cx: &mut Context<Self>) {
        if !self.is_read_only() && self.showing_rendered_image() && !self.image_selected {
            self.image_selected = true;
            self.selected_range = 0..0;
            self.marked_range = None;
            cx.notify();
        }
    }

    pub(crate) fn current_image_width_percent(&self) -> u8 {
        self.image_preview_width_percent
            .or_else(|| {
                self.image_runtime
                    .as_ref()
                    .map(|runtime| runtime.width_percent)
            })
            .unwrap_or(100)
            .clamp(10, 100)
    }

    pub(crate) fn start_image_resize(
        &mut self,
        start_x: Pixels,
        available_width: f32,
        cx: &mut Context<Self>,
    ) {
        if self.is_read_only() || !self.image_selected || !self.showing_rendered_image() {
            return;
        }
        let start_percent = self.current_image_width_percent();
        self.image_resize_session = Some(ImageResizeSession {
            start_x,
            start_percent,
            available_width: available_width.max(1.0),
        });
        self.image_preview_width_percent = Some(start_percent);
        cx.notify();
    }

    pub(crate) fn update_image_resize(&mut self, pointer_x: Pixels, cx: &mut Context<Self>) {
        let Some(session) = self.image_resize_session else {
            return;
        };
        let delta_percent = ((f32::from(pointer_x - session.start_x) / session.available_width)
            * 100.0)
            .round() as i32;
        let next = (i32::from(session.start_percent) + delta_percent).clamp(10, 100) as u8;
        if self.image_preview_width_percent != Some(next) {
            self.image_preview_width_percent = Some(next);
            cx.notify();
        }
    }

    pub(crate) fn finish_image_resize(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(session) = self.image_resize_session.take() else {
            return false;
        };
        let next = self
            .image_preview_width_percent
            .take()
            .unwrap_or(session.start_percent);
        if next == session.start_percent {
            cx.notify();
            return true;
        }
        let source = self.record.title.visible_text().to_owned();
        let Some(rewritten) = rewrite_standalone_image_width(&source, next) else {
            cx.notify();
            return true;
        };
        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        self.replace_text_in_visible_range(0..self.visible_len(), &rewritten, None, false, cx);
        true
    }

    pub(crate) fn cancel_image_selection(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.image_selected {
            return false;
        }
        self.image_selected = false;
        self.image_resize_session = None;
        self.image_preview_width_percent = None;
        cx.notify();
        true
    }
}
