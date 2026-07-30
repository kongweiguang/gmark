// @author kongweiguang

//! GPUI 无关的更新协议、签名、制品验证与应用计划。

#![forbid(unsafe_code)]

pub mod envelope;
pub mod error;
pub mod manifest;
pub mod policy;
pub mod protocol;
pub mod staging;

pub use envelope::{
    SignedEnvelopeV1, VerifiedEnvelope, parse_and_verify_envelope, verifying_key_from_base64,
};
pub use error::{Result, UpdateCoreError};
pub use manifest::{
    ArtifactV1, ArtifactV2, ManifestV1, ManifestV2, SelectedArtifact, SignedManifest,
    UpdateCheckOutcome, UpdateRelease, VerifiedManifest, evaluate_update, parse_verified_manifest,
    select_artifact,
};
pub use policy::{
    ArtifactFormat, MAX_ARTIFACT_BYTES, MAX_ENVELOPE_BYTES, MAX_PAYLOAD_BYTES, Platform,
    SystemTrust, compare_versions, rollout_bucket, rollout_eligible,
};
pub use protocol::{
    ApplyPlanV1, ApplyResultV1, CancellationV1, HelperSignalV1, MAX_APPLY_PLAN_BYTES,
    StagedApplyArtifact, StartupAcknowledgementV1, clear_helper_signal, helper_signal_present,
    parse_apply_result, read_apply_plan, read_apply_result, read_validated_apply_plan,
    stage_and_verify_apply_plan_artifact, startup_acknowledgement_matches, validate_apply_plan,
    validate_apply_plan_at_path, validate_apply_plan_files, verify_apply_plan_artifact,
    write_apply_plan, write_apply_result, write_helper_signal, write_startup_acknowledgement,
};
pub use staging::{
    BoundedTransferOutcome, DownloadControl, DownloadEvent, PartialMetadata, ResumeRequest,
    StagingPaths, copy_and_verify_bounded, copy_bounded, parse_content_range_start,
    read_partial_metadata, resume_request, verify_artifact_file, write_partial_metadata,
};
