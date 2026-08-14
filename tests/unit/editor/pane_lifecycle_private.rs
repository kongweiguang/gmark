// @author kongweiguang

use super::{
    PaneCanvasKind, pane_canvas_kind_requires_detach, pane_event_persists_workspace,
    should_persist_pane_event,
};
use crate::editor::panes::{PaneEvent, PaneId, PaneSplitDirection, TabId};

// This regression test keeps accepted and rejected structural requests on the
// same persistence gate so a failed model operation cannot silently diverge.
#[test]
fn structural_event_classification_covers_success_and_rejection_paths() {
    let pane = PaneId::new();
    let target = PaneId::new();
    let tab = TabId::new();
    for event in [
        PaneEvent::Split {
            pane,
            direction: PaneSplitDirection::Right,
        },
        PaneEvent::CopyTab {
            source: pane,
            target,
            tab,
        },
        PaneEvent::Close { pane },
        PaneEvent::MoveTab {
            source: pane,
            target,
            tab,
        },
        PaneEvent::Balance,
    ] {
        assert!(
            pane_event_persists_workspace(&event),
            "a successful or rejected structural request must use the same persistence gate"
        );
        assert!(should_persist_pane_event(&event, true));
        assert!(!should_persist_pane_event(&event, false));
    }
    assert!(!pane_event_persists_workspace(&PaneEvent::Focus { pane }));
    assert!(!pane_event_persists_workspace(&PaneEvent::CloseTab {
        pane,
        tab
    }));
}

// This regression test protects mounted read-only and Markdown entities from
// unnecessary detachment, which would lose their live view state during a rebuild.
#[test]
fn only_document_host_canvases_are_detached_for_structural_rebuilds() {
    assert!(!pane_canvas_kind_requires_detach(PaneCanvasKind::Markdown));
    assert!(pane_canvas_kind_requires_detach(
        PaneCanvasKind::DocumentHost
    ));
    assert!(!pane_canvas_kind_requires_detach(PaneCanvasKind::ReadOnly));
}
