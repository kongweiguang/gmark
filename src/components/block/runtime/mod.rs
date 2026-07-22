// @author kongweiguang

//! Editable block runtime and block-local state transitions.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::*;
use unicode_segmentation::*;

mod auto_pair;
mod code;
mod image;
mod projection;
mod table;

use self::projection::{
    ExpandedInlineProjection, ExpandedInlineSegment, ExpandedInlineSegmentKind, ExpandedLinkRun,
    ProjectedLinkSelectionSnapshot,
};
use super::element;
use super::{
    BlockEvent, BlockHostAction, BlockKind, BlockRecord, CalloutVariant, EditingCommandId,
    FootnoteRegistry, SlashMenuState, UndoCaptureKind,
};
use super::{CodeHighlightResult, highlight_code_block};
use super::{
    ImageReferenceDefinitions, ImageResolvedSource, ImageSyntax, LinkReferenceDefinitions,
    parse_standalone_image, resolve_image_source, rewrite_standalone_image_width,
};
#[cfg(test)]
use crate::components::InlineFootnoteHit;
use crate::components::markdown::inline::{
    InlineFragment, InlineInsertionAttributes, InlineRenderCache, InlineSpan, InlineTextTree,
    StyleFlag,
};
#[cfg(test)]
use crate::components::markdown::inline::{InlineLinkHit, InlineStyle};
use crate::components::{
    TableAxisHighlight, TableAxisMarker, TableCellPosition, TableColumnAlignment, TableRuntime,
};

/// Inline formatting command issued by editor actions.
#[derive(Clone, Copy)]
pub(crate) enum InlineFormat {
    /// Toggle bold formatting.
    Bold,
    /// Toggle italic formatting.
    Italic,
    /// Toggle strikethrough formatting.
    Strikethrough,
    /// Toggle underline formatting.
    Underline,
    /// Toggle highlight formatting.
    Highlight,
    /// Toggle superscript formatting.
    Superscript,
    /// Toggle subscript formatting.
    Subscript,
    /// Toggle inline code formatting.
    Code,
}

/// Editing semantics for the current block.
///
/// Rich blocks edit the attribute-based text tree, while source mode and code
/// blocks edit raw text without inline Markdown normalization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditMode {
    /// Attribute-based rich text editing for normal rendered blocks.
    RenderedRich,
    /// Raw Markdown editing for source-mode and raw fallback blocks.
    SourceRaw,
    /// Raw text editing for fenced code block contents.
    CodeBlockRaw,
}

pub(crate) type HostActionHandler = Rc<dyn Fn(BlockHostAction, &mut Window, &mut App)>;

/// 大文件 Source 行的稳定布局身份。动态字体、主题、缩放与换行宽度在实际 shape
/// 时补入缓存键；这里保留文档侧不变量，避免 revision 重置或横向窗口变化时复用旧行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceLayoutIdentity {
    pub(crate) document_epoch: u64,
    pub(crate) document_revision: u64,
    pub(crate) source_range: Range<u64>,
    pub(crate) column_window_start: u64,
    pub(crate) show_line_endings: bool,
}

/// 单个已挂载 Source 行只保留最近一次 shaped layout。整个 Source surface 最多挂载
/// 512 行，因此该缓存天然受 512 行 / 32 MiB 的更严格上限约束。
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SourceLayoutCacheKey {
    pub(crate) identity: SourceLayoutIdentity,
    pub(crate) text: SharedString,
    pub(crate) marked_range: Option<Range<usize>>,
    pub(crate) theme_identity: usize,
    pub(crate) font: Font,
    pub(crate) font_size_bits: u32,
    pub(crate) line_height_bits: u32,
    pub(crate) scale_bits: u32,
    pub(crate) wrap_width_bits: Option<u32>,
    pub(crate) soft_wrap: bool,
}

impl EditMode {
    fn for_kind(kind: &BlockKind) -> Self {
        if kind.is_code_block() {
            Self::CodeBlockRaw
        } else if matches!(
            kind,
            BlockKind::RawMarkdown
                | BlockKind::Comment
                | BlockKind::HtmlBlock
                | BlockKind::MathBlock
                | BlockKind::MermaidBlock
        ) {
            Self::SourceRaw
        } else {
            Self::RenderedRich
        }
    }

    fn uses_raw_text_editing(self) -> bool {
        matches!(self, Self::SourceRaw | Self::CodeBlockRaw)
    }

    fn supports_inline_projection(self) -> bool {
        matches!(self, Self::RenderedRich)
    }
}

#[path = "implementation/editing.rs"]
mod editing;
#[path = "implementation/inline_projection.rs"]
mod inline_projection;
#[path = "implementation/range_mapping.rs"]
mod range_mapping;

impl EventEmitter<BlockEvent> for Block {}

/// A single editable block in the document tree.
///
/// Each block holds a [`BlockRecord`] containing the persistent data (kind,
/// title, UUIDs) and a [`FocusHandle`] for keyboard routing.  Runtime state
/// such as selection, cursor blink, and layout cache live on the struct.
///
/// Blocks delegate structural operations (split, merge, indent, delete) to
/// the parent editor via `BlockEvent` emissions.
pub struct Block {
    pub record: BlockRecord,
    pub(crate) render_cache: InlineRenderCache,
    code_highlight: Option<CodeHighlightResult>,
    source_syntax_language: Option<SharedString>,
    pub(crate) last_successful_math_render: Option<crate::components::LatexSvgRender>,
    pub(crate) last_successful_mermaid_render: Option<crate::components::MermaidSvgRender>,
    pub(crate) math_render_error: Option<String>,
    pub(crate) mermaid_render_error: Option<String>,
    pub(crate) math_preview_key: Option<u64>,
    pub(crate) mermaid_preview_key: Option<u64>,
    pub(crate) math_preview_task: Option<Task<()>>,
    pub(crate) mermaid_preview_task: Option<Task<()>>,
    pub children: Vec<Entity<Block>>,
    pub focus_handle: FocusHandle,
    pub(crate) code_language_focus_handle: FocusHandle,
    pub(crate) code_language_selected_range: Range<usize>,
    pub(crate) code_language_selection_reversed: bool,
    pub(crate) code_language_marked_range: Option<Range<usize>>,
    pub(crate) code_language_last_layout: Option<ShapedLine>,
    pub(crate) code_language_last_bounds: Option<Bounds<Pixels>>,
    pub(crate) code_language_is_selecting: bool,
    pub(crate) code_language_menu_open: bool,
    pub(crate) code_language_menu_selected: usize,
    pub(crate) code_copy_feedback: bool,
    pub(crate) code_copy_feedback_task: Option<Task<()>>,
    pub selected_range: Range<usize>,
    pub selection_reversed: bool,
    pub(crate) editor_selection_range: Option<Range<usize>>,
    pub(crate) editor_selection_supports_inline_commands: bool,
    pub marked_range: Option<Range<usize>>,
    pub(crate) spelling_diagnostics: Arc<[crate::spellcheck::SpellingDiagnostic]>,
    pub last_layout: Option<Vec<WrappedLine>>,
    pub(crate) source_layout_identity: Option<SourceLayoutIdentity>,
    pub(crate) source_layout_cache_key: Option<SourceLayoutCacheKey>,
    pub(crate) source_layout_cache_hits: u64,
    pub(crate) source_layout_cache_misses: u64,
    pub last_bounds: Option<Bounds<Pixels>>,
    pub last_line_height: Pixels,
    pub render_depth: usize,
    /// Position inside the direct sibling list. This is projection metadata,
    /// refreshed by `DocumentTree` after every structural mutation.
    pub(crate) structural_sibling_index: usize,
    pub(crate) structural_sibling_count: usize,
    pub(crate) structural_context_revision: u64,
    pub quote_depth: usize,
    pub(crate) quote_group_anchor: Option<uuid::Uuid>,
    pub(crate) visible_quote_depth: usize,
    pub(crate) visible_quote_group_anchor: Option<uuid::Uuid>,
    pub(crate) callout_depth: usize,
    pub(crate) callout_anchor: Option<uuid::Uuid>,
    pub(crate) callout_variant: Option<CalloutVariant>,
    pub(crate) footnote_anchor: Option<uuid::Uuid>,
    pub(crate) parent_is_list_item: bool,
    pub list_ordinal: Option<usize>,
    pub is_selecting: bool,
    pub cursor_blink_epoch: Instant,
    pub vertical_motion_x: Option<Pixels>,
    pub(super) cursor_blink_task: Option<Task<()>>,
    /// Cached projection used to show editable inline delimiters for the
    /// currently touched inline span(s).
    pub(crate) projection: Option<ExpandedInlineProjection>,
    /// Inputs that produced the current `projection`. When the next
    /// `sync_inline_projection_for_focus` computes the same inputs, the
    /// rebuild is skipped — saves a full O(fragments + text) walk per
    /// render frame (cursor blink + every arrow keypress).
    projection_cache_key: Option<(bool, Range<usize>, Option<Range<usize>>)>,
    /// Display text held as a SharedString so renders can clone an Arc
    /// instead of re-allocating per frame. Refreshed in `sync_render_cache`,
    /// `rebuild_inline_projection`, and `clear_inline_projection`.
    cached_display_text: SharedString,
    input_placeholder: Option<SharedString>,
    host_action_handler: Option<HostActionHandler>,
    host_submit_enabled: bool,
    collapsed_caret_affinity: CollapsedCaretAffinity,
    /// When true, block-level shortcuts and inline formatting are
    /// suppressed; the block stores raw text for source-mode editing.
    pub(crate) edit_mode: EditMode,
    /// 只读投影禁止输入、选择和结构编辑，但仍允许链接与脚注导航。
    read_only: bool,
    show_source_line_numbers: bool,
    /// 大文件 viewport 的活动行由宿主统一提供行高、行号槽与选择背景；Block 只负责
    /// 普通 Source 输入与光标，不能在聚焦时再引入第二套 padding/min-height/chrome。
    compact_source_host: bool,
    /// 紧凑宿主可覆盖源码输入字号，使内联编辑器与宿主文本在聚焦前后保持同一尺寸。
    host_text_size: Option<f32>,
    pub(crate) table_runtime: Option<TableRuntime>,
    pub(crate) table_cell_position: Option<TableCellPosition>,
    pub(crate) table_cell_alignment: Option<TableColumnAlignment>,
    pub(crate) table_axis_preview: Option<TableAxisMarker>,
    pub(crate) table_axis_selection: Option<TableAxisMarker>,
    pub(crate) table_axis_highlight: TableAxisHighlight,
    pub(crate) table_append_column_edge_hovered: bool,
    pub(crate) table_append_column_hovered: bool,
    pub(crate) table_append_column_zone_hovered: bool,
    pub(crate) table_append_column_button_hovered: bool,
    pub(crate) table_append_column_close_task: Option<Task<()>>,
    pub(crate) table_append_row_edge_hovered: bool,
    pub(crate) table_append_row_hovered: bool,
    pub(crate) table_append_row_zone_hovered: bool,
    pub(crate) table_append_row_button_hovered: bool,
    pub(crate) table_append_row_close_task: Option<Task<()>>,
    image_runtime: Option<ImageRuntime>,
    image_edit_expanded: bool,
    image_expand_requested: bool,
    pub(crate) image_selected: bool,
    pub(crate) image_resize_session: Option<ImageResizeSession>,
    pub(crate) image_preview_width_percent: Option<u8>,
    pub(crate) html_details_open: bool,
    image_base_dir: Option<PathBuf>,
    image_reference_definitions: Arc<ImageReferenceDefinitions>,
    link_reference_definitions: Arc<LinkReferenceDefinitions>,
    footnote_registry: Arc<FootnoteRegistry>,
    /// 文档目录是可重建投影；不属于 Markdown 源码或 undo 状态。
    pub(crate) toc_entries: Arc<[TocRuntimeEntry]>,
    pub(crate) list_group_separator_candidate: bool,
    numbered_list_restart_requested: bool,
    quote_reparse_requested: bool,
    pub(crate) slash_menu: Option<SlashMenuState>,
    pub(crate) slash_menu_dismissed_query: Option<String>,
    pub(crate) slash_menu_scroll_handle: ScrollHandle,
    pub(crate) block_drop_placement: super::BlockDropPlacement,
    pub(crate) selection_toolbar_dismissed_range: Option<Range<usize>>,
    pub(crate) selection_toolbar_keyboard_active: bool,
    pub(crate) selection_toolbar_keyboard_index: usize,
    pub(crate) selection_toolbar_overflow_open: bool,
    pub(crate) selection_toolbar_type_menu_open: bool,
    pub(crate) selection_toolbar_link_input: Option<Entity<Block>>,
    pub(crate) selection_toolbar_link_range: Option<Range<usize>>,
    pub(crate) selection_toolbar_link_had_target: bool,
}

/// A clickable heading entry resolved by the editor for one rendered `[TOC]` block.
#[derive(Clone, Debug)]
pub(crate) struct TocRuntimeEntry {
    pub(crate) level: u8,
    pub(crate) title: SharedString,
    pub(crate) target: EntityId,
}

/// Cached standalone image presentation state for a block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImageRuntime {
    pub(crate) alt: String,
    pub(crate) src: String,
    pub(crate) title: Option<String>,
    pub(crate) width_percent: u8,
    pub(crate) resolved_source: ImageResolvedSource,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ImageResizeSession {
    pub(crate) start_x: Pixels,
    pub(crate) start_percent: u8,
    pub(crate) available_width: f32,
}

/// How a collapsed caret at an inline projection boundary inherits style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CollapsedCaretAffinity {
    /// Use the normal insertion-attribute lookup.
    #[default]
    Default,
    /// Treat the caret as being just outside the opening delimiter.
    OuterStart,
    /// Treat the caret as being just outside the closing delimiter.
    OuterEnd,
}

impl Block {
    pub fn with_record(cx: &mut Context<Self>, record: BlockRecord) -> Self {
        let edit_mode = EditMode::for_kind(&record.kind);
        let render_cache = record.title.render_cache();
        let mut block = Self {
            record,
            render_cache,
            code_highlight: None,
            source_syntax_language: None,
            last_successful_math_render: None,
            last_successful_mermaid_render: None,
            math_render_error: None,
            mermaid_render_error: None,
            math_preview_key: None,
            mermaid_preview_key: None,
            math_preview_task: None,
            mermaid_preview_task: None,
            children: Vec::new(),
            focus_handle: cx.focus_handle(),
            code_language_focus_handle: cx.focus_handle(),
            code_language_selected_range: 0..0,
            code_language_selection_reversed: false,
            code_language_marked_range: None,
            code_language_last_layout: None,
            code_language_last_bounds: None,
            code_language_is_selecting: false,
            code_language_menu_open: false,
            code_language_menu_selected: 0,
            code_copy_feedback: false,
            code_copy_feedback_task: None,
            selected_range: 0..0,
            selection_reversed: false,
            editor_selection_range: None,
            editor_selection_supports_inline_commands: false,
            marked_range: None,
            spelling_diagnostics: Arc::default(),
            last_layout: None,
            source_layout_identity: None,
            source_layout_cache_key: None,
            source_layout_cache_hits: 0,
            source_layout_cache_misses: 0,
            last_bounds: None,
            last_line_height: px(20.0),
            render_depth: 0,
            structural_sibling_index: 0,
            structural_sibling_count: 1,
            structural_context_revision: 0,
            quote_depth: 0,
            quote_group_anchor: None,
            visible_quote_depth: 0,
            visible_quote_group_anchor: None,
            callout_depth: 0,
            callout_anchor: None,
            callout_variant: None,
            footnote_anchor: None,
            parent_is_list_item: false,
            list_ordinal: None,
            is_selecting: false,
            cursor_blink_epoch: Instant::now(),
            vertical_motion_x: None,
            cursor_blink_task: None,
            projection: None,
            projection_cache_key: None,
            cached_display_text: SharedString::default(),
            input_placeholder: None,
            host_action_handler: None,
            host_submit_enabled: false,
            collapsed_caret_affinity: CollapsedCaretAffinity::Default,
            edit_mode,
            read_only: false,
            show_source_line_numbers: false,
            compact_source_host: false,
            host_text_size: None,
            table_runtime: None,
            table_cell_position: None,
            table_cell_alignment: None,
            table_axis_preview: None,
            table_axis_selection: None,
            table_axis_highlight: TableAxisHighlight::None,
            table_append_column_edge_hovered: false,
            table_append_column_hovered: false,
            table_append_column_zone_hovered: false,
            table_append_column_button_hovered: false,
            table_append_column_close_task: None,
            table_append_row_edge_hovered: false,
            table_append_row_hovered: false,
            table_append_row_zone_hovered: false,
            table_append_row_button_hovered: false,
            table_append_row_close_task: None,
            image_runtime: None,
            image_edit_expanded: false,
            image_expand_requested: false,
            image_selected: false,
            image_resize_session: None,
            image_preview_width_percent: None,
            html_details_open: false,
            image_base_dir: None,
            image_reference_definitions: Arc::default(),
            link_reference_definitions: Arc::default(),
            footnote_registry: Arc::default(),
            toc_entries: Arc::default(),
            list_group_separator_candidate: false,
            numbered_list_restart_requested: false,
            quote_reparse_requested: false,
            slash_menu: None,
            slash_menu_dismissed_query: None,
            slash_menu_scroll_handle: ScrollHandle::new(),
            block_drop_placement: super::BlockDropPlacement::Before,
            selection_toolbar_dismissed_range: None,
            selection_toolbar_keyboard_active: false,
            selection_toolbar_keyboard_index: 0,
            selection_toolbar_overflow_open: false,
            selection_toolbar_type_menu_open: false,
            selection_toolbar_link_input: None,
            selection_toolbar_link_range: None,
            selection_toolbar_link_had_target: false,
        };
        block.sync_code_highlight();
        block.refresh_cached_display_text();
        block
    }

    pub fn kind(&self) -> BlockKind {
        self.record.kind.clone()
    }

    pub(crate) fn is_source_raw_mode(&self) -> bool {
        self.edit_mode == EditMode::SourceRaw
    }

    pub(crate) fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub(crate) fn set_read_only(&mut self, read_only: bool) {
        if self.read_only == read_only {
            return;
        }
        self.read_only = read_only;
        self.is_selecting = false;
        self.code_language_is_selecting = false;
        self.marked_range = None;
        self.code_language_marked_range = None;
        if read_only {
            self.clear_inline_projection();
            self.editor_selection_range = None;
            self.editor_selection_supports_inline_commands = false;
        }
    }

    pub(crate) fn show_source_line_numbers(&self) -> bool {
        self.show_source_line_numbers
    }

    pub(crate) fn take_quote_reparse_requested(&mut self) -> bool {
        let requested = self.quote_reparse_requested;
        self.quote_reparse_requested = false;
        requested
    }

    pub(crate) fn take_numbered_list_restart_requested(&mut self) -> bool {
        let requested = self.numbered_list_restart_requested;
        self.numbered_list_restart_requested = false;
        requested
    }

    pub(crate) fn set_runtime_context(
        &mut self,
        base_dir: Option<PathBuf>,
        image_reference_definitions: Arc<ImageReferenceDefinitions>,
        link_reference_definitions: Arc<LinkReferenceDefinitions>,
        footnote_registry: Arc<FootnoteRegistry>,
    ) {
        if self.image_base_dir != base_dir {
            self.image_base_dir = base_dir;
        }
        if self.image_reference_definitions != image_reference_definitions {
            self.image_reference_definitions = image_reference_definitions;
        }
        self.sync_link_reference_definitions(link_reference_definitions);
        self.sync_footnote_registry(footnote_registry);
        self.sync_image_runtime();
    }

    pub(crate) fn uses_raw_text_editing(&self) -> bool {
        self.edit_mode.uses_raw_text_editing()
    }

    pub(crate) fn set_source_raw_mode(&mut self) {
        self.clear_inline_projection();
        self.edit_mode = EditMode::SourceRaw;
        self.show_source_line_numbers = false;
        self.compact_source_host = false;
    }

    pub(crate) fn set_compact_source_host(&mut self) {
        self.set_source_raw_mode();
        self.compact_source_host = true;
    }

    pub(crate) fn set_host_text_size(&mut self, text_size: f32) {
        self.host_text_size = Some(text_size.max(1.0));
    }

    pub(crate) fn host_text_size(&self) -> Option<f32> {
        self.host_text_size
    }

    pub(crate) fn set_source_layout_identity(&mut self, identity: SourceLayoutIdentity) {
        if self.source_layout_identity.as_ref() == Some(&identity) {
            return;
        }
        self.source_layout_identity = Some(identity);
        self.source_layout_cache_key = None;
    }

    pub(crate) fn compact_source_host(&self) -> bool {
        self.compact_source_host
    }

    pub(crate) fn set_source_document_mode(&mut self) {
        self.set_source_raw_mode();
        self.show_source_line_numbers = true;
        // Resident 文档是 Markdown 真值；Source 视图必须显式启用语法语言，
        // 否则 BlockTextElement 只会生成单色 TextRun。
        self.set_source_syntax_language(Some("markdown"));
    }

    pub(crate) fn sync_edit_mode_from_kind(&mut self) {
        if self.table_cell_position.is_some() {
            self.edit_mode = EditMode::RenderedRich;
            self.show_source_line_numbers = false;
            return;
        }
        if self.edit_mode != EditMode::SourceRaw {
            if self.kind().is_code_block() {
                self.clear_inline_projection();
            }
            self.edit_mode = EditMode::for_kind(&self.record.kind);
            self.show_source_line_numbers = false;
        }
    }

    pub fn display_text(&self) -> &str {
        self.current_cache().visible_text()
    }

    /// 返回最接近窗口纵坐标的源码字节偏移，用于 Split 滚动锚点。
    pub(crate) fn text_offset_for_window_y(&self, window_y: Pixels) -> Option<usize> {
        let bounds = self.last_bounds?;
        let lines = self.last_layout.as_ref()?;
        let relative_y = (window_y - bounds.top()).max(px(0.0));
        let (line_index, _) =
            super::element::wrapped_line_for_y(lines, self.last_line_height, relative_y)?;
        super::element::hard_line_ranges(self.display_text())
            .get(line_index)
            .map(|range| range.start)
    }

    /// 返回源码字节偏移所在硬行的窗口纵坐标。
    pub(crate) fn window_y_for_text_offset(&self, offset: usize) -> Option<Pixels> {
        let bounds = self.last_bounds?;
        let lines = self.last_layout.as_ref()?;
        let ranges = super::element::hard_line_ranges(self.display_text());
        let (line_index, _) = super::element::line_index_for_offset(&ranges, offset);
        Some(
            bounds.top()
                + super::element::wrapped_line_top(lines, self.last_line_height, line_index),
        )
    }

    /// Cheap clone of the current display text as a `SharedString` (Arc bump)
    /// — avoids a fresh String allocation per render. The cached value is
    /// refreshed by [`Self::refresh_cached_display_text`] whenever the
    /// underlying text might have changed.
    pub(crate) fn shared_display_text(&self) -> SharedString {
        self.cached_display_text.clone()
    }

    pub(crate) fn set_input_placeholder(&mut self, placeholder: impl Into<SharedString>) {
        self.input_placeholder = Some(placeholder.into());
    }

    /// 大文件活动行仍是普通 Block 输入面，但窗口级命令必须回到宿主文档处理，
    /// 否则聚焦行编辑后 Ctrl+F/Ctrl+G/保存/历史与翻页都会失去路由。
    pub(crate) fn set_host_action_handler(
        &mut self,
        handler: impl Fn(BlockHostAction, &mut Window, &mut App) + 'static,
    ) {
        self.host_action_handler = Some(Rc::new(handler));
    }

    pub(crate) fn has_host_action_handler(&self) -> bool {
        self.host_action_handler.is_some()
    }

    pub(crate) fn set_host_submit_enabled(&mut self, enabled: bool) {
        self.host_submit_enabled = enabled;
    }

    pub(crate) fn host_submit_enabled(&self) -> bool {
        self.host_submit_enabled
    }

    pub(crate) fn host_action_handler(&self) -> Option<HostActionHandler> {
        self.host_action_handler.clone()
    }

    pub(crate) fn input_placeholder(&self) -> Option<SharedString> {
        self.input_placeholder.clone()
    }

    fn refresh_cached_display_text(&mut self) {
        let current = self.current_cache().visible_text();
        if self.cached_display_text.as_ref() != current {
            self.cached_display_text = SharedString::from(current.to_string());
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/components/block/runtime/scenarios.rs"]
mod tests;
