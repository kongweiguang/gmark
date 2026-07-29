// @author kongweiguang

use crate::folding::{FoldCoordinates, FoldKind, FoldRange, line_end, push_fold};

pub(crate) fn markdown_ranges(
    source: &str,
    starts: &[usize],
    coordinates: FoldCoordinates,
) -> Vec<FoldRange> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut headings = Vec::<(usize, usize)>::new();
    let mut fence = None::<(usize, &str)>;
    for (line, text) in lines.iter().enumerate() {
        let trimmed = text.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = if trimmed.starts_with("```") {
                "```"
            } else {
                "~~~"
            };
            if let Some((start, active)) = fence {
                if active == marker {
                    let end = line_end(starts, source.len(), line);
                    if let Some(&start_byte) = starts.get(start) {
                        push_fold(
                            &mut output,
                            starts,
                            coordinates,
                            start_byte,
                            end,
                            FoldKind::MarkdownFence,
                            None,
                        );
                    }
                    fence = None;
                }
            } else {
                fence = Some((line, marker));
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }

        let level = trimmed.bytes().take_while(|byte| *byte == b'#').count();
        if (1..=6).contains(&level) && trimmed.as_bytes().get(level) == Some(&b' ') {
            while let Some((start, prior_level)) = headings.last().copied() {
                if prior_level < level {
                    break;
                }
                headings.pop();
                let end_line = line.saturating_sub(1);
                if end_line > start
                    && let Some(&start_byte) = starts.get(start)
                {
                    push_fold(
                        &mut output,
                        starts,
                        coordinates,
                        start_byte,
                        line_end(starts, source.len(), end_line),
                        FoldKind::MarkdownHeading,
                        None,
                    );
                }
            }
            headings.push((line, level));
        }
    }

    let last = lines.len().saturating_sub(1);
    for (start, _) in headings {
        if last > start
            && let Some(&start_byte) = starts.get(start)
        {
            push_fold(
                &mut output,
                starts,
                coordinates,
                start_byte,
                source.len(),
                FoldKind::MarkdownHeading,
                None,
            );
        }
    }
    output
}

pub(crate) fn indentation_ranges(
    source: &str,
    starts: &[usize],
    coordinates: FoldCoordinates,
) -> Vec<FoldRange> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut stack = Vec::<(usize, usize)>::new();
    for (line, text) in lines.iter().enumerate() {
        if text.trim().is_empty() || text.trim_start().starts_with('#') {
            continue;
        }
        let indent = indentation(text);
        while let Some((start, previous_indent)) = stack.last().copied() {
            if indent > previous_indent {
                break;
            }
            stack.pop();
            let end_line = line.saturating_sub(1);
            if end_line > start
                && let Some(&start_byte) = starts.get(start)
            {
                push_fold(
                    &mut output,
                    starts,
                    coordinates,
                    start_byte,
                    line_end(starts, source.len(), end_line),
                    FoldKind::Indentation,
                    None,
                );
            }
        }
        let trimmed = text.trim_end();
        if trimmed.ends_with(':') || trimmed.ends_with('|') || trimmed.ends_with('>') {
            stack.push((line, indent));
        }
    }

    let last = lines.len().saturating_sub(1);
    for (start, _) in stack {
        if last > start
            && let Some(&start_byte) = starts.get(start)
        {
            push_fold(
                &mut output,
                starts,
                coordinates,
                start_byte,
                source.len(),
                FoldKind::Indentation,
                None,
            );
        }
    }
    output
}

pub(crate) fn toml_ranges(
    source: &str,
    starts: &[usize],
    coordinates: FoldCoordinates,
) -> Vec<FoldRange> {
    let lines = source.lines().collect::<Vec<_>>();
    let headers = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim();
            trimmed.starts_with('[') && trimmed.ends_with(']')
        })
        .map(|(line, _)| line)
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    for (index, start) in headers.iter().copied().enumerate() {
        let end_line = headers
            .get(index + 1)
            .copied()
            .unwrap_or(lines.len())
            .saturating_sub(1);
        if end_line > start
            && let Some(&start_byte) = starts.get(start)
        {
            push_fold(
                &mut output,
                starts,
                coordinates,
                start_byte,
                line_end(starts, source.len(), end_line),
                FoldKind::TomlTable,
                None,
            );
        }
    }
    output
}

pub(crate) fn keyword_ranges(
    language: crate::SourceLanguage,
    source: &str,
    starts: &[usize],
    coordinates: FoldCoordinates,
) -> Vec<FoldRange> {
    let mut output = Vec::new();
    let mut stack = Vec::<(usize, FoldKind)>::new();
    for (line, text) in source.lines().enumerate() {
        let trimmed = text.trim();
        let opener = match language {
            crate::SourceLanguage::Mermaid if trimmed.starts_with("subgraph") => {
                Some(FoldKind::MermaidSubgraph)
            }
            crate::SourceLanguage::Ruby
                if trimmed.starts_with("class ")
                    || trimmed.starts_with("module ")
                    || trimmed.starts_with("def ")
                    || trimmed.starts_with("if ")
                    || trimmed.starts_with("unless ")
                    || trimmed.starts_with("case ")
                    || trimmed.starts_with("while ")
                    || trimmed.starts_with("for ")
                    || trimmed.ends_with(" do") =>
            {
                Some(FoldKind::RubyKeyword)
            }
            crate::SourceLanguage::Bash
                if trimmed.starts_with("if ")
                    || trimmed.starts_with("for ")
                    || trimmed.starts_with("while ")
                    || trimmed.starts_with("case ")
                    || trimmed.ends_with("() {") =>
            {
                Some(FoldKind::BashKeyword)
            }
            _ => None,
        };
        if let Some(kind) = opener {
            stack.push((line, kind));
        }
        let closes = (matches!(
            language,
            crate::SourceLanguage::Mermaid | crate::SourceLanguage::Ruby
        ) && trimmed == "end")
            || (language == crate::SourceLanguage::Bash
                && matches!(trimmed, "fi" | "done" | "esac" | "}"));
        if closes
            && let Some((start, kind)) = stack.pop()
            && let Some(&start_byte) = starts.get(start)
        {
            push_fold(
                &mut output,
                starts,
                coordinates,
                start_byte,
                line_end(starts, source.len(), line),
                kind,
                None,
            );
        }
    }
    output
}

pub(crate) fn html_ranges(
    source: &str,
    starts: &[usize],
    coordinates: FoldCoordinates,
) -> Vec<FoldRange> {
    let bytes = source.as_bytes();
    let mut stack = Vec::<(String, usize)>::new();
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'<' || bytes.get(index + 1).is_none() {
            index += 1;
            continue;
        }
        let close = bytes.get(index + 1) == Some(&b'/');
        let name_start = index + if close { 2 } else { 1 };
        let mut name_end = name_start;
        while bytes
            .get(name_end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b':' | b'_'))
        {
            name_end += 1;
        }
        if name_end == name_start {
            index += 1;
            continue;
        }
        let name = source[name_start..name_end].to_ascii_lowercase();
        let Some(relative_end) = source[name_end..].find('>') else {
            break;
        };
        let tag_end = name_end + relative_end + 1;
        if close {
            if let Some(position) = stack.iter().rposition(|(candidate, _)| *candidate == name) {
                let (_, start) = stack.remove(position);
                push_fold(
                    &mut output,
                    starts,
                    coordinates,
                    start,
                    tag_end,
                    FoldKind::HtmlElement,
                    Some('>'),
                );
            }
        } else if !source[index..tag_end].trim_end().ends_with("/>") && !is_void_html_tag(&name) {
            stack.push((name, index));
        }
        index = tag_end;
    }
    output
}

fn is_void_html_tag(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn indentation(line: &str) -> usize {
    line.bytes()
        .take_while(|byte| matches!(*byte, b' ' | b'\t'))
        .map(|byte| if byte == b'\t' { 4 } else { 1 })
        .sum()
}
