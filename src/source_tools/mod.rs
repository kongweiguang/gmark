// @author kongweiguang

//! 独立源码文件共享的语言识别、结构折叠和格式化能力。

mod folding;
mod formatter_process;
mod formatting;
mod language;

pub(crate) use folding::{FoldProjectionIndex, ResidentFoldParser, discover_fold_regions};
pub(crate) use formatter_process::run_shell_formatter;
pub(crate) use formatting::{
    FormatError, FormatterResolution, format_json, format_json_lines, format_on_save_for_file,
    indent_multiline_candidate, resolve_formatter,
};
pub(crate) use language::SourceLanguageId;
