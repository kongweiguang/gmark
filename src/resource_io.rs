// @author kongweiguang

//! Filesystem adapter for resource insertion.
//!
//! The Markdown/resource domain types are pure. This module owns the small
//! amount of filesystem work needed to materialize a selected local file and
//! to produce a portable Markdown target for it.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};

use crate::components::{ResourceKind, ResourceRecord};
use crate::preferences::ResourceInsertBehavior;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MaterializedResource {
    pub(crate) path: PathBuf,
    pub(crate) created: bool,
}

impl MaterializedResource {
    /// Removes only a copy created by the current insertion attempt. Existing
    /// source files and reused same-directory resources are never touched.
    pub(crate) fn cleanup_if_created(&self) {
        if self.created {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub(crate) fn materialize_local_resource(
    source: &Path,
    document_path: Option<&Path>,
    behavior: ResourceInsertBehavior,
) -> Result<MaterializedResource> {
    if !source.is_file() {
        return Err(anyhow!("resource source is not a regular file"));
    }
    let Some(document_path) = document_path else {
        if behavior != ResourceInsertBehavior::None {
            return Err(anyhow!(
                "save the Markdown document before copying resources"
            ));
        }
        return Ok(MaterializedResource {
            path: source.to_path_buf(),
            created: false,
        });
    };

    let root = document_path
        .parent()
        .ok_or_else(|| anyhow!("Markdown document has no parent directory"))?;
    let target_dir = match behavior {
        ResourceInsertBehavior::None | ResourceInsertBehavior::CopyToDocumentFolder => {
            root.to_path_buf()
        }
        ResourceInsertBehavior::CopyToAssetsFolder => root.join("assets"),
        ResourceInsertBehavior::CopyToNamedAssetsFolder => named_assets_dir(root, document_path),
    };

    if behavior == ResourceInsertBehavior::None || same_directory(source, &target_dir) {
        return Ok(MaterializedResource {
            path: source.to_path_buf(),
            created: false,
        });
    }

    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("resource source has no valid file name"))?;
    fs::create_dir_all(&target_dir).with_context(|| {
        format!(
            "failed to create resource directory '{}'",
            target_dir.display()
        )
    })?;
    let target = copy_without_overwrite(source, &target_dir, name)?;
    Ok(MaterializedResource {
        path: target,
        created: true,
    })
}

pub(crate) fn resource_markdown_for_path(
    label: &str,
    source: &Path,
    document_path: Option<&Path>,
    behavior: ResourceInsertBehavior,
    explicit_kind: Option<ResourceKind>,
) -> Result<(String, MaterializedResource)> {
    let materialized = materialize_local_resource(source, document_path, behavior)?;
    let target = match markdown_target(document_path, &materialized.path) {
        Ok(target) => target,
        Err(error) => {
            materialized.cleanup_if_created();
            return Err(error);
        }
    };
    let label = if label.trim().is_empty() {
        source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("resource")
            .to_owned()
    } else {
        label.to_owned()
    };
    if is_image_path(source) {
        return Ok((
            format!(
                "![{}]({})",
                escape_label(&label),
                escape_destination(&target)
            ),
            materialized,
        ));
    }
    let explicit_kind =
        explicit_kind.or_else(|| is_video_path(source).then_some(ResourceKind::Video));
    let record = ResourceRecord::from_parts(
        label,
        target,
        explicit_kind,
        document_path.and_then(Path::parent),
    );
    Ok((record.to_markdown(), materialized))
}

fn is_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "bmp" | "gif" | "jpeg" | "jpg" | "png" | "svg" | "tif" | "tiff" | "webp"
    )
}

fn is_video_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "avi" | "m4v" | "mkv" | "mov" | "mp4" | "mpeg" | "mpg" | "ogv" | "webm" | "wmv"
    )
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn escape_destination(value: &str) -> String {
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '(' | ')' | '"'))
    {
        format!(
            "<{}>",
            value
                .replace('\\', "\\\\")
                .replace('<', "\\<")
                .replace('>', "\\>")
        )
    } else {
        value
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)")
            .replace('<', "\\<")
            .replace('>', "\\>")
    }
}

pub(crate) fn markdown_target(document_path: Option<&Path>, path: &Path) -> Result<String> {
    if let Some(document_path) = document_path
        && let Some(root) = document_path.parent()
        && let Ok(relative) = path.strip_prefix(root)
    {
        return Ok(format!("./{}", markdown_path_string(relative)?));
    }
    markdown_path_string(path)
}

#[cfg(test)]
pub(crate) fn unique_file_path(dir: &Path, preferred_name: &str) -> PathBuf {
    for index in 0.. {
        let candidate = resource_candidate_path(dir, preferred_name, index);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("deterministic resource name search is unbounded")
}

fn resource_candidate_path(dir: &Path, preferred_name: &str, index: usize) -> PathBuf {
    let preferred = Path::new(preferred_name);
    let stem = preferred
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("resource");
    let extension = preferred.extension().and_then(|ext| ext.to_str());
    let name = if index == 0 {
        preferred_name.to_owned()
    } else if let Some(extension) = extension {
        format!("{stem}-{index}.{extension}")
    } else {
        format!("{stem}-{index}")
    };
    dir.join(name)
}

fn copy_without_overwrite(source: &Path, dir: &Path, preferred_name: &str) -> Result<PathBuf> {
    let mut input = File::open(source)
        .with_context(|| format!("failed to open resource '{}' for copying", source.display()))?;
    for index in 0.. {
        let target = resource_candidate_path(dir, preferred_name, index);
        let mut output = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(output) => output,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to create resource copy '{}'", target.display())
                });
            }
        };

        // `create_new` closes the exists/copy race: a concurrent insertion can
        // only claim this candidate, never be truncated by the current copy.
        if let Err(error) = io::copy(&mut input, &mut output) {
            drop(output);
            let _ = fs::remove_file(&target);
            return Err(error).with_context(|| {
                format!(
                    "failed to copy resource '{}' to '{}'",
                    source.display(),
                    target.display()
                )
            });
        }
        return Ok(target);
    }
    unreachable!("deterministic resource copy search is unbounded")
}

/// Opens a local resource with the platform default application. Arguments
/// stay structured so a resource path is never interpolated into a shell
/// command string.
pub(crate) fn open_local_resource(path: &Path) -> Result<()> {
    let mut command = platform_open_command(path);
    command
        .spawn()
        .with_context(|| format!("failed to open '{}' with the system", path.display()))?;
    Ok(())
}

/// Reveals a local resource in the platform file manager.
pub(crate) fn reveal_local_resource(path: &Path) -> Result<()> {
    let mut command = platform_reveal_command(path);
    command
        .spawn()
        .with_context(|| format!("failed to reveal '{}' in the file manager", path.display()))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn platform_open_command(path: &Path) -> Command {
    let mut command = Command::new("explorer.exe");
    command.arg(path);
    command
}

#[cfg(target_os = "macos")]
fn platform_open_command(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg("--").arg(path);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_open_command(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path);
    command
}

#[cfg(target_os = "windows")]
fn platform_reveal_command(path: &Path) -> Command {
    let mut command = Command::new("explorer.exe");
    command.arg("/select,").arg(path);
    command
}

#[cfg(target_os = "macos")]
fn platform_reveal_command(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg("-R").arg(path);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_reveal_command(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path.parent().unwrap_or(path));
    command
}

fn named_assets_dir(root: &Path, document_path: &Path) -> PathBuf {
    let stem = document_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("untitled");
    root.join(format!("{stem}.assets"))
}

fn same_directory(path: &Path, directory: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf())
        == directory
            .canonicalize()
            .unwrap_or_else(|_| directory.to_path_buf())
}

fn markdown_path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(|path| path.replace('\\', "/"))
        .ok_or_else(|| anyhow!("resource path is not valid Unicode: '{}'", path.display()))
}

#[cfg(test)]
#[path = "../tests/unit/resource_io.rs"]
mod tests;
