// @author kongweiguang

//! GPUI shell for one active pane tab and its drag preview.

use std::fmt;
use std::rc::Rc;

use gmark_document_runtime::{DocumentId, DocumentViewInstanceId};
use gpui::{
    AnyElement, Context, ElementId, FocusHandle, Hsla, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Point, Render, SharedString, Styled,
    Subscription, Window, div, px,
};

use super::document::PaneDocumentRef;
use crate::editor::panes::{PaneId, PaneTabDragPayload, PaneWorkspaceController, TabId};

/// One mounted active tab surface.
///
/// The entity is intentionally small: the owning Editor can place its
/// DocumentHost/DocumentLease-backed surface inside this shell later without
/// changing pane-tree or tab semantics.
pub struct PaneView {
    pane_id: PaneId,
    tab_id: TabId,
    document: PaneDocumentRef,
    focus_handle: FocusHandle,
    focus_subscription: Option<Subscription>,
    content_factory: Option<PaneContentFactory>,
    controller: PaneWorkspaceController,
}

/// Renderable slot supplied by the owning Editor for the active document.
///
/// The pane layer does not know whether a host is a Markdown editor, preview,
/// or another document surface.  The factory is called only for the active
/// entity, so inactive tabs cannot accidentally create a projection task.
pub type PaneContentFactory =
    Rc<dyn Fn(&PaneDocumentRef, &mut Window, &mut Context<PaneView>) -> AnyElement>;

impl fmt::Debug for PaneView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PaneView")
            .field("pane_id", &self.pane_id)
            .field("tab_id", &self.tab_id)
            .field("document", &self.document)
            .finish()
    }
}

impl PaneView {
    pub fn new(
        pane_id: PaneId,
        tab_id: TabId,
        document: PaneDocumentRef,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            pane_id,
            tab_id,
            document,
            focus_handle: cx.focus_handle(),
            focus_subscription: None,
            content_factory: None,
            controller: PaneWorkspaceController::noop(),
        }
    }

    pub fn with_content_factory(
        pane_id: PaneId,
        tab_id: TabId,
        document: PaneDocumentRef,
        content_factory: PaneContentFactory,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self::new(pane_id, tab_id, document, cx);
        view.content_factory = Some(content_factory);
        view
    }

    pub const fn pane_id(&self) -> PaneId {
        self.pane_id
    }

    pub const fn tab_id(&self) -> TabId {
        self.tab_id
    }

    pub const fn document_id(&self) -> DocumentId {
        self.document.document_id()
    }

    pub const fn view_id(&self) -> DocumentViewInstanceId {
        self.document.view_id()
    }

    pub fn document(&self) -> &PaneDocumentRef {
        &self.document
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    pub fn set_content_factory(&mut self, content_factory: Option<PaneContentFactory>) {
        self.content_factory = content_factory;
    }

    pub fn set_controller(&mut self, controller: PaneWorkspaceController) {
        self.controller = controller;
    }

    fn on_pointer_focus(
        &mut self,
        _event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // This listener only updates pane identity. The nested editor keeps the
        // original event, caret hit test, drag selection, and input focus.
        self.controller.focus(self.pane_id, window, cx);
    }
}

impl Render for PaneView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_subscription.is_none() {
            let focus_handle = self.focus_handle.clone();
            let pane = self.pane_id;
            self.focus_subscription =
                Some(
                    cx.on_focus_in(&focus_handle, window, move |this, window, cx| {
                        // Track pane identity from the real nested editor focus
                        // path, without placing a pointer hitbox over its text.
                        this.controller.focus(pane, window, cx);
                    }),
                );
        }
        let theme = cx.global::<crate::theme::ThemeManager>().current_arc();
        let workbench = &theme.colors.workbench;
        let content = self
            .content_factory
            .as_ref()
            .map(|factory| factory(&self.document, window, cx));
        let pane_id = self.pane_id.as_uuid();
        let tab_id = self.tab_id.as_uuid();
        let drop_controller = self.controller.clone();
        let target_pane = self.pane_id;
        div()
            .id(ElementId::Name(
                format!("pane-view-{pane_id}-{tab_id}").into(),
            ))
            .debug_selector(|| "pane-view".to_owned())
            .size_full()
            .flex()
            .flex_col()
            .bg(workbench.editor_surface)
            .text_color(workbench.text_primary)
            .tab_index(0)
            .track_focus(&self.focus_handle)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_pointer_focus))
            .drag_over::<PaneTabDragPayload>(move |style, _, _, _| style.opacity(0.92))
            .on_drop(move |payload: &PaneTabDragPayload, window, cx| {
                if payload.source == target_pane {
                    return;
                }
                if window.modifiers().secondary() {
                    drop_controller.copy_tab(payload.source, target_pane, payload.tab, window, cx);
                } else {
                    drop_controller.move_tab(payload.source, target_pane, payload.tab, window, cx);
                }
            })
            .child(content.unwrap_or_else(|| {
                div()
                    .id(ElementId::Name(
                        format!("pane-surface-{pane_id}-{tab_id}").into(),
                    ))
                    .flex_1()
                    .min_h(px(0.0))
                    .bg(workbench.editor_surface)
                    .into_any_element()
            }))
    }
}

#[derive(Clone)]
pub(super) struct PaneTabDragVisual {
    pub(super) title: SharedString,
    pub(super) background: Hsla,
    pub(super) text: Hsla,
}

pub(super) struct PaneTabDragPreview {
    pub(super) visual: PaneTabDragVisual,
    pub(super) position: Point<Pixels>,
}

impl Render for PaneTabDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .left(self.position.x + px(10.0))
            .top(self.position.y + px(10.0))
            .w(px(180.0))
            .h(px(32.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .overflow_hidden()
            .truncate()
            .rounded(px(5.0))
            .shadow_md()
            .bg(self.visual.background)
            .text_color(self.visual.text)
            .child(self.visual.title.clone())
    }
}
