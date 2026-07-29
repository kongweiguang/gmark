// @author kongweiguang

//! Editor adapters for the pure Markdown table-of-contents value.

/// A heading eligible for a document table of contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TocEntry {
    pub(crate) level: u8,
    pub(crate) title: String,
    pub(crate) slug: String,
}

/// Returns whether a line is a standalone Typora table-of-contents marker.
pub(crate) fn is_toc_marker(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case("[toc]")
}

/// Collects headings through the rendering-neutral Markdown value model.
///
/// The parser retains the original source and source ranges; this adapter only
/// projects the fields the existing editor and HTML exporter consume.
pub(crate) fn collect_toc_entries(markdown: &str) -> Vec<TocEntry> {
    gmark_markdown::parse_markdown(markdown)
        .toc()
        .entries
        .into_iter()
        .map(|entry| TocEntry {
            level: entry.level,
            title: entry.title,
            slug: entry.id,
        })
        .collect()
}

/// Produces a stable anchor id without discarding non-Latin heading text.
#[cfg(test)]
pub(crate) fn heading_slug(title: &str) -> String {
    gmark_markdown::slugify(title)
}

#[cfg(test)]
#[path = "../../../tests/unit/components/markdown/toc.rs"]
mod tests;
