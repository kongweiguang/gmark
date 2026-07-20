// @author kongweiguang

//! Safe Markdown byte decoding shared by every file-open entry point.

use std::path::Path;

use anyhow::{Context as _, Result, bail};
use gmark_large_document::{OpenProbe, OpenStrategy, ProbeOptions};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DocumentEncoding {
    Utf8,
    Legacy(String),
}

impl DocumentEncoding {
    pub(crate) fn is_utf8(&self) -> bool {
        matches!(self, Self::Utf8)
    }

    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Legacy(label) => label,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OpenedMarkdown {
    pub(crate) text: String,
    pub(crate) encoding: DocumentEncoding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OpenedDocument {
    Resident(OpenedMarkdown),
    Large(OpenProbe),
}

/// 文件打开策略只决定存储与主视图，不把未来的派生视图当作文档真值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocumentOpenPolicy {
    ResidentMarkdown,
    SourceBacked,
}

pub(crate) fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown")
        })
}

pub(crate) fn document_open_policy(path: &Path, probe: &OpenProbe) -> DocumentOpenPolicy {
    if is_markdown_path(path) && probe.strategy == OpenStrategy::Resident {
        DocumentOpenPolicy::ResidentMarkdown
    } else {
        // 非 Markdown 文本始终从完整 Source 打开。JSON/CSV 等派生视图只能由用户主动进入，
        // 后台结构索引完成不得改变文档的默认视图或源码坐标。
        DocumentOpenPolicy::SourceBacked
    }
}

/// 打开前先做有界探测，禁止大文件进入 `fs::read` 和完整 Markdown 投影链路。
pub(crate) fn open_document(path: &Path) -> Result<OpenedDocument> {
    let probe = gmark_large_document::probe_file(path, ProbeOptions::default())
        .with_context(|| format!("failed to inspect '{}'", path.display()))?;
    match document_open_policy(path, &probe) {
        DocumentOpenPolicy::ResidentMarkdown => {
            read_resident_text(path).map(OpenedDocument::Resident)
        }
        DocumentOpenPolicy::SourceBacked => Ok(OpenedDocument::Large(probe)),
    }
}

pub(crate) fn read_markdown_file(path: &Path) -> Result<OpenedMarkdown> {
    let probe = gmark_large_document::probe_file(path, ProbeOptions::default())
        .with_context(|| format!("failed to inspect '{}'", path.display()))?;
    if probe.strategy == OpenStrategy::Large {
        bail!(
            "'{}' is {:.1} MiB and must be opened in large-file mode",
            path.display(),
            probe.len as f64 / (1024.0 * 1024.0)
        );
    }
    read_resident_text(path)
}

fn read_resident_text(path: &Path) -> Result<OpenedMarkdown> {
    let bytes =
        std::fs::read(path).with_context(|| format!("failed to read '{}'", path.display()))?;
    decode_markdown_bytes(&bytes).with_context(|| format!("failed to decode '{}'", path.display()))
}

pub(crate) fn decode_markdown_bytes(bytes: &[u8]) -> Result<OpenedMarkdown> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(OpenedMarkdown {
            text: text.to_owned(),
            encoding: DocumentEncoding::Utf8,
        });
    }
    if let Some(payload) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return decode_utf16(payload, true, "UTF-16LE");
    }
    if let Some(payload) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return decode_utf16(payload, false, "UTF-16BE");
    }

    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let (text, _, had_errors) = encoding.decode(bytes);
    if had_errors || !looks_like_text(&text) {
        bail!("file is binary or uses an unsupported text encoding");
    }
    Ok(OpenedMarkdown {
        text: text.into_owned(),
        encoding: DocumentEncoding::Legacy(encoding.name().to_owned()),
    })
}

fn decode_utf16(bytes: &[u8], little_endian: bool, label: &str) -> Result<OpenedMarkdown> {
    if !bytes.len().is_multiple_of(2) {
        bail!("{label} byte length is not even");
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect::<Vec<_>>();
    let text = String::from_utf16(&units).with_context(|| format!("invalid {label} text"))?;
    if !looks_like_text(&text) {
        bail!("file is binary or contains unsupported control characters");
    }
    Ok(OpenedMarkdown {
        text,
        encoding: DocumentEncoding::Legacy(label.to_owned()),
    })
}

fn looks_like_text(text: &str) -> bool {
    let controls = text
        .chars()
        .filter(|ch| ch.is_control() && !matches!(*ch, '\n' | '\r' | '\t'))
        .count();
    controls <= (text.chars().count() / 100).max(1)
}

#[cfg(test)]
#[path = "../tests/unit/document_io.rs"]
mod tests;
