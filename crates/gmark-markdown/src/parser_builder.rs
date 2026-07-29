// @author kongweiguang

//! Internal structural builder for the public Markdown parser.

use crate::block::{Block, BlockKind};
use crate::event::{MarkdownEvent, MarkdownEventKind, MarkdownTag, MarkdownTagEnd};
use crate::html::HtmlDocument;
use crate::inline::{Inline, InlineKind};
use crate::resource::ResourceRecord;
use crate::source::SourceRange;
use crate::table::{Table, TableCell};

pub(crate) fn parse_blocks(source: &str, events: &[MarkdownEvent]) -> Vec<Block> {
    let mut builder = BlockBuilder::new(source);
    for event in events {
        builder.push(event);
    }
    builder.finish()
}

pub(crate) fn collect_block_ranges(blocks: &[Block], ranges: &mut Vec<SourceRange>) {
    for block in blocks {
        ranges.push(block.source);
        collect_block_ranges(&block.children, ranges);
    }
}

struct BlockBuilder<'a> {
    source: &'a str,
    frames: Vec<Frame>,
    inline_frames: Vec<InlineFrame>,
    roots: Vec<Block>,
}

enum Frame {
    Block(BlockFrame),
    Table(TableFrame),
    TableHead,
    TableRow,
    TableCell(CellFrame),
}

struct BlockFrame {
    kind: BlockKind,
    start: usize,
    inlines: Vec<Inline>,
    children: Vec<Block>,
}

struct TableFrame {
    start: usize,
    table: Table,
    current_row: Vec<TableCell>,
    in_header: bool,
}

struct CellFrame {
    start: usize,
    inlines: Vec<Inline>,
}

struct InlineFrame {
    kind: InlineKind,
    start: usize,
    children: Vec<Inline>,
}

impl<'a> BlockBuilder<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            frames: Vec::new(),
            inline_frames: Vec::new(),
            roots: Vec::new(),
        }
    }

    fn push(&mut self, event: &MarkdownEvent) {
        match &event.kind {
            MarkdownEventKind::Start(tag) => self.start(tag.clone(), event.source),
            MarkdownEventKind::End(tag) => self.end(*tag, event.source),
            MarkdownEventKind::Text(value) => self.push_inline(Inline::parsed(
                InlineKind::Text(value.clone()),
                event.source,
                Vec::new(),
            )),
            MarkdownEventKind::Code(value) => self.push_inline(Inline::parsed(
                InlineKind::Code(value.clone()),
                event.source,
                Vec::new(),
            )),
            MarkdownEventKind::InlineMath(value) => self.push_inline(Inline::parsed(
                InlineKind::InlineMath(value.clone()),
                event.source,
                Vec::new(),
            )),
            MarkdownEventKind::DisplayMath(value) => self.push_display_math(value, event.source),
            MarkdownEventKind::Html(value) | MarkdownEventKind::InlineHtml(value) => {
                self.push_inline(Inline::parsed(
                    InlineKind::Html(HtmlDocument::parse(value)),
                    event.source,
                    Vec::new(),
                ));
            }
            MarkdownEventKind::FootnoteReference(value) => self.push_inline(Inline::parsed(
                InlineKind::FootnoteReference(value.clone()),
                event.source,
                Vec::new(),
            )),
            MarkdownEventKind::SoftBreak => self.push_inline(Inline::parsed(
                InlineKind::SoftBreak,
                event.source,
                Vec::new(),
            )),
            MarkdownEventKind::HardBreak => self.push_inline(Inline::parsed(
                InlineKind::HardBreak,
                event.source,
                Vec::new(),
            )),
            MarkdownEventKind::Rule => self.push_rule(event.source),
            MarkdownEventKind::TaskListMarker(checked) => {
                self.push_task_marker(*checked, event.source)
            }
        }
    }

    fn start(&mut self, tag: MarkdownTag, source: SourceRange) {
        match tag {
            MarkdownTag::Paragraph => self.open_block(BlockKind::Paragraph, source),
            MarkdownTag::Heading(heading) => self.open_block(BlockKind::Heading(heading), source),
            MarkdownTag::BlockQuote { callout } => {
                self.open_block(BlockKind::BlockQuote { callout }, source)
            }
            MarkdownTag::CodeBlock(code) => self.open_block(BlockKind::CodeBlock(code), source),
            MarkdownTag::HtmlBlock => {
                self.open_block(BlockKind::Html(HtmlDocument::raw(String::new())), source)
            }
            MarkdownTag::List(list) => self.open_block(BlockKind::List(list), source),
            MarkdownTag::Item => self.open_block(BlockKind::ListItem { task: None }, source),
            MarkdownTag::FootnoteDefinition { label } => {
                self.open_block(BlockKind::FootnoteDefinition { label }, source)
            }
            MarkdownTag::DefinitionList => self.open_block(BlockKind::DefinitionList, source),
            MarkdownTag::DefinitionListTitle => {
                self.open_block(BlockKind::DefinitionListTitle, source)
            }
            MarkdownTag::DefinitionListDefinition => {
                self.open_block(BlockKind::DefinitionListDefinition, source)
            }
            MarkdownTag::Table(alignments) => self.frames.push(Frame::Table(TableFrame {
                start: source.start,
                table: Table {
                    alignments,
                    header: Vec::new(),
                    rows: Vec::new(),
                },
                current_row: Vec::new(),
                in_header: false,
            })),
            MarkdownTag::TableHead => {
                self.set_table_header_mode(true);
                self.frames.push(Frame::TableHead);
            }
            MarkdownTag::TableRow => self.frames.push(Frame::TableRow),
            MarkdownTag::TableCell => self.frames.push(Frame::TableCell(CellFrame {
                start: source.start,
                inlines: Vec::new(),
            })),
            MarkdownTag::Emphasis => self.open_inline(InlineKind::Emphasis, source),
            MarkdownTag::Strong => self.open_inline(InlineKind::Strong, source),
            MarkdownTag::Strikethrough => self.open_inline(InlineKind::Strikethrough, source),
            MarkdownTag::Superscript => self.open_inline(InlineKind::Superscript, source),
            MarkdownTag::Subscript => self.open_inline(InlineKind::Subscript, source),
            MarkdownTag::Link(target) => self.open_inline(InlineKind::Link(target), source),
            MarkdownTag::Image(target) => self.open_inline(InlineKind::Image(target), source),
            MarkdownTag::Metadata(kind) => self.open_block(BlockKind::Metadata(kind), source),
        }
    }

    fn end(&mut self, tag: MarkdownTagEnd, source: SourceRange) {
        match tag {
            MarkdownTagEnd::Paragraph
            | MarkdownTagEnd::Heading
            | MarkdownTagEnd::BlockQuote
            | MarkdownTagEnd::CodeBlock
            | MarkdownTagEnd::HtmlBlock
            | MarkdownTagEnd::List
            | MarkdownTagEnd::Item
            | MarkdownTagEnd::FootnoteDefinition
            | MarkdownTagEnd::DefinitionList
            | MarkdownTagEnd::DefinitionListTitle
            | MarkdownTagEnd::DefinitionListDefinition
            | MarkdownTagEnd::Metadata => self.close_block(source),
            MarkdownTagEnd::Table => self.close_table(source),
            MarkdownTagEnd::TableHead => {
                if matches!(self.frames.last(), Some(Frame::TableHead)) {
                    self.frames.pop();
                }
                // pulldown-cmark emits header cells directly under `TableHead`
                // (without a `TableRow` wrapper), so close that implicit row
                // before switching subsequent rows to the table body.
                self.finish_table_row();
                self.set_table_header_mode(false);
            }
            MarkdownTagEnd::TableRow => {
                if matches!(self.frames.last(), Some(Frame::TableRow)) {
                    self.frames.pop();
                }
                self.finish_table_row();
            }
            MarkdownTagEnd::TableCell => self.close_table_cell(source),
            MarkdownTagEnd::Emphasis
            | MarkdownTagEnd::Strong
            | MarkdownTagEnd::Strikethrough
            | MarkdownTagEnd::Superscript
            | MarkdownTagEnd::Subscript
            | MarkdownTagEnd::Link
            | MarkdownTagEnd::Image => self.close_inline(source),
        }
    }

    fn open_block(&mut self, kind: BlockKind, source: SourceRange) {
        self.frames.push(Frame::Block(BlockFrame {
            kind,
            start: source.start,
            inlines: Vec::new(),
            children: Vec::new(),
        }));
    }

    fn close_block(&mut self, source: SourceRange) {
        let Some(Frame::Block(frame)) = self.frames.pop() else {
            return;
        };
        let range = self.span(frame.start, source.end);
        let raw_source = self.raw_source(range);
        let kind = match frame.kind {
            BlockKind::Html(_) => BlockKind::Html(HtmlDocument::parse(&raw_source)),
            kind => kind,
        };
        let resource = if matches!(&kind, BlockKind::Paragraph) {
            ResourceRecord::parse(raw_source.trim(), None)
        } else {
            None
        };
        self.attach_block(Block::parsed(
            kind,
            range,
            frame.inlines,
            frame.children,
            raw_source,
            resource,
        ));
    }

    fn close_table(&mut self, source: SourceRange) {
        let Some(Frame::Table(mut frame)) = self.frames.pop() else {
            return;
        };
        if !frame.current_row.is_empty() {
            frame.finish_row();
        }
        let range = self.span(frame.start, source.end);
        self.attach_block(Block::parsed(
            BlockKind::Table(frame.table),
            range,
            Vec::new(),
            Vec::new(),
            self.raw_source(range),
            None,
        ));
    }

    fn close_table_cell(&mut self, source: SourceRange) {
        let Some(Frame::TableCell(frame)) = self.frames.pop() else {
            return;
        };
        let cell = TableCell {
            source: self.span(frame.start, source.end),
            inlines: frame.inlines,
        };
        if let Some(index) = self
            .frames
            .iter()
            .rposition(|frame| matches!(frame, Frame::Table(_)))
            && let Frame::Table(table) = &mut self.frames[index]
        {
            table.current_row.push(cell);
        }
    }

    fn finish_table_row(&mut self) {
        if let Some(index) = self
            .frames
            .iter()
            .rposition(|frame| matches!(frame, Frame::Table(_)))
            && let Frame::Table(table) = &mut self.frames[index]
        {
            table.finish_row();
        }
    }

    fn set_table_header_mode(&mut self, in_header: bool) {
        if let Some(index) = self
            .frames
            .iter()
            .rposition(|frame| matches!(frame, Frame::Table(_)))
            && let Frame::Table(table) = &mut self.frames[index]
        {
            table.in_header = in_header;
        }
    }

    fn open_inline(&mut self, kind: InlineKind, source: SourceRange) {
        self.inline_frames.push(InlineFrame {
            kind,
            start: source.start,
            children: Vec::new(),
        });
    }

    fn close_inline(&mut self, source: SourceRange) {
        let Some(frame) = self.inline_frames.pop() else {
            return;
        };
        self.push_inline(Inline::parsed(
            frame.kind,
            self.span(frame.start, source.end),
            frame.children,
        ));
    }

    fn push_inline(&mut self, inline: Inline) {
        if let Some(frame) = self.inline_frames.last_mut() {
            frame.children.push(inline);
            return;
        }
        for frame in self.frames.iter_mut().rev() {
            match frame {
                Frame::Block(block) => {
                    block.inlines.push(inline);
                    return;
                }
                Frame::TableCell(cell) => {
                    cell.inlines.push(inline);
                    return;
                }
                Frame::Table(_) | Frame::TableHead | Frame::TableRow => {}
            }
        }
    }

    fn push_task_marker(&mut self, checked: bool, source: SourceRange) {
        for frame in self.frames.iter_mut().rev() {
            if let Frame::Block(block) = frame
                && let BlockKind::ListItem { task } = &mut block.kind
            {
                *task = Some(checked);
                break;
            }
        }
        self.push_inline(Inline::parsed(
            InlineKind::TaskListMarker(checked),
            source,
            Vec::new(),
        ));
    }

    fn push_rule(&mut self, source: SourceRange) {
        let raw_source = self.raw_source(source);
        self.attach_block(Block::parsed(
            BlockKind::ThematicBreak,
            source,
            Vec::new(),
            Vec::new(),
            raw_source,
            None,
        ));
    }

    fn push_display_math(&mut self, value: &str, source: SourceRange) {
        let raw_source = self.raw_source(source);
        self.attach_block(Block::parsed(
            BlockKind::DisplayMath,
            source,
            vec![Inline::parsed(
                InlineKind::InlineMath(value.to_owned()),
                source,
                Vec::new(),
            )],
            Vec::new(),
            raw_source,
            None,
        ));
    }

    fn attach_block(&mut self, block: Block) {
        for frame in self.frames.iter_mut().rev() {
            if let Frame::Block(parent) = frame {
                parent.children.push(block);
                return;
            }
        }
        self.roots.push(block);
    }

    fn span(&self, start: usize, end: usize) -> SourceRange {
        SourceRange::from_parser(start, end.max(start))
    }

    fn raw_source(&self, range: SourceRange) -> String {
        match range.slice(self.source) {
            Ok(value) => value.to_owned(),
            Err(_) => String::new(),
        }
    }

    fn finish(mut self) -> Vec<Block> {
        // pulldown-cmark emits balanced events. This defensive drain keeps the
        // public model total even if a future parser revision omits a close.
        while let Some(frame) = self.frames.pop() {
            match frame {
                Frame::Block(frame) => {
                    let range = self.span(frame.start, self.source.len());
                    self.attach_block(Block::parsed(
                        frame.kind,
                        range,
                        frame.inlines,
                        frame.children,
                        self.raw_source(range),
                        None,
                    ));
                }
                Frame::Table(mut frame) => {
                    if !frame.current_row.is_empty() {
                        frame.finish_row();
                    }
                    let range = self.span(frame.start, self.source.len());
                    self.attach_block(Block::parsed(
                        BlockKind::Table(frame.table),
                        range,
                        Vec::new(),
                        Vec::new(),
                        self.raw_source(range),
                        None,
                    ));
                }
                Frame::TableHead | Frame::TableRow | Frame::TableCell(_) => {}
            }
        }
        self.roots
    }
}

impl TableFrame {
    fn finish_row(&mut self) {
        if self.current_row.is_empty() {
            return;
        }
        let row = std::mem::take(&mut self.current_row);
        if self.in_header && self.table.header.is_empty() {
            self.table.header = row;
        } else {
            self.table.rows.push(row);
        }
    }
}
