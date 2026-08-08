// @author kongweiguang

use super::*;
use gmark_math_edit::MathEditCommand;

#[test]
fn live_source_edit_preserves_display_fences_and_outer_whitespace() {
    let raw = "  $$\n  x^2\n$$  ";
    let mut session = MathEditSession::begin(raw, Revision::INITIAL).expect("formula");
    session
        .document_mut()
        .replace_latex_range(0..1, "y")
        .expect("body edit");
    let edit = session
        .source_edit(Revision::INITIAL, raw)
        .expect("fresh source");
    assert_eq!(&raw[edit.range.clone()], "x^2");
    assert_eq!(edit.replacement, "y^2");
    assert_eq!(edit.next_raw, "  $$\n  y^2\n$$  ");
}

#[test]
fn stale_revision_cannot_overwrite_source() {
    let session = MathEditSession::begin("$x$", Revision::INITIAL).expect("formula");
    assert_eq!(
        session.source_edit(Revision::from_u64(1), "$y$"),
        Err(MathEditSessionError::StaleSource)
    );
}

#[test]
fn unsupported_latex_falls_back_to_source_editing() {
    assert_eq!(
        MathEditSession::begin(r"$\custom{x}$", Revision::INITIAL),
        Err(MathEditSessionError::UnsupportedStructure)
    );
}

#[test]
fn empty_body_source_edit_returns_a_valid_insertion_range() {
    let raw = "$$\n\n$$";
    let session = MathEditSession::begin(raw, Revision::INITIAL).expect("formula");
    let edit = session
        .source_edit(Revision::INITIAL, raw)
        .expect("fresh source");
    assert_eq!(edit.range.start, edit.range.end);
    assert!(edit.range.end <= raw.len());
}

#[test]
fn structured_commands_are_committed_as_one_body_replacement() {
    let raw = "  $$  x  $$  ";
    let mut session = MathEditSession::begin(raw, Revision::INITIAL).expect("formula");
    session
        .document_mut()
        .replace_latex_range(0..1, "\\frac{x}{}")
        .expect("structured body replacement");
    assert_eq!(session.preview_raw(), "  $$  \\frac{x}{}  $$  ");
    let edit = session
        .source_edit(Revision::INITIAL, raw)
        .expect("fresh source");
    assert_eq!(edit.next_raw, "  $$  \\frac{x}{}  $$  ");
}

#[test]
fn local_publish_acknowledgement_keeps_live_session_revision_safe() {
    let raw = "$x$";
    let mut session = MathEditSession::begin(raw, Revision::INITIAL).expect("formula");
    session
        .execute(MathEditCommand::InsertText("+1".to_owned()))
        .expect("insert");
    assert_eq!(session.preview_raw(), "$+1x$");
    let edit = session.source_edit(Revision::INITIAL, raw).unwrap();
    assert_eq!(edit.replacement, "+1x");
    let revision = Revision::from_u64(1);
    session
        .acknowledge_local_publish(revision, &edit.next_raw)
        .expect("local publish");
    assert!(session.source_edit(revision, &edit.next_raw).is_ok());
}
