// @author kongweiguang

//! 非 UTF-8 大文本的流式 UTF-8 影子与原编码原子保存。

use std::io::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};

use encoding_rs::{CoderResult, Encoding, UTF_16BE, UTF_16LE};

use crate::{FileIdentity, FileSource, LargeDocumentError, PieceDocument, TextEncoding};

const TRANSCODE_BLOCK_BYTES: u64 = 8 * 1024 * 1024;

pub struct PreparedUtf8Source {
    source: FileSource,
    _shadow: Option<tempfile::NamedTempFile>,
    save_plan: Option<EncodedSavePlan>,
}

#[derive(Clone)]
pub struct EncodedSavePlan {
    encoding: TextEncoding,
    original_identity: FileIdentity,
}

impl PreparedUtf8Source {
    pub fn source(&self) -> &FileSource {
        &self.source
    }

    pub fn save_plan(&self) -> Option<EncodedSavePlan> {
        self.save_plan.clone()
    }

    pub fn mark_original_saved(&mut self, identity: FileIdentity) {
        if let Some(plan) = self.save_plan.as_mut() {
            plan.original_identity = identity;
        }
    }
}

impl EncodedSavePlan {
    pub fn save_atomic(
        &self,
        document: &PieceDocument,
        path: impl AsRef<Path>,
    ) -> Result<FileIdentity, LargeDocumentError> {
        self.save_atomic_cancellable(document, path, &crate::SearchCancellation::default())
    }

    pub fn save_atomic_cancellable(
        &self,
        document: &PieceDocument,
        path: impl AsRef<Path>,
        cancellation: &crate::SearchCancellation,
    ) -> Result<FileIdentity, LargeDocumentError> {
        let path = path.as_ref();
        let current = FileSource::open(path)?.identity()?;
        if current != self.original_identity {
            return Err(LargeDocumentError::SourceChanged);
        }
        self.save_atomic_inner(document, path, Some(&self.original_identity), cancellation)
    }

    pub fn save_atomic_as(
        &self,
        document: &PieceDocument,
        path: impl AsRef<Path>,
    ) -> Result<FileIdentity, LargeDocumentError> {
        self.save_atomic_as_cancellable(document, path, &crate::SearchCancellation::default())
    }

    pub fn save_atomic_as_cancellable(
        &self,
        document: &PieceDocument,
        path: impl AsRef<Path>,
        cancellation: &crate::SearchCancellation,
    ) -> Result<FileIdentity, LargeDocumentError> {
        self.save_atomic_inner(document, path.as_ref(), None, cancellation)
    }

    pub fn save_range_atomic_as_cancellable(
        &self,
        document: &PieceDocument,
        range: Range<u64>,
        path: impl AsRef<Path>,
        cancellation: &crate::SearchCancellation,
    ) -> Result<FileIdentity, LargeDocumentError> {
        let path = path.as_ref();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|source| LargeDocumentError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        self.write_range(document, range, &mut temporary, cancellation)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|source| LargeDocumentError::Io {
                path: temporary.path().to_path_buf(),
                source,
            })?;
        if let Ok(metadata) = std::fs::metadata(path) {
            temporary
                .as_file()
                .set_permissions(metadata.permissions())
                .map_err(|source| LargeDocumentError::Io {
                    path: temporary.path().to_path_buf(),
                    source,
                })?;
        }
        if cancellation.is_cancelled() {
            return Err(LargeDocumentError::Cancelled);
        }
        crate::source::persist_temporary(temporary, path)?;
        crate::source::sync_parent_directory(parent)?;
        FileSource::open(path)?.identity()
    }

    fn save_atomic_inner(
        &self,
        document: &PieceDocument,
        path: &Path,
        expected_identity: Option<&FileIdentity>,
        cancellation: &crate::SearchCancellation,
    ) -> Result<FileIdentity, LargeDocumentError> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|source| LargeDocumentError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        self.write_range(document, 0..document.len(), &mut temporary, cancellation)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|source| LargeDocumentError::Io {
                path: temporary.path().to_path_buf(),
                source,
            })?;
        if let Ok(metadata) = std::fs::metadata(path) {
            temporary
                .as_file()
                .set_permissions(metadata.permissions())
                .map_err(|source| LargeDocumentError::Io {
                    path: temporary.path().to_path_buf(),
                    source,
                })?;
        }
        if let Some(expected) = expected_identity
            && FileSource::open(path)?.identity()? != *expected
        {
            return Err(LargeDocumentError::SourceChanged);
        }
        if cancellation.is_cancelled() {
            return Err(LargeDocumentError::Cancelled);
        }
        crate::source::persist_temporary(temporary, path)?;
        crate::source::sync_parent_directory(parent)?;
        FileSource::open(path)?.identity()
    }

    fn write_range(
        &self,
        document: &PieceDocument,
        range: Range<u64>,
        temporary: &mut tempfile::NamedTempFile,
        cancellation: &crate::SearchCancellation,
    ) -> Result<(), LargeDocumentError> {
        write_bom(temporary, &self.encoding)?;
        match &self.encoding {
            TextEncoding::Utf16Le => {
                document.for_each_utf8_range_chunk(range, TRANSCODE_BLOCK_BYTES, |bytes| {
                    if cancellation.is_cancelled() {
                        return Err(LargeDocumentError::Cancelled);
                    }
                    write_utf16(temporary, bytes, true)
                })?;
            }
            TextEncoding::Utf16Be => {
                document.for_each_utf8_range_chunk(range, TRANSCODE_BLOCK_BYTES, |bytes| {
                    if cancellation.is_cancelled() {
                        return Err(LargeDocumentError::Cancelled);
                    }
                    write_utf16(temporary, bytes, false)
                })?;
            }
            _ => {
                let encoding = resolve_encoding(&self.encoding)?;
                let mut writer = EncodingWriter::new(temporary, encoding, self.encoding_name());
                document.for_each_utf8_range_chunk(range, TRANSCODE_BLOCK_BYTES, |bytes| {
                    if cancellation.is_cancelled() {
                        return Err(LargeDocumentError::Cancelled);
                    }
                    let text = std::str::from_utf8(bytes)
                        .map_err(|_| LargeDocumentError::InvalidUtf8Boundary)?;
                    writer.encode(text, false)
                })?;
                writer.finish()?;
            }
        }
        Ok(())
    }

    pub fn encoding_name(&self) -> String {
        match &self.encoding {
            TextEncoding::Utf16Le => "UTF-16LE".to_owned(),
            TextEncoding::Utf16Be => "UTF-16BE".to_owned(),
            TextEncoding::Legacy(label) => label.clone(),
            TextEncoding::Utf8 { .. } => "UTF-8".to_owned(),
        }
    }
}

pub fn prepare_utf8_source(
    original: FileSource,
    encoding: TextEncoding,
) -> Result<PreparedUtf8Source, LargeDocumentError> {
    if matches!(encoding, TextEncoding::Utf8 { .. }) {
        return Ok(PreparedUtf8Source {
            source: original,
            _shadow: None,
            save_plan: None,
        });
    }
    let identity = original.identity()?;
    let decoder_encoding = resolve_encoding(&encoding)?;
    let mut decoder = decoder_encoding.new_decoder_with_bom_removal();
    let mut shadow = tempfile::NamedTempFile::new().map_err(|source| LargeDocumentError::Io {
        path: std::env::temp_dir(),
        source,
    })?;
    let mut offset = 0u64;
    while offset < identity.len {
        let end = (offset + TRANSCODE_BLOCK_BYTES).min(identity.len);
        let input = original.read_range(offset, end)?;
        let shadow_path = shadow.path().to_path_buf();
        decode_block(
            &mut decoder,
            &input,
            end == identity.len,
            &mut shadow,
            shadow_path,
        )?;
        offset = end;
    }
    if identity.len == 0 {
        let shadow_path = shadow.path().to_path_buf();
        decode_block(&mut decoder, &[], true, &mut shadow, shadow_path)?;
    }
    shadow.flush().map_err(|source| LargeDocumentError::Io {
        path: shadow.path().to_path_buf(),
        source,
    })?;
    let source = FileSource::open(shadow.path())?;
    Ok(PreparedUtf8Source {
        source,
        _shadow: Some(shadow),
        save_plan: Some(EncodedSavePlan {
            encoding,
            original_identity: identity,
        }),
    })
}

fn resolve_encoding(encoding: &TextEncoding) -> Result<&'static Encoding, LargeDocumentError> {
    match encoding {
        TextEncoding::Utf8 { .. } => Ok(encoding_rs::UTF_8),
        TextEncoding::Utf16Le => Ok(UTF_16LE),
        TextEncoding::Utf16Be => Ok(UTF_16BE),
        TextEncoding::Legacy(label) => Encoding::for_label(label.as_bytes())
            .ok_or_else(|| LargeDocumentError::UnsupportedEncoding(label.clone())),
    }
}

fn decode_block(
    decoder: &mut encoding_rs::Decoder,
    mut input: &[u8],
    last: bool,
    output: &mut impl Write,
    output_path: PathBuf,
) -> Result<(), LargeDocumentError> {
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let (result, read, written, had_errors) = decoder.decode_to_utf8(input, &mut buffer, last);
        if had_errors {
            return Err(LargeDocumentError::Binary);
        }
        output
            .write_all(&buffer[..written])
            .map_err(|source| LargeDocumentError::Io {
                path: output_path.clone(),
                source,
            })?;
        input = &input[read..];
        if matches!(result, CoderResult::InputEmpty) {
            return Ok(());
        }
    }
}

fn write_bom(output: &mut impl Write, encoding: &TextEncoding) -> Result<(), LargeDocumentError> {
    let bom: &[u8] = match encoding {
        TextEncoding::Utf16Le => &[0xff, 0xfe],
        TextEncoding::Utf16Be => &[0xfe, 0xff],
        _ => &[],
    };
    output
        .write_all(bom)
        .map_err(|source| LargeDocumentError::Io {
            path: std::env::temp_dir(),
            source,
        })
}

fn write_utf16(
    output: &mut impl Write,
    bytes: &[u8],
    little_endian: bool,
) -> Result<(), LargeDocumentError> {
    let text = std::str::from_utf8(bytes).map_err(|_| LargeDocumentError::InvalidUtf8Boundary)?;
    let mut encoded = Vec::with_capacity(text.len().saturating_mul(2));
    for unit in text.encode_utf16() {
        let bytes = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        encoded.extend_from_slice(&bytes);
    }
    output
        .write_all(&encoded)
        .map_err(|source| LargeDocumentError::Io {
            path: std::env::temp_dir(),
            source,
        })
}

struct EncodingWriter<'a, W> {
    output: &'a mut W,
    encoder: encoding_rs::Encoder,
    encoding_name: String,
}

impl<'a, W: Write> EncodingWriter<'a, W> {
    fn new(output: &'a mut W, encoding: &'static Encoding, encoding_name: String) -> Self {
        Self {
            output,
            encoder: encoding.new_encoder(),
            encoding_name,
        }
    }

    fn finish(&mut self) -> Result<(), LargeDocumentError> {
        self.encode("", true)
    }

    fn encode(&mut self, mut input: &str, last: bool) -> Result<(), LargeDocumentError> {
        let mut buffer = vec![0u8; 256 * 1024];
        loop {
            let (result, read, written, had_errors) =
                self.encoder.encode_from_utf8(input, &mut buffer, last);
            if had_errors {
                return Err(LargeDocumentError::UnrepresentableEncoding {
                    encoding: self.encoding_name.clone(),
                });
            }
            self.output
                .write_all(&buffer[..written])
                .map_err(|source| LargeDocumentError::Io {
                    path: std::env::temp_dir(),
                    source,
                })?;
            input = &input[read..];
            if matches!(result, CoderResult::InputEmpty) {
                return Ok(());
            }
        }
    }
}
