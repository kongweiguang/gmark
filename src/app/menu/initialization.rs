// @author kongweiguang

//! Application menu lifecycle and action registration.

use super::*;

fn handle_window_closed(cx: &mut App) {
    if cx.windows().is_empty() {
        if QuitCoordinator::is_pending(cx) {
            // A discard-and-close action can remove the last window before its
            // deferred quit continuation runs.  Let the coordinator perform
            // the update handoff (or normal quit) instead of exiting early.
            cx.defer(crate::app_menu::continue_pending_quit);
        } else {
            cx.quit();
        }
    }
}

/// Installs menu state, action handlers, and the native menu bar.
pub(crate) fn init(cx: &mut App) {
    EditingCommandHistory::init(cx);
    cx.set_global(AppMenuState::default());
    QuitCoordinator::ensure(cx);
    let subscription = cx.on_window_closed(handle_window_closed);
    cx.global_mut::<AppMenuState>().window_closed_subscription = Some(subscription);

    cx.on_action(|_: &NewWindow, cx| {
        dispatch_menu_action(&NewWindow, cx);
    });
    cx.on_action(|_: &NewTab, cx| {
        dispatch_menu_action(&NewTab, cx);
    });
    cx.on_action(|_: &OpenFile, cx| {
        dispatch_menu_action(&OpenFile, cx);
    });
    cx.on_action(|_: &OpenSafeSource, cx| {
        dispatch_menu_action(&OpenSafeSource, cx);
    });
    cx.on_action(|_: &OpenFolder, cx| {
        dispatch_menu_action(&OpenFolder, cx);
    });
    cx.on_action(|_: &OpenPreferences, cx| {
        dispatch_menu_action(&OpenPreferences, cx);
    });
    cx.on_action(|action: &OpenRecentFile, cx| {
        dispatch_menu_action(action, cx);
    });
    cx.on_action(|_: &NoRecentFiles, cx| {
        dispatch_menu_action(&NoRecentFiles, cx);
    });
    cx.on_action(|_: &AddLanguageConfig, cx| {
        dispatch_menu_action(&AddLanguageConfig, cx);
    });
    cx.on_action(|_: &SaveDocument, cx| {
        dispatch_menu_action(&SaveDocument, cx);
    });
    cx.on_action(|_: &SaveDocumentAs, cx| {
        dispatch_menu_action(&SaveDocumentAs, cx);
    });
    cx.on_action(|_: &ExportHtml, cx| {
        dispatch_menu_action(&ExportHtml, cx);
    });
    cx.on_action(|_: &ExportImage, cx| {
        dispatch_menu_action(&ExportImage, cx);
    });
    cx.on_action(|_: &ExportPdf, cx| {
        dispatch_menu_action(&ExportPdf, cx);
    });
    cx.on_action(|_: &ExportSelection, cx| {
        dispatch_menu_action(&ExportSelection, cx);
    });
    cx.on_action(|_: &ShowDocumentInfo, cx| {
        dispatch_menu_action(&ShowDocumentInfo, cx);
    });
    cx.on_action(|_: &ShowDocumentOutline, cx| {
        dispatch_menu_action(&ShowDocumentOutline, cx);
    });
    cx.on_action(|_: &ShowStructureView, cx| {
        dispatch_menu_action(&ShowStructureView, cx);
    });
    cx.on_action(|_: &ShowStructuredInspector, cx| {
        dispatch_menu_action(&ShowStructuredInspector, cx);
    });
    cx.on_action(|_: &FocusStructuredFilter, cx| {
        dispatch_menu_action(&FocusStructuredFilter, cx);
    });
    cx.on_action(|_: &FocusStructuredColumns, cx| {
        dispatch_menu_action(&FocusStructuredColumns, cx);
    });
    cx.on_action(|_: &NormalizeLineEndingsLf, cx| {
        dispatch_menu_action(&NormalizeLineEndingsLf, cx);
    });
    cx.on_action(|_: &NormalizeLineEndingsCrLf, cx| {
        dispatch_menu_action(&NormalizeLineEndingsCrLf, cx);
    });
    cx.on_action(|_: &NormalizeLineEndingsCr, cx| {
        dispatch_menu_action(&NormalizeLineEndingsCr, cx);
    });
    cx.on_action(|action: &SelectLanguage, cx| {
        dispatch_menu_action(action, cx);
    });
    cx.on_action(|_: &CheckForUpdates, cx| {
        dispatch_menu_action(&CheckForUpdates, cx);
    });
    cx.on_action(|_: &OpenCrashReports, cx| {
        dispatch_menu_action(&OpenCrashReports, cx);
    });
    cx.on_action(|_: &OpenPrivacyPolicy, cx| {
        dispatch_menu_action(&OpenPrivacyPolicy, cx);
    });
    cx.on_action(|_: &ShowAbout, cx| {
        dispatch_menu_action(&ShowAbout, cx);
    });
    cx.on_action(|_: &ToggleWorkspace, cx| {
        dispatch_menu_action(&ToggleWorkspace, cx);
    });
    cx.on_action(|_: &ToggleFocusMode, cx| {
        dispatch_menu_action(&ToggleFocusMode, cx);
    });
    cx.on_action(|_: &ToggleTypewriterMode, cx| {
        dispatch_menu_action(&ToggleTypewriterMode, cx);
    });
    cx.on_action(|_: &BoldSelection, cx| {
        dispatch_menu_action(&BoldSelection, cx);
    });
    cx.on_action(|_: &ItalicSelection, cx| {
        dispatch_menu_action(&ItalicSelection, cx);
    });
    cx.on_action(|_: &StrikethroughSelection, cx| {
        dispatch_menu_action(&StrikethroughSelection, cx);
    });
    cx.on_action(|_: &UnderlineSelection, cx| {
        dispatch_menu_action(&UnderlineSelection, cx);
    });
    cx.on_action(|_: &HighlightSelection, cx| dispatch_menu_action(&HighlightSelection, cx));
    cx.on_action(|_: &SuperscriptSelection, cx| dispatch_menu_action(&SuperscriptSelection, cx));
    cx.on_action(|_: &SubscriptSelection, cx| dispatch_menu_action(&SubscriptSelection, cx));
    cx.on_action(|_: &InlineMathSelection, cx| dispatch_menu_action(&InlineMathSelection, cx));
    cx.on_action(|_: &CodeSelection, cx| {
        dispatch_menu_action(&CodeSelection, cx);
    });
    cx.on_action(|_: &LinkSelection, cx| {
        dispatch_menu_action(&LinkSelection, cx);
    });
    cx.on_action(|_: &SetHeading1, cx| dispatch_menu_action(&SetHeading1, cx));
    cx.on_action(|_: &SetHeading2, cx| dispatch_menu_action(&SetHeading2, cx));
    cx.on_action(|_: &SetHeading3, cx| dispatch_menu_action(&SetHeading3, cx));
    cx.on_action(|_: &SetHeading4, cx| dispatch_menu_action(&SetHeading4, cx));
    cx.on_action(|_: &SetHeading5, cx| dispatch_menu_action(&SetHeading5, cx));
    cx.on_action(|_: &SetHeading6, cx| dispatch_menu_action(&SetHeading6, cx));
    cx.on_action(|_: &SetParagraph, cx| dispatch_menu_action(&SetParagraph, cx));
    cx.on_action(|_: &SetBulletedList, cx| dispatch_menu_action(&SetBulletedList, cx));
    cx.on_action(|_: &SetNumberedList, cx| dispatch_menu_action(&SetNumberedList, cx));
    cx.on_action(|_: &SetTaskList, cx| dispatch_menu_action(&SetTaskList, cx));
    cx.on_action(|_: &SetQuote, cx| dispatch_menu_action(&SetQuote, cx));
    cx.on_action(|_: &SetCodeBlock, cx| dispatch_menu_action(&SetCodeBlock, cx));
    cx.on_action(|_: &InsertResource, cx| dispatch_menu_action(&InsertResource, cx));
    cx.on_action(|_: &QuitApplication, cx| {
        dispatch_menu_action(&QuitApplication, cx);
    });
    cx.on_action(|_: &CloseWindow, cx| {
        dispatch_menu_action(&CloseWindow, cx);
    });
    cx.on_action(|_: &CloseTab, cx| {
        dispatch_menu_action(&CloseTab, cx);
    });
    cx.on_action(|_: &ReopenClosedTab, cx| {
        dispatch_menu_action(&ReopenClosedTab, cx);
    });

    install_menus(cx);
    cx.activate(true);
}
