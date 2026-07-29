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

fn indentation(line: &str) -> usize {
    line.bytes()
        .take_while(|byte| matches!(*byte, b' ' | b'\t'))
        .map(|byte| if byte == b'\t' { 4 } else { 1 })
        .sum()
}
