// @author kongweiguang

use super::{
    StatusBarState, count_characters, normalized_action_id, should_render_file_status,
    source_format_labels,
};
use crate::i18n::I18nStrings;
use gmark_document::Revision;

#[test]
fn action_names_normalize_to_stable_status_button_ids() {
    assert_eq!(
        normalized_action_id("gmark::ToggleViewMode"),
        "toggle_view_mode"
    );
    assert_eq!(normalized_action_id("save-document"), "save_document");
    assert_eq!(normalized_action_id("plugin.action"), "plugin_action");
}
use gmark_document::{LineEnding, LineEndingStatus, SourceFormatSummary};

#[test]
fn empty_text_has_zero_characters() {
    assert_eq!(count_characters(""), 0);
}

#[test]
fn latin_text_counts_letters_and_spaces() {
    assert_eq!(count_characters("hello world"), 11);
    assert_eq!(count_characters("one two"), 7);
}

#[test]
fn cjk_characters_are_counted_individually() {
    assert_eq!(count_characters("你好世界"), 4);
    assert_eq!(count_characters("中文"), 2);
}

#[test]
fn virtual_region_edit_updates_cached_graphemes_for_new_revision() {
    let old_revision = Revision::from_u64(7);
    let new_revision = Revision::from_u64(8);
    let old_region = "e\u{301} 👨‍👩‍👧‍👦";
    let new_region = "你好 👩‍💻";
    let unchanged_count = count_characters("prefix\n\nsuffix");
    let mut state = StatusBarState::default();
    state.set_word_count(old_revision, unchanged_count + count_characters(old_region));

    state.apply_virtual_text_edit(old_revision, new_revision, old_region, new_region);

    assert_eq!(
        state.cached_word_count(new_revision),
        Some(unchanged_count + count_characters(new_region))
    );
    assert_eq!(state.cached_word_count(old_revision), None);
}

#[test]
fn line_endings_count_as_one_character() {
    assert_eq!(count_characters("a\nb"), 3);
    assert_eq!(count_characters("a\r\nb"), 3);
}

#[test]
fn extended_graphemes_count_as_one_visible_character() {
    assert_eq!(count_characters("e\u{301}"), 1);
    assert_eq!(count_characters("👨‍👩‍👧‍👦"), 1);
    assert_eq!(count_characters("  "), 2);
}

#[test]
fn regular_external_conflict_is_visible_without_recovery_session() {
    assert!(should_render_file_status(false, true));
    assert!(should_render_file_status(true, false));
    assert!(!should_render_file_status(false, false));
}

#[test]
fn source_format_labels_cover_bom_uniform_empty_and_mixed_documents() {
    let strings = I18nStrings::en_us();
    let format = |utf8_bom, line_endings, dominant| SourceFormatSummary {
        utf8_bom,
        line_endings,
        dominant,
    };

    assert_eq!(
        source_format_labels(
            &format(false, LineEndingStatus::None, LineEnding::CrLf),
            &crate::document_io::DocumentEncoding::Utf8,
            &strings,
        ),
        ("UTF-8".to_owned(), "CRLF".to_owned())
    );
    assert_eq!(
        source_format_labels(
            &format(
                true,
                LineEndingStatus::Uniform(LineEnding::CrLf),
                LineEnding::CrLf,
            ),
            &crate::document_io::DocumentEncoding::Utf8,
            &strings
        ),
        ("UTF-8 BOM".to_owned(), "CRLF".to_owned())
    );
    assert_eq!(
        source_format_labels(
            &format(false, LineEndingStatus::Mixed, LineEnding::Lf),
            &crate::document_io::DocumentEncoding::Utf8,
            &strings
        )
        .1,
        "Mixed"
    );
    assert_eq!(
        source_format_labels(
            &format(
                false,
                LineEndingStatus::Uniform(LineEnding::Lf),
                LineEnding::Lf,
            ),
            &crate::document_io::DocumentEncoding::Legacy("GB18030".to_owned()),
            &strings,
        )
        .0,
        "GB18030"
    );
}
