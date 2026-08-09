// @author kongweiguang

//! Production update HTTP adapter for signed v2 manifests and staged artifacts.
//!
//! `gmark-update-core` owns every trust and file-protocol decision. This module
//! translates those decisions into reqwest requests and UI-friendly events.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::VerifyingKey;
use gmark_update_core::{
    BoundedTransferOutcome, MAX_ARTIFACT_BYTES, MAX_ENVELOPE_BYTES, Platform, SignedManifest,
    StagingPaths, UpdateCheckOutcome as CoreCheckOutcome, UpdateCoreError, copy_bounded,
    evaluate_update, parse_content_range_start, parse_verified_manifest, read_partial_metadata,
    resume_request, verify_artifact_file,
    verifying_key_from_base64 as core_verifying_key_from_base64, write_partial_metadata,
};
use reqwest::header::{
    ACCEPT, CONTENT_LENGTH, CONTENT_RANGE, ETAG, HeaderMap, HeaderValue, IF_RANGE, LAST_MODIFIED,
    RANGE, USER_AGENT,
};
use semver::Version;

pub(crate) use gmark_update_core::{ArtifactFormat, DownloadControl, DownloadEvent, SystemTrust};

pub(crate) const GITHUB_UPDATE_V2_MANIFEST_URL: &str =
    "https://github.com/kongweiguang/gmark/releases/latest/download/update-manifest-v2.json";
pub(crate) const GITEE_UPDATE_V2_MANIFEST_URL: &str =
    "https://raw.giteeusercontent.com/kongweiguang/gmark/raw/release/update-manifest-v2.json";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(12);
const ARTIFACT_TIMEOUT: Duration = Duration::from_secs(15 * 60);
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

/// GUI and network adapter view of a core-verified v2 release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdateRelease {
    pub(crate) current_version: String,
    pub(crate) version: String,
    pub(crate) published_at: String,
    pub(crate) notes: String,
    pub(crate) release_url: String,
    pub(crate) artifact_url: String,
    pub(crate) artifact_size: u64,
    pub(crate) artifact_sha256: String,
    pub(crate) artifact_format: ArtifactFormat,
    pub(crate) system_trust: SystemTrust,
    /// Original verified envelope bytes are staged for helper re-verification.
    pub(crate) signed_envelope: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CheckOutcome {
    Available(UpdateRelease),
    UpToDate {
        current_version: String,
        latest_version: String,
    },
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

pub(crate) fn check_latest_version_v2(
    current_version: &str,
) -> Result<CheckOutcome, UpdateV2Error> {
    let installation_id = crate::config::load_or_create_installation_id().map_err(|error| {
        UpdateV2Error::Configuration(format!("failed to load installation id: {error}"))
    })?;
    let key = embedded_verifying_key()?;
    #[cfg(feature = "updater-e2e")]
    if let Some(url) = updater_e2e_manifest_url()? {
        let envelope =
            fetch_manifest(&url).map_err(|error| UpdateV2Error::Network(error.to_string()))?;
        return compare_signed_manifest_v2(
            current_version,
            installation_id,
            &envelope,
            "updater-e2e",
            &key,
        );
    }
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

#[cfg(feature = "updater-e2e")]
fn updater_e2e_manifest_url() -> Result<Option<String>, UpdateV2Error> {
    let Some(value) = std::env::var_os("GMARK_UPDATER_E2E_MANIFEST_URL") else {
        return Ok(None);
    };
    let value = value.into_string().map_err(|_| {
        UpdateV2Error::Configuration("updater E2E manifest URL is not UTF-8".to_owned())
    })?;
    validate_updater_e2e_manifest_url(&value).map(Some)
}

#[cfg(feature = "updater-e2e")]
fn validate_updater_e2e_manifest_url(value: &str) -> Result<String, UpdateV2Error> {
    let url = reqwest::Url::parse(value).map_err(|error| {
        UpdateV2Error::Configuration(format!("invalid updater E2E manifest URL: {error}"))
    })?;
    if !is_loopback_http_url(value) {
        return Err(UpdateV2Error::Configuration(
            "updater E2E manifest URL must be an unauthenticated loopback HTTP(S) URL without a fragment"
                .to_owned(),
        ));
    }
    Ok(url.to_string())
}

#[cfg(feature = "updater-e2e")]
fn is_loopback_http_url(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
            && url.host_str().is_some_and(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|address| address.is_loopback())
            })
    })
}

fn compare_signed_manifest_v2(
    current_version: &str,
    installation_id: uuid::Uuid,
    envelope: &[u8],
    source: &str,
    key: &VerifyingKey,
) -> Result<CheckOutcome, UpdateV2Error> {
    let verified = parse_verified_manifest(envelope, key).map_err(map_core_check_error)?;
    if !matches!(verified.manifest, SignedManifest::V2(_)) {
        return Err(UpdateV2Error::Manifest(
            "signed update manifest is not schema v2".to_owned(),
        ));
    }
    let _ = source; // transport source does not affect the embedded trust root.
    match evaluate_update(
        &verified,
        current_version,
        installation_id,
        &Platform::current(),
    )
    .map_err(map_core_check_error)?
    {
        CoreCheckOutcome::Available(release) => {
            Ok(CheckOutcome::Available(adapt_v2_release(release)?))
        }
        CoreCheckOutcome::UpToDate {
            current_version,
            latest_version,
        } => Ok(CheckOutcome::UpToDate {
            current_version,
            latest_version,
        }),
    }
}

pub(crate) fn download_release(
    release: &UpdateRelease,
    updates_root: &Path,
    control: &DownloadControl,
    on_event: impl FnMut(DownloadEvent),
) -> Result<PathBuf, DownloadError> {
    let client = artifact_client()?;
    #[cfg(feature = "updater-e2e")]
    let allow_insecure_test_url = is_loopback_http_url(&release.artifact_url);
    #[cfg(not(feature = "updater-e2e"))]
    let allow_insecure_test_url = false;
    download_release_with_client(
        release,
        updates_root,
        control,
        &client,
        allow_insecure_test_url,
        on_event,
    )
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
    let paths = StagingPaths::for_version(updates_root, &release.version)
        .map_err(map_core_download_error)?;
    paths
        .create_transaction_dir()
        .map_err(map_core_download_error)?;

    if paths.ready_path.is_file() && verify_file(&paths.ready_path, release)? {
        on_event(DownloadEvent::Finished {
            path: paths.ready_path.clone(),
        });
        return Ok(paths.ready_path);
    }
    let _ = fs::remove_file(&paths.ready_path);

    let mut offset = fs::metadata(&paths.part_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if offset > release.artifact_size {
        fs::remove_file(&paths.part_path).map_err(|error| {
            DownloadError::Io(format!("failed to reset oversized partial update: {error}"))
        })?;
        offset = 0;
    }
    if offset == release.artifact_size && offset > 0 {
        on_event(DownloadEvent::Verifying);
        if verify_file(&paths.part_path, release)? {
            commit_verified_artifact(&paths, release)?;
            on_event(DownloadEvent::Finished {
                path: paths.ready_path.clone(),
            });
            return Ok(paths.ready_path);
        }
        fs::remove_file(&paths.part_path).map_err(|error| {
            DownloadError::Io(format!("failed to reset corrupt partial update: {error}"))
        })?;
        offset = 0;
    }

    // An invalid stale sidecar must not block a clean restart of the download.
    let metadata = read_partial_metadata(&paths.partial_metadata_path).unwrap_or_default();
    let resume = resume_request(offset, release.artifact_size, &metadata)
        .map_err(map_core_download_error)?;
    let mut request = client.get(&release.artifact_url);
    if let Some(resume) = &resume {
        request = request.header(RANGE, format!("bytes={}-", resume.offset));
        if let Some(if_range) = &resume.if_range {
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
    write_partial_metadata(
        &paths.partial_metadata_path,
        &gmark_update_core::PartialMetadata {
            etag: header_string(response.headers(), ETAG),
            last_modified: header_string(response.headers(), LAST_MODIFIED),
        },
    )
    .map_err(map_core_download_error)?;

    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(!resumed)
        .open(&paths.part_path)
        .map_err(|error| {
            DownloadError::Io(format!("failed to open partial update file: {error}"))
        })?;
    if resumed {
        output.seek(SeekFrom::End(0)).map_err(|error| {
            DownloadError::Io(format!("failed to seek partial update file: {error}"))
        })?;
    }

    on_event(DownloadEvent::Started {
        downloaded: offset,
        total: release.artifact_size,
    });
    let transfer = copy_bounded(
        &mut response,
        &mut output,
        offset,
        release.artifact_size,
        || control.is_paused(),
        |downloaded| {
            on_event(DownloadEvent::Progress {
                downloaded,
                total: release.artifact_size,
            });
        },
    )
    .map_err(map_core_download_error)?;
    output.sync_all().map_err(|error| {
        DownloadError::Io(format!("failed to durably write update artifact: {error}"))
    })?;
    drop(output);
    if let BoundedTransferOutcome::Paused { downloaded } = transfer {
        on_event(DownloadEvent::Paused {
            downloaded,
            total: release.artifact_size,
        });
        return Ok(paths.part_path);
    }

    on_event(DownloadEvent::Verifying);
    match verify_artifact_file(
        &paths.part_path,
        release.artifact_size,
        &release.artifact_sha256,
    ) {
        Ok(()) => {}
        Err(UpdateCoreError::HashMismatch { expected, actual }) => {
            let _ = fs::remove_file(&paths.part_path);
            return Err(DownloadError::HashMismatch { expected, actual });
        }
        Err(error) => return Err(map_core_download_error(error)),
    }
    commit_verified_artifact(&paths, release)?;
    on_event(DownloadEvent::Finished {
        path: paths.ready_path.clone(),
    });
    Ok(paths.ready_path)
}

/// Restores the highest valid ready transaction after independently rechecking
/// its signed envelope and artifact bytes.
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
            let envelope = read_cached_envelope(&root.join("manifest.envelope.json")).ok()?;
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

/// Reads a cache envelope through a fixed bound so a corrupted cache entry
/// cannot select the allocation size before signature verification starts.
fn read_cached_envelope(path: &Path) -> Result<Vec<u8>, String> {
    let mut file =
        File::open(path).map_err(|error| format!("failed to open cached envelope: {error}"))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_ENVELOPE_BYTES.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read cached envelope: {error}"))?;
    if bytes.is_empty() || bytes.len() > MAX_ENVELOPE_BYTES {
        return Err("cached envelope exceeds its size limit".to_owned());
    }
    Ok(bytes)
}

fn adapt_v2_release(
    release: gmark_update_core::UpdateRelease,
) -> Result<UpdateRelease, UpdateV2Error> {
    let artifact_size = release.artifact.size.ok_or_else(|| {
        UpdateV2Error::Manifest("signed v2 update artifact has no size".to_owned())
    })?;
    let artifact_format = release.artifact.format.ok_or_else(|| {
        UpdateV2Error::Manifest("signed v2 update artifact has no format".to_owned())
    })?;
    let system_trust = release.artifact.system_trust.ok_or_else(|| {
        UpdateV2Error::Manifest("signed v2 update artifact has no system trust".to_owned())
    })?;
    Ok(UpdateRelease {
        current_version: release.current_version,
        version: release.version,
        published_at: release.published_at,
        notes: release.notes,
        release_url: release.release_url,
        artifact_url: release.artifact.url,
        artifact_size,
        artifact_sha256: release.artifact.sha256,
        artifact_format,
        system_trust,
        signed_envelope: release.signed_envelope,
    })
}

fn commit_verified_artifact(
    paths: &StagingPaths,
    release: &UpdateRelease,
) -> Result<(), DownloadError> {
    paths
        .write_signed_envelope(&release.signed_envelope)
        .map_err(map_core_download_error)?;
    paths.commit_ready().map_err(map_core_download_error)
}

fn validate_artifact_metadata(release: &UpdateRelease) -> Result<(), DownloadError> {
    gmark_update_core::policy::validate_official_release_url(&release.artifact_url, "artifact URL")
        .map_err(|error| DownloadError::Metadata(error.to_string()))?;
    gmark_update_core::policy::validate_sha256(&release.artifact_sha256, "update artifact")
        .map_err(|_| {
            DownloadError::Metadata("update artifact has an invalid SHA-256".to_owned())
        })?;
    gmark_update_core::policy::validate_platform_format(
        release.artifact_format,
        &Platform::current(),
    )
    .and_then(|()| {
        gmark_update_core::policy::validate_system_trust(release.system_trust, &Platform::current())
    })
    .map_err(|error| DownloadError::Metadata(error.to_string()))
}

fn verify_file(path: &Path, release: &UpdateRelease) -> Result<bool, DownloadError> {
    match verify_artifact_file(path, release.artifact_size, &release.artifact_sha256) {
        Ok(()) => Ok(true),
        Err(UpdateCoreError::HashMismatch { .. } | UpdateCoreError::Truncated { .. }) => Ok(false),
        Err(error) => Err(map_core_download_error(error)),
    }
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

#[cfg(test)]
fn verify_signed_manifest_v2(
    envelope_bytes: &[u8],
    key: &VerifyingKey,
) -> Result<gmark_update_core::ManifestV2, UpdateV2Error> {
    let verified = parse_verified_manifest(envelope_bytes, key).map_err(map_core_check_error)?;
    match verified.manifest {
        SignedManifest::V2(manifest) => Ok(manifest),
        SignedManifest::V1(_) => Err(UpdateV2Error::Manifest(
            "signed update manifest is not schema v2".to_owned(),
        )),
    }
}

fn embedded_verifying_key() -> Result<VerifyingKey, UpdateV2Error> {
    let encoded = option_env!("GMARK_UPDATE_PUBLIC_KEY_BASE64").ok_or_else(|| {
        UpdateV2Error::Configuration(
            "this build does not contain a gmark update verification key".to_owned(),
        )
    })?;
    core_verifying_key_from_base64(encoded).map_err(map_core_check_error)
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

fn header_string(headers: &HeaderMap, name: reqwest::header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

#[cfg(test)]
fn current_artifact_key() -> Option<&'static str> {
    Platform::current().artifact_key_v2()
}

fn map_core_check_error(error: UpdateCoreError) -> UpdateV2Error {
    match error {
        UpdateCoreError::Configuration(message) => UpdateV2Error::Configuration(message),
        UpdateCoreError::Envelope(message) => UpdateV2Error::Envelope(message),
        UpdateCoreError::Signature(message) => UpdateV2Error::Signature(message),
        UpdateCoreError::Manifest(message) => UpdateV2Error::Manifest(message),
        other => UpdateV2Error::Manifest(other.to_string()),
    }
}

fn map_core_download_error(error: UpdateCoreError) -> DownloadError {
    match error {
        UpdateCoreError::Download(message) => DownloadError::Network(message),
        UpdateCoreError::Io(message) => DownloadError::Io(message),
        UpdateCoreError::TooLarge => DownloadError::TooLarge,
        UpdateCoreError::Truncated { expected, actual } => {
            DownloadError::Truncated { expected, actual }
        }
        UpdateCoreError::HashMismatch { expected, actual } => {
            DownloadError::HashMismatch { expected, actual }
        }
        other => DownloadError::Metadata(other.to_string()),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/net/update_v2.rs"]
mod tests;
