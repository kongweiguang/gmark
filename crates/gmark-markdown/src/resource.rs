// @author kongweiguang

//! Standard-Markdown resource-card recognition and side-effect-free URL classification.

use std::path::{Path, PathBuf};

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};
use url::Url;

/// Link-title marker used by GMark resource cards.
pub const RESOURCE_MARKER: &str = "gmark:resource";

/// The visual resource-card kind after automatic classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    /// Generic file or unsupported media.
    File,
    /// Video media recognized from its extension or explicit type.
    Video,
}

impl ResourceKind {
    /// Returns the canonical title parameter value.
    pub const fn as_str(self) -> &'static str {
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

/// Lexical resource destination classification. No path is opened or fetched.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResourceLocation {
    /// Filesystem path, optionally resolved against an adapter-supplied base directory.
    Local(PathBuf),
    /// Non-file URL retained for a caller-owned opener.
    Url(Url),
}

/// Runtime probe status kept separate from the serializable resource value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResourceStatus {
    /// An adapter is probing metadata.
    Loading,
    /// Metadata was observed successfully.
    Ready { size: Option<u64> },
    /// The target was absent.
    Missing,
    /// The adapter lacked permission to inspect the target.
    PermissionDenied,
    /// URL policy rejected the target scheme.
    UnsafeScheme,
    /// The adapter failed to open the target for another reason.
    OpenFailed,
}

/// Pure, serializable resource-card description.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceRecord {
    /// Visible label without Markdown delimiters.
    pub label: String,
    /// Original link destination as authored.
    pub destination: String,
    /// Effective card kind after explicit override or extension inference.
    pub kind: ResourceKind,
    /// Explicit `type=file|video`; `None` uses automatic classification.
    pub explicit_kind: Option<ResourceKind>,
    /// Lexically classified destination.
    pub location: ResourceLocation,
    /// Original standalone Markdown when parsed from source.
    pub source_markdown: Option<String>,
}

impl ResourceRecord {
    /// Parses one standalone inline resource link.
    pub fn parse(markdown: &str, base_dir: Option<&Path>) -> Option<Self> {
        let parsed = parse_resource_parts(markdown)?;
        let mut record = Self::from_parts(
            parsed.label,
            parsed.destination,
            parsed.explicit_kind,
            base_dir,
        );
        record.source_markdown = Some(markdown.to_owned());
        Some(record)
    }

    /// Creates a new resource card from already separated values.
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

    /// Returns parsed source when available, otherwise canonical Markdown.
    pub fn source_or_canonical_markdown(&self) -> String {
        self.source_markdown
            .clone()
            .unwrap_or_else(|| self.to_markdown())
    }

    /// Reclassifies the destination for another lexical base directory.
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

    /// Returns canonical GMark Markdown for a newly created or edited card.
    pub fn to_markdown(&self) -> String {
        let marker = match self.explicit_kind {
            Some(kind) => format!("{RESOURCE_MARKER};type={}", kind.as_str()),
            None => RESOURCE_MARKER.to_owned(),
        };
        format!(
            "[{}]({} \"{marker}\")",
            escape_label(&self.label),
            escape_destination(&self.destination)
        )
    }

    /// Returns whether the target is lexical filesystem data.
    pub fn is_local(&self) -> bool {
        matches!(self.location, ResourceLocation::Local(_))
    }

    /// Returns a path only when the destination is local.
    pub fn local_path(&self) -> Option<&Path> {
        match &self.location {
            ResourceLocation::Local(path) => Some(path),
            ResourceLocation::Url(_) => None,
        }
    }

    /// Returns whether URL policy should block opening this target by default.
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

/// Parsed components of one standalone GMark resource link.
///
/// This stays separate from [`ResourceRecord`] so syntax validation does not
/// need to choose a filesystem base directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedResource {
    label: String,
    destination: String,
    explicit_kind: Option<ResourceKind>,
}

/// Parses one standalone resource link without performing filesystem I/O.
pub fn parse_resource_parts(markdown: &str) -> Option<ParsedResource> {
    if markdown.contains('\n') || markdown.contains('\r') {
        return None;
    }
    let mut link: Option<(String, String, LinkType)> = None;
    let mut link_depth = 0usize;
    let mut label = String::new();
    let mut outside_text = String::new();
    let mut saw_paragraph = false;
    let mut saw_link_end = false;

    for event in Parser::new_ext(markdown, Options::all()) {
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
    Some(ParsedResource {
        label,
        destination,
        explicit_kind: parse_resource_title(&title)?,
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
        explicit = Some(match value.trim() {
            "auto" => None,
            value => Some(ResourceKind::from_type(value)?),
        });
    }
    Some(explicit.flatten())
}

fn classify_location(destination: &str, base_dir: Option<&Path>) -> ResourceLocation {
    if is_windows_drive_path(destination) || !has_url_scheme(destination) {
        let path = Path::new(destination);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(base_dir) = base_dir {
            base_dir.join(path)
        } else {
            path.to_path_buf()
        };
        return ResourceLocation::Local(path);
    }
    if let Ok(url) = Url::parse(destination) {
        if url.scheme().eq_ignore_ascii_case("file")
            && let Ok(path) = url.to_file_path()
        {
            return ResourceLocation::Local(path);
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
            let trimmed = destination.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
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
        .any(|character| character.is_whitespace() || matches!(character, '(' | ')' | '"'))
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
