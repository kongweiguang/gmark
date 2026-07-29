// @author kongweiguang

//! Resident document crash-recovery journals.
//!
//! This module owns only durable recovery state. Window lifecycle, timers, and
//! user-facing conflict decisions stay in the application adapter.

mod format;
mod journal;
mod replay;
mod types;

pub use journal::ResidentRecoveryJournal;
pub use replay::{
    cleanup_resident_recovery_artifacts, fingerprint_resident_file,
    load_resident_recovery_documents, load_resident_recovery_journals,
    replay_resident_recovery_journal, replay_resident_recovery_journal_with_metadata,
};
pub use types::{
    RecoveredResidentDocument, RecoveredResidentJournal, ResidentFileFingerprint,
    ResidentRecoveryError, ResidentRecoveryReadStatus, ResidentRecoverySelection,
};
