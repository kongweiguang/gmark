// @author kongweiguang

//! Public pulldown-cmark parser adapter and document value.

use crate::block::Block;
use crate::event::{MarkdownEvent, parse_events};
use crate::parser_builder::{collect_block_ranges, parse_blocks};
use crate::source::{SourceFormat, SourceMap, SourceRange, SourceRangeError};
use crate::toc::TableOfContents;

/// Parsed Markdown source, structured values, and source-preservation metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkdownDocument {
    /// Original source byte-for-byte, including BOM and mixed line endings.
    pub source: String,
    /// Source newline and BOM information.
    pub format: SourceFormat,
    /// Lossless pulldown-cmark event tape.
    pub events: Vec<MarkdownEvent>,
    /// Pure structural values for adapters and editors.
    pub blocks: Vec<Block>,
    /// Event and block byte ranges into `source`.
    pub source_map: SourceMap,
}

impl MarkdownDocument {
    /// Parses Markdown with all pulldown-cmark extension options enabled.
    pub fn parse(source: &str) -> Self {
        MarkdownParser.parse(source)
    }

    /// Borrows exact source text for a validated byte range.
    pub fn source_slice(&self, range: SourceRange) -> Result<&str, SourceRangeError> {
        range.slice(&self.source)
    }

    /// Builds a stable table of contents from heading values.
    pub fn toc(&self) -> TableOfContents {
        TableOfContents::from_blocks(&self.blocks)
    }

    /// Preserves the original source exactly for parsed documents.
    pub fn to_markdown(&self) -> String {
        crate::serialize_markdown(self)
    }

    /// Serializes the exposed value model for documents built or edited by adapters.
    pub fn to_canonical_markdown(&self) -> String {
        crate::serialize_canonical_markdown(self)
    }
}

/// Stateless pure Markdown parser.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MarkdownParser;

impl MarkdownParser {
    /// Parses source without reading files, fetching resources, or building UI entities.
    pub fn parse(self, source: &str) -> MarkdownDocument {
        let format = SourceFormat::analyze(source);
        let (parse_source, bom_offset) = match source.strip_prefix('\u{feff}') {
            Some(body) => (body, '\u{feff}'.len_utf8()),
            None => (source, 0),
        };
        let events = parse_events(parse_source, bom_offset);
        let blocks = parse_blocks(source, &events);
        let mut source_map = SourceMap::new(
            source.len(),
            events.iter().map(|event| event.source).collect(),
        );
        let mut block_ranges = Vec::new();
        collect_block_ranges(&blocks, &mut block_ranges);
        source_map.set_block_ranges(block_ranges);
        MarkdownDocument {
            source: source.to_owned(),
            format,
            events,
            blocks,
            source_map,
        }
    }
}

/// Convenience function for parsing source with [`MarkdownParser`].
pub fn parse_markdown(source: &str) -> MarkdownDocument {
    MarkdownParser.parse(source)
}
