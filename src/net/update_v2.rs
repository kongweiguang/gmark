// @author kongweiguang

//! Production updater transport: signed v2 manifests and resumable artifact downloads.
//!
//! 清单签名、平台选择和 rollout 决策先于任何下载副作用；下载只写应用私有缓存，
//! 完整长度与 SHA-256 同时通过后才以原子 rename 暴露为可安装产物。

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, VerifyingKey};
use reqwest::Url;
use reqwest::header::{
    ACCEPT, CONTENT_LENGTH, CONTENT_RANGE, ETAG, HeaderMap, HeaderValue, IF_RANGE, LAST_MODIFIED,
    RANGE, USER_AGENT,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

pub(crate) const GITHUB_UPDATE_V2_MANIFEST_URL: &str =
    "https://github.com/kongweiguang/gmark/releases/latest/download/update-manifest-v2.json";
pub(crate) const GITEE_UPDATE_V2_MANIFEST_URL: &str =
    "https://raw.giteeusercontent.com/kongweiguang/gmark/raw/release/update-manifest-v2.json";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(12);
const ARTIFACT_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const MAX_ENVELOPE_BYTES: usize = 128 * 1024;
const MAX_PAYLOAD_BYTES: usize = 96 * 1024;
const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;
const UPDATE_USER_AGENT: &str = concat!(
    "gmark/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/kongweiguang/gmark)"
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckOrigin {
    Automatic,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CheckOutcome {
    Available(UpdateRelease),
    UpToDate {
        current_version: String,
        latest_version: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ArtifactFormat {
    WindowsSetupExe,
    MacosAppTarGz,
    LinuxAppImage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SystemTrust {
    Unsigned,
    Authenticode,
    DeveloperIdNotarized,
    NotApplicable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdateRelease {
    pub current_version: String,
    pub version: String,
    pub published_at: String,
    pub notes: String,
    pub release_url: String,
    pub artifact_url: String,
    pub artifact_size: u64,
    pub artifact_sha256: String,
    pub artifact_format: ArtifactFormat,
    pub system_trust: SystemTrust,
    /// Helper 会再次验证同一签名 envelope，不能只信 UI 进程传入的派生字段。
    pub signed_envelope: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UpdateV2Error {
    Configuration(String),
    Network(String),
    Envelope(String),
    Signature(String),
    Manifest(String),
}

impl fmt::Display for UpdateV2Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message)
            | Self::Network(message)
            | Self::Envelope(message)
            | Self::Signature(message)
            | Self::Manifest(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for UpdateV2Error {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DownloadEvent {
    Started { downloaded: u64, total: u64 },
    Progress { downloaded: u64, total: u64 },
    Verifying,
    Finished { path: PathBuf },
    Paused { downloaded: u64, total: u64 },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DownloadError {
    Metadata(String),
    Network(String),
    Io(String),
    TooLarge,
    Truncated { expected: u64, actual: u64 },
    HashMismatch { expected: String, actual: String },
}

impl DownloadError {
    pub(crate) fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Network(_) | Self::Io(_) | Self::Truncated { .. }
        )
    }
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Metadata(message) | Self::Network(message) | Self::Io(message) => {
                f.write_str(message)
            }
            Self::TooLarge => write!(
                f,
                "update artifact exceeds the {} MiB safety limit",
                MAX_ARTIFACT_BYTES / 1024 / 1024
            ),
            Self::Truncated { expected, actual } => write!(
                f,
                "update artifact ended early (expected {expected} bytes, received {actual})"
            ),
            Self::HashMismatch { expected, actual } => write!(
                f,
                "downloaded update SHA-256 mismatch (expected {expected}, got {actual})"
            ),
        }
    }
}

impl std::error::Error for DownloadError {}

#[derive(Clone, Default)]
pub(crate) struct DownloadControl(Arc<AtomicBool>);

impl DownloadControl {
    pub(crate) fn pause(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn is_paused(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedEnvelope {
    schema_version: u8,
    algorithm: String,
    payload: String,
    signature: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateManifestV2 {
    schema_version: u8,
    channel: String,
    version: String,
    published_at: String,
    notes: String,
    paused: bool,
    rollout_percent: u8,
    release_url: String,
    artifacts: std::collections::BTreeMap<String, UpdateArtifactV2>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArtifactV2 {
    url: String,
    size: u64,
    sha256: String,
    format: ArtifactFormat,
    system_trust: SystemTrust,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialMetadata {
    etag: Option<String>,
    last_modified: Option<String>,
}

pub(crate) fn check_latest_version_v2(
    current_version: &str,
) -> Result<CheckOutcome, UpdateV2Error> {
    let installation_id = crate::config::load_or_create_installation_id().map_err(|error| {
        UpdateV2Error::Configuration(format!("failed to load installation id: {error}"))
    })?;
    let key = embedded_verifying_key()?;

    let github = fetch_manifest(GITHUB_UPDATE_V2_MANIFEST_URL);
    let (source, envelope) = match github {
        Ok(bytes) => ("GitHub", bytes),
        Err(first) if first.retryable => {
            let bytes = fetch_manifest(GITEE_UPDATE_V2_MANIFEST_URL)
                .map_err(|second| UpdateV2Error::Network(format!("{first}; {second}")))?;
            ("Gitee", bytes)
        }
        Err(error) => return Err(UpdateV2Error::Network(error.to_string())),
    };
    compare_signed_manifest_v2(current_version, installation_id, &envelope, source, &key)
}

fn compare_signed_manifest_v2(
    current_version: &str,
    installation_id: uuid::Uuid,
    envelope: &[u8],
    source: &str,
    key: &VerifyingKey,
) -> Result<CheckOutcome, UpdateV2Error> {
    let current = Version::parse(current_version).map_err(|error| {
        UpdateV2Error::Manifest(format!("current app version is not valid SemVer: {error}"))
    })?;
    let manifest = verify_signed_manifest_v2(envelope, key)?;
    validate_manifest_v2(&manifest)?;
    let latest = Version::parse(&manifest.version).map_err(|error| {
        UpdateV2Error::Manifest(format!(
            "signed update version is not valid SemVer: {error}"
        ))
    })?;
    let artifact_key = current_artifact_key().ok_or_else(|| {
        UpdateV2Error::Manifest("this platform has no updater artifact mapping".to_owned())
    })?;
    let artifact = manifest.artifacts.get(artifact_key).ok_or_else(|| {
        UpdateV2Error::Manifest(format!(
            "signed update manifest has no '{artifact_key}' artifact"
        ))
    })?;
    validate_platform_format(&artifact.format)?;

    let eligible = !manifest.paused
        && rollout_bucket(installation_id, &manifest.version) < manifest.rollout_percent as u32;
    if latest <= current || !eligible {
        return Ok(CheckOutcome::UpToDate {
            current_version: current_version.to_owned(),
            latest_version: if latest > current {
                current_version.to_owned()
            } else {
                manifest.version
            },
        });
    }

    let _ = source; // 来源只用于诊断；信任根始终是内嵌 Ed25519 公钥。
    Ok(CheckOutcome::Available(UpdateRelease {
        current_version: current_version.to_owned(),
        version: manifest.version,
        published_at: manifest.published_at,
        notes: manifest.notes,
        release_url: manifest.release_url,
        artifact_url: artifact.url.clone(),
        artifact_size: artifact.size,
        artifact_sha256: artifact.sha256.to_ascii_lowercase(),
        artifact_format: artifact.format.clone(),
        system_trust: artifact.system_trust.clone(),
        signed_envelope: Arc::from(envelope.to_vec()),
    }))
}

pub(crate) fn download_release(
    release: &UpdateRelease,
    updates_root: &Path,
    control: &DownloadControl,
    on_event: impl FnMut(DownloadEvent),
) -> Result<PathBuf, DownloadError> {
    let client = artifact_client()?;
    download_release_with_client(release, updates_root, control, &client, false, on_event)
}

fn download_release_with_client(
    release: &UpdateRelease,
    updates_root: &Path,
    control: &DownloadControl,
    client: &reqwest::blocking::Client,
    allow_insecure_test_url: bool,
    mut on_event: impl FnMut(DownloadEvent),
) -> Result<PathBuf, DownloadError> {
    if !allow_insecure_test_url {
        validate_artifact_metadata(release)?;
    }
    let version_dir = updates_root.join(format!("v{}", release.version));
    fs::create_dir_all(&version_dir).map_err(|error| {
        DownloadError::Io(format!("failed to create update cache directory: {error}"))
    })?;
    let part_path = version_dir.join("artifact.part");
    let ready_path = version_dir.join("artifact.ready");
    let metadata_path = version_dir.join("partial.json");
    let envelope_path = version_dir.join("manifest.envelope.json");

    if ready_path.is_file() && verify_file(&ready_path, release)? {
        on_event(DownloadEvent::Finished {
            path: ready_path.clone(),
        });
        return Ok(ready_path);
    }
    let _ = fs::remove_file(&ready_path);

    let mut offset = fs::metadata(&part_path).map(|meta| meta.len()).unwrap_or(0);
    if offset > release.artifact_size {
        fs::remove_file(&part_path).map_err(|error| {
            DownloadError::Io(format!("failed to reset oversized partial update: {error}"))
        })?;
        offset = 0;
    }
    if offset == release.artifact_size && offset > 0 {
        on_event(DownloadEvent::Verifying);
        if verify_file(&part_path, release)? {
            fs::write(&envelope_path, release.signed_envelope.as_ref()).map_err(|error| {
                DownloadError::Io(format!("failed to persist signed update manifest: {error}"))
            })?;
            fs::rename(&part_path, &ready_path).map_err(|error| {
                DownloadError::Io(format!(
                    "failed to commit verified update artifact: {error}"
                ))
            })?;
            let _ = fs::remove_file(&metadata_path);
            sync_parent(&ready_path)?;
            on_event(DownloadEvent::Finished {
                path: ready_path.clone(),
            });
            return Ok(ready_path);
        }
        fs::remove_file(&part_path).map_err(|error| {
            DownloadError::Io(format!("failed to reset corrupt partial update: {error}"))
        })?;
        offset = 0;
    }
    let partial_metadata = read_partial_metadata(&metadata_path);
    let mut request = client.get(&release.artifact_url);
    if offset > 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
        if let Some(if_range) = partial_metadata
            .etag
            .as_deref()
            .or(partial_metadata.last_modified.as_deref())
        {
            request = request.header(IF_RANGE, if_range);
        }
    }
    let mut response = request.send().map_err(|error| {
        DownloadError::Network(format!("failed to download update artifact: {error}"))
    })?;
    if !allow_insecure_test_url && response.url().scheme() != "https" {
        return Err(DownloadError::Network(
            "update artifact response did not use HTTPS".to_owned(),
        ));
    }

    let resumed = offset > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if resumed {
        let start = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_content_range_start)
            .ok_or_else(|| {
                DownloadError::Network("resumed response has invalid Content-Range".to_owned())
            })?;
        if start != offset {
            return Err(DownloadError::Network(format!(
                "resumed response started at {start}, expected {offset}"
            )));
        }
    } else if response.status().is_success() {
        offset = 0;
    } else {
        return Err(DownloadError::Network(format!(
            "update artifact server returned HTTP {}",
            response.status()
        )));
    }

    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|remaining| offset.saturating_add(remaining) > release.artifact_size)
    {
        return Err(DownloadError::TooLarge);
    }
    let next_metadata = PartialMetadata {
        etag: header_string(response.headers(), ETAG),
        last_modified: header_string(response.headers(), LAST_MODIFIED),
    };
    write_json_atomic(&metadata_path, &next_metadata)?;

    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(!resumed)
        .open(&part_path)
        .map_err(|error| {
            DownloadError::Io(format!("failed to open partial update file: {error}"))
        })?;
    if resumed {
        output.seek(SeekFrom::End(0)).map_err(|error| {
            DownloadError::Io(format!("failed to seek partial update file: {error}"))
        })?;
    }

    let mut downloaded = offset;
    on_event(DownloadEvent::Started {
        downloaded,
        total: release.artifact_size,
    });
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        if control.is_paused() {
            output.sync_all().map_err(|error| {
                DownloadError::Io(format!("failed to persist paused update: {error}"))
            })?;
            on_event(DownloadEvent::Paused {
                downloaded,
                total: release.artifact_size,
            });
            return Ok(part_path);
        }
        let read = response.read(&mut buffer).map_err(|error| {
            DownloadError::Network(format!("failed while reading update artifact: {error}"))
        })?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or(DownloadError::TooLarge)?;
        if downloaded > release.artifact_size || downloaded > MAX_ARTIFACT_BYTES {
            return Err(DownloadError::TooLarge);
        }
        output.write_all(&buffer[..read]).map_err(|error| {
            DownloadError::Io(format!("failed to write update artifact: {error}"))
        })?;
        on_event(DownloadEvent::Progress {
            downloaded,
            total: release.artifact_size,
        });
    }
    output
        .flush()
        .and_then(|()| output.sync_all())
        .map_err(|error| {
            DownloadError::Io(format!("failed to durably write update artifact: {error}"))
        })?;
    drop(output);
    if downloaded != release.artifact_size {
        return Err(DownloadError::Truncated {
            expected: release.artifact_size,
            actual: downloaded,
        });
    }

    on_event(DownloadEvent::Verifying);
    if !verify_file(&part_path, release)? {
        let actual = sha256_file(&part_path)?;
        let _ = fs::remove_file(&part_path);
        return Err(DownloadError::HashMismatch {
            expected: release.artifact_sha256.clone(),
            actual,
        });
    }
    fs::write(&envelope_path, release.signed_envelope.as_ref()).map_err(|error| {
        DownloadError::Io(format!("failed to persist signed update manifest: {error}"))
    })?;
    fs::rename(&part_path, &ready_path).map_err(|error| {
        DownloadError::Io(format!(
            "failed to commit verified update artifact: {error}"
        ))
    })?;
    let _ = fs::remove_file(metadata_path);
    sync_parent(&ready_path)?;
    on_event(DownloadEvent::Finished {
        path: ready_path.clone(),
    });
    Ok(ready_path)
}

/// 恢复上次退出前已完成校验但尚未安装的最高版本；恢复时重新验签并重新哈希，
/// 缓存文件名和本地 metadata 都不能成为信任来源。
pub(crate) fn restore_ready_release(
    updates_root: &Path,
    current_version: &str,
) -> Option<(UpdateRelease, PathBuf)> {
    let key = embedded_verifying_key().ok()?;
    let installation_id = crate::config::load_or_create_installation_id().ok()?;
    let mut candidates = fs::read_dir(updates_root)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let root = entry.path();
            let ready = root.join("artifact.ready");
            let envelope = fs::read(root.join("manifest.envelope.json")).ok()?;
            let CheckOutcome::Available(release) = compare_signed_manifest_v2(
                current_version,
                installation_id,
                &envelope,
                "cache",
                &key,
            )
            .ok()?
            else {
                return None;
            };
            if !verify_file(&ready, &release).ok()? {
                return None;
            }
            let version = Version::parse(&release.version).ok()?;
            Some((version, release, ready))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.0.cmp(&left.0));
    candidates
        .into_iter()
        .next()
        .map(|(_, release, path)| (release, path))
}

fn validate_artifact_metadata(release: &UpdateRelease) -> Result<(), DownloadError> {
    validate_official_release_url(&release.artifact_url, "artifact URL")
        .map_err(|error| DownloadError::Metadata(error.to_string()))?;
    if release.artifact_size == 0 || release.artifact_size > MAX_ARTIFACT_BYTES {
        return Err(DownloadError::TooLarge);
    }
    if !is_sha256(&release.artifact_sha256) {
        return Err(DownloadError::Metadata(
            "update artifact has an invalid SHA-256".to_owned(),
        ));
    }
    validate_platform_format(&release.artifact_format)
        .map_err(|error| DownloadError::Metadata(error.to_string()))
}

fn verify_file(path: &Path, release: &UpdateRelease) -> Result<bool, DownloadError> {
    let length = fs::metadata(path)
        .map_err(|error| DownloadError::Io(format!("failed to inspect update artifact: {error}")))?
        .len();
    if length != release.artifact_size {
        return Ok(false);
    }
    Ok(sha256_file(path)?.eq_ignore_ascii_case(&release.artifact_sha256))
}

fn sha256_file(path: &Path) -> Result<String, DownloadError> {
    let mut file = File::open(path)
        .map_err(|error| DownloadError::Io(format!("failed to read update artifact: {error}")))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            DownloadError::Io(format!("failed to hash update artifact: {error}"))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_sha256(hasher.finalize().into()))
}

fn artifact_client() -> Result<reqwest::blocking::Client, DownloadError> {
    reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(ARTIFACT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("too many update download redirects")
            } else if attempt.url().scheme() != "https" {
                attempt.error("update download redirect must use HTTPS")
            } else {
                attempt.follow()
            }
        }))
        .default_headers(update_headers())
        .build()
        .map_err(|error| {
            DownloadError::Network(format!("failed to build update HTTP client: {error}"))
        })
}

#[derive(Debug)]
struct FetchError {
    message: String,
    retryable: bool,
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

fn fetch_manifest(url: &str) -> Result<Vec<u8>, FetchError> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(MANIFEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .default_headers(update_headers())
        .build()
        .map_err(|error| FetchError {
            message: format!("failed to build update HTTP client: {error}"),
            retryable: true,
        })?;
    let response = client.get(url).send().map_err(|error| FetchError {
        message: format!("update manifest request failed: {error}"),
        retryable: error.is_timeout() || error.is_connect() || error.is_request(),
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(FetchError {
            message: format!("update manifest server returned HTTP {status}"),
            retryable: status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS,
        });
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_ENVELOPE_BYTES as u64)
    {
        return Err(FetchError {
            message: "update manifest exceeds the response limit".to_owned(),
            retryable: false,
        });
    }
    let mut body = Vec::new();
    response
        .take(MAX_ENVELOPE_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|error| FetchError {
            message: format!("failed to read update manifest: {error}"),
            retryable: true,
        })?;
    if body.len() > MAX_ENVELOPE_BYTES {
        return Err(FetchError {
            message: "update manifest exceeds the response limit".to_owned(),
            retryable: false,
        });
    }
    Ok(body)
}

fn verify_signed_manifest_v2(
    envelope_bytes: &[u8],
    key: &VerifyingKey,
) -> Result<UpdateManifestV2, UpdateV2Error> {
    if envelope_bytes.len() > MAX_ENVELOPE_BYTES {
        return Err(UpdateV2Error::Envelope(
            "signed update envelope exceeds the size limit".to_owned(),
        ));
    }
    let envelope: SignedEnvelope = serde_json::from_slice(envelope_bytes).map_err(|error| {
        UpdateV2Error::Envelope(format!("invalid signed update envelope: {error}"))
    })?;
    if envelope.schema_version != 1 || envelope.algorithm != "Ed25519" {
        return Err(UpdateV2Error::Envelope(
            "unsupported update envelope format".to_owned(),
        ));
    }
    let payload = BASE64.decode(envelope.payload).map_err(|error| {
        UpdateV2Error::Envelope(format!("invalid update payload base64: {error}"))
    })?;
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(UpdateV2Error::Envelope(
            "signed update payload exceeds the size limit".to_owned(),
        ));
    }
    let signature = BASE64.decode(envelope.signature).map_err(|error| {
        UpdateV2Error::Envelope(format!("invalid update signature base64: {error}"))
    })?;
    let signature = Signature::from_slice(&signature).map_err(|error| {
        UpdateV2Error::Signature(format!("invalid Ed25519 signature bytes: {error}"))
    })?;
    key.verify_strict(&payload, &signature).map_err(|_| {
        UpdateV2Error::Signature("update manifest signature verification failed".to_owned())
    })?;
    serde_json::from_slice(&payload).map_err(|error| {
        UpdateV2Error::Manifest(format!("invalid signed v2 update manifest: {error}"))
    })
}

fn validate_manifest_v2(manifest: &UpdateManifestV2) -> Result<(), UpdateV2Error> {
    if manifest.schema_version != 2 {
        return Err(UpdateV2Error::Manifest(format!(
            "unsupported update manifest schema {}",
            manifest.schema_version
        )));
    }
    if manifest.channel != "stable" {
        return Err(UpdateV2Error::Manifest(
            "update manifest channel must be stable".to_owned(),
        ));
    }
    if !is_rfc3339_utc(&manifest.published_at) {
        return Err(UpdateV2Error::Manifest(
            "update manifest publication time must be RFC3339 UTC".to_owned(),
        ));
    }
    if manifest.notes.len() > 32 * 1024 {
        return Err(UpdateV2Error::Manifest(
            "update release notes exceed the size limit".to_owned(),
        ));
    }
    if manifest.rollout_percent > 100 {
        return Err(UpdateV2Error::Manifest(
            "update rollout percent exceeds 100".to_owned(),
        ));
    }
    validate_official_release_url(&manifest.release_url, "release URL")?;
    if manifest.artifacts.is_empty() {
        return Err(UpdateV2Error::Manifest(
            "signed update manifest contains no artifacts".to_owned(),
        ));
    }
    for (name, artifact) in &manifest.artifacts {
        validate_official_release_url(&artifact.url, &format!("artifact '{name}' URL"))?;
        if artifact.size == 0 || artifact.size > MAX_ARTIFACT_BYTES {
            return Err(UpdateV2Error::Manifest(format!(
                "artifact '{name}' has an invalid size"
            )));
        }
        if !is_sha256(&artifact.sha256) {
            return Err(UpdateV2Error::Manifest(format!(
                "artifact '{name}' has an invalid SHA-256"
            )));
        }
    }
    Ok(())
}

fn validate_official_release_url(value: &str, label: &str) -> Result<(), UpdateV2Error> {
    let url = Url::parse(value)
        .map_err(|error| UpdateV2Error::Manifest(format!("invalid {label}: {error}")))?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.path().starts_with("/kongweiguang/gmark/releases/")
        || url.username() != ""
        || url.password().is_some()
    {
        return Err(UpdateV2Error::Manifest(format!(
            "{label} must be an official HTTPS gmark release URL"
        )));
    }
    Ok(())
}

fn validate_platform_format(format: &ArtifactFormat) -> Result<(), UpdateV2Error> {
    let valid = matches!(
        (std::env::consts::OS, format),
        ("windows", ArtifactFormat::WindowsSetupExe)
            | ("macos", ArtifactFormat::MacosAppTarGz)
            | ("linux", ArtifactFormat::LinuxAppImage)
    );
    if valid {
        Ok(())
    } else {
        Err(UpdateV2Error::Manifest(
            "update artifact format does not match this platform".to_owned(),
        ))
    }
}

fn current_artifact_key() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("windows-x86_64"),
        ("macos", "x86_64") => Some("macos-x86_64"),
        ("macos", "aarch64") => Some("macos-aarch64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
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

fn embedded_verifying_key() -> Result<VerifyingKey, UpdateV2Error> {
    let encoded = option_env!("GMARK_UPDATE_PUBLIC_KEY_BASE64").ok_or_else(|| {
        UpdateV2Error::Configuration(
            "this build does not contain a gmark update verification key".to_owned(),
        )
    })?;
    let bytes = BASE64.decode(encoded).map_err(|error| {
        UpdateV2Error::Configuration(format!("invalid update public key base64: {error}"))
    })?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
        UpdateV2Error::Configuration(format!(
            "update public key must be 32 bytes, got {}",
            bytes.len()
        ))
    })?;
    VerifyingKey::from_bytes(&bytes).map_err(|error| {
        UpdateV2Error::Configuration(format!("invalid Ed25519 update public key: {error}"))
    })
}

fn update_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(UPDATE_USER_AGENT));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json,*/*;q=0.5"),
    );
    headers
}

fn read_partial_metadata(path: &Path) -> PartialMetadata {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<(), DownloadError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        DownloadError::Metadata(format!("failed to serialize update metadata: {error}"))
    })?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| DownloadError::Io(format!("failed to create update metadata: {error}")))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| DownloadError::Io(format!("failed to write update metadata: {error}")))?;
    temporary.persist(path).map(|_| ()).map_err(|error| {
        DownloadError::Io(format!("failed to commit update metadata: {}", error.error))
    })
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<(), DownloadError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            DownloadError::Io(format!("failed to sync update cache directory: {error}"))
        })
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> Result<(), DownloadError> {
    // Windows 没有可移植的目录 fsync；文件本身已在 rename 前 sync_all。
    Ok(())
}

fn header_string(headers: &HeaderMap, name: reqwest::header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn parse_content_range_start(value: &str) -> Option<u64> {
    value
        .strip_prefix("bytes ")?
        .split_once('-')?
        .0
        .parse()
        .ok()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_rfc3339_utc(value: &str) -> bool {
    // 发布脚本固定输出 UTC Z；这里严格拒绝宽松或本地时区文本，避免歧义与旧清单回放诊断失真。
    value.ends_with('Z')
        && value.len() >= 20
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.as_bytes().get(10) == Some(&b'T')
        && value.as_bytes().get(13) == Some(&b':')
        && value.as_bytes().get(16) == Some(&b':')
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

#[cfg(test)]
#[path = "../../tests/unit/net/update_v2.rs"]
mod tests;
