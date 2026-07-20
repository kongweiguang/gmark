// @author kongweiguang

//! Source-preserving helpers for Typora-compatible document tables of contents.

use std::collections::HashMap;

use super::inline::InlineTextTree;

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

/// Collects headings from source without interpreting frontmatter, comments or code fences.
/// The source stays untouched; callers can render this projection wherever `[TOC]` appears.
pub(crate) fn collect_toc_entries(markdown: &str) -> Vec<TocEntry> {
    let lines = markdown.lines().collect::<Vec<_>>();
    let mut entries = Vec::new();
    let mut slugs = HashMap::<String, usize>::new();
    let mut index = frontmatter_end(&lines).unwrap_or(0);
    let mut fence: Option<(char, usize)> = None;
    let mut comment_open = false;

    while index < lines.len() {
        let line = lines[index];
        if let Some((marker, length)) = fence {
            if closes_fence(line, marker, length) {
                fence = None;
            }
            index += 1;
            continue;
        }
        if let Some(next_fence) = opens_fence(line) {
            fence = Some(next_fence);
            index += 1;
            continue;
        }
        if comment_open {
            comment_open = !line.contains("-->");
            index += 1;
            continue;
        }
        if line.trim_start().starts_with("<!--") {
            comment_open = !line.contains("-->");
            index += 1;
            continue;
        }

        if let Some((level, title)) = atx_heading(line) {
            push_entry(&mut entries, &mut slugs, level, title);
            index += 1;
            continue;
        }
        if let Some(level) = lines
            .get(index + 1)
            .and_then(|underline| setext_level(underline))
            .filter(|_| !line.trim().is_empty())
        {
            push_entry(&mut entries, &mut slugs, level, line.trim().to_string());
            index += 2;
            continue;
        }
        index += 1;
    }
    entries
}

fn push_entry(
    entries: &mut Vec<TocEntry>,
    slugs: &mut HashMap<String, usize>,
    level: u8,
    raw: String,
) {
    let title = InlineTextTree::from_markdown(&raw)
        .visible_text()
        .trim()
        .to_string();
    if title.is_empty() {
        return;
    }
    let base = heading_slug(&title);
    let count = slugs.entry(base.clone()).or_insert(0);
    *count += 1;
    let slug = if *count == 1 {
        base
    } else {
        format!("{base}-{}", *count - 1)
    };
    entries.push(TocEntry { level, title, slug });
}

/// Produces a stable anchor id without discarding non-Latin heading text.
pub(crate) fn heading_slug(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_separator = false;
    for ch in title.trim().chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' || !ch.is_ascii() {
            slug.extend(ch.to_lowercase());
            previous_separator = false;
        } else if !previous_separator && !slug.is_empty() {
            slug.push('-');
            previous_separator = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "section".to_string()
    } else {
        slug.to_string()
    }
}

fn atx_heading(line: &str) -> Option<(u8, String)> {
    let trimmed = line.trim_start();
    let level = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&level)
        || !trimmed
            .as_bytes()
            .get(level)
            .is_some_and(u8::is_ascii_whitespace)
    {
        return None;
    }
    let title = trimmed[level..].trim();
    Some((
        level as u8,
        title.trim_end_matches('#').trim_end().to_string(),
    ))
}

fn setext_level(line: &str) -> Option<u8> {
    let marker = line.trim();
    (!marker.is_empty() && marker.chars().all(|ch| ch == '='))
        .then_some(1)
        .or_else(|| (!marker.is_empty() && marker.chars().all(|ch| ch == '-')).then_some(2))
}

fn opens_fence(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let length = trimmed.chars().take_while(|ch| *ch == marker).count();
    (length >= 3).then_some((marker, length))
}

fn closes_fence(line: &str, marker: char, length: usize) -> bool {
    let trimmed = line.trim();
    trimmed.chars().take_while(|ch| *ch == marker).count() >= length
}

fn frontmatter_end(lines: &[&str]) -> Option<usize> {
    (lines.first()?.trim_start_matches('\u{feff}').trim() == "---").then(|| {
        lines
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(index, line)| matches!(line.trim(), "---" | "...").then_some(index + 1))
    })?
}

#[cfg(test)]
#[path = "../../../tests/unit/components/markdown/toc.rs"]
mod tests;
