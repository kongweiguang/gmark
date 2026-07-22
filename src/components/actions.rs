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
        AddLanguageConfig,
        AddThemeConfig,
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
        QuickOpen,
        CommandPalette,
        GoToLine,
        ToggleFocusMode,
        ToggleTypewriterMode,
        NormalizeLineEndingsLf,
        NormalizeLineEndingsCrLf,
        NormalizeLineEndingsCr,
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

/// Selects a theme from the app-level theme registry.
#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema, gpui::Action)]
#[action(namespace = gmark)]
#[serde(deny_unknown_fields)]
pub struct SelectTheme {
    /// Stable theme id from the built-in theme catalog.
    pub theme_id: String,
}

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
    QuickOpen,
    CommandPalette,
    GoToLine,
    FindInDocument,
    ReplaceInDocument,
    FindNext,
    FindPrevious,
    ToggleFocusMode,
    ToggleTypewriterMode,
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

const SHORTCUT_DEFINITIONS: &[ShortcutDefinition] = &[
    ShortcutDefinition {
        command: ShortcutCommand::Newline,
        id: "newline",
        category: ShortcutCategory::Block,
        default_keys: &["enter"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::DeleteBack,
        id: "delete_back",
        category: ShortcutCategory::Edit,
        default_keys: &["backspace"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Delete,
        id: "delete",
        category: ShortcutCategory::Edit,
        default_keys: &["delete"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::WordDeleteBack,
        id: "word_delete_back",
        category: ShortcutCategory::Edit,
        default_keys: &["ctrl-backspace", "alt-backspace"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::WordDeleteForward,
        id: "word_delete_forward",
        category: ShortcutCategory::Edit,
        default_keys: &["ctrl-delete", "alt-delete"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::FocusPrev,
        id: "focus_prev",
        category: ShortcutCategory::Navigation,
        default_keys: &["up"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::FocusNext,
        id: "focus_next",
        category: ShortcutCategory::Navigation,
        default_keys: &["down"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::MoveLeft,
        id: "move_left",
        category: ShortcutCategory::Navigation,
        default_keys: &["left"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::MoveRight,
        id: "move_right",
        category: ShortcutCategory::Navigation,
        default_keys: &["right"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::WordMoveLeft,
        id: "word_move_left",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-left", "alt-left"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::WordMoveRight,
        id: "word_move_right",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-right", "alt-right"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Home,
        id: "home",
        category: ShortcutCategory::Navigation,
        default_keys: &["home"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::End,
        id: "end",
        category: ShortcutCategory::Navigation,
        default_keys: &["end"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::BlockUp,
        id: "block_up",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-up", "alt-up"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::BlockDown,
        id: "block_down",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-down", "alt-down"],
        context: BLOCK_CONTEXT,
    },
    // Page scroll and document jumps operate on the editor viewport rather than
    // a single block, so they use global bindings (no context) and stay active
    // in both Rendered and Source mode.
    ShortcutDefinition {
        command: ShortcutCommand::PageUp,
        id: "page_up",
        category: ShortcutCategory::Navigation,
        default_keys: &["pageup"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::PageDown,
        id: "page_down",
        category: ShortcutCategory::Navigation,
        default_keys: &["pagedown"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::JumpToTop,
        id: "jump_to_top",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-home", "cmd-up"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::JumpToBottom,
        id: "jump_to_bottom",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-end", "cmd-down"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SelectLeft,
        id: "select_left",
        category: ShortcutCategory::Navigation,
        default_keys: &["shift-left"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SelectRight,
        id: "select_right",
        category: ShortcutCategory::Navigation,
        default_keys: &["shift-right"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::WordSelectLeft,
        id: "word_select_left",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-shift-left", "alt-shift-left"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::WordSelectRight,
        id: "word_select_right",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-shift-right", "alt-shift-right"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SelectHome,
        id: "select_home",
        category: ShortcutCategory::Navigation,
        default_keys: &["shift-home"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SelectEnd,
        id: "select_end",
        category: ShortcutCategory::Navigation,
        default_keys: &["shift-end"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SelectAll,
        id: SELECT_ALL_ID,
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-a", "ctrl-a"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Copy,
        id: "copy",
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-c", "ctrl-c"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::CopyAsMarkdown,
        id: "copy_as_markdown",
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-shift-c", "ctrl-shift-c"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Cut,
        id: "cut",
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-x", "ctrl-x"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Paste,
        id: "paste",
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-v", "ctrl-v"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::PasteAsPlainText,
        id: "paste_as_plain_text",
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-shift-v", "ctrl-shift-v"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Undo,
        id: "undo",
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-z", "ctrl-z"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Redo,
        id: "redo",
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-shift-z", "ctrl-y"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::BoldSelection,
        id: "bold_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &["cmd-b", "ctrl-b"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::ItalicSelection,
        id: "italic_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &["cmd-i", "ctrl-i"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::StrikethroughSelection,
        id: "strikethrough_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &["cmd-shift-x", "ctrl-shift-x"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::UnderlineSelection,
        id: "underline_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &["cmd-u", "ctrl-u"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::CodeSelection,
        id: "code_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &["cmd-`", "ctrl-`"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::LinkSelection,
        id: "link_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &["cmd-k", "ctrl-k"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::HighlightSelection,
        id: "highlight_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &[],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SuperscriptSelection,
        id: "superscript_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &[],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SubscriptSelection,
        id: "subscript_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &[],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::InlineMathSelection,
        id: "inline_math_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &[],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SetParagraph,
        id: "set_paragraph",
        category: ShortcutCategory::Block,
        default_keys: &["cmd-alt-0", "ctrl-alt-0"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SetHeading1,
        id: "set_heading_1",
        category: ShortcutCategory::Block,
        default_keys: &["cmd-alt-1", "ctrl-alt-1"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SetHeading2,
        id: "set_heading_2",
        category: ShortcutCategory::Block,
        default_keys: &["cmd-alt-2", "ctrl-alt-2"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SetHeading3,
        id: "set_heading_3",
        category: ShortcutCategory::Block,
        default_keys: &["cmd-alt-3", "ctrl-alt-3"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SetHeading4,
        id: "set_heading_4",
        category: ShortcutCategory::Block,
        default_keys: &["cmd-alt-4", "ctrl-alt-4"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SetHeading5,
        id: "set_heading_5",
        category: ShortcutCategory::Block,
        default_keys: &["cmd-alt-5", "ctrl-alt-5"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SetHeading6,
        id: "set_heading_6",
        category: ShortcutCategory::Block,
        default_keys: &["cmd-alt-6", "ctrl-alt-6"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::IndentBlock,
        id: "indent_block",
        category: ShortcutCategory::Block,
        default_keys: &["tab"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::OutdentBlock,
        id: "outdent_block",
        category: ShortcutCategory::Block,
        default_keys: &["shift-tab"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::ExitCodeBlock,
        id: "exit_code_block",
        category: ShortcutCategory::Block,
        default_keys: &["cmd-enter", "ctrl-enter"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SaveDocument,
        id: "save_document",
        category: ShortcutCategory::File,
        default_keys: &["cmd-s", "ctrl-s"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SaveDocumentAs,
        id: "save_document_as",
        category: ShortcutCategory::File,
        default_keys: &["cmd-shift-s", "ctrl-shift-s"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::NewTab,
        id: "new_tab",
        category: ShortcutCategory::File,
        default_keys: &["cmd-n", "ctrl-n", "cmd-t", "ctrl-t"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::NewWindow,
        id: "new_window",
        category: ShortcutCategory::File,
        default_keys: &["cmd-shift-n", "ctrl-shift-n"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::OpenFile,
        id: "open_file",
        category: ShortcutCategory::File,
        default_keys: &["cmd-o", "ctrl-o"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::QuitApplication,
        id: "quit_application",
        category: ShortcutCategory::File,
        default_keys: QUIT_APPLICATION_DEFAULT_KEYS,
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::CloseWindow,
        id: "close_window",
        category: ShortcutCategory::File,
        default_keys: CLOSE_WINDOW_DEFAULT_KEYS,
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::CloseTab,
        id: "close_tab",
        category: ShortcutCategory::File,
        default_keys: &["cmd-w", "ctrl-w"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::ReopenClosedTab,
        id: "reopen_closed_tab",
        category: ShortcutCategory::File,
        default_keys: &["cmd-shift-t", "ctrl-shift-t"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::PreviousTab,
        id: "previous_tab",
        category: ShortcutCategory::Navigation,
        default_keys: &["cmd-pageup", "ctrl-pageup"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::NextTab,
        id: "next_tab",
        category: ShortcutCategory::Navigation,
        default_keys: &["cmd-pagedown", "ctrl-pagedown"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::DismissTransientUi,
        id: "dismiss_transient_ui",
        category: ShortcutCategory::Other,
        default_keys: &["escape"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::ToggleViewMode,
        id: "toggle_view_mode",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-tab", "cmd-tab"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::ToggleWorkspace,
        id: "toggle_workspace",
        category: ShortcutCategory::Navigation,
        default_keys: &["cmd-b", "ctrl-b"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::OpenFolder,
        id: "open_folder",
        category: ShortcutCategory::File,
        default_keys: &["cmd-shift-o", "ctrl-shift-o"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::OpenPreferences,
        id: "open_preferences",
        category: ShortcutCategory::File,
        default_keys: &["cmd-,", "ctrl-,"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::QuickOpen,
        id: "quick_open",
        category: ShortcutCategory::Navigation,
        default_keys: &["cmd-p", "ctrl-p"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::CommandPalette,
        id: "command_palette",
        category: ShortcutCategory::Navigation,
        default_keys: &["cmd-shift-p", "ctrl-shift-p"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::GoToLine,
        id: "go_to_line",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-g"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::FindInDocument,
        id: "find_in_document",
        category: ShortcutCategory::Navigation,
        default_keys: &["cmd-f", "ctrl-f"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::ReplaceInDocument,
        id: "replace_in_document",
        category: ShortcutCategory::Navigation,
        default_keys: &["cmd-alt-f", "ctrl-h"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::FindNext,
        id: "find_next",
        category: ShortcutCategory::Navigation,
        default_keys: &["f3", "cmd-g"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::FindPrevious,
        id: "find_previous",
        category: ShortcutCategory::Navigation,
        default_keys: &["shift-f3", "cmd-shift-g"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::ToggleFocusMode,
        id: "toggle_focus_mode",
        category: ShortcutCategory::Navigation,
        default_keys: &["cmd-shift-f", "ctrl-shift-f"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::ToggleTypewriterMode,
        id: "toggle_typewriter_mode",
        category: ShortcutCategory::Navigation,
        default_keys: &["cmd-alt-t", "ctrl-alt-t"],
        context: None,
    },
];
#[path = "actions_parts/shortcuts.rs"]
mod shortcuts;
#[cfg(test)]
pub fn init(cx: &mut App) {
    shortcuts::init(cx);
}
pub(crate) use shortcuts::{
    init_with_keybindings, install_keybindings, normalize_shortcut_config, normalize_shortcut_keys,
    resolved_shortcut_keys, shortcut_conflict_for, shortcut_definitions,
};
#[cfg(test)]
#[path = "../../tests/unit/components/actions.rs"]
mod tests;
