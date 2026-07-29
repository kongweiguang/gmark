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
    load_resident_recovery_documents, replay_resident_recovery_journal,
};
pub use types::{
    RecoveredResidentDocument, ResidentFileFingerprint, ResidentRecoveryError,
    ResidentRecoveryReadStatus, ResidentRecoverySelection,
};
