// @author kongweiguang

//! Resident Markdown source metadata.
//!
//! Source construction remains centralized so file, recovery, and untitled
//! adapters keep identical profile and identity semantics after the split.

use std::path::{Path, PathBuf};

use gmark_document::{SourceDocument, SourceFormatSnapshot};
use gmark_document_core::{DocumentProfile, LoadingLimits, LoadingPolicy, TextEncoding};

use super::runtime::{display_encoding, markdown_profile};
use super::types::DocumentServiceError;

/// A resident Markdown payload handed to the service by a file/recovery
/// adapter.  The `SourceDocument` is moved into the authoritative runtime
/// session; it is never copied into [`SharedResidentOpen`].
pub(crate) struct ResidentMarkdownSource {
    pub(crate) source: SourceDocument,
    pub(crate) profile: DocumentProfile,
    pub(crate) file_identity: gmark_paged_document::FileIdentity,
    pub(crate) loading_limits: LoadingLimits,
    pub(crate) source_encoding: crate::document_io::DocumentEncoding,
}

impl std::fmt::Debug for ResidentMarkdownSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResidentMarkdownSource")
            .field("profile", &self.profile)
            .field("file_identity", &self.file_identity)
            .field("loading_limits", &self.loading_limits)
            .field("len", &self.source.len())
            .finish()
    }
}

impl ResidentMarkdownSource {
    /// Builds a source from text using the supplied logical path and default
    /// UTF-8 resident metadata.  Tests and untitled/recovery adapters can use
    /// this without touching the filesystem.
    pub(crate) fn new(text: impl AsRef<str>, path: impl Into<PathBuf>) -> Self {
        Self::from_text(
            text.as_ref(),
            path,
            TextEncoding::Utf8 { bom: false },
            LoadingPolicy::default().effective_limits(),
        )
    }

    pub(crate) fn from_text(
        text: impl AsRef<str>,
        path: impl Into<PathBuf>,
        encoding: TextEncoding,
        loading_limits: LoadingLimits,
    ) -> Self {
        let text = text.as_ref();
        let source = SourceDocument::new(text);
        Self::from_source(source, path.into(), encoding, loading_limits)
    }

    pub(crate) fn from_source(
        source: SourceDocument,
        path: impl Into<PathBuf>,
        encoding: TextEncoding,
        loading_limits: LoadingLimits,
    ) -> Self {
        let path = path.into();
        let text = source.text();
        let len = text.len() as u64;
        let profile = markdown_profile(&text, encoding.clone());
        let file_identity = gmark_paged_document::FileIdentity {
            path,
            len,
            modified_nanos: None,
            os_file_id: None,
        };
        Self {
            source,
            profile,
            file_identity,
            loading_limits,
            source_encoding: display_encoding(&encoding),
        }
    }

    /// Converts the existing file adapter value without exposing a second
    /// source representation to the editor.  The adapter's frozen limits and
    /// encoding remain part of the session metadata.
    pub(crate) fn from_opened(path: &Path, opened: crate::document_io::OpenedMarkdown) -> Self {
        let crate::document_io::OpenedMarkdown {
            text,
            encoding,
            text_encoding,
            file_identity,
            loading_limits,
            ..
        } = opened;
        let identity = file_identity.unwrap_or_else(|| gmark_paged_document::FileIdentity {
            path: path.to_path_buf(),
            len: text.len() as u64,
            modified_nanos: None,
            os_file_id: None,
        });
        let source = SourceDocument::new(&text);
        let profile = markdown_profile(&text, text_encoding);
        Self {
            source,
            profile,
            file_identity: identity,
            loading_limits,
            source_encoding: encoding,
        }
    }

    /// Builds a recovered source with its serialized line-ending/format state
    /// applied before the Controller is published.  Applying this snapshot on
    /// the shared `SourceDocument` avoids an opening-time restore transaction.
    pub(crate) fn from_recovered(
        text: impl AsRef<str>,
        path: Option<PathBuf>,
        source_format: SourceFormatSnapshot,
    ) -> Result<Self, DocumentServiceError> {
        let text = text.as_ref();
        let mut source = SourceDocument::new(text);
        if !source.restore_source_format(source_format) {
            return Err(DocumentServiceError::OpenFailed(
                "recovery source format does not match recovered text".to_owned(),
            ));
        }
        Ok(Self::from_source(
            source,
            path.unwrap_or_default(),
            TextEncoding::Utf8 { bom: false },
            LoadingPolicy::default().effective_limits(),
        ))
    }

    pub(crate) fn with_profile(
        source: SourceDocument,
        profile: DocumentProfile,
        file_identity: gmark_paged_document::FileIdentity,
        loading_limits: LoadingLimits,
    ) -> Self {
        let source_encoding = display_encoding(&profile.encoding);
        Self {
            source,
            profile,
            file_identity,
            loading_limits,
            source_encoding,
        }
    }
}
