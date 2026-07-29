// @author kongweiguang

//! Stable table-of-contents extraction from pure heading values.

use std::collections::BTreeMap;

use crate::block::{Block, BlockKind};
use crate::source::SourceRange;

/// A single heading entry suitable for a rendering or navigation adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TocEntry {
    /// Heading level from one through six.
    pub level: u8,
    /// Visible title without Markdown delimiters.
    pub title: String,
    /// Stable anchor ID, explicit when supplied by Markdown attributes.
    pub id: String,
    /// Exact heading range in the original source.
    pub source: SourceRange,
}

/// Pure table-of-contents value in depth-first document order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TableOfContents {
    /// Ordered heading entries.
    pub entries: Vec<TocEntry>,
}

impl TableOfContents {
    /// Collects all headings from blocks and nested structural children.
    pub fn from_blocks(blocks: &[Block]) -> Self {
        let mut entries = Vec::new();
        let mut used_ids = BTreeMap::<String, usize>::new();
        collect_entries(blocks, &mut entries, &mut used_ids);
        Self { entries }
    }

    /// Looks up a TOC entry by its stable identifier.
    pub fn find(&self, id: &str) -> Option<&TocEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Returns whether no heading exists.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Produces a predictable Unicode-preserving anchor slug.
pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;
    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() || character == '_' {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character);
            pending_dash = false;
        } else if character.is_whitespace() || character == '-' {
            pending_dash = !slug.is_empty();
        }
    }
    if slug.is_empty() {
        "section".to_owned()
    } else {
        slug
    }
}

fn collect_entries(
    blocks: &[Block],
    entries: &mut Vec<TocEntry>,
    used_ids: &mut BTreeMap<String, usize>,
) {
    for block in blocks {
        if let BlockKind::Heading(heading) = &block.kind {
            let title = block.plain_text();
            let base_id = heading.id.clone().unwrap_or_else(|| slugify(&title));
            let occurrence = used_ids.entry(base_id.clone()).or_insert(0);
            let id = if *occurrence == 0 {
                base_id
            } else {
                format!("{base_id}-{occurrence}")
            };
            *occurrence += 1;
            entries.push(TocEntry {
                level: heading.level,
                title,
                id,
                source: block.source,
            });
        }
        collect_entries(&block.children, entries, used_ids);
    }
}
