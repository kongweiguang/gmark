// @author kongweiguang

//! gmark 仓库级质量门禁。

mod architecture;
mod metadata;
mod quality;
mod source;
mod ui_colors;
mod updater_e2e;

use std::path::{Path, PathBuf};

pub use updater_e2e::{
    DecisionPlan, E2ePaths, ParsedUpdaterE2eArgs, UnsavedDecision, UpdaterE2eOptions,
    decision_plan, parse_ack_version, parse_args as parse_updater_e2e_args, parse_timeout,
    resolve_paths, version_is_newer,
};

/// 执行一个质量子命令。
pub fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let command = arguments.first().map(String::as_str).unwrap_or("quality");
    let root = repository_root()?;
    run_at_args(&root, command, arguments.get(1..).unwrap_or(&[]))
}

/// 在指定仓库根目录执行门禁，供 fixture integration tests 使用。
pub fn run_at(root: &Path, command: &str) -> Result<(), String> {
    run_at_args(root, command, &[])
}

/// 在指定仓库根目录执行门禁，并把子命令参数传给对应实现。
pub fn run_at_args(root: &Path, command: &str, arguments: &[String]) -> Result<(), String> {
    match command {
        "source-size" => quality::check_source_size(root),
        "architecture" => architecture::check(root),
        "test-layout" => quality::check_test_layout(root),
        "authors" => quality::check_authors(root),
        "ui-colors" => ui_colors::check(root),
        "quality" => check_all(root),
        "updater-e2e" => updater_e2e::run_at(root, arguments),
        _ => Err(format!(
            "unknown xtask command '{command}'; expected source-size, architecture, test-layout, authors, ui-colors, quality, or updater-e2e"
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
        println!("Gmark quality gates passed");
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
