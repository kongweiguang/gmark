// @author kongweiguang

//! Source-size, test-layout, maintainership, and source-structure gates.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::source::{self, Token, TokenKind};

const HARD_LINE_LIMIT: usize = 800;
const WARNING_LINE_LIMIT: usize = 500;

pub(crate) fn check_source_size(root: &Path) -> Result<(), String> {
    let mut warnings = Vec::new();
    let mut violations = Vec::new();
    for path in source::manual_rust_files(root)? {
        let lines = source::line_count(&path)?;
        let relative = source::relative(root, &path);
        if lines > HARD_LINE_LIMIT {
            violations.push(format!("{lines:>5}  {relative}"));
        } else if lines > WARNING_LINE_LIMIT {
            warnings.push(format!("{lines:>5}  {relative}"));
        }
    }
    for warning in &warnings {
        println!("source-size warning: {warning}");
    }
    if violations.is_empty() {
        println!(
            "source-size passed (manual Rust hard {HARD_LINE_LIMIT}, warning {WARNING_LINE_LIMIT}, {} warnings)",
            warnings.len()
        );
        Ok(())
    } else {
        Err(format!(
            "manual Rust files exceed the {HARD_LINE_LIMIT}-line hard limit:\n{}",
            violations.join("\n")
        ))
    }
}

pub(crate) fn check_test_layout(root: &Path) -> Result<(), String> {
    let mut violations = Vec::new();
    let paths = source::manual_production_rust_files(root)?;
    let test_support_directories = explicit_test_support_directories(&paths)?;
    for path in paths {
        let relative = source::relative(root, &path);
        if source::is_test_fixture_path(root, &path)
            && !is_explicit_test_support_path(&path, &test_support_directories)
        {
            violations.push(format!(
                "{relative}: test fixture must not live below a production src/ path"
            ));
        }
        let tokens = source::rust_tokens(&source::read_text(&path)?);
        let cfg_test_module = cfg_test_inline_module(&tokens);
        if cfg_test_module.is_none()
            && let Some(line) = inline_test_module(&tokens)
        {
            violations.push(format!(
                "{relative}:{line}: inline test module is forbidden; keep test implementation outside production src/"
            ));
        }
        if let Some(line) = cfg_test_module {
            violations.push(format!(
                "{relative}:{line}: #[cfg(test)] inline module is forbidden; keep test implementation outside production src/"
            ));
        }
        if let Some(line) = test_attribute(&tokens) {
            violations.push(format!(
                "{relative}:{line}: #[test] body is mixed with production source"
            ));
        }
        if source::references_test_support(&tokens) {
            violations.push(format!(
                "{relative}: production code references test support"
            ));
        }
    }
    source::finish("test-layout", violations)
}

fn explicit_test_support_directories(paths: &[PathBuf]) -> Result<BTreeSet<PathBuf>, String> {
    let mut directories = BTreeSet::new();
    for path in paths {
        let tokens = source::rust_tokens(&source::read_text(path)?);
        for index in 0..tokens.len() {
            if cfg_test_support_module(&tokens, index) {
                directories.insert(child_module_directory(path).join("test_support"));
            }
        }
    }
    Ok(directories)
}

fn cfg_test_support_module(tokens: &[Token], index: usize) -> bool {
    if !is_cfg_test_attribute(tokens, index) {
        return false;
    }
    let Some(end) = attribute_end(tokens, index) else {
        return false;
    };
    let cursor = skip_visibility(tokens, end + 1);
    tokens.get(cursor).is_some_and(|token| token.is("mod"))
        && tokens
            .get(cursor + 1)
            .is_some_and(|token| token.is("test_support"))
        && tokens.get(cursor + 2).is_some_and(|token| token.is(";"))
}

fn is_explicit_test_support_path(path: &Path, directories: &BTreeSet<PathBuf>) -> bool {
    directories
        .iter()
        .any(|directory| path.starts_with(directory))
}

pub(crate) fn check_authors(root: &Path) -> Result<(), String> {
    let mut violations = Vec::new();
    for path in source::maintainable_files(root)? {
        let contents = source::read_text(&path)?;
        if path.extension() == Some(OsStr::new("rs")) && source::is_generated_source(&contents) {
            continue;
        }
        if !contents
            .lines()
            .take(10)
            .any(|line| line.contains("@author kongweiguang"))
        {
            violations.push(format!(
                "{}: missing @author kongweiguang",
                source::relative(root, &path)
            ));
        }
    }
    source::finish("authors", violations)
}

pub(crate) fn check_source_structure(
    root: &Path,
    violations: &mut Vec<String>,
) -> Result<(), String> {
    let sources = source::manual_production_rust_files(root)?
        .into_iter()
        .map(|path| {
            let text = source::read_text(&path)?;
            let tokens = source::rust_tokens(&text);
            Ok(SourceFile { path, text, tokens })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let reachable = declared_module_files(root, &sources);

    for file in &sources {
        let relative = source::relative(root, &file.path);
        for (index, token) in file.tokens.iter().enumerate() {
            if token.is("include")
                && file.tokens.get(index + 1).is_some_and(|next| next.is("!"))
                && !is_allowed_generated_catalog_include(root, file, index)
            {
                violations.push(format!(
                    "{relative}:{}: implementation include! is forbidden; use a real module",
                    token.line
                ));
            }
        }
        for line in lint_allow_lines(&file.tokens) {
            if !has_lint_allow_reason(&file.text, line) {
                violations.push(format!(
                    "{relative}:{line}: lint allow requires an immediately preceding reason and explicit removal condition"
                ));
            }
        }
        let stem = file
            .path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or_default();
        if is_numbered_source_stem(stem) {
            violations.push(format!(
                "{relative}: numbered production source filename is forbidden; name the responsibility"
            ));
        }
        if !is_crate_root(&file.path) && !reachable.contains(&normalized_path(&file.path)) {
            violations.push(format!(
                "{relative}: orphan Rust source is not reachable from a module declaration"
            ));
        }
    }
    Ok(())
}

struct SourceFile {
    path: PathBuf,
    text: String,
    tokens: Vec<Token>,
}

fn inline_test_module(tokens: &[Token]) -> Option<usize> {
    tokens.windows(3).find_map(|window| {
        (window[0].is("mod") && window[1].is("tests") && window[2].is("{"))
            .then_some(window[0].line)
    })
}

fn cfg_test_inline_module(tokens: &[Token]) -> Option<usize> {
    for index in 0..tokens.len() {
        if !is_cfg_test_attribute(tokens, index) {
            continue;
        }
        let mut cursor = attribute_end(tokens, index)? + 1;
        while tokens.get(cursor).is_some_and(|token| token.is("#")) {
            cursor = attribute_end(tokens, cursor)? + 1;
        }
        cursor = skip_visibility(tokens, cursor);
        if tokens.get(cursor).is_some_and(|token| token.is("mod"))
            && tokens.get(cursor + 2).is_some_and(|token| token.is("{"))
        {
            return Some(tokens[index].line);
        }
    }
    None
}

fn test_attribute(tokens: &[Token]) -> Option<usize> {
    (0..tokens.len()).find_map(|index| {
        (tokens.get(index).is_some_and(|token| token.is("#"))
            && tokens.get(index + 1).is_some_and(|token| token.is("["))
            && tokens.get(index + 2).is_some_and(|token| token.is("test"))
            && tokens.get(index + 3).is_some_and(|token| token.is("]")))
        .then_some(tokens[index].line)
    })
}

fn is_cfg_test_attribute(tokens: &[Token], index: usize) -> bool {
    ["#", "[", "cfg", "(", "test", ")", "]"]
        .iter()
        .enumerate()
        .all(|(offset, expected)| {
            tokens
                .get(index + offset)
                .is_some_and(|token| token.is(expected))
        })
}

fn lint_allow_lines(tokens: &[Token]) -> Vec<usize> {
    let mut lines = Vec::new();
    for index in 0..tokens.len() {
        let Some(open) = attribute_open_bracket(tokens, index) else {
            continue;
        };
        let Some(end) = attribute_end(tokens, index) else {
            continue;
        };
        if tokens[open + 1..end]
            .iter()
            .any(|token| token.kind == TokenKind::Identifier && token.is("allow"))
        {
            lines.push(tokens[index].line);
        }
    }
    lines
}

fn attribute_open_bracket(tokens: &[Token], index: usize) -> Option<usize> {
    if !tokens.get(index).is_some_and(|token| token.is("#")) {
        return None;
    }
    if tokens.get(index + 1).is_some_and(|token| token.is("[")) {
        return Some(index + 1);
    }
    (tokens.get(index + 1).is_some_and(|token| token.is("!"))
        && tokens.get(index + 2).is_some_and(|token| token.is("[")))
    .then_some(index + 2)
}

fn attribute_end(tokens: &[Token], index: usize) -> Option<usize> {
    let open = attribute_open_bracket(tokens, index)?;
    let mut depth = 1;
    let mut cursor = open + 1;
    while let Some(token) = tokens.get(cursor) {
        if token.is("[") {
            depth += 1;
        } else if token.is("]") {
            depth -= 1;
            if depth == 0 {
                return Some(cursor);
            }
        }
        cursor += 1;
    }
    None
}

fn skip_visibility(tokens: &[Token], mut index: usize) -> usize {
    if !tokens.get(index).is_some_and(|token| token.is("pub")) {
        return index;
    }
    index += 1;
    if !tokens.get(index).is_some_and(|token| token.is("(")) {
        return index;
    }
    let mut depth = 1;
    index += 1;
    while let Some(token) = tokens.get(index) {
        if token.is("(") {
            depth += 1;
        } else if token.is(")") {
            depth -= 1;
            if depth == 0 {
                return index + 1;
            }
        }
        index += 1;
    }
    index
}

fn has_lint_allow_reason(source_text: &str, attribute_line: usize) -> bool {
    let lines = source_text.lines().collect::<Vec<_>>();
    let Some(comment) = attribute_line
        .checked_sub(2)
        .and_then(|index| lines.get(index))
        .map(|line| line.trim_start())
    else {
        return false;
    };
    if !comment.starts_with("//") {
        return false;
    }
    let normalized = comment.to_ascii_lowercase();
    let has_reason = normalized.contains("reason:")
        || comment.contains("原因:")
        || comment.contains("原因：")
        || comment.contains("理由:")
        || comment.contains("理由：");
    let has_removal_condition = normalized.contains("remove")
        || normalized.contains("until")
        || normalized.contains("when")
        || comment.contains("删除")
        || comment.contains("移除")
        || comment.contains("淘汰");
    has_reason && has_removal_condition
}

fn is_numbered_source_stem(stem: &str) -> bool {
    if stem.starts_with("fn_")
        || stem
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
    {
        return true;
    }
    let without_digits = stem.trim_end_matches(|character: char| character.is_ascii_digit());
    without_digits.len() != stem.len()
        && without_digits
            .chars()
            .last()
            .is_some_and(|character| matches!(character, '_' | '-'))
}

fn declared_module_files(root: &Path, sources: &[SourceFile]) -> BTreeSet<PathBuf> {
    let mut reachable = BTreeSet::new();
    for file in sources {
        for (index, token) in file.tokens.iter().enumerate() {
            if !token.is("mod") || !is_external_module_declaration(&file.tokens, index) {
                continue;
            }
            let name = &file.tokens[index + 1].text;
            if let Some(path) = explicit_module_path(&file.tokens, index) {
                reachable.insert(normalized_path(
                    &file.path.parent().unwrap_or(root).join(path),
                ));
            } else {
                let directory = child_module_directory(&file.path);
                reachable.insert(normalized_path(&directory.join(format!("{name}.rs"))));
                reachable.insert(normalized_path(&directory.join(name).join("mod.rs")));
            }
        }
        for index in 0..file.tokens.len() {
            if is_allowed_generated_catalog_include(root, file, index)
                && let Some(path) = include_argument(&file.tokens, index)
            {
                reachable.insert(normalized_path(
                    &file.path.parent().unwrap_or(root).join(path),
                ));
            }
        }
    }
    reachable
}

fn is_external_module_declaration(tokens: &[Token], index: usize) -> bool {
    tokens
        .get(index + 1)
        .is_some_and(|token| token.kind == TokenKind::Identifier)
        && tokens.get(index + 2).is_some_and(|token| token.is(";"))
}

fn explicit_module_path(tokens: &[Token], module_index: usize) -> Option<&str> {
    let mut cursor = skip_visibility_backwards(tokens, module_index);
    while cursor > 0 && tokens.get(cursor - 1).is_some_and(|token| token.is("]")) {
        let close = cursor - 1;
        let open = matching_open_bracket(tokens, close)?;
        if open == 0 || !tokens.get(open - 1).is_some_and(|token| token.is("#")) {
            return None;
        }
        if tokens.get(open + 1).is_some_and(|token| token.is("path"))
            && tokens.get(open + 2).is_some_and(|token| token.is("="))
            && tokens
                .get(open + 3)
                .is_some_and(|token| token.kind == TokenKind::String)
            && open + 4 == close
        {
            return tokens.get(open + 3).map(|token| token.text.as_str());
        }
        cursor = open - 1;
    }
    None
}

fn skip_visibility_backwards(tokens: &[Token], module_index: usize) -> usize {
    if tokens
        .get(module_index.wrapping_sub(1))
        .is_some_and(|token| token.is("pub"))
    {
        return module_index - 1;
    }
    if !tokens
        .get(module_index.wrapping_sub(1))
        .is_some_and(|token| token.is(")"))
    {
        return module_index;
    }
    let mut depth = 1;
    let mut cursor = module_index - 1;
    while cursor > 0 {
        cursor -= 1;
        if tokens[cursor].is(")") {
            depth += 1;
        } else if tokens[cursor].is("(") {
            depth -= 1;
            if depth == 0 && cursor > 0 && tokens[cursor - 1].is("pub") {
                return cursor - 1;
            }
        }
    }
    module_index
}

fn matching_open_bracket(tokens: &[Token], close: usize) -> Option<usize> {
    let mut depth = 1;
    let mut cursor = close;
    while cursor > 0 {
        cursor -= 1;
        if tokens[cursor].is("]") {
            depth += 1;
        } else if tokens[cursor].is("[") {
            depth -= 1;
            if depth == 0 {
                return Some(cursor);
            }
        }
    }
    None
}

fn child_module_directory(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or_default();
    if matches!(stem, "lib" | "main" | "mod") {
        parent.to_path_buf()
    } else {
        parent.join(stem)
    }
}

fn normalized_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn is_allowed_generated_catalog_include(root: &Path, file: &SourceFile, index: usize) -> bool {
    source::relative(root, &file.path) == "src/i18n/parts/catalog.rs"
        && file
            .tokens
            .get(index)
            .is_some_and(|token| token.is("include"))
        && include_argument(&file.tokens, index) == Some("i18n_strings_catalog.rs")
}

fn include_argument(tokens: &[Token], index: usize) -> Option<&str> {
    (tokens.get(index).is_some_and(|token| token.is("include"))
        && tokens.get(index + 1).is_some_and(|token| token.is("!"))
        && tokens.get(index + 2).is_some_and(|token| token.is("("))
        && tokens
            .get(index + 3)
            .is_some_and(|token| token.kind == TokenKind::String))
    .then(|| tokens[index + 3].text.as_str())
}

fn is_crate_root(path: &Path) -> bool {
    standard_package_root(path).is_some_and(|root| root.join("Cargo.toml").is_file())
}

fn standard_package_root(path: &Path) -> Option<PathBuf> {
    if path.file_name() == Some(OsStr::new("build.rs")) {
        return path.parent().map(Path::to_path_buf);
    }
    let parent = path.parent()?;
    if parent.file_name() == Some(OsStr::new("src"))
        && matches!(path.file_name(), Some(name) if name == OsStr::new("lib.rs") || name == OsStr::new("main.rs"))
    {
        return parent.parent().map(Path::to_path_buf);
    }
    if parent.file_name() == Some(OsStr::new("bin"))
        && parent.parent()?.file_name() == Some(OsStr::new("src"))
    {
        return parent.parent()?.parent().map(Path::to_path_buf);
    }
    if path.file_name() == Some(OsStr::new("main.rs"))
        && parent.parent()?.file_name() == Some(OsStr::new("bin"))
        && parent.parent()?.parent()?.file_name() == Some(OsStr::new("src"))
    {
        return parent.parent()?.parent()?.parent().map(Path::to_path_buf);
    }
    None
}
