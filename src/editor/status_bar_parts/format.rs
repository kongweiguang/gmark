// @author kongweiguang

//! Source-format labels used by the status bar.

use gmark_document::{LineEnding, LineEndingStatus, SourceFormatSummary};
use gpui::{AnyElement, IntoElement, ParentElement, Styled, Window, div, px};

use crate::i18n::I18nStrings;
use crate::theme::Theme;

/// Keep viewport and format-label policy together so the status-bar
/// orchestrator can focus on composing the visible regions.
pub(super) fn viewport_width_for_status(window: &Window) -> f32 {
    f32::from(window.viewport_size().width)
}

/// Keep encoding and line-ending labels in one boundary so every status-bar
/// caller observes the same localization and mixed-ending policy.
pub(super) fn source_format_labels(
    format: &SourceFormatSummary,
    source_encoding: &crate::document_io::DocumentEncoding,
    strings: &I18nStrings,
) -> (String, String) {
    let encoding = if !source_encoding.is_utf8() {
        source_encoding.label().to_owned()
    } else if format.utf8_bom {
        strings.status_bar_encoding_utf8_bom.clone()
    } else {
        strings.status_bar_encoding_utf8.clone()
    };
    let line_ending = match format.line_endings {
        LineEndingStatus::None => match format.dominant {
            LineEnding::Lf => "LF".to_owned(),
            LineEnding::CrLf => "CRLF".to_owned(),
            LineEnding::Cr => "CR".to_owned(),
        },
        LineEndingStatus::Uniform(LineEnding::Lf) => "LF".to_owned(),
        LineEndingStatus::Uniform(LineEnding::CrLf) => "CRLF".to_owned(),
        LineEndingStatus::Uniform(LineEnding::Cr) => "CR".to_owned(),
        LineEndingStatus::Mixed => strings.status_bar_line_ending_mixed.clone(),
    };
    (encoding, line_ending)
}

/// Keep the compact label construction independent from the larger overflow
/// menu so its visual contract remains unchanged when the menu is refactored.
pub(super) fn render_source_format_label(label: String, theme: &Theme) -> AnyElement {
    div()
        .text_size(px(theme.dimensions.status_bar_text_size))
        .text_color(theme.colors.workbench.text_tertiary)
        .child(label)
        .into_any_element()
}
