// @author kongweiguang

//! Action definitions and key bindings for both block editing and app-level
//! window/menu commands.
//!
//! Text-editing actions are scoped to the `"BlockEditor"` key context on each
//! block. Window and menu commands use global bindings so they remain
//! available even when focus is on non-block UI such as dialogs or buttons.

use std::collections::{BTreeMap, BTreeSet};

use gpui::*;
use schemars::JsonSchema;
use serde::Deserialize;

actions!(
    gmark,
    [
        Newline,
        DeleteBack,
        Delete,
        WordDeleteBack,
        WordDeleteForward,
        FocusPrev,
        FocusNext,
        MoveLeft,
        MoveRight,
        WordMoveLeft,
        WordMoveRight,
        Home,
        End,
        BlockUp,
        BlockDown,
        PageUp,
        PageDown,
        JumpToTop,
        JumpToBottom,
        SelectLeft,
        SelectRight,
        WordSelectLeft,
        WordSelectRight,
        SelectHome,
        SelectEnd,
        SelectAll,
        Copy,
        CopyAsMarkdown,
        Cut,
        Paste,
        PasteAsPlainText,
        Undo,
        Redo,
        BoldSelection,
        ItalicSelection,
        StrikethroughSelection,
        UnderlineSelection,
        HighlightSelection,
        SuperscriptSelection,
        SubscriptSelection,
        InlineMathSelection,
        CodeSelection,
        LinkSelection,
        IndentBlock,
        OutdentBlock,
        ExitCodeBlock,
        SaveDocument,
        NewTab,
        NewWindow,
        OpenFile,
        OpenSafeSource,
        OpenFolder,
        OpenPreferences,
        NoRecentFiles,
        SaveDocumentAs,
        ExportHtml,
        ExportImage,
        ExportPdf,
        ExportSelection,
        ShowDocumentInfo,
        ShowDocumentOutline,
        ShowStructureView,
        ShowStructuredInspector,
        FocusStructuredFilter,
        FocusStructuredColumns,
        AddLanguageConfig,
        QuitApplication,
        CloseWindow,
        CloseTab,
        ReopenClosedTab,
        PreviousTab,
        NextTab,
        CheckForUpdates,
        OpenCrashReports,
        OpenPrivacyPolicy,
        ShowAbout,
        InstallCliTool,
        UninstallCliTool,
        DismissTransientUi,
        ToggleViewMode,
        ToggleWorkspace,
        ToggleDocumentSidebar,
        QuickOpen,
        CommandPalette,
        GoToLine,
        ToggleFocusMode,
        ToggleTypewriterMode,
        NormalizeLineEndingsLf,
        NormalizeLineEndingsCrLf,
        NormalizeLineEndingsCr,
        CollapseFold,
        ExpandFold,
        CollapseAllFolds,
        ExpandAllFolds,
        FormatDocument,
        FormatSelection,
        CancelFormatting,
        SetHeading1,
        SetHeading2,
        SetHeading3,
        SetHeading4,
        SetHeading5,
        SetHeading6,
        SetParagraph,
        SetBulletedList,
        SetNumberedList,
        SetTaskList,
        SetQuote,
        SetCodeBlock,
        InsertResource,
    ]
);

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = gmark)]
pub struct FindInDocument;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = gmark)]
pub struct ReplaceInDocument;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = gmark)]
pub struct FindNext;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = gmark)]
pub struct FindPrevious;

/// Selects a UI language from the app-level language registry.
#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema, gpui::Action)]
#[action(namespace = gmark)]
#[serde(deny_unknown_fields)]
pub struct SelectLanguage {
    /// Stable language id from the built-in language catalog.
    pub language_id: String,
}

/// Opens a previously recorded Markdown file path.
#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema, gpui::Action)]
#[action(namespace = gmark)]
#[serde(deny_unknown_fields)]
pub struct OpenRecentFile {
    /// Path stored in gmark's recent-file history.
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ShortcutCategory {
    File,
    Edit,
    Navigation,
    Formatting,
    Block,
    Other,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ShortcutCommand {
    Newline,
    DeleteBack,
    Delete,
    WordDeleteBack,
    WordDeleteForward,
    FocusPrev,
    FocusNext,
    MoveLeft,
    MoveRight,
    WordMoveLeft,
    WordMoveRight,
    Home,
    End,
    BlockUp,
    BlockDown,
    PageUp,
    PageDown,
    JumpToTop,
    JumpToBottom,
    SelectLeft,
    SelectRight,
    WordSelectLeft,
    WordSelectRight,
    SelectHome,
    SelectEnd,
    SelectAll,
    Copy,
    CopyAsMarkdown,
    Cut,
    Paste,
    PasteAsPlainText,
    Undo,
    Redo,
    BoldSelection,
    ItalicSelection,
    StrikethroughSelection,
    UnderlineSelection,
    CodeSelection,
    LinkSelection,
    HighlightSelection,
    SuperscriptSelection,
    SubscriptSelection,
    InlineMathSelection,
    SetParagraph,
    SetHeading1,
    SetHeading2,
    SetHeading3,
    SetHeading4,
    SetHeading5,
    SetHeading6,
    IndentBlock,
    OutdentBlock,
    ExitCodeBlock,
    SaveDocument,
    SaveDocumentAs,
    NewTab,
    NewWindow,
    OpenFile,
    OpenFolder,
    OpenPreferences,
    QuitApplication,
    CloseWindow,
    CloseTab,
    ReopenClosedTab,
    PreviousTab,
    NextTab,
    DismissTransientUi,
    ToggleViewMode,
    ToggleWorkspace,
    ToggleDocumentSidebar,
    QuickOpen,
    CommandPalette,
    GoToLine,
    FindInDocument,
    ReplaceInDocument,
    FindNext,
    FindPrevious,
    ToggleFocusMode,
    ToggleTypewriterMode,
    CollapseFold,
    ExpandFold,
    CollapseAllFolds,
    ExpandAllFolds,
    FormatDocument,
    FormatSelection,
    CancelFormatting,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ShortcutDefinition {
    pub(crate) command: ShortcutCommand,
    pub(crate) id: &'static str,
    pub(crate) category: ShortcutCategory,
    pub(crate) default_keys: &'static [&'static str],
    pub(crate) context: Option<&'static str>,
}

const BLOCK_CONTEXT: Option<&str> = Some("BlockEditor");
const SELECT_ALL_ID: &str = "select_all";
const LEGACY_SELECT_ALL_IDS: &[&str] = &[
    "select_all_source_text",
    "select_focused_block_text_rendered",
];

// On macOS cmd-q is the system quit shortcut; Windows/Linux use Alt+F4 (OS-handled).
#[cfg(target_os = "macos")]
const QUIT_APPLICATION_DEFAULT_KEYS: &[&str] = &["cmd-q"];
#[cfg(not(target_os = "macos"))]
const QUIT_APPLICATION_DEFAULT_KEYS: &[&str] = &[];

// On macOS cmd-w closes the current window; no app-level binding needed on other platforms.
#[cfg(target_os = "macos")]
const CLOSE_WINDOW_DEFAULT_KEYS: &[&str] = &["cmd-shift-w"];
#[cfg(not(target_os = "macos"))]
const CLOSE_WINDOW_DEFAULT_KEYS: &[&str] = &["ctrl-shift-w"];

mod definitions;
mod shortcuts;

/// Register default key bindings for isolated GPUI tests.
#[cfg(test)]
pub fn init(cx: &mut App) {
    shortcuts::init(cx);
}

pub(crate) use shortcuts::{
    init_with_keybindings, install_keybindings, normalize_shortcut_config, normalize_shortcut_keys,
    resolved_shortcut_keys, shortcut_conflict_for, shortcut_definitions,
};

#[cfg(test)]
#[path = "../../../tests/unit/components/actions.rs"]
mod tests;
