// @author kongweiguang

//! GPUI-free visual projection for the structural math editor.
//!
//! The projection deliberately keeps render-only placeholders out of
//! [`MathDocument::to_latex`].  It serializes a temporary view of empty slots
//! with `\\square`, asks RaTeX to parse and lay out that view, and then maps the
//! resulting em-sized box back to stable [`MathSlot`] addresses.  The mapping
//! is intentionally source/slot oriented rather than tied to any UI toolkit.
//!
//! Geometry algorithms adapted from `packetThrower/zorite@86a52230cbc6d1cd75f4d0a635643a5c9402b021`,
//! GPL-3.0-or-later.  This module is a clean-room adaptation to Gmark's
//! `MathNode`/`MathSlot` model and does not depend on the referenced source.

use super::{MathAst, MathCursor2D, MathDocument, MathNode, MathPath, MathSelection, MathSlot};

/// A rectangle in em units.  The origin is the top-left of the laid-out
/// formula and y grows downwards.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MathRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl MathRect {
    #[must_use]
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    #[must_use]
    pub const fn width(self) -> f64 {
        self.w
    }

    #[must_use]
    pub const fn height(self) -> f64 {
        self.h
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    #[must_use]
    pub const fn right(self) -> f64 {
        self.x + self.w
    }

    #[must_use]
    pub const fn bottom(self) -> f64 {
        self.y + self.h
    }

    #[must_use]
    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self::new(left, top, right - left, bottom - top)
    }
}

/// Geometry assigned to one editable slot.
#[derive(Clone, Debug, PartialEq)]
pub struct MathSlotGeometry {
    pub slot: MathSlot,
    pub rect: MathRect,
}

/// A hit-test result carrying the slot, local UTF-8 byte offset and slot rect.
#[derive(Clone, Debug, PartialEq)]
pub struct MathHitTarget {
    pub slot: MathSlot,
    pub offset: usize,
    pub rect: MathRect,
}

impl MathHitTarget {
    #[must_use]
    pub fn cursor(&self) -> MathCursor2D {
        MathCursor2D::new(self.slot.clone(), self.offset)
    }

    #[must_use]
    pub fn path(&self) -> &MathPath {
        self.slot.path()
    }

    #[must_use]
    pub fn slot(&self) -> &MathSlot {
        &self.slot
    }

    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    #[must_use]
    pub const fn rect(&self) -> MathRect {
        self.rect
    }
}

/// The GPUI-free geometry snapshot for a formula.
#[derive(Clone, Debug, PartialEq)]
pub struct MathVisualGeometry {
    bounds: MathRect,
    slots: Vec<MathSlotGeometry>,
    slot_boundaries: Vec<(MathSlot, Vec<usize>)>,
    structural_rects: Vec<(MathPath, MathRect)>,
}

impl MathVisualGeometry {
    #[must_use]
    pub fn from_document(document: &MathDocument) -> Self {
        MathVisualProjection::new(document).geometry
    }

    #[must_use]
    pub fn bounds(&self) -> MathRect {
        self.bounds
    }

    #[must_use]
    pub fn rect(&self) -> MathRect {
        self.bounds
    }

    #[must_use]
    pub fn slots(&self) -> &[MathSlotGeometry] {
        &self.slots
    }

    #[must_use]
    pub fn slot_rects(&self) -> &[MathSlotGeometry] {
        self.slots()
    }

    #[must_use]
    pub fn slot_rect(&self, slot: &MathSlot) -> Option<MathRect> {
        self.slots
            .iter()
            .find(|candidate| &candidate.slot == slot)
            .map(|candidate| candidate.rect)
    }

    #[must_use]
    pub fn caret_rect(&self, cursor: &MathCursor2D) -> Option<MathRect> {
        let rect = self.slot_rect(cursor.slot())?;
        let boundaries = self
            .slot_boundaries
            .iter()
            .find(|(slot, _)| slot == cursor.slot())
            .map_or_else(
                || vec![0, cursor.offset()],
                |(_, boundaries)| boundaries.clone(),
            );
        let len = boundaries.last().copied().unwrap_or(0);
        let offset = nearest_char_boundary(&boundaries, cursor.offset().min(len));
        let x = if len == 0 {
            rect.x + rect.w / 2.0
        } else {
            rect.x + rect.w * (offset as f64 / len as f64)
        };
        Some(MathRect::new(x, rect.y, 0.0, rect.h.max(0.12)))
    }

    /// Return the visual span of a range selection.  Structural selections
    /// resolve to the selected node's full rectangle.
    #[must_use]
    pub fn selection_rect(&self, selection: &MathSelection) -> Option<MathRect> {
        if let Some(path) = selection.structural_path() {
            return self
                .structural_rects
                .iter()
                .find(|(candidate, _)| candidate == path)
                .map(|(_, rect)| *rect);
        }
        let start = selection.anchor();
        let end = selection.focus();
        if start.slot() != end.slot() {
            return None;
        }
        let first = self.caret_rect(start)?;
        let last = self.caret_rect(end)?;
        Some(MathRect::new(
            first.x.min(last.x),
            first.y,
            (first.x - last.x).abs().max(0.02),
            first.h.max(last.h),
        ))
    }

    /// Hit-test in em coordinates, preferring the smallest (deepest) slot
    /// rectangle that contains the point.
    #[must_use]
    pub fn hit_test(&self, x: f64, y: f64) -> Option<MathHitTarget> {
        let candidate = self
            .slots
            .iter()
            .filter(|slot| slot.rect.contains(x, y))
            .min_by(|left, right| {
                let left_area = left.rect.w * left.rect.h;
                let right_area = right.rect.w * right.rect.h;
                left_area
                    .partial_cmp(&right_area)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;
        let boundaries = self
            .slot_boundaries
            .iter()
            .find(|(slot, _)| slot == &candidate.slot)
            .map_or_else(|| vec![0], |(_, boundaries)| boundaries.clone());
        let len = boundaries.last().copied().unwrap_or(0);
        let offset = if len == 0 || candidate.rect.w <= f64::EPSILON {
            0
        } else {
            let fraction = ((x - candidate.rect.x) / candidate.rect.w).clamp(0.0, 1.0);
            nearest_char_boundary(&boundaries, (fraction * len as f64).round() as usize)
        };
        Some(MathHitTarget {
            slot: candidate.slot.clone(),
            offset,
            rect: candidate.rect,
        })
    }

    #[must_use]
    pub fn caret(&self, cursor: &MathCursor2D) -> Option<MathRect> {
        self.caret_rect(cursor)
    }

    #[must_use]
    pub fn selection(&self, selection: &MathSelection) -> Option<MathRect> {
        self.selection_rect(selection)
    }

    #[must_use]
    pub fn hit(&self, x: f64, y: f64) -> Option<MathHitTarget> {
        self.hit_test(x, y)
    }
}

/// Render projection and geometry for a [`MathDocument`].
#[derive(Clone, Debug, PartialEq)]
pub struct MathVisualProjection {
    source: String,
    render_source: String,
    geometry: MathVisualGeometry,
    width: f64,
    height: f64,
    depth: f64,
}

impl MathVisualProjection {
    #[must_use]
    pub fn new(document: &MathDocument) -> Self {
        let source = document.to_latex();
        let render_source = match document.ast() {
            Some(ast) => render_latex(ast.root()),
            // An opaque document has no structural slots.  Keeping the source
            // visible is more useful than replacing it with a placeholder.
            None => source.clone(),
        };
        let (width, height, depth) = layout_metrics(&render_source);
        let mut slots = Vec::new();
        let mut slot_boundaries = Vec::new();
        let mut structural_rects = Vec::new();
        if let Some(ast) = document.ast() {
            collect_geometry(
                ast,
                ast.root(),
                &MathPath::root(),
                MathRect::new(0.0, 0.0, width.max(0.25), (height + depth).max(0.25)),
                &mut slots,
                &mut slot_boundaries,
                &mut structural_rects,
            );
        } else {
            let slot = MathSlot::root();
            slots.push(MathSlotGeometry {
                slot: slot.clone(),
                rect: MathRect::new(0.0, 0.0, width.max(0.25), (height + depth).max(0.25)),
            });
            slot_boundaries.push((slot, char_boundaries(&source)));
            structural_rects.push((
                MathPath::root(),
                MathRect::new(0.0, 0.0, width.max(0.25), (height + depth).max(0.25)),
            ));
        }
        Self {
            source,
            render_source,
            geometry: MathVisualGeometry {
                bounds: MathRect::new(0.0, 0.0, width.max(0.25), (height + depth).max(0.25)),
                slots,
                slot_boundaries,
                structural_rects,
            },
            width,
            height,
            depth,
        }
    }

    #[must_use]
    pub fn from_document(document: &MathDocument) -> Self {
        Self::new(document)
    }

    #[must_use]
    pub fn from_ast(ast: &MathAst) -> Self {
        Self::new(&MathDocument::Structured(ast.clone()))
    }

    #[must_use]
    pub fn project(document: &MathDocument) -> Self {
        Self::new(document)
    }

    #[must_use]
    pub fn from_latex(latex: impl Into<String>) -> Self {
        Self::new(&MathDocument::parse(latex))
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Temporary source passed to RaTeX.  It may contain render-only
    /// `\\square` placeholders for empty editable slots.
    #[must_use]
    pub fn render_latex(&self) -> &str {
        &self.render_source
    }

    #[must_use]
    pub fn render_source(&self) -> &str {
        self.render_latex()
    }

    #[must_use]
    pub fn to_latex(&self) -> &str {
        self.source()
    }

    #[must_use]
    pub fn geometry(&self) -> &MathVisualGeometry {
        &self.geometry
    }

    #[must_use]
    pub fn visual_geometry(&self) -> &MathVisualGeometry {
        self.geometry()
    }

    #[must_use]
    pub fn width(&self) -> f64 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> f64 {
        self.height
    }

    #[must_use]
    pub fn depth(&self) -> f64 {
        self.depth
    }

    #[must_use]
    pub fn caret_rect(&self, cursor: &MathCursor2D) -> Option<MathRect> {
        self.geometry.caret_rect(cursor)
    }

    #[must_use]
    pub fn selection_rect(&self, selection: &MathSelection) -> Option<MathRect> {
        self.geometry.selection_rect(selection)
    }

    #[must_use]
    pub fn hit_test(&self, x: f64, y: f64) -> Option<MathHitTarget> {
        self.geometry.hit_test(x, y)
    }

    #[must_use]
    pub fn caret(&self, cursor: &MathCursor2D) -> Option<MathRect> {
        self.caret_rect(cursor)
    }

    #[must_use]
    pub fn selection(&self, selection: &MathSelection) -> Option<MathRect> {
        self.selection_rect(selection)
    }

    #[must_use]
    pub fn hit(&self, x: f64, y: f64) -> Option<MathHitTarget> {
        self.hit_test(x, y)
    }
}

fn layout_metrics(source: &str) -> (f64, f64, f64) {
    let parsed = ratex_parser::parse(source).or_else(|_| ratex_parser::parse(r"\square"));
    let Ok(nodes) = parsed else {
        return (source.chars().count() as f64 * 0.5, 0.8, 0.2);
    };
    let root = ratex_layout::layout(&nodes, &ratex_layout::LayoutOptions::default());
    (root.width, root.height, root.depth)
}

fn render_latex(node: &MathNode) -> String {
    match node {
        MathNode::Sequence(children) if children.is_empty() => r"\square".to_owned(),
        MathNode::Sequence(children) => children.iter().map(render_latex).collect(),
        MathNode::Text(text) | MathNode::Opaque(text) => text.clone(),
        MathNode::Command { name } => format!("\\{name}"),
        MathNode::Group(content) => format!("{{{}}}", render_latex(content)),
        MathNode::Fraction {
            numerator,
            denominator,
        } => format!(
            r"\frac{{{}}}{{{}}}",
            render_latex(numerator),
            render_latex(denominator)
        ),
        MathNode::SquareRoot { index, radicand } => {
            let index = index
                .as_deref()
                .map(render_latex)
                .map(|value| format!("[{value}]"))
                .unwrap_or_default();
            format!(r"\sqrt{index}{{{}}}", render_latex(radicand))
        }
        MathNode::Delimited { pair, body } => pair.wrap_body(&render_latex(body)),
        MathNode::Superscript(value) => format!("^{{{}}}", render_latex(value)),
        MathNode::Subscript(value) => format!("_{{{}}}", render_latex(value)),
        MathNode::TextMode(content) => format!(r"\text{{{}}}", render_latex(content)),
        MathNode::Symbol { name } | MathNode::BigOperator { name } => format!("\\{name}"),
        MathNode::Accent { name, value } => format!(r"\{name}{{{}}}", render_latex(value)),
        MathNode::Environment { raw, .. } => render_environment(raw),
    }
}

fn render_environment(raw: &str) -> String {
    let Some(grid) = super::environment_grid(raw) else {
        return raw.to_owned();
    };
    let mut rendered = raw.to_owned();
    for cell in grid.cells.iter().rev() {
        let Some(value) = raw.get(cell.start..cell.end) else {
            continue;
        };
        if value.trim().is_empty() {
            rendered.replace_range(cell.start..cell.end, r"\square");
        }
    }
    rendered
}

fn collect_geometry(
    ast: &MathAst,
    node: &MathNode,
    path: &MathPath,
    rect: MathRect,
    slots: &mut Vec<MathSlotGeometry>,
    slot_boundaries: &mut Vec<(MathSlot, Vec<usize>)>,
    structural_rects: &mut Vec<(MathPath, MathRect)>,
) {
    structural_rects.push((path.clone(), rect));
    let slot = MathSlot::node(path.clone());
    if matches!(node, MathNode::Sequence(_)) {
        slots.push(MathSlotGeometry {
            slot: slot.clone(),
            rect,
        });
        slot_boundaries.push((slot.clone(), char_boundaries(&node.to_latex())));
    }
    match node {
        MathNode::Sequence(children) => {
            if children.is_empty() {
                return;
            }
            let mut x = rect.x;
            for (index, child) in children.iter().enumerate() {
                let child_width = layout_metrics(&render_latex(child)).0.max(0.12);
                let child_rect = MathRect::new(x, rect.y, child_width, rect.h.max(0.25));
                collect_geometry(
                    ast,
                    child,
                    &path.child(index),
                    child_rect,
                    slots,
                    slot_boundaries,
                    structural_rects,
                );
                x += child_width;
            }
        }
        MathNode::Environment { .. } => {
            let environment_slots = ast.environment_slots(path);
            let max_column = environment_slots
                .iter()
                .filter_map(MathSlot::column)
                .max()
                .unwrap_or(0)
                + 1;
            let max_row = environment_slots
                .iter()
                .filter_map(MathSlot::row)
                .max()
                .unwrap_or(0)
                + 1;
            for cell in environment_slots {
                let column = cell.column().unwrap_or(0);
                let row = cell.row().unwrap_or(0);
                let cell_rect = MathRect::new(
                    rect.x + rect.w * column as f64 / max_column as f64,
                    rect.y + rect.h * row as f64 / max_row as f64,
                    rect.w / max_column as f64,
                    rect.h / max_row as f64,
                );
                slots.push(MathSlotGeometry {
                    slot: cell.clone(),
                    rect: cell_rect,
                });
                slot_boundaries.push((cell.clone(), ast_cell_boundaries(ast, &cell)));
            }
        }
        _ => {
            for (index, child) in node.children().into_iter().enumerate() {
                let child_path = path.child(index);
                let child_rect = child_rect(node, index, rect);
                collect_geometry(
                    ast,
                    child,
                    &child_path,
                    child_rect,
                    slots,
                    slot_boundaries,
                    structural_rects,
                );
            }
        }
    }
}

fn child_rect(node: &MathNode, index: usize, rect: MathRect) -> MathRect {
    match node {
        MathNode::Fraction { .. } => {
            let half = rect.h * 0.48;
            MathRect::new(
                rect.x + rect.w * 0.12,
                rect.y + if index == 0 { 0.0 } else { half },
                rect.w * 0.76,
                half,
            )
        }
        MathNode::SquareRoot { index: Some(_), .. } if index == 0 => {
            MathRect::new(rect.x, rect.y, rect.w * 0.35, rect.h * 0.45)
        }
        MathNode::SquareRoot { index: Some(_), .. } => MathRect::new(
            rect.x + rect.w * 0.25,
            rect.y + rect.h * 0.2,
            rect.w * 0.7,
            rect.h * 0.8,
        ),
        MathNode::SquareRoot { index: None, .. } => {
            MathRect::new(rect.x + rect.w * 0.2, rect.y, rect.w * 0.8, rect.h)
        }
        MathNode::Delimited { .. } => {
            MathRect::new(rect.x + rect.w * 0.12, rect.y, rect.w * 0.76, rect.h)
        }
        MathNode::Superscript(_) => {
            MathRect::new(rect.x + rect.w * 0.2, rect.y, rect.w * 0.8, rect.h * 0.55)
        }
        MathNode::Subscript(_) => MathRect::new(
            rect.x + rect.w * 0.2,
            rect.y + rect.h * 0.45,
            rect.w * 0.8,
            rect.h * 0.55,
        ),
        _ => rect,
    }
}

fn ast_cell_boundaries(ast: &MathAst, slot: &MathSlot) -> Vec<usize> {
    super::slot_source(&MathDocument::parse(ast.to_latex()), slot)
        .map_or_else(|_| vec![0], |source| char_boundaries(&source))
}

fn char_boundaries(source: &str) -> Vec<usize> {
    let mut boundaries = source
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    boundaries.push(source.len());
    boundaries
}

fn nearest_char_boundary(boundaries: &[usize], desired: usize) -> usize {
    boundaries
        .iter()
        .min_by_key(|boundary| boundary.abs_diff(desired))
        .copied()
        .unwrap_or(0)
}
