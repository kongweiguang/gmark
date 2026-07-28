// @author kongweiguang

use super::*;

impl Editor {
    fn with_document_host_action<A: gpui::Action>(
        &mut self,
        action: &A,
        window: &mut Window,
        cx: &mut Context<Self>,
        handler: impl FnOnce(
            &mut crate::document_host::DocumentHost,
            &A,
            &mut Window,
            &mut Context<crate::document_host::DocumentHost>,
        ),
    ) {
        if let Some(host) = self.document_host.clone() {
            host.update(cx, |host, cx| handler(host, action, window, cx));
        }
    }

    pub(super) fn on_collapse_fold_action(
        &mut self,
        action: &crate::components::CollapseFold,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_document_host_action(action, window, cx, |host, action, window, cx| {
            host.on_collapse_fold(action, window, cx)
        });
    }

    pub(super) fn on_expand_fold_action(
        &mut self,
        action: &crate::components::ExpandFold,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_document_host_action(action, window, cx, |host, action, window, cx| {
            host.on_expand_fold(action, window, cx)
        });
    }

    pub(super) fn on_collapse_all_folds_action(
        &mut self,
        action: &crate::components::CollapseAllFolds,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_document_host_action(action, window, cx, |host, action, window, cx| {
            host.on_collapse_all_folds(action, window, cx)
        });
    }

    pub(super) fn on_expand_all_folds_action(
        &mut self,
        action: &crate::components::ExpandAllFolds,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_document_host_action(action, window, cx, |host, action, window, cx| {
            host.on_expand_all_folds(action, window, cx)
        });
    }

    pub(super) fn on_format_document_action(
        &mut self,
        action: &crate::components::FormatDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_document_host_action(action, window, cx, |host, action, window, cx| {
            host.on_format_document(action, window, cx)
        });
    }

    pub(super) fn on_format_selection_action(
        &mut self,
        action: &crate::components::FormatSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_document_host_action(action, window, cx, |host, action, window, cx| {
            host.on_format_selection(action, window, cx)
        });
    }

    pub(super) fn on_cancel_formatting_action(
        &mut self,
        action: &crate::components::CancelFormatting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_document_host_action(action, window, cx, |host, action, window, cx| {
            host.on_cancel_formatting(action, window, cx)
        });
    }
}
