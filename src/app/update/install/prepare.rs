// @author kongweiguang

//! Atomic preparation of one app-side V2 apply transaction.

use super::*;
use std::path::Path;

/// Stages a fresh UUID transaction only after quit approval, keeping the
/// verified source reusable when any pre-handoff validation fails.
pub(crate) fn write_apply_plan(
    updates_root: &Path,
    release: &UpdateRelease,
    artifact_path: &Path,
) -> Result<PreparedInstall, String> {
    let source_dir = artifact_path
        .parent()
        .ok_or_else(|| "verified update has no transaction directory".to_owned())?;
    let root_metadata = std::fs::symlink_metadata(updates_root)
        .map_err(|error| format!("failed to inspect update cache root: {error}"))?;
    if !is_real_directory(&root_metadata)
        || !updates_root.is_absolute()
        || !artifact_path.is_absolute()
        || source_dir.parent() != Some(updates_root)
        || source_dir.file_name().and_then(|name| name.to_str())
            != Some(format!("v{}", release.version).as_str())
    {
        return Err("verified update is outside the configured transaction root".to_owned());
    }
    let source_envelope = source_dir.join("manifest.envelope.json");
    let artifact_metadata = std::fs::symlink_metadata(artifact_path)
        .map_err(|error| format!("failed to inspect verified update artifact: {error}"))?;
    let envelope_metadata = std::fs::symlink_metadata(&source_envelope)
        .map_err(|error| format!("failed to inspect verified update manifest: {error}"))?;
    if !is_real_regular_file(&artifact_metadata) || !is_real_regular_file(&envelope_metadata) {
        return Err("verified update manifest is missing from the cache".to_owned());
    }
    let target = super::lifecycle::current_update_target()?;
    let base_plan = ApplyPlanV1 {
        schema_version: ApplyPlanV1::SCHEMA_VERSION,
        parent_pid: std::process::id(),
        current_version: release.current_version.clone(),
        target_version: release.version.clone(),
        artifact_path: artifact_path.to_path_buf(),
        artifact_url: release.artifact_url.clone(),
        artifact_size: release.artifact_size,
        artifact_sha256: release.artifact_sha256.clone(),
        artifact_format: release.artifact_format.as_protocol_name().to_owned(),
        signed_envelope_path: source_envelope.clone(),
        target_path: target.target_path.clone(),
        backup_path: sibling_backup_path(&target.expected_install_root),
        relaunch_path: current_relaunch_path(&target.target_path),
        acknowledgement_path: source_dir.join("startup-ack"),
        cancellation_path: source_dir.join("cancel-install"),
        result_path: source_dir.join("last-result.json"),
        helper_log_path: source_dir.join("last-helper.log"),
    };
    let transaction_id = uuid::Uuid::new_v4();
    let mut plan_v2 = gmark_update_core::ApplyPlanV2::from_v1(&base_plan, transaction_id);
    plan_v2.feedback_mode = if cfg!(target_os = "windows") {
        // Inno Setup supplies the native installation progress UI on Windows.
        gmark_update_core::ApplyFeedbackModeV1::ProgressFile
    } else {
        gmark_update_core::ApplyFeedbackModeV1::Agent
    };
    let transaction_dir = plan_v2
        .transaction_dir()
        .ok_or_else(|| "v2 update plan has no transaction directory".to_owned())?
        .to_path_buf();
    create_transaction_directory(&transaction_dir)?;
    if !claim_transaction(&transaction_dir)? {
        return Err("update transaction is already claimed".to_owned());
    }
    if let Err(error) = copy_verified_artifact(
        artifact_path,
        &plan_v2.artifact_path,
        release.artifact_size,
        &release.artifact_sha256,
    ) {
        cleanup_failed_prepare(transaction_id, &transaction_dir, None);
        return Err(error);
    }
    if let Err(error) = copy_regular_file(&source_envelope, &plan_v2.signed_envelope_path) {
        cleanup_failed_prepare(transaction_id, &transaction_dir, None);
        return Err(format!("failed to stage verified update manifest: {error}"));
    }
    let mut plan = base_plan;
    plan.artifact_path = plan_v2.artifact_path.clone();
    plan.signed_envelope_path = plan_v2.signed_envelope_path.clone();
    plan.target_path = plan_v2.target_path.clone();
    plan.backup_path = plan_v2.backup_path.clone();
    plan.relaunch_path = plan_v2.relaunch_path.clone();
    plan.acknowledgement_path = plan_v2.acknowledgement_path.clone();
    plan.cancellation_path = plan_v2.cancellation_path.clone();
    plan.result_path = plan_v2.result_path.clone();
    plan.helper_log_path = plan_v2.helper_log_path.clone();
    for signal in [
        HelperSignalV1::Cancellation,
        HelperSignalV1::Acknowledgement,
    ] {
        if let Err(error) = clear_helper_signal(&plan, signal) {
            cleanup_failed_prepare(transaction_id, &transaction_dir, None);
            return Err(format!(
                "failed to clear stale update helper signal: {error}"
            ));
        }
    }
    let plan_path = transaction_dir.join(gmark_update_core::ApplyPlanV2::PLAN_FILE_NAME);
    let acknowledgement_capability = match create_acknowledgement_capability(&transaction_dir) {
        Ok(capability) => capability,
        Err(error) => {
            cleanup_failed_prepare(transaction_id, &transaction_dir, None);
            return Err(error);
        }
    };
    if let Err(error) = register_lifecycle_lock(&plan_v2) {
        cleanup_failed_prepare(
            transaction_id,
            &transaction_dir,
            Some(&acknowledgement_capability),
        );
        return Err(error);
    }
    let installed_helper = match installed_helper_path() {
        Ok(path) => path,
        Err(error) => {
            cleanup_failed_prepare(
                transaction_id,
                &transaction_dir,
                Some(&acknowledgement_capability),
            );
            return Err(error);
        }
    };
    let helper = match stage_update_helper(&transaction_dir, &installed_helper) {
        Ok(helper) => helper,
        Err(error) => {
            cleanup_failed_prepare(
                transaction_id,
                &transaction_dir,
                Some(&acknowledgement_capability),
            );
            return Err(error);
        }
    };
    #[cfg(target_os = "windows")]
    let agent = None;
    #[cfg(not(target_os = "windows"))]
    let agent = {
        let installed_agent = match installed_agent_path() {
            Ok(path) => path,
            Err(error) => {
                cleanup_failed_prepare(
                    transaction_id,
                    &transaction_dir,
                    Some(&acknowledgement_capability),
                );
                return Err(error);
            }
        };
        match stage_update_agent(&transaction_dir, &installed_agent) {
            Ok(agent) => Some(agent),
            Err(error) => {
                cleanup_failed_prepare(
                    transaction_id,
                    &transaction_dir,
                    Some(&acknowledgement_capability),
                );
                return Err(error);
            }
        }
    };
    if let Err(error) = gmark_update_core::write_apply_plan_v2(&plan_path, &plan_v2) {
        cleanup_failed_prepare(
            transaction_id,
            &transaction_dir,
            Some(&acknowledgement_capability),
        );
        return Err(format!("failed to write update apply plan v2: {error}"));
    }
    let validated = gmark_update_core::read_validated_apply_plan_v2(
        &plan_path,
        &gmark_update_core::Platform::current(),
    )
    .map_err(|error| {
        cleanup_failed_prepare(
            transaction_id,
            &transaction_dir,
            Some(&acknowledgement_capability),
        );
        format!("failed to validate staged update apply plan v2: {error}")
    })?;
    if validated != plan_v2 {
        cleanup_failed_prepare(
            transaction_id,
            &transaction_dir,
            Some(&acknowledgement_capability),
        );
        return Err("staged update apply plan v2 changed during validation".to_owned());
    }
    Ok(PreparedInstall {
        plan_path,
        helper,
        agent,
        plan_v2,
        acknowledgement_capability,
    })
}

pub(crate) fn cleanup_failed_prepare(
    transaction_id: uuid::Uuid,
    transaction_dir: &Path,
    capability: Option<&str>,
) {
    let _ = release_lifecycle_lock(transaction_id);
    release_transaction_claim(transaction_dir);
    if let Some(capability) = capability {
        let _ = std::fs::remove_file(acknowledgement_capability_path(transaction_dir, capability));
    }
    let _ = std::fs::remove_dir_all(transaction_dir);
}
