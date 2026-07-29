// @author kongweiguang

use super::super::formatter_process::run_shell_formatter;
use super::*;

use gmark_paged_document::SearchCancellation;

const SHELL_TEST_TIMEOUT: Duration = Duration::from_secs(30);

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
    let spec = shell_spec(command, SHELL_TEST_TIMEOUT, 1024, directory.path());
    let output = run_shell_formatter(&spec, b"hello", &SearchCancellation::default()).unwrap();
    assert_eq!(output, "HELLO");
}

#[test]
fn shell_formatter_enforces_failure_contracts() {
    #[cfg(target_os = "windows")]
    let nonzero = "[Console]::Error.Write('bad input'); exit 7";
    #[cfg(not(target_os = "windows"))]
    let nonzero = "printf 'bad input' >&2; exit 7";
    let error = run_shell_formatter(
        &shell_spec(nonzero, SHELL_TEST_TIMEOUT, 1024, &std::env::temp_dir()),
        b"input",
        &SearchCancellation::default(),
    )
    .unwrap_err();
    assert!(matches!(error, FormatError::External(message) if message.contains("bad input")));

    #[cfg(target_os = "windows")]
    let sleep = "Start-Sleep -Seconds 5";
    #[cfg(not(target_os = "windows"))]
    let sleep = "sleep 5";
    assert_eq!(
        run_shell_formatter(
            &shell_spec(
                sleep,
                Duration::from_millis(30),
                1024,
                &std::env::temp_dir()
            ),
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
            &shell_spec(flood, SHELL_TEST_TIMEOUT, 16, &std::env::temp_dir()),
            b"",
            &SearchCancellation::default(),
        ),
        Err(FormatError::OutputTooLarge)
    );

    #[cfg(target_os = "windows")]
    let invalid_utf8 =
        "$bytes = [byte[]](255); $out = [Console]::OpenStandardOutput(); $out.Write($bytes, 0, 1)";
    #[cfg(not(target_os = "windows"))]
    let invalid_utf8 = "printf '\\377'";
    assert_eq!(
        run_shell_formatter(
            &shell_spec(
                invalid_utf8,
                SHELL_TEST_TIMEOUT,
                1024,
                &std::env::temp_dir()
            ),
            b"",
            &SearchCancellation::default(),
        ),
        Err(FormatError::InvalidUtf8)
    );
}

fn shell_spec(
    command: &str,
    timeout: Duration,
    max_output_bytes: usize,
    directory: &std::path::Path,
) -> ExternalFormatterSpec {
    ExternalFormatterSpec {
        command: command.to_owned(),
        cwd: directory.to_path_buf(),
        file: directory.join("sample.txt"),
        language: SourceLanguageId::PlainText,
        selection: None,
        timeout,
        max_output_bytes,
        supports_range: false,
        from_workspace: false,
    }
}
