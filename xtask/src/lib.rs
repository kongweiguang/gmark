// @author kongweiguang

//! gmark 仓库级质量门禁。

mod architecture;
mod metadata;
mod quality;
mod source;
mod ui_colors;

use std::path::{Path, PathBuf};

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
        "source-size" => quality::check_source_size(root),
        "architecture" => architecture::check(root),
        "test-layout" => quality::check_test_layout(root),
        "authors" => quality::check_authors(root),
        "ui-colors" => ui_colors::check(root),
        "quality" => check_all(root),
        _ => Err(format!(
            "unknown xtask command '{command}'; expected source-size, architecture, test-layout, authors, ui-colors, or quality"
        )),
    }
}

fn check_all(root: &Path) -> Result<(), String> {
    let mut failures = Vec::new();
    for (name, check) in [
        (
            "source-size",
            quality::check_source_size as fn(&Path) -> Result<(), String>,
        ),
        ("architecture", architecture::check),
        ("test-layout", quality::check_test_layout),
        ("authors", quality::check_authors),
        ("ui-colors", ui_colors::check),
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

fn repository_root() -> Result<PathBuf, String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask manifest has no repository parent".to_owned())
}
