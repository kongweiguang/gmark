// @author kongweiguang

use std::fs;
use std::io::Write;

use gmark_paged_document::{
    DelimitedIndex, DelimitedIndexOptions, DocumentFormat, ExternalChange, FileSource, JsonIndex,
    JsonIndexOptions, LineIndex, MarkdownTableIndex, OpenStrategy, PagedDocument,
    PagedDocumentError, PieceDocument, ProbeOptions, SearchCancellation, SearchOptions,
    SourceAffinity, SourceAnchor, TextEncoding, ViewportRequest, prepare_utf8_source, probe_file,
    search_file_source,
};

#[path = "large_document_behavior/encoding.rs"]
mod encoding;
#[path = "large_document_behavior/persistence.rs"]
mod persistence;
#[path = "large_document_behavior/probe_index.rs"]
mod probe_index;
#[path = "large_document_behavior/search.rs"]
mod search;
