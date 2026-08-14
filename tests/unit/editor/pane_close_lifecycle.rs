// @author kongweiguang

//! Pane save-and-close request identity tests.

use super::{
    PaneCloseRequest, PaneCloseSaveOutcome, host_save_event_is_terminal, should_close_pane_tab,
};
use crate::editor::panes::{PaneId, TabId};

#[test]
fn cancelled_request_rejects_late_success() {
    let pane = PaneId::new();
    let tab = TabId::new();
    let request = PaneCloseRequest::new(7, pane, tab);

    assert!(!should_close_pane_tab(
        None,
        request,
        PaneCloseSaveOutcome::Succeeded,
        true,
    ));
}

#[test]
fn timeout_and_failure_keep_the_tab_open() {
    let pane = PaneId::new();
    let tab = TabId::new();
    let request = PaneCloseRequest::new(7, pane, tab);

    for outcome in [PaneCloseSaveOutcome::TimedOut, PaneCloseSaveOutcome::Failed] {
        assert!(!should_close_pane_tab(
            Some(request),
            request,
            outcome,
            true,
        ));
    }
}

/// Rejects a late Host terminal signal because cancellation or a replacement
/// request may have changed the pane-close identity before the callback runs.
#[test]
fn late_host_completion_rechecks_generation_and_tab_identity() {
    let pane = PaneId::new();
    let tab = TabId::new();
    let request = PaneCloseRequest::new(7, pane, tab);
    let terminal_event = crate::document_host::DocumentHostEvent::StateChanged;

    assert!(host_save_event_is_terminal(&terminal_event, false));

    assert!(!should_close_pane_tab(
        Some(PaneCloseRequest::new(8, pane, tab)),
        request,
        PaneCloseSaveOutcome::Succeeded,
        true,
    ));
    let other_tab = TabId::new();
    assert!(!should_close_pane_tab(
        Some(PaneCloseRequest::new(7, pane, other_tab)),
        request,
        PaneCloseSaveOutcome::Succeeded,
        true,
    ));
    assert!(!should_close_pane_tab(
        Some(request),
        request,
        PaneCloseSaveOutcome::Succeeded,
        false,
    ));
}

#[test]
fn a_successful_request_can_close_only_once() {
    let pane = PaneId::new();
    let tab = TabId::new();
    let request = PaneCloseRequest::new(7, pane, tab);

    assert!(should_close_pane_tab(
        Some(request),
        request,
        PaneCloseSaveOutcome::Succeeded,
        true,
    ));
    assert!(!should_close_pane_tab(
        None,
        request,
        PaneCloseSaveOutcome::Succeeded,
        true,
    ));
}

#[test]
fn host_save_start_state_change_waits_for_a_terminal_state_change() {
    let event = crate::document_host::DocumentHostEvent::StateChanged;

    assert!(!host_save_event_is_terminal(&event, true));
    assert!(host_save_event_is_terminal(&event, false));
}
