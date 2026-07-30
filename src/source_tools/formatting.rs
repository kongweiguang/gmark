// @author kongweiguang

use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::SourceLanguageId;

const DEFAULT_FORMAT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

/// 格式化失败不得产生候选正文，调用方可安全保持原 revision。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FormatError {
    InvalidJson {
        line: usize,
        column: usize,
        message: String,
    },
    InvalidJsonLine {
        record: usize,
        column: usize,
        message: String,
    },
    MissingFormatter(String),
    External(String),
    Cancelled,
    TimedOut,
    OutputTooLarge,
    InvalidUtf8,
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson {
                line,
                column,
                message,
            } => {
                write!(formatter, "JSON 第 {line} 行第 {column} 列无效：{message}")
            }
            Self::InvalidJsonLine {
                record,
                column,
                message,
            } => {
                write!(
                    formatter,
                    "JSONL 第 {record} 条记录第 {column} 列无效：{message}"
                )
            }
            Self::MissingFormatter(message) | Self::External(message) => {
                formatter.write_str(message)
            }
            Self::Cancelled => formatter.write_str("格式化已取消"),
            Self::TimedOut => formatter.write_str("格式化超时"),
            Self::OutputTooLarge => formatter.write_str("格式化输出超过安全上限"),
            Self::InvalidUtf8 => formatter.write_str("格式化器输出不是有效 UTF-8"),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalFormatterSpec {
    pub(crate) command: String,
    pub(crate) cwd: PathBuf,
    pub(crate) file: PathBuf,
    pub(crate) language: SourceLanguageId,
    pub(crate) selection: Option<Range<u64>>,
    pub(crate) timeout: Duration,
    pub(crate) max_output_bytes: usize,
    pub(crate) supports_range: bool,
    pub(crate) from_workspace: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum FormatterResolution {
    BuiltinJson,
    BuiltinJsonLines,
    External(ExternalFormatterSpec),
    Unavailable(String),
}

/// 工作区配置覆盖用户配置；两者都显式允许 Shell，因而此函数只负责确定性解析，
/// 不悄悄执行命令。真正副作用集中在 `run_shell_formatter`。
pub(crate) fn resolve_formatter(
    language: SourceLanguageId,
    file: &Path,
    selection: Option<Range<u64>>,
) -> FormatterResolution {
    let is_jsonc = file
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonc"));
    if language == SourceLanguageId::Json && !is_jsonc {
        return FormatterResolution::BuiltinJson;
    }
    if language == SourceLanguageId::JsonLines {
        return FormatterResolution::BuiltinJsonLines;
    }

    let workspace_config = nearest_workspace_config(file);
    let global_config = crate::config::GmarkConfigDirs::from_system()
        .ok()
        .map(|dirs| dirs.app_config_file());
    for (path, from_workspace) in workspace_config
        .into_iter()
        .map(|path| (path, true))
        .chain(global_config.into_iter().map(|path| (path, false)))
    {
        if let Some(spec) =
            formatter_from_config(&path, language, file, selection.clone(), from_workspace)
        {
            return FormatterResolution::External(spec);
        }
    }

    let Some(command_template) = default_formatter_command(language) else {
        return FormatterResolution::Unavailable(format!(
            "未给 {} 配置格式化器",
            language.canonical_name()
        ));
    };
    let tool = command_template
        .split_ascii_whitespace()
        .next()
        .unwrap_or_default();
    let Some(executable) = find_formatter_executable(tool, file) else {
        return FormatterResolution::Unavailable(format!(
            "未找到 {tool}；请安装到工作区或 PATH，或在 .gmark.toml/config.toml 中配置命令"
        ));
    };
    let command = command_with_executable(command_template, &executable);
    FormatterResolution::External(ExternalFormatterSpec {
        command,
        cwd: file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
        file: file.to_path_buf(),
        language,
        selection,
        timeout: DEFAULT_FORMAT_TIMEOUT,
        max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        supports_range: false,
        from_workspace: false,
    })
}

pub(crate) fn format_on_save_for_file(file: &Path, global_default: bool) -> bool {
    nearest_workspace_config(file)
        .as_deref()
        .and_then(format_on_save_from_config)
        .or_else(|| {
            crate::config::GmarkConfigDirs::from_system()
                .ok()
                .map(|dirs| dirs.app_config_file())
                .as_deref()
                .and_then(format_on_save_from_config)
        })
        .unwrap_or(global_default)
}

fn format_on_save_from_config(path: &Path) -> Option<bool> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
        .and_then(|value| value.get("formatting")?.get("format_on_save")?.as_bool())
}

fn nearest_workspace_config(file: &Path) -> Option<PathBuf> {
    let mut directory = file.parent()?;
    loop {
        let candidate = directory.join(".gmark.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        directory = directory.parent()?;
    }
}

fn formatter_from_config(
    path: &Path,
    language: SourceLanguageId,
    file: &Path,
    selection: Option<Range<u64>>,
    from_workspace: bool,
) -> Option<ExternalFormatterSpec> {
    let text = std::fs::read_to_string(path).ok()?;
    let value = toml::from_str::<toml::Value>(&text).ok()?;
    let entry = value
        .get("formatting")?
        .get("languages")?
        .get(language.canonical_name())?;
    #[cfg(target_os = "windows")]
    let platform_key = "command_windows";
    #[cfg(not(target_os = "windows"))]
    let platform_key = "command_unix";
    let command = entry
        .get(platform_key)
        .and_then(toml::Value::as_str)
        .or_else(|| entry.get("command").and_then(toml::Value::as_str))?
        .trim();
    if command.is_empty() {
        return None;
    }
    let formatting = value.get("formatting");
    let timeout_ms = entry
        .get("timeout_ms")
        .and_then(toml::Value::as_integer)
        .or_else(|| formatting?.get("timeout_ms")?.as_integer())
        .unwrap_or(DEFAULT_FORMAT_TIMEOUT.as_millis() as i64)
        .clamp(1_000, 120_000) as u64;
    let max_output_mib = entry
        .get("max_output_mib")
        .and_then(toml::Value::as_integer)
        .or_else(|| formatting?.get("max_output_mib")?.as_integer())
        .unwrap_or(64)
        .clamp(1, 256) as usize;
    Some(ExternalFormatterSpec {
        command: command.to_owned(),
        cwd: path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
        file: file.to_path_buf(),
        language,
        selection,
        timeout: Duration::from_millis(timeout_ms),
        max_output_bytes: max_output_mib * 1024 * 1024,
        supports_range: entry
            .get("supports_range")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false),
        from_workspace,
    })
}

fn default_formatter_command(language: SourceLanguageId) -> Option<&'static str> {
    #[cfg(target_os = "windows")]
    let prettier = "prettier --stdin-filepath $env:GMARK_FILE";
    #[cfg(not(target_os = "windows"))]
    let prettier = "prettier --stdin-filepath \"$GMARK_FILE\"";
    match language {
        SourceLanguageId::Rust => Some("rustfmt --emit stdout"),
        SourceLanguageId::JavaScript
        | SourceLanguageId::JavaScriptJsx
        | SourceLanguageId::TypeScript
        | SourceLanguageId::TypeScriptTsx
        | SourceLanguageId::Markdown
        | SourceLanguageId::Yaml
        | SourceLanguageId::Css
        | SourceLanguageId::Html
        | SourceLanguageId::Json => Some(prettier),
        SourceLanguageId::Go => Some("gofmt"),
        SourceLanguageId::Python => Some("black --quiet -"),
        SourceLanguageId::Toml => Some("taplo format -"),
        SourceLanguageId::C
        | SourceLanguageId::Cpp
        | SourceLanguageId::CSharp
        | SourceLanguageId::Java => Some("clang-format"),
        SourceLanguageId::Bash => Some("shfmt"),
        _ => None,
    }
}

fn find_formatter_executable(tool: &str, file: &Path) -> Option<PathBuf> {
    let workspace = workspace_root_for_file(file);
    let mut local_candidates = Vec::new();
    #[cfg(target_os = "windows")]
    {
        local_candidates.extend([
            workspace
                .join("node_modules")
                .join(".bin")
                .join(format!("{tool}.cmd")),
            workspace
                .join("node_modules")
                .join(".bin")
                .join(format!("{tool}.exe")),
            workspace
                .join(".venv")
                .join("Scripts")
                .join(format!("{tool}.exe")),
            workspace
                .join("venv")
                .join("Scripts")
                .join(format!("{tool}.exe")),
        ]);
    }
    #[cfg(not(target_os = "windows"))]
    {
        local_candidates.extend([
            workspace.join("node_modules").join(".bin").join(tool),
            workspace.join(".venv").join("bin").join(tool),
            workspace.join("venv").join("bin").join(tool),
        ]);
    }
    if let Some(path) = local_candidates.into_iter().find(|path| path.is_file()) {
        return Some(path);
    }

    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        #[cfg(target_os = "windows")]
        {
            let extensions = std::env::var_os("PATHEXT")
                .map(|value| {
                    value
                        .to_string_lossy()
                        .split(';')
                        .filter(|extension| !extension.is_empty())
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| {
                    vec![".COM".into(), ".EXE".into(), ".BAT".into(), ".CMD".into()]
                });
            for extension in extensions {
                let candidate = directory.join(format!("{tool}{extension}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let candidate = directory.join(tool);
            if candidate.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            }) {
                return Some(candidate);
            }
        }
    }
    None
}

fn workspace_root_for_file(file: &Path) -> PathBuf {
    let fallback = file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    for directory in file.parent().into_iter().flat_map(Path::ancestors) {
        if directory.join(".gmark.toml").is_file()
            || directory.join(".git").exists()
            || directory.join("Cargo.toml").is_file()
            || directory.join("package.json").is_file()
        {
            return directory.to_path_buf();
        }
    }
    fallback
}

fn command_with_executable(template: &str, executable: &Path) -> String {
    let tool_len = template.find(char::is_whitespace).unwrap_or(template.len());
    let arguments = &template[tool_len..];
    #[cfg(target_os = "windows")]
    {
        let quoted = executable.to_string_lossy().replace('\'', "''");
        format!("& '{quoted}'{arguments}")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let quoted = executable.to_string_lossy().replace('\'', "'\\''");
        format!("'{quoted}'{arguments}")
    }
}

impl std::error::Error for FormatError {}

/// 内置 JSON 格式化委托给无 UI 依赖的领域 crate；外部进程契约仍保留在本模块。
pub(crate) fn format_json(source: &str) -> Result<String, FormatError> {
    gmark_source_tools::format_json(source).map_err(domain_formatter_error)
}

/// 选区从行中部开始时，仅给候选的后续行补回源列缩进；首行仍由原 range 起点定位。
pub(crate) fn indent_multiline_candidate(candidate: String, columns: usize) -> String {
    if columns == 0 || !candidate.contains('\n') {
        return candidate;
    }
    let indent = " ".repeat(columns);
    let mut output = String::with_capacity(
        candidate
            .len()
            .saturating_add(candidate.matches('\n').count().saturating_mul(columns)),
    );
    let mut lines = candidate.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        output.push_str(line);
        if line.ends_with('\n') && lines.peek().is_some() {
            output.push_str(&indent);
        }
    }
    output
}

/// JSONL 保持一条记录一行；词法验证和渲染都委托给领域 crate。
pub(crate) fn format_json_lines(source: &str) -> Result<String, FormatError> {
    gmark_source_tools::format_json_lines(source).map_err(domain_formatter_error)
}

fn domain_formatter_error(error: gmark_source_tools::FormatterError) -> FormatError {
    match error {
        gmark_source_tools::FormatterError::InvalidJson {
            line,
            column,
            message,
        } => FormatError::InvalidJson {
            line,
            column,
            message,
        },
        gmark_source_tools::FormatterError::InvalidJsonLine {
            record,
            column,
            message,
        } => FormatError::InvalidJsonLine {
            record,
            column,
            message,
        },
        gmark_source_tools::FormatterError::Unavailable { language } => {
            FormatError::MissingFormatter(format!(
                "未给 {} 配置格式化器",
                language.canonical_name()
            ))
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/source_tools/formatting.rs"]
mod tests;
