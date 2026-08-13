// @author kongweiguang

//! Helper-launch plan construction and cache recovery.

use std::path::PathBuf;

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

pub(super) struct PreparedInstall {
    pub(super) plan_path: PathBuf,
    pub(super) helper: StagedHelper,
    /// Windows uses Inno Setup's own progress surface, so no feedback agent
    /// is staged there; other platforms keep the existing lightweight agent.
    pub(super) agent: Option<StagedAgent>,
    pub(super) plan_v2: gmark_update_core::ApplyPlanV2,
    pub(super) acknowledgement_capability: String,
}
