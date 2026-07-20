// @author kongweiguang

//! Privacy-preserving local panic reports.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, bail};
use serde::Serialize;

use crate::config::GmarkConfigDirs;

const REPORT_SCHEMA: u32 = 1;
const REPORT_LIMIT: usize = 20;
const MAX_REPORT_BYTES: usize = 16 * 1024;
static REPORT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize)]
struct CrashReport<'a> {
    schema: u32,
    created_at_unix_ms: u128,
    app_version: &'static str,
    target_os: &'static str,
    target_arch: &'static str,
    process_id: u32,
    thread_class: &'a str,
    panic: PanicMetadata<'a>,
}

#[derive(Debug, Serialize)]
struct PanicMetadata<'a> {
    payload_kind: &'a str,
    source_file: Option<&'a str>,
    line: Option<u32>,
    column: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
struct ReportInput<'a> {
    payload_kind: &'a str,
    source_file: Option<&'a str>,
    line: Option<u32>,
    column: Option<u32>,
    thread_class: &'a str,
}

/// Installs the panic hook before GPUI or user configuration is initialized.
///
/// 报告默认只保存在本地，不含 panic 文本与 backtrace。两者都可能嵌入文档片段或用户路径，
/// 因此不能在用户审阅前进入持久化诊断数据。
pub(crate) fn install() -> anyhow::Result<()> {
    let report_dir = GmarkConfigDirs::from_system()?.crash_reports_dir();
    install_with_dir(report_dir)
}

/// Creates the local report directory and opens it with the platform file manager.
pub(crate) fn open_reports_directory() -> anyhow::Result<()> {
    let report_dir = GmarkConfigDirs::from_system()?.crash_reports_dir();
    open_reports_directory_at(&report_dir)
}

fn open_reports_directory_at(report_dir: &Path) -> anyhow::Result<()> {
    ensure_reports_directory(report_dir)?;
    let mut command = folder_open_command(report_dir);
    command.spawn().with_context(|| {
        format!(
            "failed to open crash report directory '{}'",
            report_dir.display()
        )
    })?;
    Ok(())
}

fn ensure_reports_directory(report_dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(report_dir).with_context(|| {
        format!(
            "failed to create crash report directory '{}'",
            report_dir.display()
        )
    })
}

fn folder_open_command(report_dir: &Path) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("explorer.exe");
        command.arg(report_dir);
        command
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg("--").arg(report_dir);
        command
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = Command::new("xdg-open");
        // 配置目录始终是绝对路径，不会被 xdg-open 当成命令行选项。
        command.arg(report_dir);
        command
    }
}

fn install_with_dir(report_dir: PathBuf) -> anyhow::Result<()> {
    prune_reports(&report_dir, REPORT_LIMIT)?;

    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info.location();
        let source_file = location.and_then(|location| {
            Path::new(location.file())
                .file_name()
                .and_then(|name| name.to_str())
        });
        let payload_kind = if info.payload().is::<&str>() {
            "str"
        } else if info.payload().is::<String>() {
            "String"
        } else {
            "non-string"
        };
        let current_thread = std::thread::current();
        let thread_class = match current_thread.name() {
            Some("main") => "main",
            Some(_) => "named-worker",
            None => "unnamed-worker",
        };
        let input = ReportInput {
            payload_kind,
            source_file,
            line: location.map(std::panic::Location::line),
            column: location.map(std::panic::Location::column),
            thread_class,
        };
        let _ = write_report(&report_dir, input);
        previous_hook(info);
    }));
    Ok(())
}

fn write_report(report_dir: &Path, input: ReportInput<'_>) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(report_dir).with_context(|| {
        format!(
            "failed to create crash report directory '{}'",
            report_dir.display()
        )
    })?;

    let created_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let sequence = REPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let stem = format!(
        "crash-{created_at_unix_ms}-{}-{sequence}",
        std::process::id()
    );
    let final_path = report_dir.join(format!("{stem}.json"));
    let temporary_path = report_dir.join(format!(".{stem}.tmp"));
    let report = CrashReport {
        schema: REPORT_SCHEMA,
        created_at_unix_ms,
        app_version: env!("CARGO_PKG_VERSION"),
        target_os: std::env::consts::OS,
        target_arch: std::env::consts::ARCH,
        process_id: std::process::id(),
        thread_class: input.thread_class,
        panic: PanicMetadata {
            payload_kind: input.payload_kind,
            source_file: input.source_file,
            line: input.line,
            column: input.column,
        },
    };
    let mut bytes =
        serde_json::to_vec_pretty(&report).context("failed to serialize crash report")?;
    bytes.push(b'\n');
    if bytes.len() > MAX_REPORT_BYTES {
        bail!("crash report exceeded the local size limit");
    }

    let write_result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary_path, &final_path)
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error)
            .with_context(|| format!("failed to commit crash report '{}'", final_path.display()));
    }

    let _ = prune_reports(report_dir, REPORT_LIMIT);
    Ok(final_path)
}

fn prune_reports(report_dir: &Path, limit: usize) -> anyhow::Result<()> {
    let entries = match fs::read_dir(report_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to read crash report directory '{}'",
                    report_dir.display()
                )
            });
        }
    };

    let mut reports = Vec::new();
    for entry in entries {
        let entry = entry.context("failed to enumerate crash report directory")?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with(".crash-") && name.ends_with(".tmp") {
            let _ = fs::remove_file(path);
            continue;
        }
        if !name.starts_with("crash-")
            || path.extension().and_then(|value| value.to_str()) != Some("json")
        {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        reports.push((modified, name.to_owned(), path));
    }
    reports.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let remove_count = reports.len().saturating_sub(limit);
    for (_, _, path) in reports.into_iter().take(remove_count) {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/crash_report.rs"]
mod tests;
