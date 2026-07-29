// @author kongweiguang

//! GPUI 无关的语言识别、折叠、格式化与语义代码高亮。

#![forbid(unsafe_code)]

mod folding;
mod folding_structural;
mod formatting;
mod highlight;
#[cfg(feature = "code-highlight-core")]
mod highlight_configs;
mod incremental;
mod language;
mod range;

pub use folding::{FoldKind, FoldRange, fold_ranges, fold_ranges_in_window};
pub use formatting::{FormatResult, FormatterError, format_json, format_json_lines, format_source};
pub use highlight::{
    HighlightEngine, HighlightResult, HighlightSpan, TokenClass, highlight_fenced_code,
    highlight_source,
};
pub use incremental::IncrementalFoldParser;
pub use language::{
    FENCE_LANGUAGE_MENU_ITEMS, SourceLanguage, detect_language, resolve_fence_language,
};
pub use range::{ByteRange, ByteRangeError};

/// 与旧源码工具层一致的语言标识名称，方便 Wave 2 适配器平滑迁移。
pub type SourceLanguageId = SourceLanguage;
