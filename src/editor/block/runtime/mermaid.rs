// @author kongweiguang

//! Mermaid workbench state transitions kept separate from Markdown persistence.

use gpui::{AnyWindowHandle, AsyncApp, ClipboardItem, Context, WeakEntity, Window};

use super::{Block, BlockEvent, BlockKind, MermaidViewMode};
use crate::components::MermaidSvgExportRequest;

impl Block {
    pub(crate) fn mermaid_view_mode(&self) -> MermaidViewMode {
        self.mermaid_view_mode
    }

    /// 切换工作台视图不触发文档编辑；预览同样接管焦点，使块菜单、删除和键盘导航
    /// 在没有文本输入面的情况下仍可用。
    pub(crate) fn set_mermaid_view_mode(
        &mut self,
        mode: MermaidViewMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind() != BlockKind::MermaidBlock
            || (self.is_read_only() && mode != MermaidViewMode::Preview)
            || self.mermaid_view_mode == mode
        {
            return;
        }
        self.mermaid_view_mode = mode;
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub(crate) fn copy_mermaid_source(&mut self, cx: &mut Context<Self>) {
        if self.kind() != BlockKind::MermaidBlock {
            return;
        }
        let source = self
            .record
            .raw_fallback
            .as_deref()
            .unwrap_or_else(|| self.display_text())
            .to_owned();
        cx.write_to_clipboard(ClipboardItem::new_string(source));
        self.mermaid_copy_feedback = true;
        self.mermaid_copy_feedback_task = Some(cx.spawn(
            async |this: WeakEntity<Block>, cx: &mut AsyncApp| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1_200))
                    .await;
                let _ = this.update(cx, |block, cx| {
                    block.mermaid_copy_feedback = false;
                    block.mermaid_copy_feedback_task = None;
                    cx.notify();
                });
            },
        ));
        cx.notify();
    }

    pub(crate) fn can_export_mermaid_svg(&self) -> bool {
        self.kind() == BlockKind::MermaidBlock
            && self.mermaid_render_error.is_none()
            && self.mermaid_preview_task.is_none()
            && self.mermaid_preview_key.is_some()
            && self.mermaid_preview_key == self.mermaid_successful_preview_key
            && self.last_successful_mermaid_render.is_some()
    }

    pub(crate) fn request_mermaid_svg_export(
        &mut self,
        window: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if !self.can_export_mermaid_svg() {
            return;
        }
        let Some(rendered) = self.last_successful_mermaid_render.as_ref() else {
            return;
        };
        cx.emit(BlockEvent::RequestExportMermaidSvg(
            MermaidSvgExportRequest {
                svg: rendered.svg.clone(),
                window,
            },
        ));
    }
}
