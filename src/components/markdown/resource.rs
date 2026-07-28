// @author kongweiguang

//! Markdown resource-card syntax and pure resource classification.
//!
//! Resource cards deliberately use the standard link title field so that the
//! source remains ordinary Markdown outside GMark. This module has no file or
//! process side effects; resolving a local path is only lexical and opening a
//! resource belongs to an application adapter.

use std::path::{Path, PathBuf};

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};
use url::Url;

const RESOURCE_MARKER: &str = "gmark:resource";

/// The visual kind of a resource card after automatic classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    File,
    Video,
}

impl ResourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Video => "video",
        }
    }

    fn from_type(value: &str) -> Option<Self> {
        match value {
            "file" => Some(Self::File),
            "video" => Some(Self::Video),
            _ => None,
        }
    }
}

/// Resource destination after the Markdown target has been classified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceLocation {
    /// A filesystem path, resolved relative to the Markdown document when a
    /// base directory is available.
    Local(PathBuf),
    /// A non-file URL. The URL is retained exactly enough for the normal link
    /// opener and is never fetched by the parser.
    Url(Url),
}

/// Runtime-only probe status. It is intentionally not part of the Markdown
/// serialization contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceStatus {
    Loading,
    Ready { size: Option<u64> },
    Missing,
    PermissionDenied,
    UnsafeScheme,
    OpenFailed,
}

/// Pure, serializable resource-card description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceRecord {
    /// Visible link label, without Markdown delimiters.
    pub label: String,
    /// Original link destination as written by the author.
    pub destination: String,
    /// Effective card kind after explicit override or extension inference.
    pub kind: ResourceKind,
    /// Explicit `type=file|video`; `None` means automatic classification.
    pub explicit_kind: Option<ResourceKind>,
    /// Parsed and lexically resolved destination.
    pub location: ResourceLocation,
    /// Original source, when the record came from an existing document.
    /// Newly inserted records use the canonical serializer instead.
    pub source_markdown: Option<String>,
}

impl ResourceRecord {
    pub fn parse(markdown: &str, base_dir: Option<&Path>) -> Option<Self> {
        let ParsedResource {
            label,
            destination,
            explicit_kind,
        } = parse_resource_parts(markdown)?;
        let mut record = Self::from_parts(label, destination, explicit_kind, base_dir);
        record.source_markdown = Some(markdown.to_owned());
        Some(record)
    }

    pub fn from_parts(
        label: String,
        destination: String,
        explicit_kind: Option<ResourceKind>,
        base_dir: Option<&Path>,
    ) -> Self {
        let location = classify_location(&destination, base_dir);
        let kind = explicit_kind.unwrap_or_else(|| infer_kind(&destination, &location));
        let label = if label.trim().is_empty() {
            fallback_label(&destination, &location)
        } else {
            label
        };
        Self {
            label,
            destination,
            kind,
            explicit_kind,
            location,
            source_markdown: None,
        }
    }

    pub fn source_or_canonical_markdown(&self) -> String {
        self.source_markdown
            .clone()
            .unwrap_or_else(|| self.to_markdown())
    }

    pub fn with_base_dir(&self, base_dir: Option<&Path>) -> Self {
        let mut record = Self::from_parts(
            self.label.clone(),
            self.destination.clone(),
            self.explicit_kind,
            base_dir,
        );
        record.source_markdown = self.source_markdown.clone();
        record
    }

    /// Canonical GMark Markdown form used for newly inserted or edited cards.
    pub fn to_markdown(&self) -> String {
        let label = escape_label(&self.label);
        let destination = escape_destination(&self.destination);
        let marker = match self.explicit_kind {
            None => RESOURCE_MARKER.to_owned(),
            Some(kind) => format!("{RESOURCE_MARKER};type={}", kind.as_str()),
        };
        format!("[{label}]({destination} \"{marker}\")")
    }

    pub fn is_local(&self) -> bool {
        matches!(self.location, ResourceLocation::Local(_))
    }

    pub fn local_path(&self) -> Option<&Path> {
        match &self.location {
            ResourceLocation::Local(path) => Some(path),
            ResourceLocation::Url(_) => None,
        }
    }

    pub fn is_unsafe_url(&self) -> bool {
        match &self.location {
            ResourceLocation::Local(_) => false,
            ResourceLocation::Url(url) => matches!(
                url.scheme().to_ascii_lowercase().as_str(),
                "javascript" | "data" | "blob"
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedResource {
    label: String,
    destination: String,
    explicit_kind: Option<ResourceKind>,
}

/// Parses only a single inline link with a valid GMark resource title.
///
/// Reference links and links with visible text outside the link are rejected.
/// Inline emphasis inside the label is accepted for detection, while the
/// visible label is normalized to plain text in the domain record.
pub(crate) fn parse_resource_parts(markdown: &str) -> Option<ParsedResource> {
    if markdown.contains('\n') || markdown.contains('\r') {
        return None;
    }

    let events = Parser::new_ext(markdown, Options::all());
    let mut link: Option<(String, String, LinkType)> = None;
    let mut link_depth = 0usize;
    let mut label = String::new();
    let mut outside_text = String::new();
    let mut saw_paragraph = false;
    let mut saw_link_end = false;

    for event in events {
        match event {
            Event::Start(Tag::Paragraph) => {
                if saw_paragraph || link_depth != 0 {
                    return None;
                }
                saw_paragraph = true;
            }
            Event::End(TagEnd::Paragraph) => {}
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                ..
            }) => {
                if link.is_some() || link_depth != 0 || link_type != LinkType::Inline {
                    return None;
                }
                link = Some((dest_url.into_string(), title.into_string(), link_type));
                link_depth = 1;
            }
            Event::End(TagEnd::Link) => {
                if link_depth == 0 {
                    return None;
                }
                link_depth -= 1;
                saw_link_end = true;
            }
            Event::Start(_) if link_depth > 0 => link_depth += 1,
            Event::End(_) if link_depth > 0 => link_depth = link_depth.saturating_sub(1),
            Event::Text(text) | Event::Code(text) if link_depth > 0 => label.push_str(&text),
            Event::Text(text) | Event::Code(text) => outside_text.push_str(&text),
            Event::SoftBreak | Event::HardBreak => {
                if link_depth > 0 {
                    label.push(' ');
                } else {
                    outside_text.push(' ');
                }
            }
            Event::Html(_) | Event::InlineHtml(_) | Event::FootnoteReference(_) => return None,
            _ if link_depth > 0 => {}
            // Block/list wrappers and inline formatting outside the link are
            // not part of the standalone paragraph contract. Container
            // importers strip their own marker before calling this parser.
            _ => return None,
        }
    }

    if !saw_paragraph || link_depth != 0 || !saw_link_end || !outside_text.trim().is_empty() {
        return None;
    }
    let (destination, title, link_type) = link?;
    if link_type != LinkType::Inline {
        return None;
    }
    let explicit_kind = parse_resource_title(&title)?;
    Some(ParsedResource {
        label,
        destination,
        explicit_kind,
    })
}

fn parse_resource_title(title: &str) -> Option<Option<ResourceKind>> {
    let mut parts = title.split(';').map(str::trim);
    if parts.next()? != RESOURCE_MARKER || title.trim().is_empty() {
        return None;
    }

    let mut explicit = None;
    for part in parts {
        if part.is_empty() {
            return None;
        }
        let (key, value) = part.split_once('=')?;
        if key.trim() != "type" || explicit.is_some() {
            return None;
        }
        let value = value.trim();
        if value == "auto" {
            explicit = Some(None);
        } else {
            explicit = Some(Some(ResourceKind::from_type(value)?));
        }
    }
    Some(explicit.flatten())
}

fn classify_location(destination: &str, base_dir: Option<&Path>) -> ResourceLocation {
    if is_windows_drive_path(destination) || !has_url_scheme(destination) {
        let path = Path::new(destination);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            base_dir
                .map(|base| base.join(path))
                .unwrap_or_else(|| path.to_path_buf())
        };
        return ResourceLocation::Local(path);
    }

    if let Ok(url) = Url::parse(destination) {
        if url.scheme().eq_ignore_ascii_case("file") {
            if let Ok(path) = url.to_file_path() {
                return ResourceLocation::Local(path);
            }
        }
        return ResourceLocation::Url(url);
    }

    ResourceLocation::Local(PathBuf::from(destination))
}

fn fallback_label(destination: &str, location: &ResourceLocation) -> String {
    let candidate = match location {
        ResourceLocation::Local(path) => path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty()),
        ResourceLocation::Url(url) => url.host_str().filter(|host| !host.is_empty()),
    };
    candidate
        .map(str::to_owned)
        .or_else(|| {
            let destination = destination.trim();
            (!destination.is_empty()).then(|| destination.to_owned())
        })
        .unwrap_or_else(|| "resource".to_owned())
}

fn has_url_scheme(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic()
                || (index > 0 && byte.is_ascii_digit())
                || matches!(byte, b'+' | b'-' | b'.')
        })
}

fn infer_kind(destination: &str, location: &ResourceLocation) -> ResourceKind {
    let path = match location {
        ResourceLocation::Local(path) => path.as_path(),
        ResourceLocation::Url(url) => Path::new(url.path()),
    };
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .or_else(|| {
            destination
                .rsplit_once('.')
                .map(|(_, extension)| extension.split(['?', '#']).next().unwrap_or(extension))
        })
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "avi" | "m4v" | "mkv" | "mov" | "mp4" | "mpeg" | "mpg" | "ogv" | "webm" | "wmv"
    ) {
        ResourceKind::Video
    } else {
        ResourceKind::File
    }
}

fn is_windows_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
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

#[cfg(test)]
#[path = "../../../tests/unit/components/markdown/resource.rs"]
mod tests;
