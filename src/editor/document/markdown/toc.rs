// @author kongweiguang

//! Editor adapters for the pure Markdown table-of-contents value.

/// Returns whether a line is a standalone Typora table-of-contents marker.
pub(crate) fn is_toc_marker(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case("[toc]")
}

/// Produces a stable anchor id without discarding non-Latin heading text.
#[cfg(test)]
pub(crate) fn heading_slug(title: &str) -> String {
    gmark_markdown::slugify(title)
}

#[cfg(test)]
#[path = "../../../../tests/unit/components/markdown/toc.rs"]
mod tests;
