// @author kongweiguang

//! Signed update-manifest verification and online version checks.
//!
//! 远端内容在 Ed25519 验证前不参与版本、URL 或 rollout 决策。签名覆盖 payload 的原始
//! JSON bytes，envelope 只负责传输 base64，避免跨语言 JSON canonicalization 分歧。

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, VerifyingKey};
use reqwest::Url;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

pub(crate) const GITHUB_UPDATE_MANIFEST_URL: &str =
    "https://github.com/kongweiguang/gmark/releases/latest/download/update-manifest.json";
pub(crate) const GITEE_UPDATE_MANIFEST_URL: &str =
    "https://raw.giteeusercontent.com/kongweiguang/gmark/raw/release/update-manifest.json";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_ENVELOPE_BYTES: usize = 128 * 1024;
const MAX_PAYLOAD_BYTES: usize = 96 * 1024;
const MAX_INSTALLER_BYTES: u64 = 512 * 1024 * 1024;
const INSTALLER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const STALE_UPDATE_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const UPDATE_ACCEPT: &str = "application/json,*/*;q=0.5";
const UPDATE_USER_AGENT: &str = concat!(
    "gmark/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/kongweiguang/gmark)"
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateSource {
    GitHub,
    Gitee,
}

impl UpdateSource {
    fn url(self) -> &'static str {
        match self {
            Self::GitHub => GITHUB_UPDATE_MANIFEST_URL,
            Self::Gitee => GITEE_UPDATE_MANIFEST_URL,
        }
    }
}

impl fmt::Display for UpdateSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub => f.write_str("GitHub"),
            Self::Gitee => f.write_str("Gitee"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RemoteFetchFailureKind {
    Timeout,
    HttpStatus,
    Network,
    Body,
    TooLarge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteFetchFailure {
    pub(crate) source: UpdateSource,
    pub(crate) kind: RemoteFetchFailureKind,
    detail: String,
}

impl RemoteFetchFailure {
    fn new(source: UpdateSource, kind: RemoteFetchFailureKind, detail: impl Into<String>) -> Self {
        Self {
            source,
            kind,
            detail: detail.into(),
        }
    }

    fn timeout(source: UpdateSource, detail: impl Into<String>) -> Self {
        Self::new(source, RemoteFetchFailureKind::Timeout, detail)
    }

    fn is_timeout(&self) -> bool {
        self.kind == RemoteFetchFailureKind::Timeout
    }
}

impl fmt::Display for RemoteFetchFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} update manifest fetch failed: {}",
            self.source, self.detail
        )
    }
}

impl std::error::Error for RemoteFetchFailure {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UpdateCheckError {
    Fetch(RemoteFetchFailure),
    Configuration(String),
    Envelope(String),
    Signature(String),
    Manifest(String),
    ParseVersion(String),
}

impl fmt::Display for UpdateCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fetch(error) => write!(f, "{error}"),
            Self::Configuration(detail)
            | Self::Envelope(detail)
            | Self::Signature(detail)
            | Self::Manifest(detail)
            | Self::ParseVersion(detail) => f.write_str(detail),
        }
    }
}

impl std::error::Error for UpdateCheckError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UpdateCheckResult {
    UpdateAvailable(UpdateVersionInfo),
    UpToDate(UpdateVersionInfo),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdateVersionInfo {
    pub(crate) current_version: String,
    pub(crate) latest_version: String,
    pub(crate) source: UpdateSource,
    pub(crate) release_url: String,
    pub(crate) artifact_url: String,
    pub(crate) artifact_sha256: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UpdateInstallError {
    Metadata(String),
    Network(String),
    Io(String),
    TooLarge,
    HashMismatch { expected: String, actual: String },
    Launch(String),
}

impl fmt::Display for UpdateInstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Metadata(detail)
            | Self::Network(detail)
            | Self::Io(detail)
            | Self::Launch(detail) => f.write_str(detail),
            Self::TooLarge => write!(
                f,
                "update installer exceeds the {} MiB safety limit",
                MAX_INSTALLER_BYTES / 1024 / 1024
            ),
            Self::HashMismatch { expected, actual } => write!(
                f,
                "downloaded update SHA-256 mismatch (expected {expected}, got {actual})"
            ),
        }
    }
}

impl std::error::Error for UpdateInstallError {}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedEnvelope {
    schema_version: u8,
    algorithm: String,
    payload: String,
    signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateManifest {
    schema_version: u8,
    version: String,
    published_at: String,
    paused: bool,
    rollout_percent: u8,
    release_url: String,
    artifacts: BTreeMap<String, UpdateArtifact>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArtifact {
    url: String,
    sha256: String,
}

pub(crate) fn check_latest_version(
    current_version: &str,
) -> Result<UpdateCheckResult, UpdateCheckError> {
    let installation_id = crate::config::load_or_create_installation_id().map_err(|error| {
        UpdateCheckError::Configuration(format!("failed to load installation id: {error}"))
    })?;
    let key = embedded_verifying_key()?;
    check_latest_version_with(
        current_version,
        installation_id,
        &key,
        fetch_remote_signed_manifest,
    )
}

fn check_latest_version_with<F>(
    current_version: &str,
    installation_id: uuid::Uuid,
    key: &VerifyingKey,
    mut fetch: F,
) -> Result<UpdateCheckResult, UpdateCheckError>
where
    F: FnMut(UpdateSource) -> Result<Vec<u8>, RemoteFetchFailure>,
{
    match fetch(UpdateSource::GitHub) {
        Ok(envelope) => compare_signed_manifest(
            current_version,
            installation_id,
            &envelope,
            UpdateSource::GitHub,
            key,
        ),
        Err(error) if error.is_timeout() => {
            let envelope = fetch(UpdateSource::Gitee).map_err(UpdateCheckError::Fetch)?;
            compare_signed_manifest(
                current_version,
                installation_id,
                &envelope,
                UpdateSource::Gitee,
                key,
            )
        }
        Err(error) => Err(UpdateCheckError::Fetch(error)),
    }
}

fn compare_signed_manifest(
    current_version: &str,
    installation_id: uuid::Uuid,
    envelope: &[u8],
    source: UpdateSource,
    key: &VerifyingKey,
) -> Result<UpdateCheckResult, UpdateCheckError> {
    let current = parse_semver(current_version, "current app version")?;
    let manifest = verify_signed_manifest(envelope, key)?;
    let latest = parse_semver(&manifest.version, "signed update manifest version")?;
    validate_manifest(&manifest)?;
    let artifact_key = current_artifact_key().ok_or_else(|| {
        UpdateCheckError::Manifest("this platform has no update artifact mapping".to_owned())
    })?;
    let artifact = manifest.artifacts.get(artifact_key).ok_or_else(|| {
        UpdateCheckError::Manifest(format!(
            "signed update manifest has no '{artifact_key}' artifact"
        ))
    })?;

    let eligible = !manifest.paused
        && rollout_bucket(installation_id, &manifest.version) < manifest.rollout_percent as u32;
    let (latest_version, available) = if latest > current && eligible {
        (manifest.version, true)
    } else if latest > current {
        // 未命中 rollout 的客户端不泄露尚未开放的版本，也不会错误打开下载页。
        (current_version.to_owned(), false)
    } else {
        (manifest.version, false)
    };
    let info = UpdateVersionInfo {
        current_version: current_version.to_owned(),
        latest_version,
        source,
        release_url: manifest.release_url,
        artifact_url: artifact.url.clone(),
        artifact_sha256: artifact.sha256.to_ascii_lowercase(),
    };
    Ok(if available {
        UpdateCheckResult::UpdateAvailable(info)
    } else {
        UpdateCheckResult::UpToDate(info)
    })
}

/// 下载已签名清单指定的当前平台安装包，校验后用固定参数启动平台安装流程。
///
/// 安装包在独占创建的临时目录中落盘；URL、大小和摘要任一校验失败时都不会执行文件。
pub(crate) fn download_and_launch_update(
    info: &UpdateVersionInfo,
) -> Result<PathBuf, UpdateInstallError> {
    validate_release_url(&info.artifact_url, "update artifact URL")
        .map_err(|error| UpdateInstallError::Metadata(error.to_string()))?;
    if info.artifact_sha256.len() != 64
        || !info
            .artifact_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(UpdateInstallError::Metadata(
            "update artifact has an invalid SHA-256".to_owned(),
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(INSTALLER_REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("too many update download redirects")
            } else if attempt.url().scheme() != "https" {
                attempt.error("update download redirect must use HTTPS")
            } else {
                attempt.follow()
            }
        }))
        .default_headers(update_request_headers())
        .build()
        .map_err(|error| {
            UpdateInstallError::Network(format!("failed to build update HTTP client: {error}"))
        })?;
    let response = client.get(&info.artifact_url).send().map_err(|error| {
        UpdateInstallError::Network(format!("failed to download update installer: {error}"))
    })?;
    if !response.status().is_success() {
        return Err(UpdateInstallError::Network(format!(
            "update installer server returned HTTP {}",
            response.status()
        )));
    }
    if response.url().scheme() != "https" {
        return Err(UpdateInstallError::Network(
            "update installer response did not use HTTPS".to_owned(),
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_INSTALLER_BYTES)
    {
        return Err(UpdateInstallError::TooLarge);
    }

    let update_dir = create_update_directory()?;
    let installer_path = update_dir.join(installer_file_name()?);
    let mut installer = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&installer_path)
        .map_err(|error| {
            UpdateInstallError::Io(format!("failed to create update installer file: {error}"))
        })?;
    if let Err(error) = copy_and_verify(
        response,
        &mut installer,
        MAX_INSTALLER_BYTES,
        &info.artifact_sha256,
    ) {
        let _ = fs::remove_file(&installer_path);
        let _ = fs::remove_dir(&update_dir);
        return Err(error);
    }
    if let Err(error) = installer.flush().and_then(|()| installer.sync_all()) {
        drop(installer);
        let _ = fs::remove_file(&installer_path);
        let _ = fs::remove_dir(&update_dir);
        return Err(UpdateInstallError::Io(format!(
            "failed to durably write update installer: {error}"
        )));
    }
    drop(installer);
    if let Err(error) = launch_installer(&installer_path) {
        let _ = fs::remove_file(&installer_path);
        let _ = fs::remove_dir(&update_dir);
        return Err(error);
    }
    Ok(installer_path)
}

fn create_update_directory() -> Result<PathBuf, UpdateInstallError> {
    let root = std::env::temp_dir();
    cleanup_stale_update_directories(&root);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| UpdateInstallError::Io(format!("system clock error: {error}")))?
        .as_nanos();
    for attempt in 0..32_u32 {
        let path = root.join(format!(
            "gmark-update-{}-{nonce}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(UpdateInstallError::Io(format!(
                    "failed to create update directory: {error}"
                )));
            }
        }
    }
    Err(UpdateInstallError::Io(
        "failed to allocate a unique update directory".to_owned(),
    ))
}

/// 清理七天前的 gmark updater 暂存目录；只匹配固定前缀，绝不扫描或删除其他临时数据。
fn cleanup_stale_update_directories(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.starts_with("gmark-update-") {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let is_stale = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= STALE_UPDATE_AGE);
        if metadata.is_dir() && is_stale {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn installer_file_name() -> Result<&'static str, UpdateInstallError> {
    match std::env::consts::OS {
        "windows" => Ok("gmark-setup.exe"),
        "macos" => Ok("gmark.dmg"),
        "linux" => Ok("gmark.AppImage"),
        platform => Err(UpdateInstallError::Metadata(format!(
            "platform '{platform}' cannot install gmark updates"
        ))),
    }
}

#[cfg(target_os = "windows")]
fn launch_installer(path: &Path) -> Result<(), UpdateInstallError> {
    Command::new(path).spawn().map(|_| ()).map_err(|error| {
        UpdateInstallError::Launch(format!("failed to start Windows update installer: {error}"))
    })
}

#[cfg(target_os = "macos")]
fn launch_installer(path: &Path) -> Result<(), UpdateInstallError> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            UpdateInstallError::Launch(format!("failed to open update disk image: {error}"))
        })
}

#[cfg(target_os = "linux")]
fn launch_installer(path: &Path) -> Result<(), UpdateInstallError> {
    use std::os::unix::fs::PermissionsExt as _;

    let mut permissions = fs::metadata(path)
        .map_err(|error| UpdateInstallError::Io(format!("failed to inspect AppImage: {error}")))?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).map_err(|error| {
        UpdateInstallError::Io(format!("failed to make AppImage executable: {error}"))
    })?;
    Command::new(path).spawn().map(|_| ()).map_err(|error| {
        UpdateInstallError::Launch(format!("failed to start update AppImage: {error}"))
    })
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn launch_installer(_path: &Path) -> Result<(), UpdateInstallError> {
    Err(UpdateInstallError::Launch(
        "this platform cannot launch gmark updates".to_owned(),
    ))
}

fn copy_and_verify(
    mut reader: impl std::io::Read,
    writer: &mut impl std::io::Write,
    max_bytes: u64,
    expected_sha256: &str,
) -> Result<u64, UpdateInstallError> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            UpdateInstallError::Network(format!("failed while reading update installer: {error}"))
        })?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or(UpdateInstallError::TooLarge)?;
        if total > max_bytes {
            return Err(UpdateInstallError::TooLarge);
        }
        hasher.update(&buffer[..read]);
        writer.write_all(&buffer[..read]).map_err(|error| {
            UpdateInstallError::Io(format!("failed to write update installer: {error}"))
        })?;
    }
    let actual = hex_sha256(hasher.finalize().into());
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(UpdateInstallError::HashMismatch {
            expected: expected_sha256.to_ascii_lowercase(),
            actual,
        });
    }
    Ok(total)
}

fn hex_sha256(bytes: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn embedded_verifying_key() -> Result<VerifyingKey, UpdateCheckError> {
    let encoded = option_env!("GMARK_UPDATE_PUBLIC_KEY_BASE64").ok_or_else(|| {
        UpdateCheckError::Configuration(
            "this build does not contain a gmark update verification key".to_owned(),
        )
    })?;
    verifying_key_from_base64(encoded)
}

fn verifying_key_from_base64(encoded: &str) -> Result<VerifyingKey, UpdateCheckError> {
    let bytes = BASE64.decode(encoded).map_err(|error| {
        UpdateCheckError::Configuration(format!("invalid update public key base64: {error}"))
    })?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
        UpdateCheckError::Configuration(format!(
            "update public key must be 32 bytes, got {}",
            bytes.len()
        ))
    })?;
    VerifyingKey::from_bytes(&bytes).map_err(|error| {
        UpdateCheckError::Configuration(format!("invalid Ed25519 update public key: {error}"))
    })
}

fn verify_signed_manifest(
    envelope_bytes: &[u8],
    key: &VerifyingKey,
) -> Result<UpdateManifest, UpdateCheckError> {
    if envelope_bytes.len() > MAX_ENVELOPE_BYTES {
        return Err(UpdateCheckError::Envelope(format!(
            "signed update envelope exceeds {MAX_ENVELOPE_BYTES} bytes"
        )));
    }
    let envelope: SignedEnvelope = serde_json::from_slice(envelope_bytes).map_err(|error| {
        UpdateCheckError::Envelope(format!("invalid signed update envelope: {error}"))
    })?;
    if envelope.schema_version != 1 {
        return Err(UpdateCheckError::Envelope(format!(
            "unsupported update envelope schema {}",
            envelope.schema_version
        )));
    }
    if envelope.algorithm != "Ed25519" {
        return Err(UpdateCheckError::Envelope(format!(
            "unsupported update signature algorithm '{}'",
            envelope.algorithm
        )));
    }
    let payload = BASE64.decode(&envelope.payload).map_err(|error| {
        UpdateCheckError::Envelope(format!("invalid update payload base64: {error}"))
    })?;
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(UpdateCheckError::Envelope(format!(
            "signed update payload exceeds {MAX_PAYLOAD_BYTES} bytes"
        )));
    }
    let signature = BASE64.decode(&envelope.signature).map_err(|error| {
        UpdateCheckError::Envelope(format!("invalid update signature base64: {error}"))
    })?;
    let signature = Signature::from_slice(&signature).map_err(|error| {
        UpdateCheckError::Signature(format!("invalid Ed25519 signature bytes: {error}"))
    })?;
    key.verify_strict(&payload, &signature).map_err(|_| {
        UpdateCheckError::Signature("update manifest signature verification failed".to_owned())
    })?;
    serde_json::from_slice(&payload).map_err(|error| {
        UpdateCheckError::Manifest(format!("invalid signed update manifest: {error}"))
    })
}

fn validate_manifest(manifest: &UpdateManifest) -> Result<(), UpdateCheckError> {
    if manifest.schema_version != 1 {
        return Err(UpdateCheckError::Manifest(format!(
            "unsupported update manifest schema {}",
            manifest.schema_version
        )));
    }
    if manifest.published_at.trim().is_empty() {
        return Err(UpdateCheckError::Manifest(
            "signed update manifest has no publication time".to_owned(),
        ));
    }
    if manifest.rollout_percent > 100 {
        return Err(UpdateCheckError::Manifest(format!(
            "rollout_percent {} exceeds 100",
            manifest.rollout_percent
        )));
    }
    validate_release_url(&manifest.release_url, "release_url")?;
    if manifest.artifacts.is_empty() {
        return Err(UpdateCheckError::Manifest(
            "signed update manifest contains no artifacts".to_owned(),
        ));
    }
    for (name, artifact) in &manifest.artifacts {
        validate_release_url(&artifact.url, &format!("artifact '{name}' URL"))?;
        if artifact.sha256.len() != 64
            || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(UpdateCheckError::Manifest(format!(
                "artifact '{name}' has an invalid SHA-256"
            )));
        }
    }
    let current_artifact = current_artifact_key().ok_or_else(|| {
        UpdateCheckError::Manifest("this platform has no update artifact mapping".to_owned())
    })?;
    if !manifest.artifacts.contains_key(current_artifact) {
        return Err(UpdateCheckError::Manifest(format!(
            "signed update manifest has no '{current_artifact}' artifact"
        )));
    }
    Ok(())
}

fn validate_release_url(value: &str, label: &str) -> Result<(), UpdateCheckError> {
    let url = Url::parse(value)
        .map_err(|error| UpdateCheckError::Manifest(format!("invalid {label}: {error}")))?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.path().starts_with("/kongweiguang/gmark/releases/")
        || url.username() != ""
        || url.password().is_some()
    {
        return Err(UpdateCheckError::Manifest(format!(
            "{label} must be an official HTTPS gmark release URL"
        )));
    }
    Ok(())
}

fn current_artifact_key() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("windows-x86_64"),
        ("windows", "aarch64") => Some("windows-aarch64"),
        ("macos", "x86_64") => Some("macos-x86_64"),
        ("macos", "aarch64") => Some("macos-aarch64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("linux", "aarch64") => Some("linux-aarch64"),
        _ => None,
    }
}

fn rollout_bucket(installation_id: uuid::Uuid, version: &str) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(installation_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(version.as_bytes());
    hasher.finalize() % 100
}

fn parse_semver(version: &str, label: &str) -> Result<Version, UpdateCheckError> {
    Version::parse(version).map_err(|error| {
        UpdateCheckError::ParseVersion(format!("{label} '{version}' is not valid SemVer: {error}"))
    })
}

fn fetch_remote_signed_manifest(source: UpdateSource) -> Result<Vec<u8>, RemoteFetchFailure> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .default_headers(update_request_headers())
        .build()
        .map_err(|error| {
            RemoteFetchFailure::new(
                source,
                RemoteFetchFailureKind::Network,
                format!("failed to build HTTP client: {error}"),
            )
        })?;
    let response = client.get(source.url()).send().map_err(|error| {
        if error.is_timeout() {
            RemoteFetchFailure::timeout(source, "request timed out after 5 seconds")
        } else {
            RemoteFetchFailure::new(source, RemoteFetchFailureKind::Network, error.to_string())
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(RemoteFetchFailure::new(
            source,
            RemoteFetchFailureKind::HttpStatus,
            format!("server returned HTTP {status}"),
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_ENVELOPE_BYTES as u64)
    {
        return Err(RemoteFetchFailure::new(
            source,
            RemoteFetchFailureKind::TooLarge,
            "response Content-Length exceeds the update envelope limit",
        ));
    }
    let mut body = Vec::new();
    response
        .take(MAX_ENVELOPE_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|error| {
            RemoteFetchFailure::new(source, RemoteFetchFailureKind::Body, error.to_string())
        })?;
    if body.len() > MAX_ENVELOPE_BYTES {
        return Err(RemoteFetchFailure::new(
            source,
            RemoteFetchFailureKind::TooLarge,
            "response body exceeds the update envelope limit",
        ));
    }
    Ok(body)
}

fn update_request_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(UPDATE_USER_AGENT));
    headers.insert(ACCEPT, HeaderValue::from_static(UPDATE_ACCEPT));
    headers
}

#[cfg(test)]
#[path = "../../tests/unit/net/update.rs"]
mod tests;
