// @author kongweiguang

//! Apply protocol v2 bounded I/O, layout validation, and artifact verification.

use std::{
    fs,
    io::ErrorKind,
    path::{Component, Path},
};

use ed25519_dalek::VerifyingKey;

use crate::{
    Result, UpdateCoreError,
    manifest::VerifiedManifest,
    policy::{Platform, validate_official_artifact_url, validate_sha256},
    staging::verify_artifact_file,
};

use super::super::{
    MAX_APPLY_MESSAGE_BYTES, MAX_APPLY_PLAN_BYTES, MAX_APPLY_PROGRESS_BYTES,
    MAX_APPLY_RESULT_V2_BYTES, MAX_STARTUP_ACKNOWLEDGEMENT_BYTES, StagedApplyArtifact, artifact,
    read_bounded, validate_clean_absolute_path, validate_platform_plan,
    validate_platform_target_on_disk, write_apply_plan_atomic, write_atomic,
};
use super::*;

/// Reads a strict v2 plan with the same bounded/no-follow guarantees as v1.
pub fn read_apply_plan_v2(path: impl AsRef<Path>) -> Result<ApplyPlanV2> {
    let path = path.as_ref();
    validate_clean_absolute_path(path, "apply plan v2")?;
    validate_existing_path_components(path, "apply plan v2")?;
    let bytes = read_bounded(path, MAX_APPLY_PLAN_BYTES, "apply plan v2")?;
    let plan: ApplyPlanV2 = serde_json::from_slice(&bytes)
        .map_err(|error| UpdateCoreError::Protocol(format!("invalid apply plan v2: {error}")))?;
    if plan.schema_version != ApplyPlanV2::SCHEMA_VERSION {
        return Err(UpdateCoreError::Protocol(
            "unsupported apply plan v2 schema".to_owned(),
        ));
    }
    Ok(plan)
}

/// Reads and validates a v2 plan against both its supplied path and platform.
pub fn read_validated_apply_plan_v2(
    path: impl AsRef<Path>,
    platform: &Platform,
) -> Result<ApplyPlanV2> {
    let path = path.as_ref();
    let plan = read_apply_plan_v2(path)?;
    validate_apply_plan_v2_at_path(&plan, path, platform)?;
    Ok(plan)
}

/// Writes only schema-2 plans and commits them with an atomic rename.
pub fn write_apply_plan_v2(path: impl AsRef<Path>, plan: &ApplyPlanV2) -> Result<()> {
    validate_apply_plan_v2_layout(plan)?;
    validate_plan_path_v2(plan, path.as_ref())?;
    validate_existing_path_components(path.as_ref(), "apply plan v2")?;
    let bytes = serde_json::to_vec_pretty(plan).map_err(|error| {
        UpdateCoreError::Protocol(format!("failed to serialize update apply plan v2: {error}"))
    })?;
    if bytes.len() as u64 > MAX_APPLY_PLAN_BYTES {
        return Err(UpdateCoreError::Protocol(
            "apply plan v2 exceeds its size limit".to_owned(),
        ));
    }
    write_apply_plan_atomic(path.as_ref(), &bytes)
}

/// Strict pure validation of a schema-2 plan.
pub fn validate_apply_plan_v2(plan: &ApplyPlanV2, platform: &Platform) -> Result<()> {
    validate_apply_plan_v2_layout(plan)?;
    let target = semver::Version::parse(&plan.target_version)
        .map_err(|error| UpdateCoreError::Protocol(format!("invalid target version: {error}")))?;
    let current = semver::Version::parse(&plan.current_version)
        .map_err(|error| UpdateCoreError::Protocol(format!("invalid current version: {error}")))?;
    if target <= current {
        return Err(UpdateCoreError::Protocol(
            "target version must be newer than current version".to_owned(),
        ));
    }
    if plan.target_version.len().saturating_add(1) > MAX_STARTUP_ACKNOWLEDGEMENT_BYTES {
        return Err(UpdateCoreError::Protocol(
            "target version exceeds the acknowledgement size limit".to_owned(),
        ));
    }
    if plan.artifact_size == 0 || plan.artifact_size > crate::MAX_ARTIFACT_BYTES {
        return Err(UpdateCoreError::Protocol(
            "apply plan has invalid artifact bounds or digest".to_owned(),
        ));
    }
    validate_sha256(&plan.artifact_sha256, "apply plan artifact").map_err(|_| {
        UpdateCoreError::Protocol("apply plan has invalid artifact bounds or digest".to_owned())
    })?;
    validate_official_artifact_url(&plan.artifact_url)?;
    validate_platform_plan(&plan.projection_for_artifact(), platform)?;
    // Existing transaction files are inspected when present, but a plan may
    // legitimately be validated before its helper creates the files.
    validate_v2_existing_protocol_files(plan)
}

/// Binds a v2 plan to the one transaction directory represented by its path.
pub fn validate_apply_plan_v2_at_path(
    plan: &ApplyPlanV2,
    plan_path: impl AsRef<Path>,
    platform: &Platform,
) -> Result<()> {
    validate_apply_plan_v2(plan, platform)?;
    validate_plan_path_v2(plan, plan_path.as_ref())
}

/// Performs no-follow filesystem checks after pure v2 validation succeeds.
pub fn validate_apply_plan_v2_files(plan: &ApplyPlanV2, platform: &Platform) -> Result<()> {
    validate_apply_plan_v2(plan, platform)?;
    let transaction_dir = plan.transaction_dir().ok_or_else(|| {
        UpdateCoreError::Protocol("v2 plan has no transaction directory".to_owned())
    })?;
    if !artifact::is_directory_non_reparse(transaction_dir)
        || !artifact::is_regular_non_reparse_file(&plan.artifact_path)
        || !artifact::is_regular_non_reparse_file(&plan.signed_envelope_path)
    {
        return Err(UpdateCoreError::Protocol(
            "verified v2 update files are missing".to_owned(),
        ));
    }
    validate_v2_existing_protocol_files(plan)?;
    validate_platform_target_on_disk(&plan.projection_for_artifact(), platform)
}

/// Re-verifies the signed manifest and artifact bytes from a v2 plan.
pub fn verify_apply_plan_artifact_v2(
    plan: &ApplyPlanV2,
    key: &VerifyingKey,
    platform: &Platform,
) -> Result<VerifiedManifest> {
    validate_apply_plan_v2_files(plan, platform)?;
    let projection = plan.projection_for_artifact();
    let verified = artifact::verify_apply_plan_manifest_metadata(&projection, key, platform)?;
    verify_artifact_file(
        &plan.artifact_path,
        plan.artifact_size,
        &plan.artifact_sha256,
    )?;
    Ok(verified)
}

/// Stages a private, guarded artifact copy for a v2 transaction.
pub fn stage_and_verify_apply_plan_artifact_v2(
    plan: &ApplyPlanV2,
    key: &VerifyingKey,
    platform: &Platform,
    staging_directory: impl AsRef<Path>,
) -> Result<StagedApplyArtifact> {
    validate_apply_plan_v2_files(plan, platform)?;
    let projection = plan.projection_for_artifact();
    artifact::stage_and_verify_prevalidated_apply_plan_artifact(
        &projection,
        key,
        platform,
        staging_directory,
    )
}

/// Reads a bounded, strict progress snapshot.
pub fn read_apply_progress(path: impl AsRef<Path>) -> Result<ApplyProgressV1> {
    let path = path.as_ref();
    validate_clean_absolute_path(path, "update progress")?;
    validate_existing_path_components(path, "update progress")?;
    let bytes = read_bounded(path, MAX_APPLY_PROGRESS_BYTES, "update progress")?;
    parse_apply_progress(&bytes)
}

/// Schema-surface alias for adapters that make the version explicit.
pub fn read_apply_progress_v1(path: impl AsRef<Path>) -> Result<ApplyProgressV1> {
    read_apply_progress(path)
}

pub fn parse_apply_progress(bytes: &[u8]) -> Result<ApplyProgressV1> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_APPLY_PROGRESS_BYTES {
        return Err(UpdateCoreError::Protocol(
            "update progress exceeds its size limit".to_owned(),
        ));
    }
    let progress: ApplyProgressV1 = serde_json::from_slice(bytes)
        .map_err(|error| UpdateCoreError::Protocol(format!("invalid update progress: {error}")))?;
    validate_apply_progress(&progress)?;
    Ok(progress)
}

pub fn parse_apply_progress_v1(bytes: &[u8]) -> Result<ApplyProgressV1> {
    parse_apply_progress(bytes)
}

pub fn validate_apply_progress(progress: &ApplyProgressV1) -> Result<()> {
    if progress.schema_version != ApplyProgressV1::SCHEMA_VERSION {
        return Err(UpdateCoreError::Protocol(
            "unsupported update progress schema".to_owned(),
        ));
    }
    validate_transaction_id(progress.transaction_id)?;
    validate_message(&progress.message, "update progress")
}

/// Atomically replaces a v2 progress file after validating its size and schema.
pub fn write_apply_progress(path: impl AsRef<Path>, progress: &ApplyProgressV1) -> Result<()> {
    validate_apply_progress(progress)?;
    let path = path.as_ref();
    validate_clean_absolute_path(path, "update progress")?;
    validate_existing_path_components(path, "update progress")?;
    let bytes = serde_json::to_vec_pretty(progress).map_err(|error| {
        UpdateCoreError::Protocol(format!("failed to serialize update progress: {error}"))
    })?;
    if bytes.len() as u64 > MAX_APPLY_PROGRESS_BYTES {
        return Err(UpdateCoreError::Protocol(
            "update progress exceeds its size limit".to_owned(),
        ));
    }
    write_atomic(path, &bytes, "update progress")
}

pub fn write_apply_progress_v1(path: impl AsRef<Path>, progress: &ApplyProgressV1) -> Result<()> {
    write_apply_progress(path, progress)
}

/// Validates that a snapshot belongs to the plan's transaction and fixed path.
pub fn validate_apply_progress_at_path(
    progress: &ApplyProgressV1,
    plan: &ApplyPlanV2,
    path: impl AsRef<Path>,
) -> Result<()> {
    validate_apply_progress(progress)?;
    if progress.transaction_id != plan.transaction_id {
        return Err(UpdateCoreError::Protocol(
            "update progress transaction id does not match the apply plan".to_owned(),
        ));
    }
    if path.as_ref() != plan.progress_path {
        return Err(UpdateCoreError::Protocol(
            "update progress path does not match the apply plan".to_owned(),
        ));
    }
    Ok(())
}

pub fn read_validated_apply_progress(
    plan: &ApplyPlanV2,
    platform: &Platform,
) -> Result<ApplyProgressV1> {
    validate_apply_plan_v2(plan, platform)?;
    let progress = read_apply_progress(&plan.progress_path)?;
    validate_apply_progress_at_path(&progress, plan, &plan.progress_path)?;
    Ok(progress)
}

pub fn read_validated_apply_progress_v1(
    plan: &ApplyPlanV2,
    platform: &Platform,
) -> Result<ApplyProgressV1> {
    read_validated_apply_progress(plan, platform)
}

pub fn write_apply_progress_for_plan(plan: &ApplyPlanV2, progress: &ApplyProgressV1) -> Result<()> {
    validate_apply_plan_v2_layout(plan)?;
    validate_apply_progress_at_path(progress, plan, &plan.progress_path)?;
    write_apply_progress(&plan.progress_path, progress)
}

/// Reads a bounded strict v2 result and validates its failure/recovery pair.
pub fn read_apply_result_v2(path: impl AsRef<Path>) -> Result<ApplyResultV2> {
    let path = path.as_ref();
    validate_clean_absolute_path(path, "update result v2")?;
    validate_existing_path_components(path, "update result v2")?;
    let bytes = read_bounded(path, MAX_APPLY_RESULT_V2_BYTES, "update result v2")?;
    parse_apply_result_v2(&bytes)
}

pub fn parse_apply_result_v2(bytes: &[u8]) -> Result<ApplyResultV2> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_APPLY_RESULT_V2_BYTES {
        return Err(UpdateCoreError::Protocol(
            "update result v2 exceeds its size limit".to_owned(),
        ));
    }
    let result: ApplyResultV2 = serde_json::from_slice(bytes)
        .map_err(|error| UpdateCoreError::Protocol(format!("invalid update result v2: {error}")))?;
    validate_apply_result_v2(&result)?;
    Ok(result)
}

pub fn validate_apply_result_v2(result: &ApplyResultV2) -> Result<()> {
    if result.schema_version != ApplyResultV2::SCHEMA_VERSION {
        return Err(UpdateCoreError::Protocol(
            "unsupported update result v2 schema".to_owned(),
        ));
    }
    validate_transaction_id(result.transaction_id)?;
    if !matches!(result.status.as_str(), "succeeded" | "failed") {
        return Err(UpdateCoreError::Protocol(
            "update result v2 has an unsupported status".to_owned(),
        ));
    }
    semver::Version::parse(&result.from_version).map_err(|error| {
        UpdateCoreError::Protocol(format!("invalid result source version: {error}"))
    })?;
    semver::Version::parse(&result.to_version).map_err(|error| {
        UpdateCoreError::Protocol(format!("invalid result target version: {error}"))
    })?;
    validate_message(&result.message, "update result")?;
    match result.status.as_str() {
        "succeeded" if result.failure_code.is_some() || result.recovery_action.is_some() => {
            Err(UpdateCoreError::Protocol(
                "successful update result cannot contain failure recovery fields".to_owned(),
            ))
        }
        "failed" if result.failure_code.is_none() || result.recovery_action.is_none() => {
            Err(UpdateCoreError::Protocol(
                "failed update result must contain failure recovery fields".to_owned(),
            ))
        }
        _ => Ok(()),
    }
}

pub fn write_apply_result_v2(path: impl AsRef<Path>, result: &ApplyResultV2) -> Result<()> {
    validate_apply_result_v2(result)?;
    let path = path.as_ref();
    validate_clean_absolute_path(path, "update result v2")?;
    validate_existing_path_components(path, "update result v2")?;
    let bytes = serde_json::to_vec_pretty(result).map_err(|error| {
        UpdateCoreError::Protocol(format!("failed to serialize update result v2: {error}"))
    })?;
    if bytes.len() as u64 > MAX_APPLY_RESULT_V2_BYTES {
        return Err(UpdateCoreError::Protocol(
            "update result v2 exceeds its size limit".to_owned(),
        ));
    }
    write_atomic(path, &bytes, "update result v2")
}

pub fn validate_apply_result_v2_at_path(
    result: &ApplyResultV2,
    plan: &ApplyPlanV2,
    path: impl AsRef<Path>,
) -> Result<()> {
    validate_apply_result_v2(result)?;
    if result.transaction_id != plan.transaction_id {
        return Err(UpdateCoreError::Protocol(
            "update result transaction id does not match the apply plan".to_owned(),
        ));
    }
    if path.as_ref() != plan.result_path {
        return Err(UpdateCoreError::Protocol(
            "update result path does not match the apply plan".to_owned(),
        ));
    }
    Ok(())
}

pub fn read_validated_apply_result(
    plan: &ApplyPlanV2,
    platform: &Platform,
) -> Result<ApplyResultV2> {
    validate_apply_plan_v2(plan, platform)?;
    let result = read_apply_result_v2(&plan.result_path)?;
    validate_apply_result_v2_at_path(&result, plan, &plan.result_path)?;
    Ok(result)
}

pub fn read_validated_apply_result_v2(
    plan: &ApplyPlanV2,
    platform: &Platform,
) -> Result<ApplyResultV2> {
    read_validated_apply_result(plan, platform)
}

pub fn write_apply_result_for_plan(plan: &ApplyPlanV2, result: &ApplyResultV2) -> Result<()> {
    validate_apply_plan_v2_layout(plan)?;
    validate_apply_result_v2_at_path(result, plan, &plan.result_path)?;
    write_apply_result_v2(&plan.result_path, result)
}

fn validate_apply_plan_v2_layout(plan: &ApplyPlanV2) -> Result<()> {
    if plan.schema_version != ApplyPlanV2::SCHEMA_VERSION {
        return Err(UpdateCoreError::Protocol(
            "unsupported apply plan v2 schema".to_owned(),
        ));
    }
    validate_transaction_id(plan.transaction_id)?;
    for (label, path) in [
        ("update artifact", &plan.artifact_path),
        ("signed manifest", &plan.signed_envelope_path),
        ("expected install root", &plan.expected_install_root),
        ("update target", &plan.target_path),
        ("update backup", &plan.backup_path),
        ("update relaunch", &plan.relaunch_path),
        ("startup acknowledgement", &plan.acknowledgement_path),
        ("cancellation marker", &plan.cancellation_path),
        ("update result", &plan.result_path),
        ("helper log", &plan.helper_log_path),
        ("lifetime lock", &plan.lifetime_lock_path),
        ("update progress", &plan.progress_path),
        ("installer log", &plan.installer_log_path),
    ] {
        validate_clean_absolute_path(path, label)?;
        validate_existing_path_components(path, label)?;
    }
    let transaction_dir = plan.transaction_dir().ok_or_else(|| {
        UpdateCoreError::Protocol("v2 artifact has no transaction directory".to_owned())
    })?;
    if [
        &plan.expected_install_root,
        &plan.target_path,
        &plan.backup_path,
        &plan.relaunch_path,
    ]
    .iter()
    .any(|path| path.starts_with(transaction_dir))
    {
        return Err(UpdateCoreError::Protocol(
            "installed target paths must remain outside the update transaction".to_owned(),
        ));
    }
    let fixed_layout_matches = plan.artifact_path
        == transaction_dir.join(ApplyPlanV2::ARTIFACT_FILE_NAME)
        && plan.signed_envelope_path
            == transaction_dir.join(ApplyPlanV2::SIGNED_ENVELOPE_FILE_NAME)
        && plan.acknowledgement_path
            == transaction_dir.join(ApplyPlanV2::ACKNOWLEDGEMENT_FILE_NAME)
        && plan.cancellation_path == transaction_dir.join(ApplyPlanV2::CANCELLATION_FILE_NAME)
        && plan.result_path == transaction_dir.join(ApplyPlanV2::RESULT_FILE_NAME)
        && plan.helper_log_path == transaction_dir.join(ApplyPlanV2::HELPER_LOG_FILE_NAME)
        && plan.lifetime_lock_path == transaction_dir.join(ApplyPlanV2::LIFETIME_LOCK_FILE_NAME)
        && plan.progress_path == transaction_dir.join(ApplyPlanV2::PROGRESS_FILE_NAME)
        && plan.installer_log_path == transaction_dir.join(ApplyPlanV2::INSTALLER_LOG_FILE_NAME);
    if !fixed_layout_matches {
        return Err(UpdateCoreError::Protocol(
            "v2 apply plan paths do not match the fixed transaction layout".to_owned(),
        ));
    }
    let transaction_name = plan.transaction_id.hyphenated().to_string();
    let transactions_dir = transaction_dir.parent().ok_or_else(|| {
        UpdateCoreError::Protocol("v2 transaction has no transactions root".to_owned())
    })?;
    let version_dir = transactions_dir.parent().ok_or_else(|| {
        UpdateCoreError::Protocol("v2 transaction has no version root".to_owned())
    })?;
    if transaction_dir.file_name().and_then(|name| name.to_str()) != Some(transaction_name.as_str())
        || transactions_dir.file_name().and_then(|name| name.to_str())
            != Some(ApplyPlanV2::TRANSACTIONS_DIR_NAME)
        || version_dir.file_name().and_then(|name| name.to_str())
            != Some(format!("v{}", plan.target_version).as_str())
    {
        return Err(UpdateCoreError::Protocol(
            "v2 transaction directory does not match the target version and transaction id"
                .to_owned(),
        ));
    }
    validate_install_root_binding(plan)?;
    Ok(())
}

fn validate_install_root_binding(plan: &ApplyPlanV2) -> Result<()> {
    let target_bound = plan.target_path == plan.expected_install_root
        || plan.target_path.starts_with(&plan.expected_install_root);
    let relaunch_bound = plan.relaunch_path == plan.expected_install_root
        || plan.relaunch_path.starts_with(&plan.expected_install_root);
    if !target_bound || !relaunch_bound {
        return Err(UpdateCoreError::Protocol(
            "update target and relaunch paths must be bound to the expected install root"
                .to_owned(),
        ));
    }
    let backup_parent = plan.backup_path.parent();
    let install_parent = plan.expected_install_root.parent();
    let backup_owned = plan
        .backup_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains(&plan.transaction_id.hyphenated().to_string()));
    if backup_parent != install_parent || !backup_owned {
        return Err(UpdateCoreError::Protocol(
            "update backup must be a transaction-owned sibling of the expected install root"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_plan_path_v2(plan: &ApplyPlanV2, plan_path: &Path) -> Result<()> {
    validate_clean_absolute_path(plan_path, "apply plan v2")?;
    let transaction_dir = plan.transaction_dir().ok_or_else(|| {
        UpdateCoreError::Protocol("v2 artifact has no transaction directory".to_owned())
    })?;
    if plan_path.file_name().and_then(|name| name.to_str()) != Some(ApplyPlanV2::PLAN_FILE_NAME)
        || plan_path.parent() != Some(transaction_dir)
    {
        return Err(UpdateCoreError::Protocol(
            "v2 apply plan path does not match the transaction directory".to_owned(),
        ));
    }
    Ok(())
}

fn validate_transaction_id(transaction_id: uuid::Uuid) -> Result<()> {
    if transaction_id.is_nil() {
        return Err(UpdateCoreError::Protocol(
            "update transaction id must not be nil".to_owned(),
        ));
    }
    Ok(())
}

fn validate_message(message: &str, label: &str) -> Result<()> {
    if message.len() > MAX_APPLY_MESSAGE_BYTES {
        return Err(UpdateCoreError::Protocol(format!(
            "{label} message exceeds its size limit"
        )));
    }
    Ok(())
}

fn validate_v2_existing_protocol_files(plan: &ApplyPlanV2) -> Result<()> {
    for (label, path) in [
        (
            "v2 apply plan transaction",
            plan.transaction_dir().unwrap_or(Path::new("")),
        ),
        ("v2 result", plan.result_path.as_path()),
        ("v2 progress", plan.progress_path.as_path()),
        ("v2 lifetime lock", plan.lifetime_lock_path.as_path()),
        ("v2 installer log", plan.installer_log_path.as_path()),
        ("v2 helper log", plan.helper_log_path.as_path()),
    ] {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(UpdateCoreError::Io(format!(
                    "failed to inspect {label}: {error}"
                )));
            }
        };
        if metadata.file_type().is_symlink() || artifact::is_reparse_metadata(&metadata) {
            return Err(UpdateCoreError::Protocol(format!(
                "{label} must not be a symlink or reparse point"
            )));
        }
        if label != "v2 apply plan transaction" && !metadata.file_type().is_file() {
            return Err(UpdateCoreError::Protocol(format!(
                "{label} must be a regular file"
            )));
        }
    }
    Ok(())
}

fn validate_existing_path_components(path: &Path, label: &str) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        #[cfg(windows)]
        if matches!(component, Component::Prefix(_)) {
            // `C:` alone is a drive-relative namespace, not a filesystem
            // object. Inspect from `C:\` onward once RootDir is present.
            continue;
        }
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => break,
            Err(error) => {
                return Err(UpdateCoreError::Io(format!(
                    "failed to inspect {label} path: {error}"
                )));
            }
        };
        if metadata.file_type().is_symlink() || artifact::is_reparse_metadata(&metadata) {
            return Err(UpdateCoreError::Protocol(format!(
                "{label} path contains a symlink or reparse point"
            )));
        }
    }
    Ok(())
}
