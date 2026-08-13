// @author kongweiguang

//! Pure update-policy decisions: platform mapping, trust, versions, and rollout.

use std::cmp::Ordering;

use semver::Version;
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::{Result, UpdateCoreError};

pub const MAX_ENVELOPE_BYTES: usize = 128 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 96 * 1024;
pub const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;

/// Target platform supplied by an adapter or constructed from the current process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Platform {
    pub os: String,
    pub arch: String,
}

impl Platform {
    #[must_use]
    pub fn new(os: impl Into<String>, arch: impl Into<String>) -> Self {
        Self {
            os: os.into(),
            arch: arch.into(),
        }
    }

    #[must_use]
    pub fn current() -> Self {
        Self::new(std::env::consts::OS, std::env::consts::ARCH)
    }

    /// Legacy v1 retained mappings for Windows/Linux aarch64 packages.
    #[must_use]
    pub fn artifact_key_v1(&self) -> Option<&'static str> {
        match (self.os.as_str(), self.arch.as_str()) {
            ("windows", "x86_64") => Some("windows-x86_64"),
            ("windows", "aarch64") => Some("windows-aarch64"),
            ("macos", "x86_64") => Some("macos-x86_64"),
            ("macos", "aarch64") => Some("macos-aarch64"),
            ("linux", "x86_64") => Some("linux-x86_64"),
            ("linux", "aarch64") => Some("linux-aarch64"),
            _ => None,
        }
    }

    /// v2 keeps the platform keys accepted by the current updater implementation.
    #[must_use]
    pub fn artifact_key_v2(&self) -> Option<&'static str> {
        match (self.os.as_str(), self.arch.as_str()) {
            ("windows", "x86_64") => Some("windows-x86_64"),
            ("macos", "x86_64") => Some("macos-x86_64"),
            ("macos", "aarch64") => Some("macos-aarch64"),
            ("linux", "x86_64") => Some("linux-x86_64"),
            _ => None,
        }
    }
}

/// v2 artifact package names use the existing kebab-case JSON values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactFormat {
    WindowsSetupExe,
    MacosAppTarGz,
    LinuxAppImage,
}

impl ArtifactFormat {
    #[must_use]
    pub const fn as_protocol_name(self) -> &'static str {
        match self {
            Self::WindowsSetupExe => "windows-setup-exe",
            Self::MacosAppTarGz => "macos-app-tar-gz",
            Self::LinuxAppImage => "linux-app-image",
        }
    }

    #[must_use]
    pub fn from_protocol_name(value: &str) -> Option<Self> {
        match value {
            "windows-setup-exe" => Some(Self::WindowsSetupExe),
            "macos-app-tar-gz" => Some(Self::MacosAppTarGz),
            "linux-app-image" => Some(Self::LinuxAppImage),
            _ => None,
        }
    }
}

/// System code-signing status carried in a v2 manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SystemTrust {
    Unsigned,
    Authenticode,
    DeveloperIdNotarized,
    NotApplicable,
}

/// Accepts only the GitHub release namespace used by signed Gmark manifests.
pub fn validate_official_release_url(value: &str, label: &str) -> Result<()> {
    let url = Url::parse(value)
        .map_err(|error| UpdateCoreError::Manifest(format!("invalid {label}: {error}")))?;
    #[cfg(feature = "updater-e2e")]
    if is_updater_e2e_loopback_url(&url) {
        return Ok(());
    }
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.path().starts_with("/kongweiguang/gmark/releases/")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(UpdateCoreError::Manifest(format!(
            "{label} must be an official HTTPS gmark release URL"
        )));
    }
    Ok(())
}

/// Helper apply plans must reference a downloadable GitHub release asset.
pub fn validate_official_artifact_url(value: &str) -> Result<()> {
    let url = Url::parse(value)
        .map_err(|error| UpdateCoreError::Protocol(format!("invalid artifact URL: {error}")))?;
    #[cfg(feature = "updater-e2e")]
    if is_updater_e2e_loopback_url(&url) {
        return Ok(());
    }
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url
            .path()
            .starts_with("/kongweiguang/gmark/releases/download/")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(UpdateCoreError::Protocol(
            "apply plan artifact URL is not an official GitHub release URL".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(feature = "updater-e2e")]
fn is_updater_e2e_loopback_url(url: &Url) -> bool {
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
}

/// Validates an ASCII hexadecimal SHA-256 digest, preserving old case-insensitive input.
pub fn validate_sha256(value: &str, label: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(UpdateCoreError::Manifest(format!(
            "{label} has an invalid SHA-256"
        )));
    }
    Ok(())
}

/// Validates the format against the selected operating system.
pub fn validate_platform_format(format: ArtifactFormat, platform: &Platform) -> Result<()> {
    let valid = matches!(
        (platform.os.as_str(), format),
        ("windows", ArtifactFormat::WindowsSetupExe)
            | ("macos", ArtifactFormat::MacosAppTarGz)
            | ("linux", ArtifactFormat::LinuxAppImage)
    );
    if valid {
        Ok(())
    } else {
        Err(UpdateCoreError::Manifest(
            "update artifact format does not match this platform".to_owned(),
        ))
    }
}

/// Validates v2 system-trust metadata with the same policy as the helper.
pub fn validate_system_trust(trust: SystemTrust, platform: &Platform) -> Result<()> {
    let valid = match platform.os.as_str() {
        "windows" => matches!(trust, SystemTrust::Unsigned | SystemTrust::Authenticode),
        "macos" => matches!(
            trust,
            SystemTrust::Unsigned | SystemTrust::DeveloperIdNotarized
        ),
        "linux" => trust == SystemTrust::NotApplicable,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(UpdateCoreError::Manifest(
            "update artifact system trust does not match this platform".to_owned(),
        ))
    }
}

/// Current manifests intentionally use a compact UTC-Z shape check for compatibility.
#[must_use]
pub fn is_rfc3339_utc(value: &str) -> bool {
    value.ends_with('Z')
        && value.len() >= 20
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.as_bytes().get(10) == Some(&b'T')
        && value.as_bytes().get(13) == Some(&b':')
        && value.as_bytes().get(16) == Some(&b':')
}

/// Parses a SemVer string while preserving a meaningful domain label.
pub fn parse_semver(value: &str, label: &str) -> Result<Version> {
    Version::parse(value).map_err(|error| {
        UpdateCoreError::Manifest(format!("{label} '{value}' is not valid SemVer: {error}"))
    })
}

/// Compares two SemVer strings after validating both inputs.
pub fn compare_versions(current: &str, candidate: &str) -> Result<Ordering> {
    let current = parse_semver(current, "current app version")?;
    let candidate = parse_semver(candidate, "signed update version")?;
    Ok(candidate.cmp(&current))
}

/// Stable rollout bucket used by legacy and v2 update checks.
#[must_use]
pub fn rollout_bucket(installation_id: Uuid, version: &str) -> u32 {
    let mut bytes = Vec::with_capacity(installation_id.as_bytes().len() + 1 + version.len());
    bytes.extend_from_slice(installation_id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(version.as_bytes());
    crc32_ieee(&bytes) % 100
}

/// Returns rollout eligibility using the existing `< rollout_percent` invariant.
#[must_use]
pub fn rollout_eligible(
    installation_id: Uuid,
    version: &str,
    paused: bool,
    rollout_percent: u8,
) -> bool {
    !paused && rollout_bucket(installation_id, version) < u32::from(rollout_percent)
}

pub(crate) fn hex_sha256(bytes: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(HEX[usize::from(byte >> 4)] as char);
        output.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    output
}

// crc32fast uses IEEE CRC-32. Keeping this tiny implementation avoids adding a
// new crate while retaining the exact historical rollout algorithm.
fn crc32_ieee(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}
