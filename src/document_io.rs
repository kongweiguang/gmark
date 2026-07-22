// @author kongweiguang

//! Safe Markdown byte decoding shared by every file-open entry point.

use std::path::Path;

use anyhow::{Context as _, Result, bail};
use gmark_document_core::{DocumentBackendKind, LoadingPolicy, OpenPolicyResolver};
use gmark_paged_document::{OpenProbe, OpenStrategy, ProbeOptions};

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
    pub(crate) text_encoding: gmark_document_core::TextEncoding,
    pub(crate) file_identity: Option<gmark_paged_document::FileIdentity>,
    /// 打开时冻结的有效阈值；已打开会话不跟随后续设置变化。
    pub(crate) loading_limits: gmark_document_core::LoadingLimits,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OpenedDocument {
    Resident(OpenedMarkdown),
    ResidentFormat(OpenProbe),
    Paged(OpenProbe),
}

/// 文件打开策略只决定存储与主视图，不把未来的派生视图当作文档真值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocumentOpenPolicy {
    ResidentMarkdown,
    ResidentFormat,
    PagedSource,
}

pub(crate) fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown")
        })
}

pub(crate) fn document_open_policy(path: &Path, probe: &OpenProbe) -> DocumentOpenPolicy {
    if probe.strategy == OpenStrategy::Paged {
        DocumentOpenPolicy::PagedSource
    } else if is_markdown_path(path) {
        DocumentOpenPolicy::ResidentMarkdown
    } else {
        DocumentOpenPolicy::ResidentFormat
    }
}

/// 打开前先做有界探测，禁止大文件进入 `fs::read` 和完整 Markdown 投影链路。
pub(crate) fn open_document(path: &Path) -> Result<OpenedDocument> {
    #[cfg(test)]
    let loading = LoadingPolicy::default();
    #[cfg(not(test))]
    let loading = crate::config::read_app_preferences()
        .map(|preferences| preferences.document_loading.policy())
        .unwrap_or_default();
    open_document_with_policy(path, loading)
}

pub(crate) fn open_document_with_policy(
    path: &Path,
    loading: LoadingPolicy,
) -> Result<OpenedDocument> {
    let probe_started = crate::perf::start();
    let limits = loading.effective_limits();
    let mut probe = gmark_paged_document::probe_file(
        path,
        ProbeOptions {
            max_resident_bytes: limits.max_resident_bytes,
            max_resident_lines: limits.max_resident_lines,
            max_structural_units: limits.max_structural_units,
            ..ProbeOptions::default()
        },
    )
    .with_context(|| format!("failed to inspect '{}'", path.display()))?;
    let profile = probe.profile();
    let plan = OpenPolicyResolver.resolve(loading, &profile);
    if let Some(started) = probe_started {
        crate::perf::emit_document(
            "document_open_plan",
            started,
            usize::try_from(probe.len).ok(),
            Some(true),
            &profile.format,
            &plan,
            None,
        );
    }
    probe.force_safe_source = loading.force_safe_source;
    probe.strategy = match plan.backend {
        DocumentBackendKind::Resident => OpenStrategy::Resident,
        DocumentBackendKind::Paged => OpenStrategy::Paged,
    };
    match document_open_policy(path, &probe) {
        DocumentOpenPolicy::ResidentMarkdown => {
            read_resident_text_from_probe(path, &probe, limits).map(OpenedDocument::Resident)
        }
        DocumentOpenPolicy::ResidentFormat => Ok(OpenedDocument::ResidentFormat(probe)),
        DocumentOpenPolicy::PagedSource => Ok(OpenedDocument::Paged(probe)),
    }
}

pub(crate) fn read_markdown_file(path: &Path) -> Result<OpenedMarkdown> {
    let probe = gmark_paged_document::probe_file(path, ProbeOptions::default())
        .with_context(|| format!("failed to inspect '{}'", path.display()))?;
    if probe.strategy == OpenStrategy::Paged {
        bail!(
            "'{}' is {:.1} MiB and must be opened in Paged Source mode",
            path.display(),
            probe.len as f64 / (1024.0 * 1024.0)
        );
    }
    read_resident_text_from_probe(path, &probe, LoadingPolicy::default().effective_limits())
}

fn read_resident_text_from_probe(
    path: &Path,
    probe: &OpenProbe,
    loading_limits: gmark_document_core::LoadingLimits,
) -> Result<OpenedMarkdown> {
    let source = gmark_paged_document::FileSource::open(path)
        .with_context(|| format!("failed to reopen '{}'", path.display()))?;
    if source.identity()? != probe.identity {
        bail!("'{}' changed after it was inspected", path.display());
    }
    let bytes = source
        .read_range(0, probe.len)
        .with_context(|| format!("failed to read '{}'", path.display()))?;
    if source.identity()? != probe.identity {
        bail!("'{}' changed while it was being read", path.display());
    }
    let mut opened = decode_markdown_bytes(&bytes)
        .with_context(|| format!("failed to decode '{}'", path.display()))?;
    opened.loading_limits = loading_limits;
    opened.text_encoding = probe.encoding.clone();
    opened.file_identity = Some(probe.identity.clone());
    Ok(opened)
}

pub(crate) fn decode_markdown_bytes(bytes: &[u8]) -> Result<OpenedMarkdown> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(OpenedMarkdown {
            text: text.to_owned(),
            encoding: DocumentEncoding::Utf8,
            text_encoding: gmark_document_core::TextEncoding::Utf8 {
                bom: bytes.starts_with(&[0xef, 0xbb, 0xbf]),
            },
            file_identity: None,
            loading_limits: LoadingPolicy::default().effective_limits(),
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
        text_encoding: gmark_document_core::TextEncoding::Legacy(encoding.name().to_owned()),
        file_identity: None,
        loading_limits: LoadingPolicy::default().effective_limits(),
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
        text_encoding: if little_endian {
            gmark_document_core::TextEncoding::Utf16Le
        } else {
            gmark_document_core::TextEncoding::Utf16Be
        },
        file_identity: None,
        loading_limits: LoadingPolicy::default().effective_limits(),
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
