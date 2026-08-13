// @author kongweiguang

//! Host-local constants and format-neutral contracts.

use super::*;

pub(super) const PREFIX_PREVIEW_BYTES: u64 = 256 * 1024;
pub(super) const DOCUMENT_HOST_KEY_CONTEXT: &str = "BlockEditor";
pub(super) const MAX_RENDERED_LINE_BYTES: u64 = 64 * 1024;
pub(super) const SOURCE_SCROLL_BYTES_PER_PIXEL: f32 = 32.0;
pub(super) const FALLBACK_SOURCE_ROW_HEIGHT: f32 = 25.6;
pub(super) const SOURCE_OVERSCAN_ROWS: usize = 96;

/// GPUI/DirectWrite on Windows does not resolve the CSS-style generic `monospace` family.
/// Use a platform font that is part of the base OS so a fresh profile cannot panic on first paint.
pub(crate) fn source_monospace_font_family() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "Consolas"
    }
    #[cfg(target_os = "macos")]
    {
        "Menlo"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        "monospace"
    }
}

/// Source-backed documents own the same reading rhythm as the regular source
/// surface without importing editor layout helpers.
pub(super) fn source_surface_padding(dimensions: &crate::theme::ThemeDimensions) -> (f32, f32) {
    (dimensions.editor_padding * 0.5, dimensions.editor_padding)
}

// GPUI 的滚动坐标是 f32；把数千万行直接乘行高会在文件尾产生 32–128px
// 量化，最终表现为行号重叠和跳行。uniform_list 永远只承载一个局部滑窗，
// 全局位置由 source_list_origin 和 SourceAnchor 保存。
pub(crate) const SOURCE_LIST_WINDOW_ROWS: usize = 65_536;
// 单行窗口最多 64 KiB；512 行同时给 row/entity/shaped-line 缓存提供 32 MiB
// 的硬上界，且低于契约允许的 2,048 行上限。
pub(crate) const MAX_SOURCE_CACHED_ROWS: usize = 512;
pub(super) const STRUCTURED_OVERSCAN_ROWS: usize = 64;
pub(super) const STRUCTURED_CELL_BYTES: usize = 8 * 1024;
pub(super) const STRUCTURED_CELL_WIDTH: f32 = 220.0;
pub(super) const STRUCTURED_COLUMN_WINDOW: usize = 16;
pub(super) const FIND_CASE_ICON: &str = "icon/ui/case-sensitive.svg";
pub(super) const FIND_WORD_ICON: &str = "icon/ui/whole-word.svg";
pub(super) const FIND_REGEX_ICON: &str = "icon/ui/regex.svg";
pub(super) const CHEVRON_UP_ICON: &str = "icon/ui/chevron-up.svg";
pub(super) const CHEVRON_DOWN_ICON: &str = "icon/ui/chevron-down.svg";
pub(super) const CLOSE_ICON: &str = "icon/ui/close.svg";

pub(super) fn localized_document_error(error: &PagedDocumentError, cx: &App) -> SharedString {
    cx.global::<I18nManager>()
        .strings()
        .large_document_error(error)
        .into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocumentHostViewMode {
    Live,
    Source,
    Structure,
    Split,
}

#[derive(Clone, Copy)]
pub(super) enum SourceContextCommand {
    Copy,
    Cut,
    Paste,
    SelectAll,
    ExportSelection,
    ExportSelectionUtf8,
    FormatDocument,
    FormatSelection,
}

#[derive(Clone, Debug)]
pub(crate) enum DocumentHostEvent {
    SavedAs(PathBuf),
    StateChanged,
    ViewModeChanged(DocumentHostMode),
    SplitRatioChanged(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocumentHostMode {
    Live,
    Source,
    Preview,
    Split,
}

/// DocumentHost 与 Editor 菜单共享的中立格式契约；宿主不能反向依赖 Editor 类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocumentMenuFormat {
    Markdown,
    Json,
    JsonLines,
    Csv,
    Tsv,
    Text,
}

impl DocumentMenuFormat {
    pub(crate) fn from_document_format(format: &DocumentFormat) -> Self {
        match format {
            DocumentFormat::Markdown => Self::Markdown,
            DocumentFormat::Json => Self::Json,
            DocumentFormat::JsonLines => Self::JsonLines,
            DocumentFormat::Delimited { delimiter: b'\t' } => Self::Tsv,
            DocumentFormat::Delimited { .. } => Self::Csv,
            DocumentFormat::PlainText => Self::Text,
        }
    }

    pub(crate) fn label(self, chinese: bool) -> &'static str {
        match (self, chinese) {
            (Self::Markdown, _) => "Markdown",
            (Self::Json, _) => "JSON",
            (Self::JsonLines, _) => "JSONL",
            (Self::Csv, _) => "CSV",
            (Self::Tsv, _) => "TSV",
            (Self::Text, true) => "文本",
            (Self::Text, false) => "Text",
        }
    }
}

#[derive(Clone)]
pub(super) enum StructuredIndex {
    Delimited(DelimitedIndex),
    MarkdownTables {
        tables: Vec<MarkdownTableIndex>,
        selected: usize,
    },
    Json {
        index: JsonIndex,
        source: FileSource,
    },
    JsonLines {
        lines: StructuredLines,
        source: StructuredTextSource,
        record_count: u64,
    },
}

#[derive(Clone)]
pub(super) enum StructuredLines {
    File(LineIndex),
    Snapshot(Arc<[Range<u64>]>),
}

impl StructuredLines {
    pub(super) fn line_range(&self, line: u64) -> Option<Range<u64>> {
        match self {
            Self::File(lines) => lines.line_range(line),
            Self::Snapshot(lines) => lines.get(usize::try_from(line).ok()?).cloned(),
        }
    }

    pub(super) fn line_count(&self) -> u64 {
        match self {
            Self::File(lines) => lines.line_count(),
            Self::Snapshot(lines) => lines.len() as u64,
        }
    }
}

#[derive(Clone)]
pub(super) enum StructuredTextSource {
    File(FileSource),
    Snapshot(Arc<[u8]>),
}

impl StructuredTextSource {
    pub(super) fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>, PagedDocumentError> {
        match self {
            Self::File(source) => source.read_range(range.start, range.end),
            Self::Snapshot(bytes) => {
                let len = bytes.len() as u64;
                if range.start > range.end || range.end > len {
                    return Err(PagedDocumentError::InvalidRange {
                        start: range.start,
                        end: range.end,
                        len,
                    });
                }
                let start =
                    usize::try_from(range.start).map_err(|_| PagedDocumentError::RangeTooLarge)?;
                let end =
                    usize::try_from(range.end).map_err(|_| PagedDocumentError::RangeTooLarge)?;
                Ok(bytes[start..end].to_vec())
            }
        }
    }
}
