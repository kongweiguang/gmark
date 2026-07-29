// @author kongweiguang

//! Markdown serialization for preserved source and newly edited pure values.

use crate::block::{Block, BlockKind, CodeFence, MetadataKind};
use crate::inline::{Inline, InlineKind, LinkKind, LinkTarget};
use crate::parser::MarkdownDocument;
use crate::table::{Table, TableCell};

/// Serialization policy selected by a caller.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SerializationMode {
    /// Return parsed source byte-for-byte, including BOM and mixed newlines.
    #[default]
    PreserveSource,
    /// Serialize the exposed value model using stable canonical Markdown.
    Canonical,
}

/// Stateless serializer with an explicit source-preservation policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MarkdownSerializer {
    /// Active serialization policy.
    pub mode: SerializationMode,
}

impl MarkdownSerializer {
    /// Creates a serializer with the given policy.
    pub const fn new(mode: SerializationMode) -> Self {
        Self { mode }
    }

    /// Serializes a document according to this serializer's policy.
    pub fn serialize(self, document: &MarkdownDocument) -> String {
        match self.mode {
            SerializationMode::PreserveSource => document.source.clone(),
            SerializationMode::Canonical => serialize_blocks(&document.blocks),
        }
    }
}

/// Returns original parsed Markdown byte-for-byte.
pub fn serialize_markdown(document: &MarkdownDocument) -> String {
    MarkdownSerializer::default().serialize(document)
}

/// Serializes blocks from the public value model with canonical delimiters.
pub fn serialize_canonical_markdown(document: &MarkdownDocument) -> String {
    MarkdownSerializer::new(SerializationMode::Canonical).serialize(document)
}

/// Serializes one inline sequence with canonical Markdown delimiters.
///
/// UI adapters use this at their boundary to retain editor state while
/// delegating pure Markdown spelling to this crate.
pub fn serialize_inlines_canonical(inlines: &[Inline]) -> String {
    serialize_inlines(inlines)
}

/// Serializes one GFM table with canonical outer pipes and delimiters.
pub fn serialize_table_canonical(table: &Table) -> String {
    serialize_table(table)
}

fn serialize_blocks(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(|block| serialize_block(block, ""))
        .filter(|markdown| !markdown.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn serialize_block(block: &Block, indent: &str) -> String {
    match &block.kind {
        BlockKind::Paragraph => format!("{indent}{}", serialize_inlines(&block.inlines)),
        BlockKind::Heading(heading) => {
            let attributes = serialize_heading_attributes(heading);
            format!(
                "{indent}{} {}{}",
                "#".repeat(usize::from(heading.level)),
                serialize_inlines(&block.inlines),
                attributes
            )
        }
        BlockKind::BlockQuote { .. } => serialize_quote(block, indent),
        BlockKind::CodeBlock(code) => {
            serialize_code_block(block, code.fence, code.info.as_deref(), indent)
        }
        BlockKind::Html(document) => prefix_all_lines(&document.raw_source, indent),
        BlockKind::List(list) => serialize_list(block, list.start, indent),
        BlockKind::ListItem { .. } => serialize_list_item(block, "- ", indent),
        BlockKind::FootnoteDefinition { label } => {
            let body = serialize_block_body(block);
            format!("{indent}[^{label}]: {body}")
        }
        BlockKind::DefinitionList => serialize_children_or_inline(block, indent),
        BlockKind::DefinitionListTitle => format!("{indent}{}", serialize_block_body(block)),
        BlockKind::DefinitionListDefinition => format!("{indent}: {}", serialize_block_body(block)),
        BlockKind::Table(table) => prefix_all_lines(&serialize_table(table), indent),
        BlockKind::Metadata(kind) => serialize_metadata(block, *kind, indent),
        BlockKind::ThematicBreak => format!("{indent}---"),
        BlockKind::DisplayMath => format!(
            "{indent}$$\n{}\n{indent}$$",
            prefix_all_lines(&block.plain_text(), indent)
        ),
        BlockKind::RawMarkdown => prefix_all_lines(&block.raw_source, indent),
    }
}

fn serialize_heading_attributes(heading: &crate::Heading) -> String {
    if heading.id.is_none() && heading.classes.is_empty() && heading.attributes.is_empty() {
        return String::new();
    }
    let mut fields = Vec::new();
    if let Some(id) = &heading.id {
        fields.push(format!("#{id}"));
    }
    fields.extend(heading.classes.iter().map(|class| format!(".{class}")));
    fields.extend(
        heading
            .attributes
            .iter()
            .map(|attribute| match &attribute.value {
                Some(value) => format!("{}={value}", attribute.name),
                None => attribute.name.clone(),
            }),
    );
    format!(" {{ {} }}", fields.join(" "))
}

fn serialize_quote(block: &Block, indent: &str) -> String {
    let body = serialize_children_or_inline(block, "");
    prefix_all_lines(&body, &format!("{indent}> "))
}

fn serialize_code_block(
    block: &Block,
    fence: CodeFence,
    info: Option<&str>,
    indent: &str,
) -> String {
    let content = block.plain_text();
    match fence {
        CodeFence::Indented => prefix_all_lines(&content, &format!("{indent}    ")),
        CodeFence::Fenced => {
            let marker = fence_marker(&content);
            let info = info.unwrap_or_default();
            let content = content.trim_end_matches(['\r', '\n']);
            format!("{indent}{marker}{info}\n{content}\n{indent}{marker}")
        }
    }
}

fn fence_marker(content: &str) -> String {
    let mut longest = 0usize;
    let mut current = 0usize;
    for character in content.chars() {
        if character == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

fn serialize_list(block: &Block, start: Option<u64>, indent: &str) -> String {
    let mut items = Vec::new();
    for (index, child) in block.children.iter().enumerate() {
        let prefix = match start {
            Some(start) => format!("{}. ", start + u64::try_from(index).unwrap_or(u64::MAX)),
            None => "- ".to_owned(),
        };
        items.push(serialize_list_item(child, &prefix, indent));
    }
    if items.is_empty() && !block.inlines.is_empty() {
        items.push(format!("{indent}- {}", serialize_inlines(&block.inlines)));
    }
    items.join("\n")
}

fn serialize_list_item(block: &Block, prefix: &str, indent: &str) -> String {
    let task_prefix = match block.task_state() {
        Some(true) => "[x] ",
        Some(false) => "[ ] ",
        None => "",
    };
    let body = serialize_block_body(block);
    let mut lines = body.lines();
    let first = lines.next().unwrap_or_default();
    let continuation = " ".repeat(prefix.chars().count() + task_prefix.chars().count());
    let mut output = format!("{indent}{prefix}{task_prefix}{first}");
    for line in lines {
        output.push('\n');
        output.push_str(indent);
        output.push_str(&continuation);
        output.push_str(line);
    }
    output
}

fn serialize_block_body(block: &Block) -> String {
    if !block.inlines.is_empty() {
        return serialize_inlines(&block.inlines);
    }
    block
        .children
        .iter()
        .map(|child| serialize_block(child, ""))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn serialize_children_or_inline(block: &Block, indent: &str) -> String {
    let body = if block.children.is_empty() {
        serialize_inlines(&block.inlines)
    } else {
        block
            .children
            .iter()
            .map(|child| serialize_block(child, ""))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    prefix_all_lines(&body, indent)
}

fn serialize_metadata(block: &Block, kind: MetadataKind, indent: &str) -> String {
    if !block.raw_source.is_empty() {
        return prefix_all_lines(&block.raw_source, indent);
    }
    let fence = match kind {
        MetadataKind::Yaml => "---",
        MetadataKind::Pluses => "+++",
    };
    format!("{indent}{fence}\n{}\n{indent}{fence}", block.plain_text())
}

fn serialize_table(table: &Table) -> String {
    let columns = table.column_count();
    let header = serialize_table_row(&table.header, columns);
    let delimiter = (0..columns)
        .map(|index| {
            table
                .alignments
                .get(index)
                .copied()
                .unwrap_or_default()
                .delimiter()
        })
        .collect::<Vec<_>>();
    let mut lines = vec![header, format!("| {} |", delimiter.join(" | "))];
    lines.extend(
        table
            .rows
            .iter()
            .map(|row| serialize_table_row(row, columns)),
    );
    lines.join("\n")
}

fn serialize_table_row(row: &[TableCell], columns: usize) -> String {
    let cells = (0..columns)
        .map(|index| {
            row.get(index)
                .map(|cell| serialize_inlines(&cell.inlines))
                .unwrap_or_default()
                .replace('\\', "\\\\")
                .replace('|', "\\|")
                .replace('\n', " ")
        })
        .collect::<Vec<_>>();
    format!("| {} |", cells.join(" | "))
}

fn serialize_inlines(inlines: &[Inline]) -> String {
    inlines.iter().map(serialize_inline).collect()
}

fn serialize_inline(inline: &Inline) -> String {
    let children = serialize_inlines(&inline.children);
    match &inline.kind {
        InlineKind::Text(value) => escape_inline_text(value),
        InlineKind::Code(value) => serialize_inline_code(value),
        InlineKind::InlineMath(value) => format!("${value}$"),
        InlineKind::SoftBreak => "\n".to_owned(),
        InlineKind::HardBreak => "\\\n".to_owned(),
        InlineKind::Emphasis => format!("*{children}*"),
        InlineKind::Strong => format!("**{children}**"),
        InlineKind::Strikethrough => format!("~~{children}~~"),
        InlineKind::Superscript => format!("^{children}^"),
        InlineKind::Subscript => format!("~{children}~"),
        InlineKind::Link(target) => serialize_link(target, &children, false),
        InlineKind::Image(target) => serialize_link(target, &children, true),
        InlineKind::Html(document) => document.raw_source.clone(),
        InlineKind::FootnoteReference(label) => format!("[^{label}]"),
        InlineKind::TaskListMarker(_) => String::new(),
    }
}

fn serialize_link(target: &LinkTarget, label: &str, image: bool) -> String {
    let prefix = if image { "!" } else { "" };
    match target.kind {
        LinkKind::Reference => format!("{prefix}[{label}][{}]", target.reference),
        LinkKind::Collapsed => format!("{prefix}[{label}][]"),
        LinkKind::Shortcut => format!("{prefix}[{label}]"),
        LinkKind::WikiLink { piped } => {
            if piped {
                format!("[[{}|{label}]]", target.destination)
            } else {
                format!("[[{}]]", target.destination)
            }
        }
        LinkKind::Autolink if !image => format!("<{}>", target.destination),
        LinkKind::Email if !image => format!("<{}>", target.destination),
        LinkKind::Inline
        | LinkKind::ReferenceUnknown
        | LinkKind::CollapsedUnknown
        | LinkKind::ShortcutUnknown
        | LinkKind::Autolink
        | LinkKind::Email => {
            let title = (!target.title.is_empty()).then(|| format!(" \"{}\"", target.title));
            format!(
                "{prefix}[{label}]({}{})",
                escape_destination(&target.destination),
                title.unwrap_or_default()
            )
        }
    }
}

fn serialize_inline_code(value: &str) -> String {
    let marker = fence_marker(value);
    format!("{marker}{value}{marker}")
}

fn escape_inline_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn escape_destination(value: &str) -> String {
    if value
        .chars()
        .any(|character| character.is_whitespace() || matches!(character, '(' | ')' | '"'))
    {
        format!("<{}>", value.replace('>', "%3E").replace('<', "%3C"))
    } else {
        value.replace(')', "\\)").replace('(', "\\(")
    }
}

fn prefix_all_lines(value: &str, prefix: &str) -> String {
    value
        .split('\n')
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
