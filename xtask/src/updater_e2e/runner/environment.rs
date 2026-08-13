// @author kongweiguang

use super::*;

/// Publishes the V2-only test contract in one place so the application and UI driver receive
/// identical transaction paths without making the legacy discovery manifest a selection input.
pub(super) fn runtime_environment(
    options: &UpdaterE2eOptions,
    paths: &E2ePaths,
    phase: &str,
) -> Vec<(OsString, OsString)> {
    let mut values = vec![
        ("GMARK_E2E_PHASE".into(), phase.into()),
        ("GMARK_E2E_PLATFORM".into(), platform_name().into()),
        (
            "GMARK_E2E_DECISION".into(),
            options.decision.as_str().into(),
        ),
        (
            "GMARK_UI_CHECK_ROOT".into(),
            paths.ui_check_root.clone().into_os_string(),
        ),
        (
            "GMARK_UPDATER_E2E_UPDATE_ROOT".into(),
            paths.updates_root.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_CURRENT_BINARY".into(),
            options
                .current_binary
                .clone()
                .unwrap_or_default()
                .into_os_string(),
        ),
        (
            "GMARK_E2E_NEXT_BINARY".into(),
            options
                .next_binary
                .clone()
                .unwrap_or_default()
                .into_os_string(),
        ),
        (
            "GMARK_E2E_ACK_PATH".into(),
            paths.acknowledgement.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_VERSION_PATH".into(),
            paths.version_marker.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_LIFETIME_LOCK_PATH".into(),
            paths.lifetime_lock.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_HELPER_LOG_PATH".into(),
            paths.helper_log.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_RESULT_PATH".into(),
            paths.result.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_BACKUP_PATH".into(),
            paths.backup.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_HELPER_PID_PATH".into(),
            paths.helper_pid.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_AGENT_PID_PATH".into(),
            paths.agent_pid.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_NEW_PID_PATH".into(),
            paths.new_pid.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_INSTALLER_LOG".into(),
            paths.installer_log.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_OLD_PID_PATH".into(),
            paths.old_pid.clone().into_os_string(),
        ),
        ("GMARK_E2E_V2_ONLY".into(), "1".into()),
        ("GMARK_E2E_LEGACY_MANIFEST_REQUIRED".into(), "1".into()),
    ];
    append_optional_environment(&mut values, options);
    values
}

/// Adds only caller-supplied fixtures so absent optional artifacts cannot become empty trusted
/// paths in a platform driver.
fn append_optional_environment(
    values: &mut Vec<(OsString, OsString)>,
    options: &UpdaterE2eOptions,
) {
    push_optional_text(values, "GMARK_E2E_TARGET_VERSION", &options.target_version);
    if let Some(url) = &options.manifest_url {
        values.push(("GMARK_UPDATER_E2E_MANIFEST_URL".into(), url.clone().into()));
        values.push(("GMARK_E2E_V2_MANIFEST_URL".into(), url.clone().into()));
    }
    for (name, path) in [
        (
            "GMARK_E2E_SIGNING_PRIVATE_KEY",
            &options.signing_private_key,
        ),
        ("GMARK_E2E_SIGNING_PUBLIC_KEY", &options.signing_public_key),
        ("GMARK_E2E_HELPER", &options.helper),
        ("GMARK_E2E_AGENT", &options.agent),
        ("GMARK_E2E_APPLY_PLAN", &options.apply_plan),
        ("GMARK_E2E_CURRENT_INSTALLER", &options.current_installer),
        ("GMARK_E2E_NEXT_INSTALLER", &options.next_installer),
    ] {
        if let Some(path) = path {
            values.push((name.into(), path.clone().into_os_string()));
        }
    }
    push_optional_text(
        values,
        "GMARK_E2E_PUBLIC_KEY_BASE64",
        &options.public_key_base64,
    );
}

/// Inserts a present text value without converting `None` into an ambiguous empty variable.
fn push_optional_text(values: &mut Vec<(OsString, OsString)>, name: &str, value: &Option<String>) {
    if let Some(value) = value {
        values.push((name.into(), value.clone().into()));
    }
}
