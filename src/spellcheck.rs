// @author kongweiguang

//! Local spelling provider backed by Harper. Source text never leaves the process.

use std::ops::Range;
use std::sync::{Mutex, OnceLock};

use harper_core::linting::{LintGroup, LintKind, Linter, Suggestion};
use harper_core::spell::FstDictionary;
use harper_core::{Dialect, Document};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpellingDiagnostic {
    pub(crate) range: Range<usize>,
    pub(crate) original: String,
    pub(crate) message: String,
    pub(crate) replacements: Vec<String>,
}

static LINTER: OnceLock<Mutex<LintGroup>> = OnceLock::new();

/// Harper reports character offsets; GPUI and gmark transactions use UTF-8 bytes.
pub(crate) fn check_spelling(text: &str) -> Vec<SpellingDiagnostic> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let document = Document::new_plain_english_curated(text);
    let linter = LINTER.get_or_init(|| {
        Mutex::new(LintGroup::new_curated(
            FstDictionary::curated(),
            Dialect::American,
        ))
    });
    let Ok(mut linter) = linter.lock() else {
        return Vec::new();
    };
    let lints = linter.lint(&document);
    let mut char_to_byte = text
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    char_to_byte.push(text.len());

    lints
        .into_iter()
        .filter(|lint| lint.lint_kind == LintKind::Spelling)
        .filter_map(|lint| {
            let start = *char_to_byte.get(lint.span.start)?;
            let end = *char_to_byte.get(lint.span.end)?;
            let replacements = lint
                .suggestions
                .into_iter()
                .filter_map(|suggestion| match suggestion {
                    Suggestion::ReplaceWith(chars) => Some(chars.into_iter().collect()),
                    _ => None,
                })
                .take(5)
                .collect();
            Some(SpellingDiagnostic {
                range: start..end,
                original: text[start..end].to_owned(),
                message: lint.message,
                replacements,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/spellcheck.rs"]
mod tests;
