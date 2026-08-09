// @author kongweiguang

use accesskit::{Live, Role};

use super::{Snapshot, build_tree};

#[test]
fn busy_state_is_a_polite_progress_indicator() {
    let tree = build_tree(Snapshot {
        phase: "Installing".to_owned(),
        message: "Installing update".to_owned(),
        failure: false,
    });
    let status = tree.nodes.iter().find(|(id, _)| id.0 == 1).unwrap();
    assert_eq!(status.1.role(), Role::ProgressIndicator);
    assert_eq!(status.1.live(), Some(Live::Polite));
}

#[test]
fn failure_is_an_assertive_alert() {
    let tree = build_tree(Snapshot {
        phase: "Failed".to_owned(),
        message: "rollback failed".to_owned(),
        failure: true,
    });
    let status = tree.nodes.iter().find(|(id, _)| id.0 == 1).unwrap();
    assert_eq!(status.1.role(), Role::Alert);
    assert_eq!(status.1.live(), Some(Live::Assertive));
}
