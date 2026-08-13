// @author kongweiguang

//! Helper-launch plan construction and cache recovery.

use std::{path::PathBuf, time::Instant};

use super::*;

mod cache;
mod lifecycle;
mod prepare;
mod staging;

pub(super) use cache::*;
pub(super) use lifecycle::*;
pub(super) use prepare::{cleanup_failed_prepare, write_apply_plan as prepare_apply_plan};
pub(super) use staging::*;

pub(super) fn resolve_current_update_target() -> Result<CurrentUpdateTarget, String> {
    lifecycle::current_update_target()
}

/// Inherited rather than passed on the command line so an acknowledgement path
/// alone is not a capability to write into the update cache.
pub(super) const UPDATE_ACK_CAPABILITY_ENV: &str = "GMARK_UPDATE_ACK_CAPABILITY";

pub(super) enum WorkerEvent {
    Download(DownloadEvent),
    Failed { message: String, retryable: bool },
}

pub(super) struct PendingInstall {
    pub(super) release: UpdateRelease,
    pub(super) artifact_path: PathBuf,
    pub(super) plan: ApplyPlanV1,
}

pub(super) struct PreparedInstall {
    pub(super) plan_path: PathBuf,
    pub(super) helper: StagedHelper,
    pub(super) agent: StagedAgent,
    pub(super) plan: ApplyPlanV1,
    pub(super) plan_v2: gmark_update_core::ApplyPlanV2,
    pub(super) acknowledgement_capability: String,
}

/// Metadata kept outside the GPUI entity so a dropped entity cannot release
/// the process lifetime lock while the OS process is still alive.
#[derive(Clone)]
pub(super) struct InstallAttempt {
    pub(super) release: UpdateRelease,
    pub(super) artifact_path: PathBuf,
    pub(super) plan_path: PathBuf,
    pub(super) plan: gmark_update_core::ApplyPlanV2,
    pub(super) helper: StagedHelper,
    pub(super) agent: StagedAgent,
    pub(super) acknowledgement_capability: String,
    pub(super) started_at: Instant,
}

/// A retry is deliberately independent from a previous transaction.  The
/// source artifact and signed envelope remain in `v<version>`; preparing the
/// retry allocates a new UUID transaction directory.
#[derive(Clone)]
pub(super) struct RetryPayload {
    pub(super) release: UpdateRelease,
    pub(super) artifact_path: PathBuf,
}
