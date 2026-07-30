// @author kongweiguang

// reason: v1 manifest 兼容测试仍覆盖旧协议；remove when: v1 fixture 与兼容承诺一并淘汰。
#![allow(dead_code)]

//! Legacy update-manifest HTTP adapter and installer launcher.
//!
//! Trust, manifest parsing, version comparison, rollout, and artifact hashing
//! live in `gmark-update-core`; this module keeps only HTTP and process work.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::VerifyingKey;
use gmark_update_core::{
    MAX_ARTIFACT_BYTES, MAX_ENVELOPE_BYTES, Platform, SignedManifest, UpdateCheckOutcome,
    UpdateCoreError, copy_and_verify_bounded, evaluate_update, parse_verified_manifest,
    select_artifact, verifying_key_from_base64 as core_verifying_key_from_base64,
};
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};

pub(crate) const GITHUB_UPDATE_MANIFEST_URL: &str =
    "https://github.com/kongweiguang/gmark/releases/latest/download/update-manifest.json";
pub(crate) const GITEE_UPDATE_MANIFEST_URL: &str =
    "https://raw.giteeusercontent.com/kongweiguang/gmark/raw/release/update-manifest.json";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_INSTALLER_BYTES: u64 = MAX_ARTIFACT_BYTES;
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
    let verified = parse_verified_manifest(envelope, key).map_err(map_core_check_error)?;
    let SignedManifest::V1(manifest) = &verified.manifest else {
        return Err(UpdateCheckError::Manifest(
            "signed update manifest is not schema v1".to_owned(),
        ));
    };
    let artifact =
        select_artifact(&verified.manifest, &Platform::current()).map_err(map_core_check_error)?;
    let outcome = evaluate_update(
        &verified,
        current_version,
        installation_id,
        &Platform::current(),
    )
    .map_err(map_core_check_error)?;
    let (latest_version, available) = match outcome {
        UpdateCheckOutcome::Available(_) => (manifest.version.clone(), true),
        UpdateCheckOutcome::UpToDate { latest_version, .. } => (latest_version, false),
    };
    let info = UpdateVersionInfo {
        current_version: current_version.to_owned(),
        latest_version,
        source,
        release_url: manifest.release_url.clone(),
        artifact_url: artifact.url,
        artifact_sha256: artifact.sha256,
    };
    Ok(if available {
        UpdateCheckResult::UpdateAvailable(info)
    } else {
        UpdateCheckResult::UpToDate(info)
    })
}

/// Downloads a legacy signed-manifest artifact and starts the platform installer.
pub(crate) fn download_and_launch_update(
    info: &UpdateVersionInfo,
) -> Result<PathBuf, UpdateInstallError> {
    gmark_update_core::policy::validate_official_release_url(
        &info.artifact_url,
        "update artifact URL",
    )
    .map_err(|error| UpdateInstallError::Metadata(error.to_string()))?;
    gmark_update_core::policy::validate_sha256(&info.artifact_sha256, "update artifact").map_err(
        |_| UpdateInstallError::Metadata("update artifact has an invalid SHA-256".to_owned()),
    )?;

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
    let mut response = client.get(&info.artifact_url).send().map_err(|error| {
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
        &mut response,
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

/// Cleans only stale updater directories from the system temporary root.
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
    copy_and_verify_bounded(&mut reader, writer, max_bytes, expected_sha256)
        .map_err(map_core_install_error)
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
    core_verifying_key_from_base64(encoded).map_err(map_core_check_error)
}

fn verify_signed_manifest(
    envelope_bytes: &[u8],
    key: &VerifyingKey,
) -> Result<gmark_update_core::ManifestV1, UpdateCheckError> {
    let verified = parse_verified_manifest(envelope_bytes, key).map_err(map_core_check_error)?;
    match verified.manifest {
        SignedManifest::V1(manifest) => Ok(manifest),
        SignedManifest::V2(_) => Err(UpdateCheckError::Manifest(
            "signed update manifest is not schema v1".to_owned(),
        )),
    }
}

fn current_artifact_key() -> Option<&'static str> {
    Platform::current().artifact_key_v1()
}

fn map_core_check_error(error: UpdateCoreError) -> UpdateCheckError {
    match error {
        UpdateCoreError::Configuration(message) => UpdateCheckError::Configuration(message),
        UpdateCoreError::Envelope(message) => UpdateCheckError::Envelope(message),
        UpdateCoreError::Signature(message) => UpdateCheckError::Signature(message),
        UpdateCoreError::Manifest(message) if message.starts_with("current app version") => {
            UpdateCheckError::ParseVersion(message)
        }
        UpdateCoreError::Manifest(message) if message.starts_with("signed update version") => {
            UpdateCheckError::ParseVersion(message.replacen(
                "signed update version",
                "signed update manifest version",
                1,
            ))
        }
        UpdateCoreError::Manifest(message) => UpdateCheckError::Manifest(message),
        other => UpdateCheckError::Manifest(other.to_string()),
    }
}

fn map_core_install_error(error: UpdateCoreError) -> UpdateInstallError {
    match error {
        UpdateCoreError::Download(message) => UpdateInstallError::Network(message),
        UpdateCoreError::Io(message) => UpdateInstallError::Io(message),
        UpdateCoreError::TooLarge => UpdateInstallError::TooLarge,
        UpdateCoreError::HashMismatch { expected, actual } => {
            UpdateInstallError::HashMismatch { expected, actual }
        }
        other => UpdateInstallError::Metadata(other.to_string()),
    }
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
