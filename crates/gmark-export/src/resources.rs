// @author kongweiguang

//! Resource-card projection, copy-on-export, and cleanup ownership.

use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context as _, anyhow};
use gmark_markdown::{ResourceKind, ResourceLocation, ResourceRecord, escape_html};

use crate::ExportCancellation;
use crate::markup::{is_closing_fence, opening_fence};

/// The source projection and filesystem objects created beside an HTML export.
/// Existing matching resources are deliberately not owned by this value.
#[derive(Debug)]
pub struct PreparedHtmlResources {
    /// Markdown with standalone resource cards replaced by safe static HTML.
    pub markdown: String,
    /// Files created during this export; callers may inspect these for progress
    /// or remove them after a failed atomic document write.
    pub created_files: Vec<PathBuf>,
    /// Directory created exclusively for this export, when applicable.
    pub created_asset_dir: Option<PathBuf>,
}

impl PreparedHtmlResources {
    /// Removes only resources created for this export. Pre-existing identical
    /// resources remain untouched, and directory removal succeeds only when it
    /// became empty after owned files were removed.
    pub fn cleanup_created(&self) {
        for path in &self.created_files {
            let _ = fs::remove_file(path);
        }
        if let Some(directory) = self.created_asset_dir.as_deref() {
            let _ = fs::remove_dir(directory);
        }
    }
}

/// Copies local resource-card targets beside `export_path`, preserving the
/// source as a static card projection. See [`PreparedHtmlResources`] for
/// cleanup ownership.
pub fn prepare_html_resources<C: ExportCancellation + ?Sized>(
    markdown: &str,
    source_base_dir: Option<&Path>,
    export_path: &Path,
    cancelled: &C,
) -> anyhow::Result<PreparedHtmlResources> {
    prepare_html_resources_with_progress(markdown, source_base_dir, export_path, cancelled, None)
}

/// Resource-copy variant with an optional completed-local-card counter. The
/// counter stays deliberately narrow so UI shells do not leak into the engine.
pub fn prepare_html_resources_with_progress<C: ExportCancellation + ?Sized>(
    markdown: &str,
    source_base_dir: Option<&Path>,
    export_path: &Path,
    cancelled: &C,
    completed: Option<&AtomicUsize>,
) -> anyhow::Result<PreparedHtmlResources> {
    let export_directory = export_path
        .parent()
        .ok_or_else(|| anyhow!("HTML export path has no parent directory"))?;
    let stem = export_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("untitled");
    let asset_directory = export_directory.join(format!("{stem}.assets"));
    let mut created_files = Vec::new();
    let mut created_asset_dir = None;
    let mut rewritten = Vec::with_capacity(markdown.lines().count());
    let mut active_fence = None;

    let result = (|| {
        for line in markdown.split('\n') {
            if cancelled.is_cancelled() {
                return Err(anyhow!("export cancelled"));
            }
            if let Some((marker, length)) = active_fence {
                rewritten.push(line.to_owned());
                if is_closing_fence(line, marker, length) {
                    active_fence = None;
                }
                continue;
            }
            if let Some(fence) = opening_fence(line) {
                active_fence = Some(fence);
                rewritten.push(line.to_owned());
                continue;
            }
            let Some((prefix, record)) = standalone_resource_line(line, source_base_dir) else {
                rewritten.push(line.to_owned());
                continue;
            };

            let (href, status) = match &record.location {
                ResourceLocation::Local(path) if path.is_file() => {
                    if !asset_directory.exists() {
                        fs::create_dir_all(&asset_directory).with_context(|| {
                            format!("failed to create '{}'", asset_directory.display())
                        })?;
                        created_asset_dir = Some(asset_directory.clone());
                    }
                    let file_name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .filter(|name| !name.is_empty())
                        .unwrap_or("resource");
                    let preferred = asset_directory.join(file_name);
                    let (target, reused) = if preferred.exists() {
                        // 附件可能很大，相同内容比较不能同时整文件读入内存。
                        if files_have_same_contents(path, &preferred).unwrap_or(false) {
                            (preferred, true)
                        } else {
                            (unique_export_asset_path(&asset_directory, file_name), false)
                        }
                    } else {
                        (preferred, false)
                    };
                    if !reused {
                        copy_export_asset_cancellable(path, &target, cancelled)?;
                        created_files.push(target.clone());
                    }
                    let href = relative_export_asset_href(
                        asset_directory
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("assets"),
                        target
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("resource"),
                    );
                    (Some(href), "就绪")
                }
                ResourceLocation::Local(_) => (None, "文件不存在"),
                ResourceLocation::Url(_) if record.is_unsafe_url() => (None, "协议不支持"),
                ResourceLocation::Url(url) => (Some(url.to_string()), "在线"),
            };
            if matches!(record.location, ResourceLocation::Local(_))
                && let Some(completed) = completed
            {
                completed.fetch_add(1, Ordering::Release);
            }
            rewritten.push(format!(
                "{prefix}{}",
                render_resource_card_html(&record, href.as_deref(), status)
            ));
        }
        Ok::<(), anyhow::Error>(())
    })();

    if let Err(error) = result {
        for path in &created_files {
            let _ = fs::remove_file(path);
        }
        if let Some(directory) = created_asset_dir.as_deref() {
            let _ = fs::remove_dir(directory);
        }
        return Err(error);
    }

    Ok(PreparedHtmlResources {
        markdown: rewritten.join("\n"),
        created_files,
        created_asset_dir,
    })
}

/// Counts local standalone resource cards while ignoring fenced snippets.
pub fn count_local_resource_cards(markdown: &str, source_base_dir: Option<&Path>) -> usize {
    let mut active_fence = None;
    markdown
        .split('\n')
        .filter_map(|line| {
            if let Some((marker, length)) = active_fence {
                if is_closing_fence(line, marker, length) {
                    active_fence = None;
                }
                return None;
            }
            if let Some(fence) = opening_fence(line) {
                active_fence = Some(fence);
                return None;
            }
            let (_, resource) = standalone_resource_line(line, source_base_dir)?;
            matches!(resource.location, ResourceLocation::Local(_)).then_some(())
        })
        .count()
}

/// Streams a resource copy without overwriting an existing path. A cancelled
/// copy removes its own partial output while preserving any prior target.
pub fn copy_export_asset_cancellable<C: ExportCancellation + ?Sized>(
    source: &Path,
    target: &Path,
    cancelled: &C,
) -> anyhow::Result<()> {
    let mut target_created = false;
    let result = (|| {
        let source_file = File::open(source)
            .with_context(|| format!("failed to open resource '{}'", source.display()))?;
        // exists 仅做确定命名，create_new 才是禁止竞态覆盖的最终防线。
        let target_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .with_context(|| format!("failed to create resource '{}'", target.display()))?;
        target_created = true;
        let mut reader = BufReader::new(source_file);
        let mut writer = BufWriter::new(target_file);
        let mut buffer = vec![0_u8; 256 * 1024];
        loop {
            if cancelled.is_cancelled() {
                return Err(anyhow!("export cancelled"));
            }
            let read = reader
                .read(&mut buffer)
                .with_context(|| format!("failed to read resource '{}'", source.display()))?;
            if read == 0 {
                break;
            }
            writer
                .write_all(&buffer[..read])
                .with_context(|| format!("failed to copy resource to '{}'", target.display()))?;
        }
        writer
            .flush()
            .with_context(|| format!("failed to finish resource '{}'", target.display()))
    })();
    if result.is_err() && target_created {
        let _ = fs::remove_file(target);
    }
    result
}

pub(crate) fn rewrite_standalone_resource_cards(markdown: &str, base_dir: Option<&Path>) -> String {
    let mut active_fence = None;
    let mut rewritten = Vec::with_capacity(markdown.lines().count());
    for line in markdown.split('\n') {
        if let Some((marker, length)) = active_fence {
            rewritten.push(line.to_owned());
            if is_closing_fence(line, marker, length) {
                active_fence = None;
            }
            continue;
        }
        if let Some(fence) = opening_fence(line) {
            active_fence = Some(fence);
            rewritten.push(line.to_owned());
            continue;
        }
        let replacement = standalone_resource_line(line, base_dir).map(|(prefix, record)| {
            let status = match &record.location {
                ResourceLocation::Local(path) if path.is_file() => "就绪",
                ResourceLocation::Local(_) => "文件不存在",
                ResourceLocation::Url(_) if record.is_unsafe_url() => "协议不支持",
                ResourceLocation::Url(_) => "在线",
            };
            let href = match &record.location {
                ResourceLocation::Url(url) if !record.is_unsafe_url() => Some(url.to_string()),
                _ => None,
            };
            format!(
                "{prefix}{}",
                render_resource_card_html(&record, href.as_deref(), status)
            )
        });
        rewritten.push(replacement.unwrap_or_else(|| line.to_owned()));
    }
    rewritten.join("\n")
}

pub(crate) fn standalone_resource_line(
    line: &str,
    base_dir: Option<&Path>,
) -> Option<(String, ResourceRecord)> {
    let indent_length = line.len() - line.trim_start().len();
    let indent = &line[..indent_length];
    let rest = &line[indent_length..];
    let (prefix_length, content) = resource_container_prefix(rest);
    let record = ResourceRecord::parse(content.trim(), base_dir)?;
    Some((format!("{indent}{}", &rest[..prefix_length]), record))
}

fn resource_container_prefix(line: &str) -> (usize, &str) {
    if let Some(after_marker) = line.strip_prefix('>') {
        let whitespace_length = after_marker.len() - after_marker.trim_start().len();
        if whitespace_length > 0 {
            let mut prefix_length = 1 + whitespace_length;
            let content = &line[prefix_length..];
            if content.starts_with("[!")
                && let Some(close) = content.find(']')
            {
                let after_header = &content[close + 1..];
                let separator_length = after_header.len() - after_header.trim_start().len();
                if separator_length > 0 {
                    prefix_length += close + 1 + separator_length;
                }
            }
            return (prefix_length, &line[prefix_length..]);
        }
    }
    if matches!(line.as_bytes().first(), Some(b'-' | b'*' | b'+')) {
        let whitespace_length = line[1..].len() - line[1..].trim_start().len();
        if whitespace_length > 0 {
            let mut prefix_length = 1 + whitespace_length;
            let content = &line[prefix_length..];
            if matches!(content.get(..3), Some("[ ]" | "[x]" | "[X]"))
                && content
                    .as_bytes()
                    .get(3)
                    .is_some_and(u8::is_ascii_whitespace)
            {
                let separator_length = content[3..].len() - content[3..].trim_start().len();
                prefix_length += 3 + separator_length;
            }
            return (prefix_length, &line[prefix_length..]);
        }
    }
    let digit_length = line
        .as_bytes()
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digit_length > 0
        && matches!(line.as_bytes().get(digit_length), Some(b'.' | b')'))
        && line
            .as_bytes()
            .get(digit_length + 1)
            .is_some_and(u8::is_ascii_whitespace)
    {
        let whitespace_start = digit_length + 1;
        let whitespace_length =
            line[whitespace_start..].len() - line[whitespace_start..].trim_start().len();
        let prefix_length = whitespace_start + whitespace_length;
        return (prefix_length, &line[prefix_length..]);
    }
    (0, line)
}

fn render_resource_card_html(record: &ResourceRecord, href: Option<&str>, status: &str) -> String {
    let kind = match record.kind {
        ResourceKind::File => "FILE",
        ResourceKind::Video => "VIDEO",
    };
    let target = match &record.location {
        ResourceLocation::Local(path) => path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("resource"),
        ResourceLocation::Url(url) => url.host_str().unwrap_or(url.as_str()),
    };
    let content = format!(
        "<span class=\"gmark-resource-kind\">{}</span><span class=\"gmark-resource-main\"><strong>{}</strong><small>{}</small></span><span class=\"gmark-resource-status\">{}</span>",
        escape_html(kind),
        escape_html(&record.label),
        escape_html(target),
        escape_html(status),
    );
    match href.filter(|value| !value.is_empty()) {
        Some(value) => format!(
            "<a class=\"gmark-resource-card\" href=\"{}\">{content}</a>",
            escape_html(value)
        ),
        None => format!("<div class=\"gmark-resource-card\">{content}</div>"),
    }
}

fn unique_export_asset_path(directory: &Path, preferred_name: &str) -> PathBuf {
    let preferred = Path::new(preferred_name);
    let stem = preferred
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("resource");
    let extension = preferred.extension().and_then(|value| value.to_str());
    for index in 0.. {
        let name = if index == 0 {
            preferred_name.to_owned()
        } else if let Some(extension) = extension {
            format!("{stem}{index}.{extension}")
        } else {
            format!("{stem}{index}")
        };
        let candidate = directory.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("unbounded asset name search should always return")
}

fn files_have_same_contents(source: &Path, target: &Path) -> std::io::Result<bool> {
    if fs::metadata(source)?.len() != fs::metadata(target)?.len() {
        return Ok(false);
    }
    let mut source = BufReader::new(File::open(source)?);
    let mut target = BufReader::new(File::open(target)?);
    let mut source_buffer = [0_u8; 64 * 1024];
    let mut target_buffer = [0_u8; 64 * 1024];
    loop {
        let source_length = source.read(&mut source_buffer)?;
        let target_length = target.read(&mut target_buffer)?;
        if source_length != target_length
            || source_buffer[..source_length] != target_buffer[..target_length]
        {
            return Ok(false);
        }
        if source_length == 0 {
            return Ok(true);
        }
    }
}

fn relative_export_asset_href(asset_directory_name: &str, file_name: &str) -> String {
    let Ok(mut url) = url::Url::parse("https://gmark.invalid/") else {
        return format!("./{asset_directory_name}/{file_name}");
    };
    let Ok(mut segments) = url.path_segments_mut() else {
        return format!("./{asset_directory_name}/{file_name}");
    };
    segments.push(asset_directory_name).push(file_name);
    drop(segments);
    format!(".{}", url.path())
}
