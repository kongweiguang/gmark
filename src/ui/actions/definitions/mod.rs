// @author kongweiguang

//! Lazily joins semantic shortcut groups while preserving their stable order.

use std::sync::LazyLock;

use super::ShortcutDefinition;

mod editing;
mod formatting;
mod workspace;

pub(super) fn all() -> &'static [ShortcutDefinition] {
    static DEFINITIONS: LazyLock<Vec<ShortcutDefinition>> = LazyLock::new(|| {
        let mut definitions = Vec::with_capacity(
            editing::DEFINITIONS.len()
                + formatting::DEFINITIONS.len()
                + workspace::DEFINITIONS.len(),
        );
        definitions.extend_from_slice(editing::DEFINITIONS);
        definitions.extend_from_slice(formatting::DEFINITIONS);
        definitions.extend_from_slice(workspace::DEFINITIONS);
        definitions
    });

    DEFINITIONS.as_slice()
}
