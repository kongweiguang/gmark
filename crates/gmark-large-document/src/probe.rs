// @author kongweiguang

use std::path::Path;

use crate::{FileSource, LargeDocumentError};

pub const DEFAULT_LARGE_FILE_THRESHOLD: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentFormat {
    PlainText,
    Markdown,
    Json,
    JsonLines,
    Delimited { delimiter: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8 { bom: bool },
    Utf16Le,
    Utf16Be,
    Legacy(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenStrategy {
    Resident,
    Large,
}

#[derive(Clone, Copy, Debug)]
pub struct ProbeOptions {
    pub large_file_threshold: u64,
    pub sample_bytes: usize,
}

impl Default for ProbeOptions {
    fn default() -> Self {
        Self {
            large_file_threshold: DEFAULT_LARGE_FILE_THRESHOLD,
            sample_bytes: 64 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenProbe {
    pub len: u64,
    pub format: DocumentFormat,
    pub encoding: TextEncoding,
    pub strategy: OpenStrategy,
    pub estimated_lines: u64,
    pub estimated_structural_units: u64,
}

pub fn probe_file(
    path: impl AsRef<Path>,
    options: ProbeOptions,
) -> Result<OpenProbe, LargeDocumentError> {
    let path = path.as_ref();
    let source = FileSource::open(path)?;
    let identity = source.identity()?;
    let sample_len = identity.len.min(options.sample_bytes as u64) as usize;
    let head = source.read_range(0, sample_len as u64)?;
    let tail_start = identity.len.saturating_sub(options.sample_bytes as u64);
    let tail = if tail_start > sample_len as u64 {
        source.read_range(tail_start, identity.len)?
    } else {
        Vec::new()
    };
    let encoding = detect_encoding(&head, &tail)?;
    let format = detect_format(path);
    let sampled = (head.len() + tail.len()).max(1) as u64;
    let newline_count = head
        .iter()
        .chain(&tail)
        .filter(|byte| **byte == b'\n')
        .count() as u64;
    let structural_count = head
        .iter()
        .chain(&tail)
        .filter(|byte| matches!(**byte, b'|' | b',' | b'\t' | b'{' | b'}' | b'[' | b']'))
        .count() as u64;
    let scale = identity.len.div_ceil(sampled);
    let estimated_lines = newline_count.saturating_add(1).saturating_mul(scale);
    let estimated_structural_units = structural_count.saturating_mul(scale);
    let strategy = if identity.len > options.large_file_threshold
        || estimated_lines > 100_000
        || estimated_structural_units > 500_000
    {
        OpenStrategy::Large
    } else {
        OpenStrategy::Resident
    };
    Ok(OpenProbe {
        len: identity.len,
        format,
        encoding,
        strategy,
        estimated_lines,
        estimated_structural_units,
    })
}

fn detect_format(path: &Path) -> DocumentFormat {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "md" | "markdown" => DocumentFormat::Markdown,
        "json" => DocumentFormat::Json,
        "jsonl" | "ndjson" => DocumentFormat::JsonLines,
        "csv" => DocumentFormat::Delimited { delimiter: b',' },
        "tsv" | "tab" => DocumentFormat::Delimited { delimiter: b'\t' },
        _ => DocumentFormat::PlainText,
    }
}

fn detect_encoding(head: &[u8], tail: &[u8]) -> Result<TextEncoding, LargeDocumentError> {
    if head.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Ok(TextEncoding::Utf8 { bom: true });
    }
    if head.starts_with(&[0xff, 0xfe]) {
        return Ok(TextEncoding::Utf16Le);
    }
    if head.starts_with(&[0xfe, 0xff]) {
        return Ok(TextEncoding::Utf16Be);
    }
    if !looks_like_text_sample(head, tail) {
        return Err(LargeDocumentError::Binary);
    }
    if std::str::from_utf8(head).is_ok() && tail_is_utf8(tail) {
        return Ok(TextEncoding::Utf8 { bom: false });
    }
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(head, tail.is_empty());
    if !tail.is_empty() {
        detector.feed(tail, true);
    }
    let encoding = detector.guess(None, true);
    Ok(TextEncoding::Legacy(encoding.name().to_owned()))
}

/// BOM 已在调用方处理；剩余文本不应包含 NUL。其它 C0/DEL 控制字允许极少量，
/// 兼容日志中的偶发控制符，同时拒绝压缩包、图片和可执行文件常见的控制字密度。
fn looks_like_text_sample(head: &[u8], tail: &[u8]) -> bool {
    let mut sampled = 0usize;
    let mut controls = 0usize;
    for byte in head.iter().chain(tail) {
        sampled = sampled.saturating_add(1);
        if *byte == 0 {
            return false;
        }
        if matches!(*byte, 0x01..=0x08 | 0x0b..=0x0c | 0x0e..=0x1f | 0x7f) {
            controls = controls.saturating_add(1);
        }
    }
    controls <= (sampled / 100).max(1)
}

fn tail_is_utf8(tail: &[u8]) -> bool {
    if tail.is_empty() {
        return true;
    }
    let first_boundary = tail
        .iter()
        .position(|byte| byte & 0b1100_0000 != 0b1000_0000)
        .unwrap_or(tail.len());
    std::str::from_utf8(&tail[first_boundary..]).is_ok()
}
