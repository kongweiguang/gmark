// @author kongweiguang

//! Update acknowledgement validation and atomic persistence.

use super::*;

/// Writes only the acknowledgement tied to the helper's active update transaction.
/// New helpers supply a random capability, which must validate without fallback.
/// Its absence is accepted only for the immediately preceding helper protocol,
/// after the same fixed transaction and apply-plan checks have succeeded.
pub(crate) fn write_update_acknowledgement(
    requested_path: &Path,
    updates_root: &Path,
    capability: Option<&str>,
    current_version: &str,
) -> Result<(), String> {
    write_update_acknowledgement_inner(
        requested_path,
        updates_root,
        capability,
        current_version,
        None,
    )
}

#[cfg(test)]
// 原因：测试夹具需要复用生产校验流程并显式绑定安装目标，避免为测试引入旁路协议。
pub(crate) fn write_update_acknowledgement_for_target(
    requested_path: &Path,
    updates_root: &Path,
    capability: Option<&str>,
    current_version: &str,
    current_target_path: &Path,
    current_install_root: &Path,
) -> Result<(), String> {
    write_update_acknowledgement_inner(
        requested_path,
        updates_root,
        capability,
        current_version,
        Some((current_target_path, current_install_root)),
    )
}

// 原因：只有在事务、计划和能力都通过校验后才允许写入启动确认，避免确认文件脱离活动更新。
fn write_update_acknowledgement_inner(
    requested_path: &Path,
    updates_root: &Path,
    capability: Option<&str>,
    current_version: &str,
    expected_target: Option<(&Path, &Path)>,
) -> Result<(), String> {
    let capability = capability
        .map(|capability| {
            Uuid::parse_str(capability)
                .map_err(|_| {
                    "update acknowledgement has an invalid transaction capability".to_owned()
                })
                .and_then(|capability| {
                    if capability.is_nil() {
                        Err("update acknowledgement capability must be non-nil".to_owned())
                    } else {
                        Ok(capability.hyphenated().to_string())
                    }
                })
        })
        .transpose()?;
    let transaction = acknowledgement_transaction_dir(requested_path, updates_root)?;
    if transaction.external && capability.is_none() {
        return Err("external update acknowledgement requires a v2 capability".to_owned());
    }
    let plan = validate_active_acknowledgement_plan(
        &transaction.transaction_dir,
        &transaction.plan_path,
        current_version,
        transaction.external,
        expected_target,
    )?;
    if transaction.external && !matches!(plan, ActiveAcknowledgementPlan::V2 { .. }) {
        return Err(
            "update acknowledgement outside the active cache root requires protocol v2".to_owned(),
        );
    }
    if matches!(plan, ActiveAcknowledgementPlan::V2 { .. }) && capability.is_none() {
        return Err("update protocol v2 requires an acknowledgement capability".to_owned());
    }
    if let Some(capability) = capability.as_deref() {
        validate_acknowledgement_capability(
            &transaction.transaction_dir,
            capability,
            plan.transaction_id(),
        )?;
    }
    write_acknowledgement_exclusive(&transaction.transaction_dir, current_version)
}

struct AcknowledgementTransaction {
    transaction_dir: PathBuf,
    plan_path: PathBuf,
    external: bool,
}

// 原因：逐段检查确认路径的链接和重解析点，阻止路径校验被文件系统别名绕过。
fn validate_acknowledgement_path_components(path: &Path) -> Result<(), String> {
    if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )
    }) {
        return Err("update acknowledgement path must be normalized".to_owned());
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(format!(
                    "failed to inspect update acknowledgement path '{}': {error}",
                    current.display()
                ));
            }
        };
        if metadata.file_type().is_symlink() {
            return Err("update acknowledgement path contains a symbolic link".to_owned());
        }
        if is_reparse_metadata(&metadata) {
            return Err("update acknowledgement path contains a reparse point".to_owned());
        }
    }
    Ok(())
}

// 原因：把请求路径绑定到活动更新目录，确保 helper 不能向任意位置写入确认文件。
fn acknowledgement_transaction_dir(
    requested_path: &Path,
    updates_root: &Path,
) -> Result<AcknowledgementTransaction, String> {
    if !requested_path.is_absolute() {
        return Err("update acknowledgement path must be absolute".to_owned());
    }
    if requested_path.file_name().and_then(|name| name.to_str()) != Some(ACKNOWLEDGEMENT_FILE_NAME)
    {
        return Err("update acknowledgement path has an invalid file name".to_owned());
    }
    validate_acknowledgement_path_components(requested_path)?;
    let requested_parent = requested_path
        .parent()
        .ok_or_else(|| "update acknowledgement path has no transaction directory".to_owned())?;
    let canonical_root = fs::canonicalize(updates_root)
        .map_err(|error| format!("failed to resolve update cache root: {error}"))?;
    let root_metadata = fs::symlink_metadata(&canonical_root)
        .map_err(|error| format!("failed to inspect update cache root: {error}"))?;
    if !root_metadata.file_type().is_dir()
        || root_metadata.file_type().is_symlink()
        || is_reparse_metadata(&root_metadata)
    {
        return Err("update cache root is not a real directory".to_owned());
    }
    let transaction_dir = fs::canonicalize(requested_parent).map_err(|error| {
        format!("failed to resolve update acknowledgement transaction: {error}")
    })?;
    let transaction_metadata = fs::symlink_metadata(&transaction_dir).map_err(|error| {
        format!("failed to inspect update acknowledgement transaction: {error}")
    })?;
    if !transaction_metadata.file_type().is_dir()
        || transaction_metadata.file_type().is_symlink()
        || is_reparse_metadata(&transaction_metadata)
    {
        return Err("update acknowledgement transaction is not a real directory".to_owned());
    }
    let in_root = transaction_dir.starts_with(&canonical_root);
    let version_dir = if in_root && transaction_dir.parent() == Some(canonical_root.as_path()) {
        transaction_dir.as_path()
    } else {
        let transactions_dir = transaction_dir.parent().ok_or_else(|| {
            "update acknowledgement transaction has no transactions root".to_owned()
        })?;
        let version_dir = transactions_dir
            .parent()
            .ok_or_else(|| "update acknowledgement transaction has no version root".to_owned())?;
        let is_v2_layout = transactions_dir.file_name().and_then(|name| name.to_str())
            == Some(gmark_update_core::ApplyPlanV2::TRANSACTIONS_DIR_NAME)
            && transaction_dir
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| Uuid::parse_str(name).ok())
                .is_some_and(|transaction_id| !transaction_id.is_nil());
        if !is_v2_layout || (in_root && version_dir.parent() != Some(canonical_root.as_path())) {
            return Err(
                "update acknowledgement is outside the active update cache root".to_owned(),
            );
        }
        version_dir
    };
    let version = version_dir
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix('v'))
        .ok_or_else(|| "update acknowledgement transaction has an invalid name".to_owned())?;
    semver::Version::parse(version)
        .map_err(|_| "update acknowledgement transaction has an invalid version".to_owned())?;
    Ok(AcknowledgementTransaction {
        plan_path: requested_parent.join(gmark_update_core::ApplyPlanV2::PLAN_FILE_NAME),
        external: !in_root,
        transaction_dir,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveAcknowledgementPlan {
    V1,
    V2 { transaction_id: Uuid },
}

impl ActiveAcknowledgementPlan {
    // 原因：把协议事务标识集中提供给能力校验，避免调用方重复处理 v1/v2 分支。
    fn transaction_id(self) -> Option<Uuid> {
        match self {
            Self::V1 => None,
            Self::V2 { transaction_id } => Some(transaction_id),
        }
    }
}

// 原因：统一校验 v1/v2 计划与当前事务的绑定关系，保持两种协议共享同一写入门槛。
fn validate_active_acknowledgement_plan(
    transaction_dir: &Path,
    plan_path: &Path,
    current_version: &str,
    external: bool,
    expected_target: Option<(&Path, &Path)>,
) -> Result<ActiveAcknowledgementPlan, String> {
    let plan_metadata = fs::symlink_metadata(plan_path)
        .map_err(|error| format!("failed to inspect update acknowledgement plan: {error}"))?;
    if !plan_metadata.file_type().is_file()
        || plan_metadata.file_type().is_symlink()
        || is_reparse_metadata(&plan_metadata)
    {
        return Err("update acknowledgement plan is not a regular file".to_owned());
    }
    if plan_metadata.len() > gmark_update_core::MAX_APPLY_PLAN_BYTES {
        return Err("update acknowledgement plan exceeds its size limit".to_owned());
    }
    let plan_bytes = fs::read(plan_path)
        .map_err(|error| format!("failed to inspect update acknowledgement plan: {error}"))?;
    let schema_version = serde_json::from_slice::<serde_json::Value>(&plan_bytes)
        .ok()
        .and_then(|value| {
            value
                .get("schema_version")
                .and_then(serde_json::Value::as_u64)
        });
    if schema_version == Some(u64::from(gmark_update_core::ApplyPlanV2::SCHEMA_VERSION)) {
        let plan = gmark_update_core::read_apply_plan_v2(plan_path)
            .map_err(|error| format!("failed to read update acknowledgement plan v2: {error}"))?;
        let declared_plan_path = plan
            .transaction_dir()
            .ok_or_else(|| {
                "update acknowledgement plan v2 has no transaction directory".to_owned()
            })?
            .join(gmark_update_core::ApplyPlanV2::PLAN_FILE_NAME);
        if external {
            validate_external_v2_plan(&plan, plan_path, transaction_dir, current_version)?;
        } else {
            gmark_update_core::validate_apply_plan_v2_at_path(
                &plan,
                plan_path,
                &gmark_update_core::Platform::current(),
            )
            .map_err(|error| {
                format!("failed to validate update acknowledgement plan v2: {error}")
            })?;
        }
        // Windows canonicalization adds a verbatim path prefix. Validate the
        // plan's lexical fixed layout first, then bind that exact file back to
        // the canonical transaction opened above instead of comparing unlike
        // path spellings.
        let canonical_declared_plan = fs::canonicalize(&declared_plan_path).map_err(|error| {
            format!("failed to resolve declared update acknowledgement plan v2: {error}")
        })?;
        if canonical_declared_plan
            != fs::canonicalize(plan_path).map_err(|error| {
                format!("failed to resolve update acknowledgement plan v2: {error}")
            })?
        {
            return Err(
                "update acknowledgement plan v2 does not resolve to the active transaction"
                    .to_owned(),
            );
        }
        let plan_transaction = plan
            .transaction_dir()
            .and_then(|path| fs::canonicalize(path).ok());
        if plan.target_version != current_version
            || plan_transaction.as_deref() != Some(transaction_dir)
            || !plan_path_in_transaction(
                &plan.acknowledgement_path,
                transaction_dir,
                ACKNOWLEDGEMENT_FILE_NAME,
            )
        {
            return Err(
                "update acknowledgement is not bound to the active v2 transaction".to_owned(),
            );
        }
        if external {
            validate_external_v2_install_binding(&plan, expected_target)?;
        }
        return Ok(ActiveAcknowledgementPlan::V2 {
            transaction_id: plan.transaction_id,
        });
    }

    let plan = gmark_update_core::read_apply_plan(plan_path)
        .map_err(|error| format!("failed to read update acknowledgement plan: {error}"))?;
    if plan.target_version != current_version
        || !plan_path_in_transaction(&plan.artifact_path, transaction_dir, "artifact.ready")
        || !plan_path_in_transaction(
            &plan.signed_envelope_path,
            transaction_dir,
            "manifest.envelope.json",
        )
        || !plan_path_in_transaction(
            &plan.acknowledgement_path,
            transaction_dir,
            ACKNOWLEDGEMENT_FILE_NAME,
        )
        || !plan_path_in_transaction(&plan.cancellation_path, transaction_dir, "cancel-install")
    {
        return Err("update acknowledgement is not bound to the active transaction".to_owned());
    }
    Ok(ActiveAcknowledgementPlan::V1)
}

// 原因：外部更新目录不在默认缓存根内时，必须把计划绑定到当前运行安装，避免接受跨安装确认。
fn validate_external_v2_install_binding(
    plan: &gmark_update_core::ApplyPlanV2,
    expected_target: Option<(&Path, &Path)>,
) -> Result<(), String> {
    if let Some((target_path, install_root)) = expected_target {
        return validate_external_v2_install_binding_against(plan, target_path, install_root);
    }
    let current = crate::updater::resolve_current_update_target()
        .map_err(|error| format!("failed to resolve current update target: {error}"))?;
    validate_external_v2_install_binding_against(
        plan,
        &current.target_path,
        &current.expected_install_root,
    )
}

// 原因：测试和运行时都使用真实路径比较安装目标，避免不同路径拼写造成错误放行。
pub(crate) fn validate_external_v2_install_binding_against(
    plan: &gmark_update_core::ApplyPlanV2,
    current_target_path: &Path,
    current_install_root: &Path,
) -> Result<(), String> {
    let expected_install_root =
        canonical_real_path(current_install_root, "current installation root")?;
    let plan_install_root =
        canonical_real_path(&plan.expected_install_root, "update plan installation root")?;
    if plan_install_root != expected_install_root {
        return Err(
            "update acknowledgement plan is not bound to the current installation root".to_owned(),
        );
    }

    let current_target = canonical_real_path(current_target_path, "current update target")?;
    let plan_target = canonical_real_path(&plan.target_path, "update plan target")?;
    if plan_target != current_target {
        return Err(
            "update acknowledgement plan is not bound to the current running program".to_owned(),
        );
    }

    let current_executable = std::env::current_exe()
        .map_err(|error| format!("failed to locate current running program: {error}"))?;
    let expected_relaunch = if cfg!(target_os = "macos") {
        current_executable
    } else {
        current_target_path.to_path_buf()
    };
    let plan_relaunch = canonical_real_path(&plan.relaunch_path, "update plan relaunch path")?;
    let expected_relaunch = canonical_real_path(&expected_relaunch, "current relaunch path")?;
    if plan_relaunch != expected_relaunch {
        return Err(
            "update acknowledgement plan is not bound to the current relaunch program".to_owned(),
        );
    }
    Ok(())
}

// 原因：外部 v2 计划需要完整检查固定布局和安装路径，防止目录外的伪造计划进入确认流程。
fn validate_external_v2_plan(
    plan: &gmark_update_core::ApplyPlanV2,
    plan_path: &Path,
    transaction_dir: &Path,
    current_version: &str,
) -> Result<(), String> {
    if plan.schema_version != gmark_update_core::ApplyPlanV2::SCHEMA_VERSION {
        return Err("update acknowledgement plan v2 has an unsupported schema".to_owned());
    }
    if plan.transaction_id.is_nil() {
        return Err("update acknowledgement plan v2 has an invalid transaction id".to_owned());
    }
    for (label, path) in [
        ("artifact", &plan.artifact_path),
        ("signed envelope", &plan.signed_envelope_path),
        ("expected install root", &plan.expected_install_root),
        ("target", &plan.target_path),
        ("backup", &plan.backup_path),
        ("relaunch", &plan.relaunch_path),
        ("acknowledgement", &plan.acknowledgement_path),
        ("cancellation", &plan.cancellation_path),
        ("result", &plan.result_path),
        ("helper log", &plan.helper_log_path),
        ("lifetime lock", &plan.lifetime_lock_path),
        ("progress", &plan.progress_path),
        ("installer log", &plan.installer_log_path),
    ] {
        validate_clean_plan_path(path, label)?;
        validate_no_link_components(path, label)?;
    }
    let transaction_from_plan = plan
        .transaction_dir()
        .ok_or_else(|| "update acknowledgement plan v2 has no transaction directory".to_owned())?;
    let expected_plan_path = transaction_dir.join(gmark_update_core::ApplyPlanV2::PLAN_FILE_NAME);
    let plan_transaction_canonical = fs::canonicalize(transaction_from_plan).map_err(|error| {
        format!("failed to resolve update acknowledgement transaction: {error}")
    })?;
    let plan_path_canonical = fs::canonicalize(plan_path)
        .map_err(|error| format!("failed to resolve update acknowledgement plan: {error}"))?;
    let expected_plan_path_canonical = fs::canonicalize(&expected_plan_path)
        .map_err(|error| format!("failed to resolve update acknowledgement plan: {error}"))?;
    if plan_transaction_canonical != transaction_dir
        || plan_path_canonical != expected_plan_path_canonical
    {
        return Err(
            "update acknowledgement plan v2 is outside the requested transaction".to_owned(),
        );
    }
    let expected_transaction_name = plan.transaction_id.hyphenated().to_string();
    let transactions_dir = transaction_dir
        .parent()
        .ok_or_else(|| "update acknowledgement transaction has no transactions root".to_owned())?;
    let version_dir = transactions_dir
        .parent()
        .ok_or_else(|| "update acknowledgement transaction has no version root".to_owned())?;
    if transaction_dir.file_name().and_then(|name| name.to_str())
        != Some(expected_transaction_name.as_str())
        || transactions_dir.file_name().and_then(|name| name.to_str())
            != Some(gmark_update_core::ApplyPlanV2::TRANSACTIONS_DIR_NAME)
        || version_dir.file_name().and_then(|name| name.to_str())
            != Some(format!("v{}", plan.target_version).as_str())
    {
        return Err("update acknowledgement plan v2 has an invalid transaction layout".to_owned());
    }
    let current = semver::Version::parse(&plan.current_version).map_err(|error| {
        format!("update acknowledgement plan v2 current version is invalid: {error}")
    })?;
    let target = semver::Version::parse(&plan.target_version).map_err(|error| {
        format!("update acknowledgement plan v2 target version is invalid: {error}")
    })?;
    if target <= current || plan.target_version != current_version {
        return Err(
            "update acknowledgement plan v2 target version does not match current".to_owned(),
        );
    }
    if plan.artifact_size == 0 || plan.artifact_size > gmark_update_core::MAX_ARTIFACT_BYTES {
        return Err("update acknowledgement plan v2 has invalid artifact bounds".to_owned());
    }
    if plan.artifact_sha256.len() != 64
        || !plan
            .artifact_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("update acknowledgement plan v2 has an invalid artifact digest".to_owned());
    }
    let artifact_url = url::Url::parse(&plan.artifact_url).map_err(|error| {
        format!("update acknowledgement plan v2 artifact URL is invalid: {error}")
    })?;
    let official_artifact_url = artifact_url.scheme() == "https"
        && artifact_url.host_str() == Some("github.com")
        && artifact_url
            .path()
            .starts_with("/kongweiguang/gmark/releases/download/")
        && artifact_url.username().is_empty()
        && artifact_url.password().is_none();
    #[cfg(feature = "updater-e2e")]
    let loopback_artifact_url = artifact_url.username().is_empty()
        && artifact_url.password().is_none()
        && artifact_url.fragment().is_none()
        && matches!(artifact_url.scheme(), "http" | "https")
        && artifact_url.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        });
    #[cfg(not(feature = "updater-e2e"))]
    let loopback_artifact_url = false;
    if !official_artifact_url && !loopback_artifact_url {
        return Err("update acknowledgement plan v2 artifact URL is not official".to_owned());
    }
    let expected_format = if cfg!(target_os = "windows") {
        "windows-setup-exe"
    } else if cfg!(target_os = "macos") {
        "macos-app-tar-gz"
    } else if cfg!(target_os = "linux") {
        "linux-app-image"
    } else {
        return Err("this platform cannot apply gmark updates".to_owned());
    };
    if plan.artifact_format != expected_format {
        return Err(
            "update acknowledgement plan v2 artifact format does not match platform".to_owned(),
        );
    }
    let fixed_paths = [
        (
            &plan.artifact_path,
            gmark_update_core::ApplyPlanV2::ARTIFACT_FILE_NAME,
        ),
        (
            &plan.signed_envelope_path,
            gmark_update_core::ApplyPlanV2::SIGNED_ENVELOPE_FILE_NAME,
        ),
        (
            &plan.acknowledgement_path,
            gmark_update_core::ApplyPlanV2::ACKNOWLEDGEMENT_FILE_NAME,
        ),
        (
            &plan.cancellation_path,
            gmark_update_core::ApplyPlanV2::CANCELLATION_FILE_NAME,
        ),
        (
            &plan.result_path,
            gmark_update_core::ApplyPlanV2::RESULT_FILE_NAME,
        ),
        (
            &plan.helper_log_path,
            gmark_update_core::ApplyPlanV2::HELPER_LOG_FILE_NAME,
        ),
        (
            &plan.lifetime_lock_path,
            gmark_update_core::ApplyPlanV2::LIFETIME_LOCK_FILE_NAME,
        ),
        (
            &plan.progress_path,
            gmark_update_core::ApplyPlanV2::PROGRESS_FILE_NAME,
        ),
        (
            &plan.installer_log_path,
            gmark_update_core::ApplyPlanV2::INSTALLER_LOG_FILE_NAME,
        ),
    ];
    if fixed_paths.iter().any(|(path, name)| {
        path.file_name().and_then(|value| value.to_str()) != Some(*name)
            || path.parent() != Some(transaction_from_plan)
    }) {
        return Err("update acknowledgement plan v2 paths do not match fixed layout".to_owned());
    }
    if [
        &plan.expected_install_root,
        &plan.target_path,
        &plan.backup_path,
        &plan.relaunch_path,
    ]
    .iter()
    .any(|path| path.starts_with(transaction_from_plan))
    {
        return Err(
            "update acknowledgement plan v2 install paths escape the transaction".to_owned(),
        );
    }
    let target_bound = plan.target_path == plan.expected_install_root
        || plan.target_path.starts_with(&plan.expected_install_root);
    let relaunch_bound = plan.relaunch_path == plan.expected_install_root
        || plan.relaunch_path.starts_with(&plan.expected_install_root);
    let backup_parent_matches = plan.backup_path.parent() == plan.expected_install_root.parent();
    let backup_owned = plan
        .backup_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains(&expected_transaction_name));
    if !target_bound || !relaunch_bound || !backup_parent_matches || !backup_owned {
        return Err("update acknowledgement plan v2 install paths are not bound".to_owned());
    }
    Ok(())
}

// 原因：计划路径必须先通过词法规范化检查，避免父目录跳转改变后续绑定语义。
fn validate_clean_plan_path(path: &Path, label: &str) -> Result<(), String> {
    if !path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        return Err(format!(
            "update acknowledgement plan {label} path is not normalized"
        ));
    }
    Ok(())
}

// 原因：校验计划路径的每个组件，防止链接或重解析点把安全检查引向另一棵目录树。
fn validate_no_link_components(path: &Path, label: &str) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        #[cfg(windows)]
        if matches!(component, Component::Prefix(_)) {
            current.push(component.as_os_str());
            continue;
        }
        current.push(component.as_os_str());
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(format!(
                    "failed to inspect update plan {label} path: {error}"
                ));
            }
        };
        if metadata.file_type().is_symlink() || is_reparse_metadata(&metadata) {
            return Err(format!(
                "update plan {label} path contains a symlink or reparse point"
            ));
        }
    }
    Ok(())
}

// 原因：在比较关键安装路径前拒绝链接并解析真实位置，保证绑定判断针对实际文件系统对象。
fn canonical_real_path(path: &Path, label: &str) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {label}: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("{label} is a symbolic link"));
    }
    if is_reparse_metadata(&metadata) {
        return Err(format!("{label} is a reparse point"));
    }
    fs::canonicalize(path).map_err(|error| format!("failed to resolve {label}: {error}"))
}

#[cfg(windows)]
fn is_reparse_metadata(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_metadata(_metadata: &fs::Metadata) -> bool {
    false
}

// 原因：确认计划文件的叶子名和事务父目录一致，防止同名文件跨事务冒用。
fn plan_path_in_transaction(path: &Path, transaction_dir: &Path, expected_name: &str) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(expected_name)
        && path
            .parent()
            .and_then(|parent| fs::canonicalize(parent).ok())
            .as_deref()
            == Some(transaction_dir)
}

// 原因：能力文件内容必须匹配当前事务，避免持有任意 UUID 就能触发确认。
fn validate_acknowledgement_capability(
    transaction_dir: &Path,
    capability: &str,
    transaction_id: Option<Uuid>,
) -> Result<(), String> {
    let path = transaction_dir.join(format!("{ACK_CAPABILITY_FILE_PREFIX}{capability}"));
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("failed to inspect update acknowledgement capability: {error}"))?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || is_reparse_metadata(&metadata)
    {
        return Err("update acknowledgement capability is not a regular file".to_owned());
    }
    let mut file = File::open(&path)
        .map_err(|error| format!("failed to read update acknowledgement capability: {error}"))?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(MAX_ACK_CAPABILITY_BYTES.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read update acknowledgement capability: {error}"))?;
    let expected = transaction_id.map_or_else(
        || format!("{capability}\n"),
        |transaction_id| format!("{}:{capability}\n", transaction_id.hyphenated()),
    );
    if bytes != expected.as_bytes() {
        let transaction = transaction_id
            .map(|value| value.hyphenated().to_string())
            .unwrap_or_else(|| "legacy".to_owned());
        return Err(format!(
            "update acknowledgement capability did not match transaction {transaction}"
        ));
    }
    Ok(())
}

// 原因：通过临时文件和无覆盖提交写入确认，避免并发或符号链接覆盖既有结果。
fn write_acknowledgement_exclusive(
    transaction_dir: &Path,
    current_version: &str,
) -> Result<(), String> {
    let acknowledgement_path = transaction_dir.join(ACKNOWLEDGEMENT_FILE_NAME);
    match fs::symlink_metadata(&acknowledgement_path) {
        Ok(metadata) if metadata.file_type().is_symlink() || is_reparse_metadata(&metadata) => {
            return Err("update acknowledgement target is a symbolic link".to_owned());
        }
        Ok(_) => return Err("update acknowledgement already exists".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to inspect update acknowledgement target: {error}"
            ));
        }
    }
    let mut temporary = NamedTempFile::new_in(transaction_dir)
        .map_err(|error| format!("failed to create update acknowledgement: {error}"))?;
    temporary
        .write_all(
            gmark_update_core::StartupAcknowledgementV1::for_target_version(current_version)
                .marker_bytes()
                .as_slice(),
        )
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("failed to persist update acknowledgement: {error}"))?;
    set_private_acknowledgement_permissions(temporary.as_file())?;
    // persist_noclobber is the atomic no-overwrite commit; an attacker-created
    // final symlink is an existing destination and is never followed or truncated.
    temporary
        .persist_noclobber(&acknowledgement_path)
        .map_err(|error| {
            format!(
                "failed to commit update acknowledgement '{}': {}",
                acknowledgement_path.display(),
                error.error
            )
        })?;
    Ok(())
}

#[cfg(unix)]
// 原因：限制确认文件权限，避免启动确认内容被无关进程读取。
fn set_private_acknowledgement_permissions(file: &File) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;

    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to secure update acknowledgement: {error}"))
}

#[cfg(not(unix))]
// 原因：限制确认文件权限，避免启动确认内容被无关进程读取。
fn set_private_acknowledgement_permissions(_file: &File) -> Result<(), String> {
    Ok(())
}

/// Helper 参数不属于用户 CLI；必须在普通参数解析前消费，避免文件路径或未知参数分支误判。
pub(crate) fn take_update_acknowledgement(args: &mut Vec<String>) -> Option<PathBuf> {
    let index = args
        .iter()
        .position(|argument| argument == "--update-ack")?;
    if index + 1 >= args.len() {
        args.remove(index);
        return None;
    }
    let path = PathBuf::from(args.remove(index + 1));
    args.remove(index);
    Some(path)
}
