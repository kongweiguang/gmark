// @author kongweiguang

use std::fmt;
use std::io::{Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use gmark_paged_document::SearchCancellation;

use super::SourceLanguageId;

const DEFAULT_FORMAT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 1024 * 1024;

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

pub(crate) fn run_shell_formatter(
    spec: &ExternalFormatterSpec,
    input: &[u8],
    cancellation: &SearchCancellation,
) -> Result<String, FormatError> {
    if spec.selection.is_some() && !spec.supports_range {
        return Err(FormatError::MissingFormatter(
            "当前格式化器不支持选区格式化".to_owned(),
        ));
    }
    #[cfg(target_os = "windows")]
    let mut command = {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", &spec.command]);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    };
    #[cfg(not(target_os = "windows"))]
    let mut command = {
        use std::os::unix::process::CommandExt as _;
        let mut command = Command::new("/bin/sh");
        command.args(["-lc", &spec.command]);
        // 独立进程组让取消/超时能够终止 Shell 及其所有后代，而不波及 GMark。
        command.process_group(0);
        command
    };
    command
        .current_dir(&spec.cwd)
        .env("GMARK_FILE", &spec.file)
        .env("GMARK_LANGUAGE", spec.language.canonical_name())
        .env(
            "GMARK_RANGE_START",
            spec.selection
                .as_ref()
                .map_or(0, |range| range.start)
                .to_string(),
        )
        .env(
            "GMARK_RANGE_END",
            spec.selection
                .as_ref()
                .map_or(0, |range| range.end)
                .to_string(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| FormatError::External(format!("无法启动格式化器：{error}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| FormatError::External("格式化器 stdin 不可用".to_owned()))?;
    let input = input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| FormatError::External("格式化器 stdout 不可用".to_owned()))?;
    let output_limit = spec.max_output_bytes;
    let output_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .by_ref()
            .take(output_limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| FormatError::External("格式化器 stderr 不可用".to_owned()))?;
    let error_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr
            .by_ref()
            .take(MAX_STDERR_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let started = Instant::now();
    let status = loop {
        if cancellation.is_cancelled() {
            terminate_formatter(&mut child);
            break Err(FormatError::Cancelled);
        }
        if started.elapsed() >= spec.timeout {
            terminate_formatter(&mut child);
            break Err(FormatError::TimedOut);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                terminate_formatter(&mut child);
                break Err(FormatError::External(format!("等待格式化器失败：{error}")));
            }
        }
    };
    // 无论成功、失败、取消或超时，都等待三个管道线程退出，保证没有后台线程继续
    // 持有文档输入或子进程句柄。
    let _ = writer.join();
    let output = output_reader
        .join()
        .map_err(|_| FormatError::External("读取格式化输出失败".to_owned()))?
        .map_err(|error| FormatError::External(error.to_string()))?;
    let stderr = error_reader
        .join()
        .map_err(|_| FormatError::External("读取格式化错误失败".to_owned()))?
        .map_err(|error| FormatError::External(error.to_string()))?;
    let status = status?;
    if output.len() > spec.max_output_bytes {
        return Err(FormatError::OutputTooLarge);
    }
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr);
        return Err(FormatError::External(format!(
            "格式化器退出状态 {status}：{}",
            detail.trim()
        )));
    }
    String::from_utf8(output).map_err(|_| FormatError::InvalidUtf8)
}

fn terminate_formatter(child: &mut std::process::Child) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 子进程以自己的 process group 启动；负 PID 表示向完整进程组发送 TERM。
        let _ = Command::new("kill")
            .args(["-TERM", "--", &format!("-{}", child.id())])
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

impl std::error::Error for FormatError {}

/// 只重写 JSON token 之间的空白，因此不会重排 key 或规范化数字、转义词法。
pub(crate) fn format_json(source: &str) -> Result<String, FormatError> {
    if let Err(error) = serde_json::from_str::<serde_json::Value>(source) {
        return Err(FormatError::InvalidJson {
            line: error.line(),
            column: error.column(),
            message: error.to_string(),
        });
    }
    Ok(format_json_tokens(source, false))
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

/// JSONL 保持一条记录一行，仅移除记录内部 token 之间的无意义空白。
pub(crate) fn format_json_lines(source: &str) -> Result<String, FormatError> {
    let trailing_newline = source.ends_with('\n');
    let mut output = String::new();
    for (index, line) in source.lines().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        if line.trim().is_empty() {
            continue;
        }
        if let Err(error) = serde_json::from_str::<serde_json::Value>(line) {
            return Err(FormatError::InvalidJsonLine {
                record: index + 1,
                column: error.column(),
                message: error.to_string(),
            });
        }
        output.push_str(&format_json_tokens(line, true));
    }
    if trailing_newline && !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

fn format_json_tokens(source: &str, compact: bool) -> String {
    let trailing_newline = source.ends_with('\n');
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len().saturating_add(source.len() / 8));
    let mut index = 0usize;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if !byte.is_ascii() {
            let character = source[index..]
                .chars()
                .next()
                .expect("UTF-8 source has a character at this byte");
            output.push(character);
            index += character.len_utf8();
            continue;
        }
        if in_string {
            output.push(byte as char);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if byte == b'"' {
            in_string = true;
            output.push('"');
            index += 1;
            continue;
        }
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        match byte {
            b'{' | b'[' => {
                output.push(byte as char);
                let next = next_non_whitespace(bytes, index + 1);
                let expected = if byte == b'{' { b'}' } else { b']' };
                if next != Some(expected) {
                    depth += 1;
                    separator(&mut output, compact, depth);
                }
            }
            b'}' | b']' => {
                let previous = output.as_bytes().last().copied();
                let opener = if byte == b'}' { b'{' } else { b'[' };
                if previous != Some(opener) {
                    depth = depth.saturating_sub(1);
                    separator(&mut output, compact, depth);
                }
                output.push(byte as char);
            }
            b',' => {
                output.push(',');
                separator(&mut output, compact, depth);
            }
            b':' => {
                output.push(':');
                if !compact {
                    output.push(' ');
                }
            }
            _ => output.push(byte as char),
        }
        index += 1;
    }
    if trailing_newline && !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn next_non_whitespace(bytes: &[u8], mut index: usize) -> Option<u8> {
    while let Some(byte) = bytes.get(index).copied() {
        if !byte.is_ascii_whitespace() {
            return Some(byte);
        }
        index += 1;
    }
    None
}

fn separator(output: &mut String, compact: bool, depth: usize) {
    if compact {
        return;
    }
    output.push('\n');
    output.extend(std::iter::repeat_n(' ', depth.saturating_mul(2)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_formatting_preserves_key_number_and_escape_lexemes() {
        let source =
            "{\"z\":1e2,\"a\":\"\\u4e16\\u754c\",\"actual\":\"世界\",\"nested\":[true,false]}\n";
        let formatted = format_json(source).unwrap();
        assert_eq!(
            formatted,
            "{\n  \"z\": 1e2,\n  \"a\": \"\\u4e16\\u754c\",\n  \"actual\": \"世界\",\n  \"nested\": [\n    true,\n    false\n  ]\n}\n"
        );
    }

    #[test]
    fn json_formatting_rejects_invalid_input_without_candidate() {
        assert!(matches!(
            format_json("{\"a\":}"),
            Err(FormatError::InvalidJson { .. })
        ));
    }

    #[test]
    fn json_lines_remains_one_record_per_line() {
        let formatted = format_json_lines(" { \"b\" : 2 }\n[ 1, 2 ]\n").unwrap();
        assert_eq!(formatted, "{\"b\":2}\n[1,2]\n");
    }

    #[test]
    fn selection_candidate_keeps_source_column_on_following_lines() {
        let formatted = format_json("{\"a\":1}").unwrap();
        assert_eq!(
            indent_multiline_candidate(formatted, 4),
            "{\n      \"a\": 1\n    }"
        );
    }

    #[test]
    fn workspace_formatter_and_format_on_save_override_global_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("sample.rs");
        std::fs::write(&file, "fn main() {}\n").unwrap();
        std::fs::write(
            directory.path().join(".gmark.toml"),
            "[formatting]\nformat_on_save = true\n[formatting.languages.rust]\ncommand = \"custom-rustfmt\"\nsupports_range = true\n",
        )
        .unwrap();

        let config = nearest_workspace_config(&file).expect("workspace config should be found");
        let parsed =
            toml::from_str::<toml::Value>(&std::fs::read_to_string(config).unwrap()).unwrap();
        assert_eq!(
            parsed
                .get("formatting")
                .and_then(|value| value.get("format_on_save"))
                .and_then(toml::Value::as_bool),
            Some(true)
        );
        assert!(format_on_save_for_file(&file, false));
        let FormatterResolution::External(spec) =
            resolve_formatter(SourceLanguageId::Rust, &file, Some(0..2))
        else {
            panic!("workspace formatter should resolve");
        };
        assert_eq!(spec.command, "custom-rustfmt");
        assert!(spec.supports_range);
        assert!(spec.from_workspace);
    }

    #[test]
    fn shell_formatter_uses_stdin_stdout_protocol() {
        let directory = tempfile::tempdir().unwrap();
        #[cfg(target_os = "windows")]
        let command =
            "$text = [Console]::In.ReadToEnd(); [Console]::Out.Write($text.ToUpperInvariant())";
        #[cfg(not(target_os = "windows"))]
        let command = "tr '[:lower:]' '[:upper:]'";
        let spec = ExternalFormatterSpec {
            command: command.to_owned(),
            cwd: directory.path().to_path_buf(),
            file: directory.path().join("sample.txt"),
            language: SourceLanguageId::PlainText,
            selection: None,
            timeout: Duration::from_secs(5),
            max_output_bytes: 1024,
            supports_range: false,
            from_workspace: false,
        };
        let output = run_shell_formatter(&spec, b"hello", &SearchCancellation::default()).unwrap();
        assert_eq!(output, "HELLO");
    }

    fn shell_spec(
        command: &str,
        timeout: Duration,
        max_output_bytes: usize,
    ) -> ExternalFormatterSpec {
        let directory = std::env::temp_dir();
        ExternalFormatterSpec {
            command: command.to_owned(),
            cwd: directory.clone(),
            file: directory.join("sample.txt"),
            language: SourceLanguageId::PlainText,
            selection: None,
            timeout,
            max_output_bytes,
            supports_range: false,
            from_workspace: false,
        }
    }

    #[test]
    fn shell_formatter_rejects_nonzero_exit_with_stderr() {
        #[cfg(target_os = "windows")]
        let command = "[Console]::Error.Write('bad input'); exit 7";
        #[cfg(not(target_os = "windows"))]
        let command = "printf 'bad input' >&2; exit 7";
        let error = run_shell_formatter(
            &shell_spec(command, Duration::from_secs(5), 1024),
            b"input",
            &SearchCancellation::default(),
        )
        .unwrap_err();
        assert!(matches!(error, FormatError::External(message) if message.contains("bad input")));
    }

    #[test]
    fn shell_formatter_enforces_timeout_and_output_limit() {
        #[cfg(target_os = "windows")]
        let sleep = "Start-Sleep -Seconds 5";
        #[cfg(not(target_os = "windows"))]
        let sleep = "sleep 5";
        assert_eq!(
            run_shell_formatter(
                &shell_spec(sleep, Duration::from_millis(30), 1024),
                b"",
                &SearchCancellation::default(),
            ),
            Err(FormatError::TimedOut)
        );

        #[cfg(target_os = "windows")]
        let flood = "[Console]::Out.Write('x' * 64)";
        #[cfg(not(target_os = "windows"))]
        let flood = "printf '%064d' 0";
        assert_eq!(
            run_shell_formatter(
                &shell_spec(flood, Duration::from_secs(5), 16),
                b"",
                &SearchCancellation::default(),
            ),
            Err(FormatError::OutputTooLarge)
        );
    }

    #[test]
    fn shell_formatter_rejects_invalid_utf8() {
        #[cfg(target_os = "windows")]
        let command = "$bytes = [byte[]](255); $out = [Console]::OpenStandardOutput(); $out.Write($bytes, 0, 1)";
        #[cfg(not(target_os = "windows"))]
        let command = "printf '\\377'";
        assert_eq!(
            run_shell_formatter(
                &shell_spec(command, Duration::from_secs(5), 1024),
                b"",
                &SearchCancellation::default(),
            ),
            Err(FormatError::InvalidUtf8)
        );
    }
}
