// @author kongweiguang

//! Versioned file protocol shared conceptually with the out-of-process update helper.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApplyPlanV1 {
    pub schema_version: u8,
    pub parent_pid: u32,
    pub current_version: String,
    pub target_version: String,
    pub artifact_path: PathBuf,
    pub artifact_url: String,
    pub artifact_size: u64,
    pub artifact_sha256: String,
    pub artifact_format: String,
    pub signed_envelope_path: PathBuf,
    pub target_path: PathBuf,
    pub backup_path: PathBuf,
    pub relaunch_path: PathBuf,
    pub acknowledgement_path: PathBuf,
    pub cancellation_path: PathBuf,
    pub result_path: PathBuf,
    pub helper_log_path: PathBuf,
}

impl ApplyPlanV1 {
    pub(crate) const SCHEMA_VERSION: u8 = 1;
}
