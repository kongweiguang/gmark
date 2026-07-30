// @author kongweiguang

use super::*;
use gmark_document_core::{SourceAffinity, SourceSelection};

fn selection(offset: u64) -> SourceSelection {
    SourceSelection::collapsed(offset, SourceAffinity::After)
}

#[test]
fn clean_recovery_baseline_shares_one_source_allocation() {
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = ResidentRecoveryJournal::create(temporary.path(), None, "baseline").unwrap();

    assert!(Arc::ptr_eq(&journal.base_source, &journal.last_source));
    journal.checkpoint(None, "saved baseline").unwrap();
    assert!(Arc::ptr_eq(&journal.base_source, &journal.last_source));

    journal
        .record("saved baseline plus edit", selection(24), "rendered")
        .unwrap();
    assert!(!Arc::ptr_eq(&journal.base_source, &journal.last_source));
}
