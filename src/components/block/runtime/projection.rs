// @author kongweiguang

//! Inline projection engine for editable Markdown delimiters.

use std::ops::Range;

use crate::components::InlineFootnoteReference;
use crate::components::markdown::inline::{
    InlineFragment, InlineLink, InlineRenderCache, InlineScript, InlineStyle, InlineTextTree,
    StyleFlag, can_use_markdown_script_delimiters,
};

use super::CollapsedCaretAffinity;

/// One displayed segment in an expanded inline projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ExpandedInlineSegment {
    pub(super) display_range: Range<usize>,
    pub(super) clean_range: Range<usize>,
    pub(super) fragment_index: usize,
    pub(super) link_group: Option<usize>,
    pub(super) kind: ExpandedInlineSegmentKind,
}

/// Inline construct whose Markdown delimiters can be projected for editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExpandedInlineKind {
    /// Link label and target syntax.
    Link,
    /// Bold Markdown delimiters.
    BoldMarkdown,
    /// Italic Markdown delimiters.
    ItalicMarkdown,
    /// Strikethrough delimiters.
    Strikethrough,
    /// Code span backtick delimiters.
    Code,
    /// Superscript Markdown delimiters.
    SuperscriptMarkdown,
    /// Superscript HTML delimiters.
    SuperscriptHtml,
    /// Subscript Markdown delimiters.
    SubscriptMarkdown,
    /// Subscript HTML delimiters.
    SubscriptHtml,
}

impl ExpandedInlineKind {
    fn applies_to(self, style: InlineStyle) -> bool {
        match self {
            Self::Link => false,
            Self::BoldMarkdown => style.bold,
            Self::ItalicMarkdown => style.italic,
            Self::Strikethrough => style.strikethrough,
            Self::Code => style.code,
            Self::SuperscriptMarkdown | Self::SuperscriptHtml => {
                style.script == InlineScript::Superscript
            }
            Self::SubscriptMarkdown | Self::SubscriptHtml => {
                style.script == InlineScript::Subscript
            }
        }
    }

    fn open_marker(self) -> &'static str {
        match self {
            Self::Link => "[",
            Self::BoldMarkdown => "**",
            Self::ItalicMarkdown => "*",
            Self::Strikethrough => "~~",
            Self::Code => "`",
            Self::SuperscriptMarkdown => "^",
            Self::SuperscriptHtml => "<sup>",
            Self::SubscriptMarkdown => "~",
            Self::SubscriptHtml => "<sub>",
        }
    }

    fn close_marker(self) -> &'static str {
        match self {
            Self::Link => ")",
            Self::SuperscriptHtml => "</sup>",
            Self::SubscriptHtml => "</sub>",
            _ => self.open_marker(),
        }
    }

    pub(super) fn style_flag(self) -> Option<StyleFlag> {
        match self {
            Self::Link => None,
            Self::BoldMarkdown => Some(StyleFlag::Bold),
            Self::ItalicMarkdown => Some(StyleFlag::Italic),
            Self::Strikethrough => Some(StyleFlag::Strikethrough),
            Self::Code => Some(StyleFlag::Code),
            Self::SuperscriptMarkdown | Self::SuperscriptHtml => Some(StyleFlag::Superscript),
            Self::SubscriptMarkdown | Self::SubscriptHtml => Some(StyleFlag::Subscript),
        }
    }

    fn projection_rank(self) -> u8 {
        match self {
            Self::Link => 0,
            Self::BoldMarkdown => 1,
            Self::Strikethrough => 2,
            Self::SuperscriptMarkdown
            | Self::SuperscriptHtml
            | Self::SubscriptMarkdown
            | Self::SubscriptHtml => 3,
            Self::ItalicMarkdown => 4,
            Self::Code => 5,
        }
    }
}

/// Display role of one projected inline segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExpandedInlineSegmentKind {
    /// Text with no projected inline syntax.
    PlainText,
    /// Text carrying projected style.
    StyledText,
    /// Opening delimiter such as `[` or backticks.
    OpeningDelimiter(ExpandedInlineKind),
    /// Middle delimiter such as `](` for links.
    MiddleDelimiter(ExpandedInlineKind),
    /// Editable link target text.
    LinkTargetText,
    /// Editable footnote id text.
    FootnoteIdText,
    /// Closing delimiter such as `)` or backticks.
    ClosingDelimiter(ExpandedInlineKind),
}

/// One projected link run spanning one or more inline fragments.
#[derive(Clone, Debug)]
pub(super) struct ExpandedLinkRun {
    pub(super) link: InlineLink,
    pub(super) start_fragment_index: usize,
    pub(super) end_fragment_index: usize,
    pub(super) clean_range: Range<usize>,
    pub(super) display_range: Range<usize>,
    pub(super) target_display_range: Range<usize>,
}

/// One projected footnote reference run.
#[derive(Clone, Debug)]
pub(super) struct ExpandedFootnoteRun {
    pub(super) footnote: InlineFootnoteReference,
    pub(super) clean_range: Range<usize>,
    pub(super) display_range: Range<usize>,
}

/// Selection snapshot translated into an expanded link display range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectedLinkSelectionSnapshot {
    pub(super) clean_range: Range<usize>,
    pub(super) display_relative_range: Range<usize>,
    pub(super) selection_reversed: bool,
}

/// Render cache and offset maps for an expanded inline projection.
#[derive(Clone, Debug)]
pub(crate) struct ExpandedInlineProjection {
    pub(super) cache: InlineRenderCache,
    pub(super) segments: Vec<ExpandedInlineSegment>,
    pub(super) clean_to_display_cursor: Vec<usize>,
    pub(super) display_to_clean: Vec<usize>,
    pub(super) link_runs: Vec<ExpandedLinkRun>,
    pub(super) footnote_runs: Vec<ExpandedFootnoteRun>,
}

#[cfg(test)]
pub(super) fn expanded_display_offset_for_clean(
    fragments: &[InlineFragment],
    clean: usize,
) -> usize {
    let mut display = 0usize;
    let mut clean_cursor = 0usize;
    for fragment in fragments {
        let clean_len = fragment.text.len();
        let clean_end = clean_cursor + clean_len;
        if clean <= clean_end {
            let off = clean.saturating_sub(clean_cursor);
            if fragment.style.code && clean_len > 0 {
                if off == clean_len {
                    return display + clean_len + 2;
                }
                return display + 1 + off;
            }
            if fragment.style.script != InlineScript::Normal && clean_len > 0 {
                if off == clean_len {
                    return display + clean_len + 2;
                }
                return display + 1 + off;
            }
            return display + off;
        }
        clean_cursor = clean_end;
        display += if fragment.style.code && clean_len > 0 {
            clean_len + 2
        } else if fragment.style.script != InlineScript::Normal && clean_len > 0 {
            clean_len + 2
        } else {
            clean_len
        };
    }
    display
}

#[cfg(test)]
pub(super) fn expanded_display_cursor_offset_for_clean(
    fragments: &[InlineFragment],
    clean: usize,
) -> usize {
    let mut display = 0usize;
    let mut clean_cursor = 0usize;
    for fragment in fragments {
        let clean_len = fragment.text.len();
        let clean_end = clean_cursor + clean_len;
        if clean <= clean_end {
            let off = clean.saturating_sub(clean_cursor);
            if fragment.style.code && clean_len > 0 {
                if off == 0 {
                    return display + 1;
                }
                if off >= clean_len {
                    return display + clean_len + 1;
                }
                return display + 1 + off;
            }
            if fragment.style.script != InlineScript::Normal && clean_len > 0 {
                if off == 0 {
                    return display + 1;
                }
                if off >= clean_len {
                    return display + clean_len + 1;
                }
                return display + 1 + off;
            }
            return display + off;
        }
        clean_cursor = clean_end;
        display += if fragment.style.code && clean_len > 0 {
            clean_len + 2
        } else if fragment.style.script != InlineScript::Normal && clean_len > 0 {
            clean_len + 2
        } else {
            clean_len
        };
    }
    display
}

impl ExpandedInlineProjection {}

#[path = "projection_parts/builder.rs"]
mod builder;

fn marker_style_for_projection(mut style: InlineStyle, kind: ExpandedInlineKind) -> InlineStyle {
    if matches!(
        kind,
        ExpandedInlineKind::SuperscriptMarkdown
            | ExpandedInlineKind::SuperscriptHtml
            | ExpandedInlineKind::SubscriptMarkdown
            | ExpandedInlineKind::SubscriptHtml
    ) {
        style.script = InlineScript::Normal;
    }
    style
}

/// Emit one inline fragment, wrapped in the projected emphasis delimiters for
/// `kinds`. Shared by standalone and link-label fragments so anchor text reveals
/// its bold/italic/code markers like ordinary text. `force_styled` keeps a
/// marker-less fragment styled (link labels while a link run is expanded).
// reason: 热路径参数保持借用并避免临时上下文分配；remove when projection inputs share one stable context.
#[allow(clippy::too_many_arguments)]
fn push_projected_fragment(
    fragment: &InlineFragment,
    fragment_index: usize,
    clean_range: Range<usize>,
    kinds: &[ExpandedInlineKind],
    link_group: Option<usize>,
    force_styled: bool,
    projected_fragments: &mut Vec<InlineFragment>,
    segments: &mut Vec<ExpandedInlineSegment>,
    clean_to_display_cursor: &mut [usize],
    display_to_clean: &mut Vec<usize>,
    display_cursor: &mut usize,
    any_expanded: &mut bool,
) {
    let fragment_len = fragment.text.len();

    for kind in kinds {
        *any_expanded = true;
        let marker = kind.open_marker().to_string();
        let marker_len = marker.len();
        let marker_style = marker_style_for_projection(fragment.style, *kind);
        projected_fragments.push(InlineFragment {
            text: marker,
            style: marker_style,
            html_style: fragment.html_style,
            link: None,
            footnote: None,
            math: None,
        });
        segments.push(ExpandedInlineSegment {
            display_range: *display_cursor..*display_cursor + marker_len,
            clean_range: clean_range.start..clean_range.start,
            fragment_index,
            link_group,
            kind: ExpandedInlineSegmentKind::OpeningDelimiter(*kind),
        });
        for _ in 0..marker_len {
            display_to_clean.push(clean_range.start);
        }
        *display_cursor += marker_len;
    }

    let text_segment_kind = if kinds.is_empty() && !force_styled {
        ExpandedInlineSegmentKind::PlainText
    } else {
        ExpandedInlineSegmentKind::StyledText
    };
    projected_fragments.push(fragment.clone());
    segments.push(ExpandedInlineSegment {
        display_range: *display_cursor..*display_cursor + fragment_len,
        clean_range: clean_range.clone(),
        fragment_index,
        link_group,
        kind: text_segment_kind,
    });
    for offset in 0..=fragment_len {
        clean_to_display_cursor[clean_range.start + offset] = *display_cursor + offset;
    }
    for offset in 1..=fragment_len {
        display_to_clean.push(clean_range.start + offset);
    }
    *display_cursor += fragment_len;

    for kind in kinds.iter().rev() {
        let marker = kind.close_marker().to_string();
        let marker_len = marker.len();
        let marker_style = marker_style_for_projection(fragment.style, *kind);
        projected_fragments.push(InlineFragment {
            text: marker,
            style: marker_style,
            html_style: fragment.html_style,
            link: None,
            footnote: None,
            math: None,
        });
        segments.push(ExpandedInlineSegment {
            display_range: *display_cursor..*display_cursor + marker_len,
            clean_range: clean_range.end..clean_range.end,
            fragment_index,
            link_group,
            kind: ExpandedInlineSegmentKind::ClosingDelimiter(*kind),
        });
        for _ in 0..marker_len {
            display_to_clean.push(clean_range.end);
        }
        *display_cursor += marker_len;
    }
}
