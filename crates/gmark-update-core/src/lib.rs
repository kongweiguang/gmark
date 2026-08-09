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
    ApplyFailureCode, ApplyFeedbackModeV1, ApplyPhaseV1, ApplyPlanV1, ApplyPlanV2, ApplyProgressV1,
    ApplyResultV1, ApplyResultV2, CancellationV1, FeedbackMode, FeedbackModeV1, HelperSignalV1,
    MAX_APPLY_MESSAGE_BYTES, MAX_APPLY_PLAN_BYTES, MAX_APPLY_PROGRESS_BYTES,
    MAX_APPLY_RESULT_BYTES, MAX_APPLY_RESULT_V2_BYTES, RecoveryAction, StagedApplyArtifact,
    StartupAcknowledgementV1, clear_helper_signal, helper_signal_present, parse_apply_progress,
    parse_apply_progress_v1, parse_apply_result, parse_apply_result_v2, read_apply_plan,
    read_apply_plan_v2, read_apply_progress, read_apply_progress_v1, read_apply_result,
    read_apply_result_v2, read_validated_apply_plan, read_validated_apply_plan_v2,
    read_validated_apply_progress, read_validated_apply_progress_v1, read_validated_apply_result,
    read_validated_apply_result_v2, stage_and_verify_apply_plan_artifact,
    stage_and_verify_apply_plan_artifact_v2, startup_acknowledgement_matches, validate_apply_plan,
    validate_apply_plan_at_path, validate_apply_plan_files, validate_apply_plan_v2,
    validate_apply_plan_v2_at_path, validate_apply_plan_v2_files, validate_apply_progress,
    validate_apply_progress_at_path, validate_apply_result_v2, validate_apply_result_v2_at_path,
    verify_apply_plan_artifact, verify_apply_plan_artifact_v2, write_apply_plan,
    write_apply_plan_v2, write_apply_progress, write_apply_progress_for_plan,
    write_apply_progress_v1, write_apply_result, write_apply_result_for_plan,
    write_apply_result_v2, write_helper_signal, write_startup_acknowledgement,
};
pub use staging::{
    BoundedTransferOutcome, DownloadControl, DownloadEvent, PartialMetadata, ResumeRequest,
    StagingPaths, copy_and_verify_bounded, copy_bounded, parse_content_range_start,
    read_partial_metadata, resume_request, verify_artifact_file, write_partial_metadata,
};
