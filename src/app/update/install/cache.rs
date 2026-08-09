// @author kongweiguang

//! Update-cache recovery and retention.  Only version transaction children are
//! eligible for cleanup; the verified source artifact remains the retry source.

use super::*;
use std::{
    fs::{self, File, OpenOptions},
    io::Read as _,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

pub(crate) const MAX_CACHED_RESULT_BYTES: usize = 64 * 1024;
pub(crate) const MAX_DISPLAYED_RESULT_BYTES: usize = 128;
pub(crate) const HELPER_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) fn restored_startup_state(updates_root: &Path) -> Option<UpdateState> {
    let mut v2_results = Vec::new();
    collect_v2_result_paths(updates_root, &mut v2_results);
    v2_results.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });
    if let Some(result_path) = v2_results.pop()
        && let Ok(bytes) =
            read_bounded_cache_file(&result_path, MAX_CACHED_RESULT_BYTES, "v2 update result")
        && let Ok(result) = gmark_update_core::parse_apply_result_v2(&bytes)
        && result_matches_transaction_directory(&result, &result_path)
    {
        let fingerprint = result_fingerprint(&bytes);
        let displayed_path = result_path.with_file_name("result-displayed");
        let displayed = read_bounded_cache_file(
            &displayed_path,
            MAX_DISPLAYED_RESULT_BYTES,
            "displayed v2 update result",
        )
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok());
        if displayed.as_deref() != Some(fingerprint.as_str()) {
            write_display_fingerprint(&displayed_path, &fingerprint);
            return Some(state_from_v2_result(&result));
        }
    }

    let result_path = updates_root.join("last-result.json");
    let bytes =
        read_bounded_cache_file(&result_path, MAX_CACHED_RESULT_BYTES, "update result").ok()?;
    let fingerprint = result_fingerprint(&bytes);
    let displayed_path = updates_root.join("last-result-displayed");
    let displayed = read_bounded_cache_file(
        &displayed_path,
        MAX_DISPLAYED_RESULT_BYTES,
        "displayed update result",
    )
    .ok()
    .and_then(|bytes| String::from_utf8(bytes).ok());
    if displayed.as_deref() == Some(fingerprint.as_str()) {
        return None;
    }
    let result = parse_apply_result(&bytes).ok()?;
    write_display_fingerprint(&displayed_path, &fingerprint);
    Some(if result.status == "succeeded" {
        UpdateState::Succeeded {
            version: result.to_version,
            message: result.message,
        }
    } else {
        UpdateState::Failed {
            release: None,
            message: result.message,
            retryable: false,
        }
    })
}

fn result_fingerprint(bytes: &[u8]) -> String {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(bytes);
    format!("{:08x}\n", hasher.finalize())
}

fn write_display_fingerprint(path: &Path, fingerprint: &str) {
    let existing = match fs::symlink_metadata(path) {
        Ok(metadata) if is_real_regular_file(&metadata) => true,
        Ok(_) => return,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => return,
    };
    if existing {
        let _ = fs::write(path, fingerprint);
        return;
    }
    if let Ok(mut file) = OpenOptions::new().write(true).create_new(true).open(path) {
        let _ = std::io::Write::write_all(&mut file, fingerprint.as_bytes())
            .and_then(|()| file.sync_all());
    }
}

fn collect_v2_result_paths(root: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let version_dir = entry.path();
        let Ok(version_metadata) = fs::symlink_metadata(&version_dir) else {
            continue;
        };
        let Some(version_name) = version_dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_real_directory(&version_metadata)
            || !version_name.starts_with('v')
            || semver::Version::parse(version_name.trim_start_matches('v')).is_err()
        {
            continue;
        }
        let transactions = version_dir.join(gmark_update_core::ApplyPlanV2::TRANSACTIONS_DIR_NAME);
        let Ok(transactions_metadata) = fs::symlink_metadata(&transactions) else {
            continue;
        };
        if !is_real_directory(&transactions_metadata) {
            continue;
        }
        let Ok(attempts) = fs::read_dir(transactions) else {
            continue;
        };
        for attempt in attempts.flatten() {
            let transaction_dir = attempt.path();
            let Ok(metadata) = fs::symlink_metadata(&transaction_dir) else {
                continue;
            };
            if !is_real_directory(&metadata) {
                continue;
            }
            let result = transaction_dir.join(gmark_update_core::ApplyPlanV2::RESULT_FILE_NAME);
            if fs::symlink_metadata(&result)
                .ok()
                .is_some_and(|metadata| is_real_regular_file(&metadata))
            {
                paths.push(result);
            }
        }
    }
}

fn result_matches_transaction_directory(
    result: &gmark_update_core::ApplyResultV2,
    result_path: &Path,
) -> bool {
    result_path
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == result.transaction_id.hyphenated().to_string())
}

pub(crate) fn state_from_v2_result(result: &gmark_update_core::ApplyResultV2) -> UpdateState {
    if result.status == "succeeded" {
        return UpdateState::Succeeded {
            version: result.to_version.clone(),
            message: result.message.clone(),
        };
    }
    let retryable = result.recovery_action.is_some_and(|action| {
        matches!(
            action,
            gmark_update_core::RecoveryAction::ReattemptInstall
                | gmark_update_core::RecoveryAction::Redownload
                | gmark_update_core::RecoveryAction::Recheck
        )
    });
    UpdateState::Failed {
        release: None,
        message: format_v2_failure(result),
        retryable,
    }
}

pub(crate) fn read_v2_result(
    plan: &gmark_update_core::ApplyPlanV2,
) -> Option<gmark_update_core::ApplyResultV2> {
    let result = gmark_update_core::read_apply_result_v2(&plan.result_path).ok()?;
    (result.transaction_id == plan.transaction_id).then_some(result)
}

pub(crate) fn read_v2_progress(
    plan: &gmark_update_core::ApplyPlanV2,
) -> Option<gmark_update_core::ApplyProgressV1> {
    let progress = gmark_update_core::read_apply_progress_v1(&plan.progress_path).ok()?;
    (progress.transaction_id == plan.transaction_id).then_some(progress)
}

pub(crate) fn format_v2_failure(result: &gmark_update_core::ApplyResultV2) -> String {
    result
        .failure_code
        .map(|code| format!("{code:?}: {}", result.message))
        .unwrap_or_else(|| result.message.clone())
}

pub(crate) fn read_bounded_cache_file(
    path: &Path,
    max_bytes: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {label}: {error}"))?;
    if !is_real_regular_file(&metadata) {
        return Err(format!("{label} is not a regular non-link file"));
    }
    let mut file = File::open(path).map_err(|error| format!("failed to open {label}: {error}"))?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read {label}: {error}"))?;
    if bytes.is_empty() || bytes.len() > max_bytes {
        return Err(format!("{label} exceeds its size limit"));
    }
    Ok(bytes)
}

pub(crate) fn cleanup_update_cache(updates_root: &Path) {
    const RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
    let now = SystemTime::now();
    let Ok(entries) = fs::read_dir(updates_root) else {
        return;
    };
    for entry in entries.flatten() {
        let version_dir = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&version_dir) else {
            continue;
        };
        let Some(name) = version_dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_real_directory(&metadata)
            || !name.starts_with('v')
            || semver::Version::parse(name.trim_start_matches('v')).is_err()
        {
            continue;
        }
        cleanup_transaction_children(&version_dir, now, RETENTION);
    }
}

pub(crate) fn discard_verified_source(updates_root: &Path, version: &str) -> Result<(), String> {
    semver::Version::parse(version)
        .map_err(|error| format!("cached update version is invalid SemVer: {error}"))?;
    let version_dir = updates_root.join(format!("v{version}"));
    let metadata = match fs::symlink_metadata(&version_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("failed to inspect cached update version: {error}")),
    };
    if !is_real_directory(&metadata) {
        return Err("cached update version is not a real directory".to_owned());
    }
    for name in ["artifact.ready", "manifest.envelope.json"] {
        let path = version_dir.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if is_real_regular_file(&metadata) => {
                fs::remove_file(path)
                    .map_err(|error| format!("failed to remove cached update source: {error}"))?;
            }
            Ok(_) => return Err("cached update source is not a regular file".to_owned()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!("failed to inspect cached update source: {error}"));
            }
        }
    }
    Ok(())
}

fn cleanup_transaction_children(version_dir: &Path, now: SystemTime, retention: Duration) {
    let transactions = version_dir.join(gmark_update_core::ApplyPlanV2::TRANSACTIONS_DIR_NAME);
    let Ok(transactions_metadata) = fs::symlink_metadata(&transactions) else {
        return;
    };
    if !is_real_directory(&transactions_metadata) {
        return;
    }
    let Ok(entries) = fs::read_dir(&transactions) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !is_real_directory(&metadata)
            || metadata
                .modified()
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_none_or(|age| age < retention)
            || transaction_is_active(&path)
        {
            continue;
        }
        if read_terminal_result(&path).is_some() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn transaction_is_active(transaction_dir: &Path) -> bool {
    let lock_path = transaction_dir.join(gmark_update_core::ApplyPlanV2::LIFETIME_LOCK_FILE_NAME);
    let metadata = match fs::symlink_metadata(&lock_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(_) => return true,
    };
    if !is_real_regular_file(&metadata) {
        return true;
    }
    let Ok(file) = OpenOptions::new().read(true).write(true).open(lock_path) else {
        return true;
    };
    match file.try_lock() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(std::fs::TryLockError::WouldBlock) => true,
        Err(std::fs::TryLockError::Error(_)) => true,
    }
}

fn read_terminal_result(transaction_dir: &Path) -> Option<gmark_update_core::ApplyResultV2> {
    let path = transaction_dir.join(gmark_update_core::ApplyPlanV2::RESULT_FILE_NAME);
    let bytes = read_bounded_cache_file(&path, MAX_CACHED_RESULT_BYTES, "v2 update result").ok()?;
    let result = gmark_update_core::parse_apply_result_v2(&bytes).ok()?;
    result_matches_transaction_directory(&result, &path)
        .then_some(result)
        .filter(|result| matches!(result.status.as_str(), "succeeded" | "failed"))
}

pub(crate) fn helper_timeout_expired(started_at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(started_at) >= HELPER_TIMEOUT
}
