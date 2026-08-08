// @author kongweiguang

//! 无 UI 依赖的、保真 LaTeX 公式编辑领域模型。
//!
//! 这里的 AST 不是 LaTeX 合法性的裁决器。它仅在语义可以确定时提升节点；其余
//! 输入保留为 [`MathNode::Opaque`]，因此编辑器可在用户尚未完成输入时安全往返源码。

use std::{error::Error, fmt, ops::Range};

#[path = "parser.rs"]
mod parser;

#[path = "ast.rs"]
mod ast;
pub use ast::*;
#[path = "cursor.rs"]
mod cursor;
pub use cursor::*;
#[path = "editor.rs"]
mod editor;
pub use editor::*;
#[path = "geometry.rs"]
mod geometry;
pub use geometry::*;

/// Stable row-major order of the 5×8 symbol palette shared by UI and tests.
pub const MATH_SYMBOL_PALETTE_KEYS: [&str; 40] = [
    "fraction",
    "sqrt",
    "nth_root",
    "matrix",
    "paren",
    "bracket",
    "brace",
    "abs",
    "norm",
    "angle",
    "floor",
    "ceil",
    "integral",
    "sum",
    "product",
    "infinity",
    "pi",
    "theta",
    "alpha",
    "beta",
    "gamma",
    "delta",
    "lambda",
    "mu",
    "sigma",
    "phi",
    "omega",
    "uppercase_delta",
    "less_or_equal",
    "greater_or_equal",
    "not_equal",
    "approximately",
    "times",
    "divide",
    "dot",
    "plus_minus",
    "right_arrow",
    "partial",
    "nabla",
    "member",
];

/// Compatibility alias for hosts that call the stateful runner a session.
pub type MathEditSession = MathEditor;

/// 领域编辑错误。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MathEditError {
    InvalidRange { range: Range<usize>, len: usize },
    InvalidCursorOffset { offset: usize, len: usize },
    UnknownPath(MathPath),
    ParentIsNotSequence(MathPath),
    RootOperation,
    OpaqueDocument,
    InvalidSlot(MathSlot),
}
impl fmt::Display for MathEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRange { range, len } => {
                write!(formatter, "invalid LaTeX range {range:?} for length {len}")
            }
            Self::InvalidCursorOffset { offset, len } => write!(
                formatter,
                "invalid LaTeX cursor offset {offset} for length {len}"
            ),
            Self::UnknownPath(path) => write!(formatter, "unknown math AST path {path:?}"),
            Self::ParentIsNotSequence(path) => write!(
                formatter,
                "math AST path parent is not a sequence: {path:?}"
            ),
            Self::RootOperation => {
                formatter.write_str("operation is not defined for the math AST root")
            }
            Self::OpaqueDocument => {
                formatter.write_str("operation requires a structured math document")
            }
            Self::InvalidSlot(slot) => write!(formatter, "invalid math editor slot: {slot:?}"),
        }
    }
}

impl Error for MathEditError {}

fn validate_range(source: &str, range: &Range<usize>) -> Result<(), MathEditError> {
    if range.start > range.end
        || range.end > source.len()
        || !source.is_char_boundary(range.start)
        || !source.is_char_boundary(range.end)
    {
        return Err(MathEditError::InvalidRange {
            range: range.clone(),
            len: source.len(),
        });
    }
    Ok(())
}

fn validate_cursor_offset(source: &str, offset: usize) -> Result<(), MathEditError> {
    if offset > source.len() || !source.is_char_boundary(offset) {
        return Err(MathEditError::InvalidCursorOffset {
            offset,
            len: source.len(),
        });
    }
    Ok(())
}

fn node_at<'a>(node: &'a MathNode, path: &[usize]) -> Option<&'a MathNode> {
    let Some((&index, rest)) = path.split_first() else {
        return Some(node);
    };
    match node {
        MathNode::Sequence(children) => children.get(index).and_then(|child| node_at(child, rest)),
        MathNode::Group(content)
        | MathNode::Superscript(content)
        | MathNode::Subscript(content)
        | MathNode::TextMode(content)
        | MathNode::Delimited { body: content, .. }
        | MathNode::Accent { value: content, .. } => {
            (index == 0).then(|| node_at(content, rest)).flatten()
        }
        MathNode::Fraction {
            numerator,
            denominator,
        } => match index {
            0 => node_at(numerator, rest),
            1 => node_at(denominator, rest),
            _ => None,
        },
        MathNode::SquareRoot {
            index: root_index,
            radicand,
        } => match index {
            0 if root_index.is_some() => node_at(root_index.as_deref()?, rest),
            0 if root_index.is_none() => node_at(radicand, rest),
            1 if root_index.is_some() => node_at(radicand, rest),
            _ => None,
        },
        MathNode::Text(_)
        | MathNode::Symbol { .. }
        | MathNode::BigOperator { .. }
        | MathNode::Command { .. }
        | MathNode::Environment { .. }
        | MathNode::Opaque(_) => None,
    }
}

fn node_at_mut<'a>(node: &'a mut MathNode, path: &[usize]) -> Option<&'a mut MathNode> {
    let Some((&index, rest)) = path.split_first() else {
        return Some(node);
    };
    match node {
        MathNode::Sequence(children) => children
            .get_mut(index)
            .and_then(|child| node_at_mut(child, rest)),
        MathNode::Group(content)
        | MathNode::Superscript(content)
        | MathNode::Subscript(content)
        | MathNode::TextMode(content)
        | MathNode::Delimited { body: content, .. }
        | MathNode::Accent { value: content, .. } => {
            (index == 0).then(|| node_at_mut(content, rest)).flatten()
        }
        MathNode::Fraction {
            numerator,
            denominator,
        } => match index {
            0 => node_at_mut(numerator, rest),
            1 => node_at_mut(denominator, rest),
            _ => None,
        },
        MathNode::SquareRoot {
            index: root_index,
            radicand,
        } => match index {
            0 if root_index.is_some() => node_at_mut(root_index.as_deref_mut()?, rest),
            0 if root_index.is_none() => node_at_mut(radicand, rest),
            1 if root_index.is_some() => node_at_mut(radicand, rest),
            _ => None,
        },
        MathNode::Text(_)
        | MathNode::Symbol { .. }
        | MathNode::BigOperator { .. }
        | MathNode::Command { .. }
        | MathNode::Environment { .. }
        | MathNode::Opaque(_) => None,
    }
}

fn replace_node(
    node: &mut MathNode,
    path: &[usize],
    replacement: MathNode,
) -> Result<(), MathEditError> {
    let Some((&index, parent_path)) = path.split_last() else {
        *node = replacement;
        return Ok(());
    };
    let parent = node_at_mut(node, parent_path)
        .ok_or_else(|| MathEditError::UnknownPath(MathPath(path.to_vec())))?;
    match parent {
        MathNode::Sequence(children) => {
            let slot = children
                .get_mut(index)
                .ok_or_else(|| MathEditError::UnknownPath(MathPath(path.to_vec())))?;
            *slot = replacement;
        }
        MathNode::Group(content)
        | MathNode::Superscript(content)
        | MathNode::Subscript(content)
        | MathNode::Delimited { body: content, .. }
            if index == 0 =>
        {
            **content = replacement
        }
        MathNode::Fraction {
            numerator,
            denominator,
        } => match index {
            0 => **numerator = replacement,
            1 => **denominator = replacement,
            _ => return Err(MathEditError::UnknownPath(MathPath(path.to_vec()))),
        },
        MathNode::SquareRoot {
            index: root_index,
            radicand,
        } => match (index, root_index.as_mut()) {
            (0, Some(index)) => **index = replacement,
            (0, None) => **radicand = replacement,
            (1, Some(_)) => **radicand = replacement,
            _ => return Err(MathEditError::UnknownPath(MathPath(path.to_vec()))),
        },
        _ => return Err(MathEditError::UnknownPath(MathPath(path.to_vec()))),
    }
    Ok(())
}

fn source_range_at(node: &MathNode, path: &[usize], start: usize) -> Option<Range<usize>> {
    if path.is_empty() {
        return Some(start..start + node.to_latex().len());
    }
    let (&index, rest) = path.split_first()?;
    match node {
        MathNode::Sequence(children) => {
            let mut offset = start;
            for (child_index, child) in children.iter().enumerate() {
                let end = offset + child.to_latex().len();
                if child_index == index {
                    return source_range_at(child, rest, offset);
                }
                offset = end;
            }
            None
        }
        MathNode::Group(content)
        | MathNode::Superscript(content)
        | MathNode::Subscript(content)
        | MathNode::TextMode(content)
        | MathNode::Delimited { body: content, .. }
        | MathNode::Accent { value: content, .. }
            if index == 0 =>
        {
            let prefix = match node {
                MathNode::Group(_) => 1,
                MathNode::Superscript(_) | MathNode::Subscript(_) => 1,
                MathNode::TextMode(_) => 6,
                MathNode::Delimited { pair, .. } => {
                    // Alphabetic delimiter commands require a separating
                    // space when the body does not already begin with one.
                    // Keep the body range aligned with `wrap_body`; an
                    // omitted byte here makes every cursor/edit in angle,
                    // floor, and ceiling delimiters target the wrong text.
                    let separator = pair
                        .open()
                        .chars()
                        .last()
                        .is_some_and(|character| character.is_ascii_alphabetic())
                        && !content
                            .to_latex()
                            .chars()
                            .next()
                            .is_some_and(char::is_whitespace);
                    5 + pair.open().len() + usize::from(separator)
                }
                MathNode::Accent { name, .. } => name.len() + 2,
                _ => 0,
            };
            source_range_at(content, rest, start + prefix)
        }
        MathNode::Fraction {
            numerator,
            denominator,
        } => match index {
            0 => source_range_at(numerator, rest, start + 6),
            1 => {
                let numerator_len = numerator.to_latex().len();
                // `\frac{` + numerator + `}{` precedes the denominator.
                source_range_at(denominator, rest, start + 8 + numerator_len)
            }
            _ => None,
        },
        MathNode::SquareRoot {
            index: root_index,
            radicand,
        } => {
            let command_len = 5;
            if let Some(root_index) = root_index {
                match index {
                    0 => source_range_at(root_index, rest, start + command_len + 1),
                    1 => source_range_at(
                        radicand,
                        rest,
                        start + command_len + 3 + root_index.to_latex().len(),
                    ),
                    _ => None,
                }
            } else if index == 0 {
                source_range_at(radicand, rest, start + command_len + 1)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn collect_paths(node: &MathNode, path: &MathPath, output: &mut Vec<MathPath>) {
    output.push(path.clone());
    for (index, child) in node.children().into_iter().enumerate() {
        collect_paths(child, &path.child(index), output);
    }
}

fn slot_source(document: &MathDocument, slot: &MathSlot) -> Result<String, MathEditError> {
    if let MathSlotRole::EnvironmentCell { .. } = slot.role {
        let source = document.to_latex();
        let range = environment_cell_range(&source, slot)?;
        return Ok(source[range].to_owned());
    }
    let Some(ast) = document.ast() else {
        if slot.path.is_root() {
            return Ok(document.to_latex());
        }
        return Err(MathEditError::OpaqueDocument);
    };
    ast.node(&slot.path)
        .map(MathNode::to_latex)
        .ok_or_else(|| MathEditError::InvalidSlot(slot.clone()))
}

#[derive(Clone, Debug)]
struct EnvironmentCellRange {
    row: usize,
    column: usize,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug)]
struct EnvironmentGrid {
    cells: Vec<EnvironmentCellRange>,
}

fn environment_grid(source: &str) -> Option<EnvironmentGrid> {
    let open_end = source.find('}')?.checked_add(1)?;
    let close_start = source.rfind("\\end{")?;
    if close_start < open_end {
        return None;
    }
    let body_start = open_end;
    let body_end = close_start;
    let mut cells = Vec::new();
    let mut row = 0;
    let mut column = 0;
    let mut cell_start = body_start;
    let mut offset = body_start;
    let mut brace_depth = 0usize;
    let mut nested_environment_depth = 0usize;
    let bytes = source.as_bytes();
    while offset < body_end {
        let character = source[offset..].chars().next()?;
        let width = character.len_utf8();
        // Nested environments contain their own `&` and row separators.  Do
        // not expose those separators as cells of the outer grid; they remain
        // editable through the cell's source slot instead.
        if source[offset..].starts_with("\\begin{")
            && let Some(close) = source[offset + 7..body_end].find('}')
        {
            nested_environment_depth = nested_environment_depth.saturating_add(1);
            offset += 7 + close + 1;
            continue;
        }
        if source[offset..].starts_with("\\end{")
            && let Some(close) = source[offset + 5..body_end].find('}')
        {
            nested_environment_depth = nested_environment_depth.saturating_sub(1);
            offset += 5 + close + 1;
            continue;
        }
        match character {
            '{' | '[' => brace_depth = brace_depth.saturating_add(1),
            '}' | ']' => brace_depth = brace_depth.saturating_sub(1),
            '&' if brace_depth == 0 && nested_environment_depth == 0 => {
                cells.push(EnvironmentCellRange {
                    row,
                    column,
                    start: cell_start,
                    end: offset,
                });
                column += 1;
                cell_start = offset + width;
            }
            '\\' if brace_depth == 0
                && nested_environment_depth == 0
                && offset + 1 < body_end
                && bytes.get(offset + 1) == Some(&b'\\') =>
            {
                cells.push(EnvironmentCellRange {
                    row,
                    column,
                    start: cell_start,
                    end: offset,
                });
                row += 1;
                column = 0;
                offset += 1;
                cell_start = offset + 1;
            }
            _ => {}
        }
        offset += width;
    }
    if cell_start <= body_end {
        cells.push(EnvironmentCellRange {
            row,
            column,
            start: cell_start,
            end: body_end,
        });
    }
    // Empty environments have one empty editable cell, matching the behavior
    // of a fresh matrix template.
    if body_start == body_end && cells.is_empty() {
        cells.push(EnvironmentCellRange {
            row: 0,
            column: 0,
            start: body_start,
            end: body_end,
        });
    }
    Some(EnvironmentGrid { cells })
}

fn environment_cell_range(source: &str, slot: &MathSlot) -> Result<Range<usize>, MathEditError> {
    let MathSlotRole::EnvironmentCell { row, column } = slot.role else {
        return Err(MathEditError::InvalidSlot(slot.clone()));
    };
    let ast = MathAst::parse(source);
    let env_range = ast
        .source_range(slot.path())
        .ok_or_else(|| MathEditError::InvalidSlot(slot.clone()))?;
    let environment_source = source
        .get(env_range.clone())
        .ok_or_else(|| MathEditError::InvalidSlot(slot.clone()))?;
    let open = environment_source
        .find("\\begin{")
        .ok_or_else(|| MathEditError::InvalidSlot(slot.clone()))?;
    let open_end = environment_source[open..]
        .find('}')
        .map(|index| open + index + 1)
        .ok_or_else(|| MathEditError::InvalidSlot(slot.clone()))?;
    let name = environment_source[open + 7..open_end - 1].to_owned();
    let close = matching_environment_close(environment_source, open_end, &name)
        .ok_or_else(|| MathEditError::InvalidSlot(slot.clone()))?;
    let mut cells = Vec::new();
    let mut row_index = 0;
    let mut column_index = 0;
    let mut start = open_end;
    let mut offset = open_end;
    let mut depth = 0usize;
    let mut nested_environment_depth = 0usize;
    let bytes = environment_source.as_bytes();
    while offset < close {
        let character = environment_source[offset..]
            .chars()
            .next()
            .ok_or_else(|| MathEditError::InvalidSlot(slot.clone()))?;
        let width = character.len_utf8();
        if environment_source[offset..].starts_with("\\begin{")
            && let Some(end) = environment_source[offset + 7..close].find('}')
        {
            nested_environment_depth = nested_environment_depth.saturating_add(1);
            offset += 7 + end + 1;
            continue;
        }
        if environment_source[offset..].starts_with("\\end{")
            && let Some(end) = environment_source[offset + 5..close].find('}')
        {
            nested_environment_depth = nested_environment_depth.saturating_sub(1);
            offset += 5 + end + 1;
            continue;
        }
        match character {
            '{' | '[' => depth += 1,
            '}' | ']' => depth = depth.saturating_sub(1),
            '&' if depth == 0 && nested_environment_depth == 0 => {
                cells.push((row_index, column_index, start..offset));
                column_index += 1;
                start = offset + width;
            }
            '\\' if depth == 0
                && nested_environment_depth == 0
                && offset + 1 < close
                && bytes[offset + 1] == b'\\' =>
            {
                cells.push((row_index, column_index, start..offset));
                row_index += 1;
                column_index = 0;
                offset += 1;
                start = offset + 1;
            }
            _ => {}
        }
        offset += width;
    }
    cells.push((row_index, column_index, start..close));
    cells
        .into_iter()
        .find(|(cell_row, cell_column, _)| *cell_row == row && *cell_column == column)
        .map(|(_, _, range)| (env_range.start + range.start)..(env_range.start + range.end))
        .ok_or_else(|| MathEditError::InvalidSlot(slot.clone()))
}

fn matching_environment_close(source: &str, body_start: usize, name: &str) -> Option<usize> {
    let mut depth = 1usize;
    let mut offset = body_start;
    while offset < source.len() {
        if source[offset..].starts_with("\\begin{")
            && let Some(end) = source[offset + 7..].find('}')
        {
            depth = depth.saturating_add(1);
            offset += 7 + end + 1;
            continue;
        }
        if source[offset..].starts_with("\\end{")
            && let Some(end) = source[offset + 5..].find('}')
        {
            let candidate = &source[offset + 5..offset + 5 + end];
            if candidate == name {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(offset);
                }
            }
            offset += 5 + end + 1;
            continue;
        }
        offset += source[offset..].chars().next()?.len_utf8();
    }
    None
}

fn wrap_template(
    target: &(Range<usize>, String),
    command: &str,
    open: &str,
    close: &str,
) -> (Range<usize>, String, usize) {
    let selected = &target.1[target.0.clone()];
    let replacement = if close.is_empty() {
        let _ = open;
        format!("{command}{{{selected}}}")
    } else {
        format!("{command}{{{selected}}}{{}}")
    };
    let cursor = target.0.start + replacement.len();
    (target.0.clone(), replacement, cursor)
}

fn wrap_suffix(target: &(Range<usize>, String), suffix: &str) -> (Range<usize>, String, usize) {
    let selected = &target.1[target.0.clone()];
    let replacement = format!("{selected}{suffix}");
    let cursor = target.0.start + replacement.len();
    (target.0.clone(), replacement, cursor)
}

fn wrap_environment(
    target: &(Range<usize>, String),
    name: &str,
    rows: usize,
    columns: usize,
) -> (Range<usize>, String, usize) {
    let selected = &target.1[target.0.clone()];
    let rows = rows.max(1);
    let columns = columns.max(1);
    let mut body = String::new();
    for row in 0..rows {
        if row > 0 {
            body.push_str(" \\\\ ");
        }
        for column in 0..columns {
            if column > 0 {
                body.push_str(" & ");
            }
            if row == 0 && column == 0 {
                body.push_str(selected);
            }
        }
    }
    let replacement = format!("\\begin{{{name}}}{body}\\end{{{name}}}");
    let cursor = target.0.start + replacement.len();
    (target.0.clone(), replacement, cursor)
}
