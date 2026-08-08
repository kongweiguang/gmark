// @author kongweiguang

//! 质量门禁共用的文件遍历与轻量 Rust 词法工具。

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

const SCAN_ROOTS: &[&str] = &[
    "src", "crates", "tests", "benches", "examples", "fuzz", "scripts", "docs", ".github", "xtask",
];
const ROOT_FILES: &[&str] = &[
    "AGENTS.md",
    "Cargo.toml",
    "README.md",
    "build.rs",
    "rust-toolchain.toml",
];

/// Optional visual-evidence adapters are kept in the tree for local capture
/// experiments, but are not part of the production module graph until their
/// platform capture backend is available. The marker is intentionally narrow
/// so a source file cannot silently opt out of quality checks.
pub(crate) const OPTIONAL_BOARD_EVIDENCE_MARKER: &str =
    "@quality-exempt optional board evidence harness";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TokenKind {
    Identifier,
    Punctuation,
    String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) line: usize,
    pub(crate) text: String,
}

impl Token {
    pub(crate) fn is(&self, text: &str) -> bool {
        self.text == text
    }
}

pub(crate) fn finish(label: &str, mut violations: Vec<String>) -> Result<(), String> {
    violations.sort();
    violations.dedup();
    if violations.is_empty() {
        println!("{label} passed");
        Ok(())
    } else {
        Err(violations.join("\n"))
    }
}

pub(crate) fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map_err(|error| format!("failed to read '{}': {error}", path.display()))
}

pub(crate) fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(crate) fn walk_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("failed to read '{}': {error}", directory.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("failed to inspect '{}': {error}", directory.display()))?;
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|error| format!("failed to inspect '{}': {error}", path.display()))?;
            if kind.is_dir() {
                let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
                if !matches!(
                    name,
                    "target" | "vendor" | "node_modules" | ".git" | ".codegraph" | ".tmp"
                ) {
                    pending.push(path);
                }
            } else if kind.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

pub(crate) fn manual_rust_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    walk_files(root)?
        .into_iter()
        .filter(|path| path.extension() == Some(OsStr::new("rs")))
        .filter_map(|path| match read_text(&path) {
            Ok(source) if !is_generated_source(&source) => Some(Ok(path)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

pub(crate) fn manual_production_rust_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    Ok(manual_rust_files(root)?
        .into_iter()
        .filter(|path| is_production_rust(root, path))
        .collect())
}

pub(crate) fn maintainable_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let extensions = ["rs", "md", "py", "ps1", "sh", "yml", "yaml", "toml"];
    let mut files = BTreeSet::new();
    for scan_root in SCAN_ROOTS {
        for path in walk_files(&root.join(scan_root))? {
            if path
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|extension| extensions.contains(&extension))
            {
                files.insert(path);
            }
        }
    }
    for file in ROOT_FILES {
        let path = root.join(file);
        if path.is_file() {
            files.insert(path);
        }
    }
    Ok(files.into_iter().collect())
}

pub(crate) fn line_count(path: &Path) -> Result<usize, String> {
    let source = read_text(path)?;
    Ok(if source.is_empty() {
        0
    } else {
        source.lines().count()
    })
}

pub(crate) fn is_test_fixture_path(root: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let components = relative
        .components()
        .filter_map(component_name)
        .collect::<Vec<_>>();
    let Some(source_index) = components.iter().position(|component| *component == "src") else {
        return false;
    };
    let source_components = &components[source_index + 1..];
    source_components
        .iter()
        .any(|component| is_test_fixture_component(component))
}

fn is_test_fixture_component(component: &str) -> bool {
    let name = component
        .strip_suffix(".rs")
        .unwrap_or(component)
        .to_ascii_lowercase();
    name.split(['_', '-']).any(|segment| {
        matches!(
            segment,
            "test"
                | "tests"
                | "fixture"
                | "fixtures"
                | "testdata"
                | "mock"
                | "mocks"
                | "fake"
                | "fakes"
        )
    })
}

pub(crate) fn is_production_rust(root: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    path.file_name() == Some(OsStr::new("build.rs"))
        || relative
            .components()
            .any(|component| component_name(component) == Some("src"))
}

pub(crate) fn is_generated_source(source: &str) -> bool {
    source.lines().take(8).any(|line| {
        let normalized = line.to_ascii_lowercase();
        normalized.contains("@generated") || normalized.contains("do not edit")
    })
}

pub(crate) fn is_optional_board_evidence_source(source: &str) -> bool {
    source
        .lines()
        .take(8)
        .any(|line| line.contains(OPTIONAL_BOARD_EVIDENCE_MARKER))
}

pub(crate) fn rust_tokens(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut line = 1;

    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                line += 1;
                index += 1;
            }
            byte if byte.is_ascii_whitespace() => index += 1,
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = skip_line_comment(bytes, index + 2);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let end = skip_block_comment(bytes, index + 2);
                line += count_newlines(&bytes[index..end]);
                index = end;
            }
            b'r' | b'b' if raw_string_start(bytes, index).is_some() => {
                let (quote, hashes) = raw_string_start(bytes, index).expect("checked above");
                let (end, value) = read_raw_string(bytes, quote, hashes);
                tokens.push(Token {
                    kind: TokenKind::String,
                    line,
                    text: value,
                });
                line += count_newlines(&bytes[index..end]);
                index = end;
            }
            b'"' => {
                let (end, value) = read_quoted_string(bytes, index);
                tokens.push(Token {
                    kind: TokenKind::String,
                    line,
                    text: value,
                });
                line += count_newlines(&bytes[index..end]);
                index = end;
            }
            b'b' if bytes.get(index + 1) == Some(&b'"') => {
                let (end, value) = read_quoted_string(bytes, index + 1);
                tokens.push(Token {
                    kind: TokenKind::String,
                    line,
                    text: value,
                });
                line += count_newlines(&bytes[index..end]);
                index = end;
            }
            b'\'' => {
                if let Some(end) = quoted_character_end(bytes, index) {
                    line += count_newlines(&bytes[index..end]);
                    index = end;
                } else {
                    tokens.push(punctuation("'", line));
                    index += 1;
                }
            }
            b'b' if bytes.get(index + 1) == Some(&b'\'') => {
                if let Some(end) = quoted_character_end(bytes, index + 1) {
                    line += count_newlines(&bytes[index..end]);
                    index = end;
                } else {
                    tokens.push(identifier("b", line));
                    index += 1;
                }
            }
            byte if is_identifier_start(byte) => {
                let start = index;
                index += 1;
                while bytes
                    .get(index)
                    .is_some_and(|byte| is_identifier_continue(*byte))
                {
                    index += 1;
                }
                tokens.push(identifier(
                    std::str::from_utf8(&bytes[start..index]).unwrap_or_default(),
                    line,
                ));
            }
            b':' if bytes.get(index + 1) == Some(&b':') => {
                tokens.push(punctuation("::", line));
                index += 2;
            }
            byte if byte.is_ascii() => {
                tokens.push(punctuation(&(byte as char).to_string(), line));
                index += 1;
            }
            _ => {
                let width = source[index..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(1);
                index += width;
            }
        }
    }
    tokens
}

pub(crate) fn references_test_support(tokens: &[Token]) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        token.kind == TokenKind::String
            && (token.text.contains("tests/support") || token.text.contains("tests\\support"))
            && (is_include_argument(tokens, index) || is_path_attribute_argument(tokens, index))
    })
}

fn component_name(component: Component<'_>) -> Option<&str> {
    match component {
        Component::Normal(value) => value.to_str(),
        _ => None,
    }
}

fn is_include_argument(tokens: &[Token], index: usize) -> bool {
    index >= 3
        && tokens[index - 1].is("(")
        && tokens[index - 2].is("!")
        && matches!(
            tokens[index - 3].text.as_str(),
            "include" | "include_str" | "include_bytes"
        )
}

fn is_path_attribute_argument(tokens: &[Token], index: usize) -> bool {
    index >= 4
        && tokens[index - 1].is("=")
        && tokens[index - 2].is("path")
        && tokens[index - 3].is("[")
        && tokens[index - 4].is("#")
}

fn identifier(text: &str, line: usize) -> Token {
    Token {
        kind: TokenKind::Identifier,
        line,
        text: text.to_owned(),
    }
}

fn punctuation(text: &str, line: usize) -> Token {
    Token {
        kind: TokenKind::Punctuation,
        line,
        text: text.to_owned(),
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn skip_block_comment(bytes: &[u8], mut index: usize) -> usize {
    let mut depth = 1;
    while index < bytes.len() && depth > 0 {
        match (bytes[index], bytes.get(index + 1)) {
            (b'/', Some(b'*')) => {
                depth += 1;
                index += 2;
            }
            (b'*', Some(b'/')) => {
                depth -= 1;
                index += 2;
            }
            _ => index += 1,
        }
    }
    index
}

fn raw_string_start(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    let raw = match bytes.get(index) {
        Some(b'r') => index,
        Some(b'b') if bytes.get(index + 1) == Some(&b'r') => index + 1,
        _ => return None,
    };
    let mut quote = raw + 1;
    while bytes.get(quote) == Some(&b'#') {
        quote += 1;
    }
    (bytes.get(quote) == Some(&b'"')).then_some((quote, quote - raw - 1))
}

fn read_raw_string(bytes: &[u8], quote: usize, hashes: usize) -> (usize, String) {
    let mut cursor = quote + 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"'
            && bytes
                .get(cursor + 1..cursor + 1 + hashes)
                .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
        {
            let value = String::from_utf8_lossy(&bytes[quote + 1..cursor]).into_owned();
            return (cursor + hashes + 1, value);
        }
        cursor += 1;
    }
    (
        bytes.len(),
        String::from_utf8_lossy(&bytes[quote + 1..]).into_owned(),
    )
}

fn read_quoted_string(bytes: &[u8], quote: usize) -> (usize, String) {
    let mut value = String::new();
    let mut cursor = quote + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => return (cursor + 1, value),
            b'\\' if cursor + 1 < bytes.len() => {
                cursor += 1;
                value.push(match bytes[cursor] {
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    byte => byte as char,
                });
                cursor += 1;
            }
            byte => {
                value.push(byte as char);
                cursor += 1;
            }
        }
    }
    (bytes.len(), value)
}

fn quoted_character_end(bytes: &[u8], quote: usize) -> Option<usize> {
    let mut cursor = quote + 1;
    while cursor < bytes.len() && bytes[cursor] != b'\n' {
        if bytes[cursor] == b'\\' {
            cursor += 2;
        } else if bytes[cursor] == b'\'' {
            return Some(cursor + 1);
        } else {
            cursor += 1;
        }
    }
    None
}

fn count_newlines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|byte| **byte == b'\n').count()
}
