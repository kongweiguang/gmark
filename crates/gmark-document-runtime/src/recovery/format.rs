// @author kongweiguang

use std::ops::Range;

use gmark_document::{LineEnding, SourceFormatSnapshot};

use super::ResidentRecoveryError;
use super::types::{StoredFormatPatch, StoredSourceFormat};

pub(super) fn validate_source_format(
    source: &str,
    format: &SourceFormatSnapshot,
) -> Result<(), ResidentRecoveryError> {
    let newline_count = source.bytes().filter(|byte| *byte == b'\n').count();
    if newline_count != format.endings.len() {
        return Err(ResidentRecoveryError::InvalidSourceFormat {
            ending_count: format.endings.len(),
            newline_count,
        });
    }
    Ok(())
}

pub(super) fn default_source_format(source: &str) -> SourceFormatSnapshot {
    SourceFormatSnapshot {
        utf8_bom: false,
        endings: vec![LineEnding::Lf; source.bytes().filter(|byte| *byte == b'\n').count()],
        dominant: LineEnding::Lf,
    }
}

pub(super) fn build_format_patch(
    previous: &SourceFormatSnapshot,
    current: &SourceFormatSnapshot,
) -> StoredFormatPatch {
    let prefix = previous
        .endings
        .iter()
        .zip(&current.endings)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = previous.endings[prefix..]
        .iter()
        .rev()
        .zip(current.endings[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    StoredFormatPatch {
        start: prefix,
        removed: previous.endings.len() - prefix - suffix,
        inserted: current.endings[prefix..current.endings.len() - suffix]
            .iter()
            .copied()
            .map(Into::into)
            .collect(),
        utf8_bom: current.utf8_bom,
        dominant: current.dominant.into(),
    }
}

pub(super) fn apply_format_patch(
    format: &mut SourceFormatSnapshot,
    patch: StoredFormatPatch,
) -> Result<(), ResidentRecoveryError> {
    let end = patch.start.checked_add(patch.removed).ok_or_else(|| {
        ResidentRecoveryError::JournalFormat("recovery format patch range overflow".to_owned())
    })?;
    if end > format.endings.len() {
        return Err(ResidentRecoveryError::JournalFormat(
            "recovery format patch is outside the ending table".to_owned(),
        ));
    }
    format
        .endings
        .splice(patch.start..end, patch.inserted.into_iter().map(Into::into));
    format.utf8_bom = patch.utf8_bom;
    format.dominant = patch.dominant.into();
    Ok(())
}

pub(super) fn minimal_edit<'a>(
    previous: &str,
    current: &'a str,
) -> Option<(Range<usize>, &'a str)> {
    if previous == current {
        return None;
    }
    let prefix = previous
        .chars()
        .zip(current.chars())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum::<usize>();
    let suffix = previous[prefix..]
        .chars()
        .rev()
        .zip(current[prefix..].chars().rev())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum::<usize>();
    Some((
        prefix..previous.len() - suffix,
        &current[prefix..current.len() - suffix],
    ))
}

pub(super) fn stored_format(format: &SourceFormatSnapshot) -> StoredSourceFormat {
    format.into()
}
