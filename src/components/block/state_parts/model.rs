// @author kongweiguang

use super::*;

/// Persistent data of a block independent of the editor runtime.
///
/// Holds the block's identity, kind, inline-formatted title, and tree
/// references (parent/children via UUID). Raw-preserved Markdown keeps its
/// original source in `raw_fallback` so it round-trips through save/load
/// losslessly.
#[derive(Debug, Clone)]
pub struct BlockRecord {
    pub id: Uuid,
    pub kind: BlockKind,
    pub title: InlineTextTree,
    pub table: Option<TableData>,
    pub html: Option<HtmlDocument>,
    pub parent: Option<Uuid>,
    pub content: Vec<Uuid>,
    pub raw_fallback: Option<String>,
}

impl BlockRecord {
    pub fn new(kind: BlockKind, title: InlineTextTree) -> Self {
        let mut record = Self {
            id: Uuid::new_v4(),
            kind,
            title,
            table: None,
            html: None,
            parent: None,
            content: Vec::new(),
            raw_fallback: None,
        };
        record.sync_raw_fallback();
        record
    }

    pub fn with_plain_text(kind: BlockKind, text: impl Into<String>) -> Self {
        Self::new(kind, InlineTextTree::plain(text.into()))
    }

    pub fn paragraph(text: impl Into<String>) -> Self {
        Self::with_plain_text(BlockKind::Paragraph, text)
    }

    pub fn raw_markdown(markdown: impl Into<String>) -> Self {
        let markdown = markdown.into();
        let mut record = Self::with_plain_text(BlockKind::RawMarkdown, markdown.clone());
        record.raw_fallback = Some(markdown);
        record
    }

    /// Frontmatter 继续按不透明源码保真，但 Live 初始焦点应落到正文，避免打开文件
    /// 就把两条 YAML fence 暴露成正在编辑的原始文本。
    pub fn is_yaml_frontmatter(&self) -> bool {
        if self.kind != BlockKind::RawMarkdown {
            return false;
        }
        let Some(raw) = self.raw_fallback.as_deref() else {
            return false;
        };
        let mut lines = raw.lines();
        let opening = lines
            .next()
            .map(|line| line.strip_prefix('\u{feff}').unwrap_or(line));
        let closing = raw.lines().next_back().map(str::trim_end);
        opening.is_some_and(|line| line.trim_end() == "---")
            && closing.is_some_and(|line| matches!(line, "---" | "..."))
    }

    pub fn comment(markdown: impl Into<String>) -> Self {
        let markdown = markdown.into();
        let mut record = Self::with_plain_text(BlockKind::Comment, markdown.clone());
        record.raw_fallback = Some(markdown);
        record
    }

    #[cfg(test)]
    pub fn html(markdown: impl Into<String>) -> Self {
        let markdown = markdown.into();
        let html = parse_html_document(&markdown);
        let mut record = Self::with_plain_text(BlockKind::HtmlBlock, markdown.clone());
        record.html = Some(html);
        record.raw_fallback = Some(markdown);
        record
    }

    pub fn math(markdown: impl Into<String>) -> Self {
        let markdown = markdown.into();
        let mut record = Self::with_plain_text(BlockKind::MathBlock, markdown.clone());
        record.raw_fallback = Some(markdown);
        record
    }

    pub fn mermaid(markdown: impl Into<String>) -> Self {
        let markdown = markdown.into();
        let mut record = Self::with_plain_text(BlockKind::MermaidBlock, markdown.clone());
        record.raw_fallback = Some(markdown);
        record
    }

    pub fn table(table: TableData) -> Self {
        let mut record = Self::new(BlockKind::Table, InlineTextTree::plain(String::new()));
        record.table = Some(table);
        record
    }

    pub fn set_title(&mut self, title: InlineTextTree) {
        self.title = title;
        self.sync_raw_fallback();
    }

    /// Export the block title as Markdown: fragment style flags are
    /// serialized back to delimiter markers via [`InlineTextTree::serialize_markdown`].
    pub fn title_markdown(&self) -> String {
        self.title.serialize_markdown()
    }

    /// Returns true for block kinds that keep their original source text
    /// in `raw_fallback` because they are preserved as opaque Markdown.
    pub fn kind_uses_raw_fallback(&self) -> bool {
        matches!(
            self.kind,
            BlockKind::RawMarkdown
                | BlockKind::Comment
                | BlockKind::HtmlBlock
                | BlockKind::MathBlock
                | BlockKind::MermaidBlock
        )
    }

    /// Serialize this block back to a single Markdown line, including
    /// indentation for nested blocks and list ordinal for numbered items.
    /// Raw-preserved blocks produce their fallback text when at depth 0.
    pub fn markdown_line(&self, depth: usize, list_ordinal: Option<usize>) -> String {
        let indentation = "  ".repeat(depth);
        let title_markdown = self.title_markdown_for_output();
        match self.kind {
            BlockKind::Paragraph => indent_multiline(&title_markdown, &indentation),
            BlockKind::Separator => "---".to_string(),
            BlockKind::Heading { level } => {
                format!(
                    "{indentation}{} {title_markdown}",
                    "#".repeat(level as usize)
                )
            }
            BlockKind::BulletedListItem => prefixed_multiline(
                &title_markdown,
                &format!("{indentation}- "),
                &format!("{indentation}  "),
            ),
            BlockKind::TaskListItem { checked } => prefixed_multiline(
                &title_markdown,
                &format!("{indentation}- [{}] ", if checked { "x" } else { " " }),
                &format!("{indentation}      "),
            ),
            BlockKind::NumberedListItem => {
                let ordinal = list_ordinal.unwrap_or(1);
                prefixed_multiline(
                    &title_markdown,
                    &format!("{indentation}{ordinal}. "),
                    &format!("{indentation}   "),
                )
            }
            BlockKind::Quote => prefixed_multiline(
                &CalloutVariant::escape_plain_quote_header(&title_markdown),
                &format!("{indentation}> "),
                &format!("{indentation}> "),
            ),
            BlockKind::Callout(variant) => format!(
                "{indentation}> {}",
                variant.header_markdown(&title_markdown)
            ),
            BlockKind::FootnoteDefinition => {
                format!("{indentation}[^{}]: ", self.title.visible_text())
            }
            BlockKind::Table => String::new(),
            BlockKind::CodeBlock { .. } => title_markdown,
            BlockKind::RawMarkdown
            | BlockKind::Comment
            | BlockKind::HtmlBlock
            | BlockKind::MathBlock
            | BlockKind::MermaidBlock => {
                if depth == 0 {
                    self.raw_fallback.clone().unwrap_or(title_markdown)
                } else {
                    indent_multiline(
                        &self.raw_fallback.clone().unwrap_or(title_markdown),
                        &indentation,
                    )
                }
            }
        }
    }

    fn title_markdown_for_output(&self) -> String {
        let visible = self.title.visible_text();
        if self.can_present_title_as_standalone_image()
            && parse_standalone_image(&visible).is_some()
        {
            return visible;
        }

        self.title_markdown()
    }

    fn can_present_title_as_standalone_image(&self) -> bool {
        matches!(
            self.kind,
            BlockKind::Paragraph
                | BlockKind::BulletedListItem
                | BlockKind::NumberedListItem
                | BlockKind::TaskListItem { .. }
        )
    }

    fn sync_raw_fallback(&mut self) {
        if self.kind_uses_raw_fallback() {
            self.raw_fallback = Some(self.title.visible_text().to_string());
            if self.kind == BlockKind::HtmlBlock {
                self.html = self
                    .raw_fallback
                    .as_ref()
                    .map(|raw| parse_html_document(raw));
            }
        } else {
            self.raw_fallback = None;
            self.html = None;
        }
    }
}

fn indent_multiline(content: &str, indentation: &str) -> String {
    content
        .split('\n')
        .map(|line| format!("{indentation}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn prefixed_multiline(content: &str, first_prefix: &str, continuation_prefix: &str) -> String {
    let mut lines = content.split('\n');
    let mut rendered = String::new();
    if let Some(first) = lines.next() {
        rendered.push_str(first_prefix);
        rendered.push_str(first);
    }

    for line in lines {
        rendered.push('\n');
        rendered.push_str(continuation_prefix);
        rendered.push_str(line);
    }

    rendered
}

/// Image payload extracted from GPUI's clipboard abstraction.
///
/// File-manager copies are usually represented as local paths, while bitmap
/// copies from image editors or browsers arrive as encoded image bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PastedImageSource {
    ClipboardImage(Image),
    LocalPath(PathBuf),
}

/// Events emitted by a block to its parent editor when structural
/// changes or focus transfers are needed that the block cannot handle alone.
///
/// The Editor subscribes to these events on every block via
/// `cx.subscribe(&block, Self::on_block_event)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BlockHostAction {
    /// Carries the authoritative host-input text so the receiver never has to
    /// re-read an entity that is still inside its GPUI update lease.
    Submit(gpui::SharedString),
    Save,
    Undo,
    Redo,
    Find,
    FindNext,
    FindPrevious,
    GoToLine,
    PageUp,
    PageDown,
    JumpToTop,
    JumpToBottom,
    DismissTransientUi,
}

#[derive(Clone, Debug)]
pub(crate) struct BlockDragPayload {
    pub(crate) source: EntityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockDropPlacement {
    Before,
    After,
}

#[derive(Debug, Clone)]
pub enum BlockEvent {
    /// Capture the current document state before an upcoming mutation.
    PrepareUndo { kind: UndoCaptureKind },
    /// The block's content or kind changed; the editor should mark the
    /// document dirty and optionally scroll to keep the block visible.
    Changed,
    /// Pointer or keyboard navigation changed the visible UTF-8 selection.
    /// Hosts with their own document model use this to translate the local
    /// block range into a stable source anchor without waiting for an edit.
    SelectionChanged,
    /// The user pressed Enter; a new block should be created after this
    /// one with the given trailing text.
    RequestNewline {
        trailing: InlineTextTree,
        source_already_mutated: bool,
    },
    /// The user pressed Enter on a callout header; the editor should ensure
    /// the callout owns a body entry and move focus into it.
    RequestEnterCalloutBody,
    /// The user requested a quote-group break at the current quote depth.
    /// The editor should insert a new empty quote group at the current depth,
    /// with whatever separator structure is required by Markdown at that level.
    RequestQuoteBreak,
    /// The user requested to exit the current callout into a plain text block.
    /// The editor should insert the separator structure needed to end the
    /// surrounding quote group, then focus a plain paragraph entry below it.
    RequestCalloutBreak,
    /// The user pressed Backspace at the start of this block; its entire
    /// content should be appended to the previous block.
    RequestMergeIntoPrev { content: InlineTextTree },
    /// A multi-line paste was detected; the editor must split the pasted
    /// lines into separate blocks and re-attach the leading/trailing text
    /// to the correct positions.
    RequestPasteMultiline {
        leading: InlineTextTree,
        lines: Vec<String>,
        trailing: InlineTextTree,
        split_physical_lines: bool,
    },
    /// An image-like clipboard payload was pasted. The editor resolves
    /// storage preferences and inserts either an image block or image text.
    RequestPasteImage {
        leading: InlineTextTree,
        source: PastedImageSource,
        trailing: InlineTextTree,
    },
    /// Execute the selected rendered-mode slash command as one editor transaction.
    RequestSlashCommand {
        command: SlashCommand,
        trigger_range: Range<usize>,
    },
    /// Execute a contextual editing command whose structural effect belongs to the editor.
    RequestEditingCommand { command: EditingCommandId },
    /// Move the dragged sibling immediately before or after this target block.
    RequestMoveBlock {
        source: EntityId,
        placement: BlockDropPlacement,
    },
    /// Replace the current editor-level cross-block selection with text
    /// submitted through the focused block input handler.
    RequestReplaceCrossBlockSelection {
        text: String,
        selected_range_relative: Option<Range<usize>>,
        mark_inserted_text: bool,
        undo_kind: UndoCaptureKind,
    },
    /// Ctrl/Cmd+A was pressed in rendered editing. The editor decides whether
    /// this press selects the focused block or upgrades to all rendered blocks.
    RequestRenderedSelectAll,
    /// Tab pressed in list context; increase the current block's nesting when
    /// the previous visible block can adopt it.
    RequestIndent,
    /// Shift-Tab pressed in list context; lift the current block out one level.
    RequestOutdent,
    /// Backspace on a nested list item should remove its marker first,
    /// degrading it into a direct list-child paragraph at the same depth.
    RequestDowngradeNestedListItemToChildParagraph,
    /// Toggle the checked state of a task-list item.
    ToggleTaskChecked,
    /// Prompt to open the clicked inline link destination.
    /// `prompt_target` preserves the raw syntax target shown to the user,
    /// while `open_target` is the resolved destination actually opened.
    RequestOpenLink {
        prompt_target: String,
        open_target: String,
    },
    /// Open the current cached Mermaid SVG in an editor-level read-only viewer.
    RequestOpenMermaidOverlay {
        preview_key: u64,
        rendered: crate::components::MermaidSvgRender,
    },
    /// Jump from a rendered footnote reference to the corresponding
    /// in-place footnote definition block.
    RequestJumpToFootnoteDefinition { id: String },
    /// Jump from an in-place footnote definition back to its first reference.
    RequestJumpToFootnoteBackref { id: String },
    /// Navigate from a rendered `[TOC]` entry to a current heading block.
    RequestJumpToTocHeading { target: EntityId },
    /// Move focus horizontally across native table cells.
    RequestTableCellMoveHorizontal { delta: i32 },
    /// Move focus vertically across native table cells.
    RequestTableCellMoveVertical { delta: i32 },
    /// Append one empty column to a native table.
    RequestAppendTableColumn,
    /// Append one empty body row to a native table.
    RequestAppendTableRow,
    /// A native table axis handle was entered or left by the pointer.
    /// `hovered` distinguishes the two so the editor can ignore a leave
    /// that arrives after an adjacent handle has already taken the preview.
    RequestTableAxisPreview {
        kind: TableAxisKind,
        index: usize,
        hovered: bool,
    },
    /// Open the axis context menu for a native table row or column.
    RequestOpenTableAxisMenu {
        kind: TableAxisKind,
        index: usize,
        position: Point<Pixels>,
    },
    /// Cursor reached the top of this block; move focus to the previous
    /// visible block, preserving the preferred horizontal position.
    RequestFocusPrev { preferred_x: Option<f32> },
    /// Cursor reached the bottom of this block; move focus to the next
    /// visible block, preserving the preferred horizontal position.
    RequestFocusNext { preferred_x: Option<f32> },
    /// Move focus to the start of the previous visible block.
    RequestBlockUp,
    /// Move focus to the start of the next visible block.
    RequestBlockDown,
    /// This block should be deleted (empty and backspace/delete pressed).
    RequestDelete,
    /// The user clicked this block; notify siblings so they re-render
    /// in display mode.
    RequestFocus,
}

/// Undo coalescing category captured before a mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoCaptureKind {
    /// Text edits that may merge with adjacent typing within the coalescing window.
    CoalescibleText,
    /// An open IME composition; updates remain one transaction regardless of typing pauses.
    ImeComposition,
    /// The final IME commit/cancel update, which seals the open composition transaction.
    ImeCompositionCommit,
    /// Structural or discrete edits that always form their own undo entry.
    NonCoalescible,
}
