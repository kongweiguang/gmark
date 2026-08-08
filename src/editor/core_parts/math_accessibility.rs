// @author kongweiguang

use gmark_math_edit::{MathDocument, MathSlot, MathSlotRole};

pub(super) fn math_accessibility_controls(
    strings: &crate::i18n::I18nStrings,
) -> Vec<crate::accessibility::AccessibilityMathControl> {
    const STRUCTURES: &[&str] = &[
        "fraction",
        "sqrt",
        "superscript",
        "subscript",
        "matrix",
        "cases",
        "aligned",
        "text_mode",
        "alpha",
        "sum",
    ];
    gmark_math_edit::MATH_SYMBOL_PALETTE_KEYS
        .iter()
        .map(|key| crate::accessibility::AccessibilityMathControl {
            key: (*key).to_owned(),
            label: strings.math_palette_text(key),
            page: crate::accessibility::AccessibilityMathPage::Symbols,
        })
        .chain(
            STRUCTURES
                .iter()
                .map(|key| crate::accessibility::AccessibilityMathControl {
                    key: (*key).to_owned(),
                    label: strings.math_palette_text(key),
                    page: crate::accessibility::AccessibilityMathPage::Structures,
                }),
        )
        .collect()
}

pub(super) fn math_slot_source(document: &MathDocument, slot: &MathSlot) -> Option<String> {
    let source = document.to_latex();
    if let MathSlotRole::EnvironmentCell { row, column } = slot.role() {
        let range = document.ast()?.source_range(slot.path())?;
        let environment = source.get(range)?;
        return environment_cell_source(environment, row, column);
    }
    let range = document.ast()?.source_range(slot.path())?;
    source.get(range).map(str::to_owned)
}

fn environment_cell_source(environment: &str, row: usize, column: usize) -> Option<String> {
    let body_start = environment.find('}')?.saturating_add(1);
    let body_end = environment.rfind(r"\end")?;
    if body_end < body_start {
        return None;
    }
    let body = &environment[body_start..body_end];
    body.split(r"\\")
        .nth(row)
        .and_then(|line| line.split('&').nth(column))
        .map(str::trim)
        .map(str::to_owned)
}

pub(super) fn math_accessibility_grid(
    document: &MathDocument,
    slot: &MathSlot,
) -> Option<crate::accessibility::AccessibilityMathGrid> {
    let MathSlotRole::EnvironmentCell {
        row: active_row,
        column: active_column,
    } = slot.role()
    else {
        return None;
    };
    let ast = document.ast()?;
    let slots = ast.environment_slots(slot.path());
    let mut cells = Vec::with_capacity(slots.len());
    let mut rows = 0usize;
    let mut columns = 0usize;
    for candidate in slots {
        let row = candidate.row()?;
        let column = candidate.column()?;
        rows = rows.max(row.saturating_add(1));
        columns = columns.max(column.saturating_add(1));
        cells.push(crate::accessibility::AccessibilityMathGridCell {
            row,
            column,
            value: math_slot_source(document, &candidate).unwrap_or_default(),
        });
    }
    (!cells.is_empty()).then_some(crate::accessibility::AccessibilityMathGrid {
        rows,
        columns,
        active_row,
        active_column,
        cells,
    })
}
