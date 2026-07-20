// @author kongweiguang

//! Transactional workspace rename and move planning.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context as _, Result, anyhow, bail};
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag};
use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WorkspaceCreateKind {
    MarkdownFile,
    Directory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkspaceCreatePlan {
    pub(super) root: PathBuf,
    pub(super) path: PathBuf,
    pub(super) kind: WorkspaceCreateKind,
    initial_bytes: Vec<u8>,
}

impl WorkspaceCreatePlan {
    pub(super) fn execute(&self) -> Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| anyhow!("created path has no parent"))?;
        let resolved_parent = dunce::canonicalize(parent).with_context(|| {
            format!("failed to resolve parent directory '{}'", parent.display())
        })?;
        ensure_within_root(&self.root, &resolved_parent, "creation parent")?;
        if self.path.exists() {
            bail!("destination already exists: '{}'", self.path.display());
        }
        match self.kind {
            WorkspaceCreateKind::MarkdownFile => {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&self.path)
                    .with_context(|| {
                        format!("failed to create Markdown file '{}'", self.path.display())
                    })?;
                file.write_all(&self.initial_bytes)?;
                file.flush()?;
                file.sync_all()?;
            }
            WorkspaceCreateKind::Directory => fs::create_dir(&self.path)
                .with_context(|| format!("failed to create directory '{}'", self.path.display()))?,
        }
        Ok(())
    }

    pub(super) fn undo(&self) -> Result<()> {
        let metadata = fs::symlink_metadata(&self.path)
            .with_context(|| format!("created path no longer exists: '{}'", self.path.display()))?;
        if metadata.file_type().is_symlink() {
            bail!("created path was replaced by a symbolic link");
        }
        match self.kind {
            WorkspaceCreateKind::MarkdownFile => {
                if !metadata.is_file() || fs::read(&self.path)? != self.initial_bytes {
                    bail!("created file has changed and cannot be removed safely");
                }
                fs::remove_file(&self.path).with_context(|| {
                    format!("failed to remove created file '{}'", self.path.display())
                })?;
            }
            WorkspaceCreateKind::Directory => {
                if !metadata.is_dir() || fs::read_dir(&self.path)?.next().is_some() {
                    bail!("created directory is not empty and cannot be removed safely");
                }
                fs::remove_dir(&self.path).with_context(|| {
                    format!(
                        "failed to remove created directory '{}'",
                        self.path.display()
                    )
                })?;
            }
        }
        Ok(())
    }
}

pub(super) fn plan_workspace_create(
    root: &Path,
    parent: &Path,
    name: &str,
    kind: WorkspaceCreateKind,
) -> Result<WorkspaceCreatePlan> {
    let root = dunce::canonicalize(root)
        .with_context(|| format!("failed to resolve workspace root '{}'", root.display()))?;
    let parent = dunce::canonicalize(parent)
        .with_context(|| format!("failed to resolve creation parent '{}'", parent.display()))?;
    ensure_within_root(&root, &parent, "creation parent")?;
    let mut components = Path::new(name).components();
    let Some(Component::Normal(file_name)) = components.next() else {
        bail!("name must contain one file-system component");
    };
    if components.next().is_some() || file_name.is_empty() {
        bail!("name must contain one file-system component");
    }
    let path = parent.join(file_name);
    if path.exists() {
        bail!("destination already exists: '{}'", path.display());
    }
    if kind == WorkspaceCreateKind::MarkdownFile && !super::workspace::is_markdown_file(&path) {
        bail!("new files must use the .md or .markdown extension");
    }
    Ok(WorkspaceCreatePlan {
        root,
        path,
        kind,
        initial_bytes: Vec::new(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkspaceFileRewrite {
    pub(super) before_path: PathBuf,
    pub(super) after_path: PathBuf,
    pub(super) before: Vec<u8>,
    pub(super) after: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkspaceMovePlan {
    pub(super) root: PathBuf,
    pub(super) source: PathBuf,
    pub(super) destination: PathBuf,
    pub(super) rewrites: Vec<WorkspaceFileRewrite>,
}

impl WorkspaceMovePlan {
    pub(super) fn reversed(&self) -> Self {
        Self {
            root: self.root.clone(),
            source: self.destination.clone(),
            destination: self.source.clone(),
            rewrites: self
                .rewrites
                .iter()
                .map(|rewrite| WorkspaceFileRewrite {
                    before_path: rewrite.after_path.clone(),
                    after_path: rewrite.before_path.clone(),
                    before: rewrite.after.clone(),
                    after: rewrite.before.clone(),
                })
                .collect(),
        }
    }

    pub(super) fn execute(&self) -> Result<()> {
        self.validate_snapshot()?;
        fs::rename(&self.source, &self.destination).with_context(|| {
            format!(
                "failed to move '{}' to '{}'",
                self.source.display(),
                self.destination.display()
            )
        })?;

        let mut committed = Vec::new();
        for rewrite in &self.rewrites {
            if let Err(error) = gmark_document::atomic_write(&rewrite.after_path, &rewrite.after) {
                let rollback = self.rollback_after_write_failure(&committed);
                return match rollback {
                    Ok(()) => Err(error.into()),
                    Err(rollback_error) => Err(anyhow!(
                        "workspace move failed: {error}; rollback also failed: {rollback_error}"
                    )),
                };
            }
            committed.push(rewrite);
        }
        Ok(())
    }

    fn validate_snapshot(&self) -> Result<()> {
        if !self.source.exists() {
            bail!("source no longer exists: '{}'", self.source.display());
        }
        if self.destination.exists() {
            bail!(
                "destination already exists: '{}'",
                self.destination.display()
            );
        }
        for rewrite in &self.rewrites {
            let current = fs::read(&rewrite.before_path).with_context(|| {
                format!(
                    "failed to verify changed file '{}'",
                    rewrite.before_path.display()
                )
            })?;
            if current != rewrite.before {
                bail!(
                    "file changed after the move was planned: '{}'",
                    rewrite.before_path.display()
                );
            }
        }
        Ok(())
    }

    fn rollback_after_write_failure(&self, committed: &[&WorkspaceFileRewrite]) -> Result<()> {
        let mut failures = Vec::new();
        for rewrite in committed.iter().rev() {
            if let Err(error) = gmark_document::atomic_write(&rewrite.after_path, &rewrite.before) {
                failures.push(error.to_string());
            }
        }
        if let Err(error) = fs::rename(&self.destination, &self.source) {
            failures.push(format!("rename rollback failed: {error}"));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            bail!(failures.join("; "))
        }
    }
}

pub(super) fn plan_workspace_move(
    root: &Path,
    source: &Path,
    destination: &Path,
) -> Result<WorkspaceMovePlan> {
    let root = dunce::canonicalize(root)
        .with_context(|| format!("failed to resolve workspace root '{}'", root.display()))?;
    let source = dunce::canonicalize(source)
        .with_context(|| format!("failed to resolve source '{}'", source.display()))?;
    ensure_within_root(&root, &source, "source")?;
    if source == root {
        bail!("the workspace root cannot be moved");
    }

    if destination.exists() {
        bail!("destination already exists: '{}'", destination.display());
    }
    let destination_parent_source = destination
        .parent()
        .ok_or_else(|| anyhow!("destination has no parent directory"))?;
    let destination_parent = dunce::canonicalize(destination_parent_source).with_context(|| {
        format!(
            "failed to resolve destination parent for '{}'",
            destination.display()
        )
    })?;
    ensure_within_root(&root, &destination_parent, "destination")?;
    let destination_name = destination
        .file_name()
        .ok_or_else(|| anyhow!("destination has no file name"))?;
    let destination = destination_parent.join(destination_name);
    if destination.starts_with(&source) {
        bail!("a directory cannot be moved into itself");
    }

    let mut rewrites = Vec::new();
    for path in markdown_files(&root) {
        let before = fs::read(&path)
            .with_context(|| format!("failed to read Markdown file '{}'", path.display()))?;
        let after_path = map_moved_path(&path, &source, &destination);
        let after = rewrite_markdown_links(&before, &path, &after_path, &source, &destination)?;
        if after != before {
            rewrites.push(WorkspaceFileRewrite {
                before_path: path,
                after_path,
                before,
                after,
            });
        }
    }

    Ok(WorkspaceMovePlan {
        root,
        source,
        destination,
        rewrites,
    })
}

fn ensure_within_root(root: &Path, path: &Path, label: &str) -> Result<()> {
    if !path.starts_with(root) {
        bail!("{label} escapes the workspace root: '{}'", path.display());
    }
    Ok(())
}

fn markdown_files(root: &Path) -> Vec<PathBuf> {
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_exclude(true)
        .require_git(false)
        .parents(true)
        .build()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .map(|entry| entry.into_path())
        .filter(|path| super::workspace::is_markdown_file(path))
        .collect()
}

pub(super) fn map_moved_path(path: &Path, source: &Path, destination: &Path) -> PathBuf {
    if path == source {
        return destination.to_path_buf();
    }
    path.strip_prefix(source)
        .map(|suffix| destination.join(suffix))
        .unwrap_or_else(|_| path.to_path_buf())
}

pub(super) fn canonicalize_workspace_path(path: &Path) -> io::Result<PathBuf> {
    dunce::canonicalize(path)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DestinationSpan {
    range: std::ops::Range<usize>,
    parsed: String,
}

fn rewrite_markdown_links(
    bytes: &[u8],
    before_path: &Path,
    after_path: &Path,
    moved_source: &Path,
    moved_destination: &Path,
) -> Result<Vec<u8>> {
    let decoded = decode_preserving_encoding(bytes)?;
    let source = decoded.text.as_str();
    let mut spans = markdown_destination_spans(source);
    spans.sort_by_key(|span| span.range.start);
    spans.dedup_by(|left, right| left.range == right.range);

    let mut replacements = Vec::new();
    for span in spans {
        let raw_destination = &source[span.range.clone()];
        let Some(rewritten) = rewrite_destination(
            &span.parsed,
            before_path,
            after_path,
            moved_source,
            moved_destination,
        ) else {
            continue;
        };
        if rewritten != raw_destination {
            replacements.push((span.range, rewritten));
        }
    }
    if replacements.is_empty() {
        return Ok(bytes.to_vec());
    }

    let mut rewritten = source.to_owned();
    for (range, replacement) in replacements.into_iter().rev() {
        rewritten.replace_range(range, &replacement);
    }
    decoded.encode(&rewritten)
}

fn markdown_destination_spans(source: &str) -> Vec<DestinationSpan> {
    let mut spans = Vec::new();
    let parser = Parser::new_ext(source, markdown_options());
    for (_, definition) in parser.reference_definitions().iter() {
        if let Some(range) =
            reference_destination_range(source, definition.span.clone(), definition.dest.as_ref())
        {
            spans.push(DestinationSpan {
                range,
                parsed: definition.dest.to_string(),
            });
        }
    }

    for (event, range) in parser.into_offset_iter() {
        let Event::Start(tag) = event else {
            continue;
        };
        let (link_type, parsed) = match tag {
            Tag::Link {
                link_type,
                dest_url,
                ..
            }
            | Tag::Image {
                link_type,
                dest_url,
                ..
            } => (link_type, dest_url.to_string()),
            _ => continue,
        };
        if link_type != LinkType::Inline {
            continue;
        }
        if let Some(destination) = inline_destination_range(source, range) {
            spans.push(DestinationSpan {
                range: destination,
                parsed,
            });
        }
    }
    spans
}

fn markdown_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
}

fn inline_destination_range(
    source: &str,
    event_range: std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    let segment = source.get(event_range.clone())?;
    let opening = segment.rfind("](")? + 2;
    destination_token_range(segment, opening)
        .map(|range| event_range.start + range.start..event_range.start + range.end)
}

fn reference_destination_range(
    source: &str,
    definition_range: std::ops::Range<usize>,
    parsed_destination: &str,
) -> Option<std::ops::Range<usize>> {
    let segment = source.get(definition_range.clone())?;
    if let Some(separator) = segment.find("]:") {
        if let Some(range) = destination_token_range(segment, separator + 2) {
            return Some(definition_range.start + range.start..definition_range.start + range.end);
        }
    }
    segment.rfind(parsed_destination).map(|start| {
        definition_range.start + start..definition_range.start + start + parsed_destination.len()
    })
}

fn destination_token_range(source: &str, mut start: usize) -> Option<std::ops::Range<usize>> {
    let bytes = source.as_bytes();
    while matches!(bytes.get(start), Some(b' ' | b'\t' | b'\r' | b'\n')) {
        start += 1;
    }
    if bytes.get(start) == Some(&b'<') {
        let token_start = start + 1;
        let mut escaped = false;
        for index in token_start..bytes.len() {
            let byte = bytes[index];
            if byte == b'>' && !escaped {
                return Some(token_start..index);
            }
            escaped = byte == b'\\' && !escaped;
            if byte != b'\\' {
                escaped = false;
            }
        }
        return None;
    }

    let token_start = start;
    let mut depth = 0usize;
    let mut escaped = false;
    for index in token_start..bytes.len() {
        let byte = bytes[index];
        if !escaped {
            match byte {
                b'(' => depth += 1,
                b')' if depth == 0 => return (index > token_start).then_some(token_start..index),
                b')' => depth -= 1,
                b' ' | b'\t' | b'\r' | b'\n' if depth == 0 => {
                    return (index > token_start).then_some(token_start..index);
                }
                _ => {}
            }
        }
        escaped = byte == b'\\' && !escaped;
        if byte != b'\\' {
            escaped = false;
        }
    }
    (bytes.len() > token_start).then_some(token_start..bytes.len())
}

fn rewrite_destination(
    raw: &str,
    before_path: &Path,
    after_path: &Path,
    moved_source: &Path,
    moved_destination: &Path,
) -> Option<String> {
    if raw.is_empty()
        || raw.starts_with('#')
        || raw.starts_with('/')
        || raw.starts_with('\\')
        || Url::parse(raw).is_ok()
    {
        return None;
    }
    let before_base = Url::from_directory_path(before_path.parent()?).ok()?;
    let resolved = before_base.join(raw).ok()?;
    if resolved.scheme() != "file" {
        return None;
    }
    let target_before = resolved.to_file_path().ok()?;
    // Windows file URLs do not retain the verbatim prefix returned by
    // canonicalize. Existing targets are canonicalized again so subtree
    // identity remains stable across the URL/path boundary.
    let target_before = dunce::canonicalize(&target_before).unwrap_or(target_before);
    let target_after = map_moved_path(&target_before, moved_source, moved_destination);
    if before_path == after_path && target_before == target_after {
        return None;
    }

    let after_base = Url::from_directory_path(after_path.parent()?).ok()?;
    let mut target_url = Url::from_file_path(target_after).ok()?;
    target_url.set_query(resolved.query());
    target_url.set_fragment(resolved.fragment());
    after_base.make_relative(&target_url)
}

struct DecodedDocument {
    text: String,
    encoding: PreservedEncoding,
}

enum PreservedEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Legacy(&'static encoding_rs::Encoding),
}

impl DecodedDocument {
    fn encode(&self, text: &str) -> Result<Vec<u8>> {
        match self.encoding {
            PreservedEncoding::Utf8 => Ok(text.as_bytes().to_vec()),
            PreservedEncoding::Utf16Le => {
                let mut bytes = vec![0xff, 0xfe];
                for unit in text.encode_utf16() {
                    bytes.extend_from_slice(&unit.to_le_bytes());
                }
                Ok(bytes)
            }
            PreservedEncoding::Utf16Be => {
                let mut bytes = vec![0xfe, 0xff];
                for unit in text.encode_utf16() {
                    bytes.extend_from_slice(&unit.to_be_bytes());
                }
                Ok(bytes)
            }
            PreservedEncoding::Legacy(encoding) => {
                let (bytes, _, had_errors) = encoding.encode(text);
                if had_errors {
                    bail!(
                        "rewritten link cannot be represented as {}",
                        encoding.name()
                    );
                }
                Ok(bytes.into_owned())
            }
        }
    }
}

fn decode_preserving_encoding(bytes: &[u8]) -> Result<DecodedDocument> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(DecodedDocument {
            text: text.to_owned(),
            encoding: PreservedEncoding::Utf8,
        });
    }
    if let Some(payload) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return decode_utf16(payload, true);
    }
    if let Some(payload) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return decode_utf16(payload, false);
    }
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let (text, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        bail!("unsupported Markdown encoding");
    }
    Ok(DecodedDocument {
        text: text.into_owned(),
        encoding: PreservedEncoding::Legacy(encoding),
    })
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<DecodedDocument> {
    if !bytes.len().is_multiple_of(2) {
        bail!("invalid UTF-16 byte length");
    }
    let units = bytes.chunks_exact(2).map(|pair| {
        if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        }
    });
    let text = String::from_utf16(&units.collect::<Vec<_>>())?;
    Ok(DecodedDocument {
        text,
        encoding: if little_endian {
            PreservedEncoding::Utf16Le
        } else {
            PreservedEncoding::Utf16Be
        },
    })
}

#[cfg(test)]
#[path = "../../tests/unit/editor/workspace_file_ops.rs"]
mod tests;
