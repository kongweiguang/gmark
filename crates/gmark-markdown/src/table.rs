// @author kongweiguang

//! Rendering-neutral GFM pipe-table values.

use crate::inline::Inline;
use crate::source::SourceRange;

/// Horizontal alignment declared for one table column.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TableAlignment {
    /// No explicit alignment marker.
    #[default]
    Default,
    /// `:---`.
    Left,
    /// `:---:`.
    Center,
    /// `---:`.
    Right,
}

impl TableAlignment {
    /// Returns a canonical pipe-table delimiter cell.
    pub const fn delimiter(self) -> &'static str {
        match self {
            Self::Default => "---",
            Self::Left => ":---",
            Self::Center => ":---:",
            Self::Right => "---:",
        }
    }
}

/// A single table cell and its source-byte coverage.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TableCell {
    /// Exact source range for this cell's Markdown contents.
    pub source: SourceRange,
    /// Inline content parsed with normal pulldown-cmark semantics.
    pub inlines: Vec<Inline>,
}

impl TableCell {
    /// Makes a synthetic empty cell.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns plain text recursively.
    pub fn plain_text(&self) -> String {
        self.inlines.iter().map(Inline::plain_text).collect()
    }
}

/// A GFM table with one header row and zero or more body rows.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Table {
    /// Alignment metadata in source column order.
    pub alignments: Vec<TableAlignment>,
    /// Header cells.
    pub header: Vec<TableCell>,
    /// Body rows.
    pub rows: Vec<Vec<TableCell>>,
}

impl Table {
    /// Creates an empty table with `columns` columns.
    pub fn empty(columns: usize) -> Self {
        let columns = columns.max(1);
        Self {
            alignments: vec![TableAlignment::Default; columns],
            header: vec![TableCell::empty(); columns],
            rows: Vec::new(),
        }
    }

    /// Returns the widest declared table shape, with a minimum of one column.
    pub fn column_count(&self) -> usize {
        self.header
            .len()
            .max(self.alignments.len())
            .max(self.rows.iter().map(Vec::len).max().unwrap_or(0))
            .max(1)
    }

    /// Returns a cell by visual row, where row zero is the header.
    pub fn cell(&self, row: usize, column: usize) -> Option<&TableCell> {
        if row == 0 {
            return self.header.get(column);
        }
        self.rows.get(row - 1).and_then(|cells| cells.get(column))
    }

    /// Pads every row and alignment vector to a stable rectangular shape.
    pub fn normalize_shape(&mut self) {
        let columns = self.column_count();
        self.header.resize(columns, TableCell::empty());
        self.alignments.resize(columns, TableAlignment::Default);
        for row in &mut self.rows {
            row.resize(columns, TableCell::empty());
        }
    }

    /// Appends a body row after normalizing the existing table.
    pub fn append_empty_row(&mut self) {
        self.normalize_shape();
        self.rows
            .push(vec![TableCell::empty(); self.column_count()]);
    }
}
