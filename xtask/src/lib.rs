// @author kongweiguang

//! gmark 仓库级质量门禁。

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const HARD_LINE_LIMIT: usize = 800;
const WARNING_LINE_LIMIT: usize = 500;
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

/// 执行一个质量子命令。
pub fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let command = arguments.first().map(String::as_str).unwrap_or("quality");
    let root = repository_root()?;
    run_at(&root, command)
}

/// 在指定仓库根目录执行门禁，供 fixture integration tests 使用。
pub fn run_at(root: &Path, command: &str) -> Result<(), String> {
    match command {
        "source-size" => check_source_size(root),
        "architecture" => check_architecture(root),
        "test-layout" => check_test_layout(root),
        "authors" => check_authors(root),
        "quality" => {
            let mut failures = Vec::new();
            for (name, check) in [
                (
                    "source-size",
                    check_source_size as fn(&Path) -> Result<(), String>,
                ),
                ("architecture", check_architecture),
                ("test-layout", check_test_layout),
                ("authors", check_authors),
            ] {
                if let Err(error) = check(root) {
                    failures.push(format!("{name}:\n{error}"));
                }
            }
            if failures.is_empty() {
                println!("gmark quality gates passed");
                Ok(())
            } else {
                Err(failures.join("\n\n"))
            }
        }
        _ => Err(format!(
            "unknown xtask command '{command}'; expected source-size, architecture, test-layout, authors, or quality"
        )),
    }
}

fn repository_root() -> Result<PathBuf, String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask manifest has no repository parent".to_owned())
}

fn check_source_size(root: &Path) -> Result<(), String> {
    let mut warnings = Vec::new();
    let mut violations = Vec::new();
    for path in source_files(root)? {
        let lines = line_count(&path)?;
        let relative = relative(root, &path);
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
            "source-size passed (hard {HARD_LINE_LIMIT}, warning {WARNING_LINE_LIMIT}, {} warnings)",
            warnings.len()
        );
        Ok(())
    } else {
        Err(format!(
            "files exceed the {HARD_LINE_LIMIT}-line hard limit:\n{}",
            violations.join("\n")
        ))
    }
}

fn check_test_layout(root: &Path) -> Result<(), String> {
    let mut violations = Vec::new();
    for path in rust_files_under(root, "src")?
        .into_iter()
        .chain(rust_files_under(root, "crates")?)
        .filter(|path| is_production_rust(root, path))
    {
        let relative = relative(root, &path);
        let normalized = relative.replace('\\', "/");
        if normalized.contains("/src/") && path.file_name() == Some(OsStr::new("tests.rs"))
            || normalized.starts_with("src/") && path.file_name() == Some(OsStr::new("tests.rs"))
        {
            violations.push(format!(
                "{relative}: test implementation must live under tests/"
            ));
        }
        let source = read_text(&path)?;
        if source.contains("mod tests {") {
            violations.push(format!(
                "{relative}: inline test module is forbidden; use #[path] to tests/unit"
            ));
        }
        if source
            .lines()
            .any(|line| line.trim_start().starts_with("#[test]"))
        {
            violations.push(format!(
                "{relative}: #[test] body is mixed with production source"
            ));
        }
    }
    finish("test-layout", violations)
}

fn is_production_rust(root: &Path, path: &Path) -> bool {
    let relative = relative(root, path);
    relative.starts_with("src/") || relative.contains("/src/")
}

fn check_architecture(root: &Path) -> Result<(), String> {
    let mut violations = Vec::new();
    for domain in [
        "crates/gmark-document",
        "crates/gmark-large-document",
        "crates/gmark-recovery-codec",
    ] {
        let manifest = root.join(domain).join("Cargo.toml");
        if !manifest.exists() {
            continue;
        }
        let source = read_text(&manifest)?;
        for forbidden in ["gpui =", "accesskit", "windows ="] {
            if source.lines().any(|line| {
                let line = line.trim_start();
                !line.starts_with('#') && line.starts_with(forbidden)
            }) {
                violations.push(format!(
                    "{domain}/Cargo.toml: domain crate depends on UI/platform crate '{forbidden}'"
                ));
            }
        }
    }

    let layers = BTreeMap::from([
        (
            "components",
            ["editor", "app_menu", "large_file"].as_slice(),
        ),
        ("config", ["editor", "components", "app_menu"].as_slice()),
        ("export", ["editor", "app_menu"].as_slice()),
        ("net", ["editor", "app_menu"].as_slice()),
        ("theme", ["editor", "app_menu"].as_slice()),
    ]);
    for (source_layer, forbidden_targets) in layers {
        let layer_root = root.join("src").join(source_layer);
        for path in walk_files(&layer_root)? {
            if path.extension() != Some(OsStr::new("rs")) {
                continue;
            }
            let source = read_text(&path)?;
            for target in forbidden_targets {
                let patterns = [
                    format!("crate::{target}"),
                    format!("super::{target}"),
                    format!("use {target}::"),
                ];
                if patterns.iter().any(|pattern| source.contains(pattern)) {
                    violations.push(format!(
                        "{}: layer '{source_layer}' must not depend on '{target}'",
                        relative(root, &path)
                    ));
                }
            }
            if source.contains("tests/support") || source.contains("tests\\support") {
                violations.push(format!(
                    "{}: production code references test support",
                    relative(root, &path)
                ));
            }
        }
    }
    check_source_structure(root, &mut violations)?;
    finish("architecture", violations)
}

fn check_source_structure(root: &Path, violations: &mut Vec<String>) -> Result<(), String> {
    let sources = rust_files_under(root, "src")?
        .into_iter()
        .chain(rust_files_under(root, "crates")?)
        .filter(|path| is_production_rust(root, path))
        .map(|path| {
            let source = read_text(&path)?;
            Ok((path, source))
        })
        .collect::<Result<Vec<_>, String>>()?;

    for (path, source) in &sources {
        let relative = relative(root, path);
        let normalized = relative.replace('\\', "/");
        for (line_index, line) in source.lines().enumerate() {
            if line.contains("include!(")
                && !(normalized == "src/i18n/parts/catalog.rs"
                    && line.contains("include!(\"i18n_strings_catalog.rs\")"))
            {
                violations.push(format!(
                    "{relative}:{}: implementation include! is forbidden; use a real module",
                    line_index + 1
                ));
            }
            let trimmed = line.trim_start();
            if (trimmed.starts_with("#[allow(")
                || trimmed.starts_with("#[cfg_attr(") && trimmed.contains("allow("))
                && !has_lint_allow_reason(source, line_index)
            {
                violations.push(format!(
                    "{relative}:{}: lint allow requires a reason comment and removal condition",
                    line_index + 1
                ));
            }
        }

        let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or_default();
        if is_mechanical_source_stem(stem) {
            violations.push(format!(
                "{relative}: mechanical source filename is forbidden; name the responsibility"
            ));
        }
        if is_orphan_source(path, source, &sources) {
            violations.push(format!(
                "{relative}: orphan Rust source is not reachable from a module or data include"
            ));
        }
    }
    Ok(())
}

fn has_lint_allow_reason(source: &str, line_index: usize) -> bool {
    let lines = source.lines().collect::<Vec<_>>();
    lines[..line_index.min(lines.len())]
        .iter()
        .rev()
        .take(2)
        .any(|line| {
            let comment = line.trim_start();
            comment.starts_with("//")
                && (comment.contains("reason:")
                    || comment.contains("原因:")
                    || comment.contains("理由:"))
                && (comment.contains("remove")
                    || comment.contains("until")
                    || comment.contains("下游")
                    || comment.contains("删除")
                    || comment.contains("兼容"))
        })
}

fn is_mechanical_source_stem(stem: &str) -> bool {
    if stem.starts_with("fn_") {
        return true;
    }
    let without_digits = stem.trim_end_matches(|character: char| character.is_ascii_digit());
    without_digits.len() != stem.len() && without_digits.ends_with('_')
}

fn is_orphan_source(path: &Path, source: &str, sources: &[(PathBuf, String)]) -> bool {
    let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or_default();
    if matches!(file_name, "lib.rs" | "main.rs" | "mod.rs") {
        return false;
    }
    let module_declaration = format!("mod {stem};");
    sources.iter().all(|(candidate, candidate_source)| {
        candidate == path
            || !(candidate_source.contains(file_name)
                || candidate_source.contains(&module_declaration))
    }) && !source
        .lines()
        .any(|line| line.trim_start().starts_with("#!["))
}

fn check_authors(root: &Path) -> Result<(), String> {
    let mut violations = Vec::new();
    for path in maintainable_files(root)? {
        let source = read_text(&path)?;
        if !source
            .lines()
            .take(10)
            .any(|line| line.contains("@author kongweiguang"))
        {
            violations.push(format!(
                "{}: missing @author kongweiguang",
                relative(root, &path)
            ));
        }
    }
    finish("authors", violations)
}

fn finish(label: &str, mut violations: Vec<String>) -> Result<(), String> {
    violations.sort();
    violations.dedup();
    if violations.is_empty() {
        println!("{label} passed");
        Ok(())
    } else {
        Err(violations.join("\n"))
    }
}

fn source_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = BTreeSet::new();
    for scan_root in ["src", "crates", "tests", "xtask"] {
        for path in walk_files(&root.join(scan_root))? {
            if path.extension() == Some(OsStr::new("rs")) {
                files.insert(path);
            }
        }
    }
    Ok(files.into_iter().collect())
}

fn maintainable_files(root: &Path) -> Result<Vec<PathBuf>, String> {
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

fn rust_files_under(root: &Path, relative_root: &str) -> Result<Vec<PathBuf>, String> {
    Ok(walk_files(&root.join(relative_root))?
        .into_iter()
        .filter(|path| path.extension() == Some(OsStr::new("rs")))
        .collect())
}

fn walk_files(root: &Path) -> Result<Vec<PathBuf>, String> {
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
                    "target" | "vendor" | "node_modules" | ".git" | ".codegraph"
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

fn line_count(path: &Path) -> Result<usize, String> {
    let source = read_text(path)?;
    if source.is_empty() {
        Ok(0)
    } else {
        Ok(source.lines().count())
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map_err(|error| format!("failed to read '{}': {error}", path.display()))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
