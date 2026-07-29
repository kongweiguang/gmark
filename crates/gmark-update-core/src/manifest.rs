// @author kongweiguang

//! Versioned signed-manifest contracts and pure update selection.

use std::{collections::BTreeMap, sync::Arc};

use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Result, UpdateCoreError,
    envelope::{VerifiedEnvelope, parse_and_verify_envelope},
    policy::{
        ArtifactFormat, MAX_ARTIFACT_BYTES, Platform, SystemTrust, compare_versions,
        is_rfc3339_utc, rollout_eligible, validate_official_release_url, validate_platform_format,
        validate_sha256, validate_system_trust,
    },
};

/// Legacy manifest artifact metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactV1 {
    pub url: String,
    pub sha256: String,
}

/// v1 signed-manifest payload. Field names intentionally match the existing JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestV1 {
    pub schema_version: u8,
    pub version: String,
    pub published_at: String,
    pub paused: bool,
    pub rollout_percent: u8,
    pub release_url: String,
    pub artifacts: BTreeMap<String, ArtifactV1>,
}

/// v2 artifact metadata used for resumable downloads and helper validation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactV2 {
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub format: ArtifactFormat,
    pub system_trust: SystemTrust,
}

/// v2 signed-manifest payload. Field names and schema remain wire-compatible.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestV2 {
    pub schema_version: u8,
    pub channel: String,
    pub version: String,
    pub published_at: String,
    pub notes: String,
    pub paused: bool,
    pub rollout_percent: u8,
    pub release_url: String,
    pub artifacts: BTreeMap<String, ArtifactV2>,
}

/// Parsed signed payload without losing which historical schema produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignedManifest {
    V1(ManifestV1),
    V2(ManifestV2),
}

/// A manifest whose envelope signature and schema have both been verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedManifest {
    pub envelope: VerifiedEnvelope,
    pub manifest: SignedManifest,
}

/// Platform artifact selected from either manifest schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedArtifact {
    pub key: String,
    pub url: String,
    pub sha256: String,
    pub size: Option<u64>,
    pub format: Option<ArtifactFormat>,
    pub system_trust: Option<SystemTrust>,
}

/// Domain release supplied to download and helper adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateRelease {
    pub current_version: String,
    pub version: String,
    pub published_at: String,
    pub notes: String,
    pub release_url: String,
    pub artifact: SelectedArtifact,
    /// The helper must reverify these original bytes, not a reconstructed envelope.
    pub signed_envelope: Arc<[u8]>,
}

/// Result of an update check after signature, policy, and platform validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateCheckOutcome {
    Available(UpdateRelease),
    UpToDate {
        current_version: String,
        latest_version: String,
    },
}

#[derive(Deserialize)]
struct SchemaMarker {
    schema_version: u8,
}

/// Verifies an envelope, parses its v1/v2 payload, and validates all common metadata.
pub fn parse_verified_manifest(
    envelope_bytes: &[u8],
    key: &VerifyingKey,
) -> Result<VerifiedManifest> {
    let envelope = parse_and_verify_envelope(envelope_bytes, key)?;
    let marker: SchemaMarker = serde_json::from_slice(&envelope.raw_payload).map_err(|error| {
        UpdateCoreError::Manifest(format!("invalid signed update manifest: {error}"))
    })?;
    let manifest = match marker.schema_version {
        1 => SignedManifest::V1(serde_json::from_slice(&envelope.raw_payload).map_err(
            |error| UpdateCoreError::Manifest(format!("invalid signed update manifest: {error}")),
        )?),
        2 => SignedManifest::V2(serde_json::from_slice(&envelope.raw_payload).map_err(
            |error| {
                UpdateCoreError::Manifest(format!("invalid signed v2 update manifest: {error}"))
            },
        )?),
        schema => {
            return Err(UpdateCoreError::Manifest(format!(
                "unsupported update manifest schema {schema}"
            )));
        }
    };
    validate_manifest(&manifest)?;
    Ok(VerifiedManifest { envelope, manifest })
}

/// Selects and validates the artifact for a concrete adapter platform.
pub fn select_artifact(manifest: &SignedManifest, platform: &Platform) -> Result<SelectedArtifact> {
    match manifest {
        SignedManifest::V1(manifest) => {
            let key = platform.artifact_key_v1().ok_or_else(|| {
                UpdateCoreError::Manifest("this platform has no update artifact mapping".to_owned())
            })?;
            let artifact = manifest.artifacts.get(key).ok_or_else(|| {
                UpdateCoreError::Manifest(format!("signed update manifest has no '{key}' artifact"))
            })?;
            validate_official_release_url(&artifact.url, &format!("artifact '{key}' URL"))?;
            validate_sha256(&artifact.sha256, &format!("artifact '{key}'"))?;
            Ok(SelectedArtifact {
                key: key.to_owned(),
                url: artifact.url.clone(),
                sha256: artifact.sha256.to_ascii_lowercase(),
                size: None,
                format: None,
                system_trust: None,
            })
        }
        SignedManifest::V2(manifest) => {
            let key = platform.artifact_key_v2().ok_or_else(|| {
                UpdateCoreError::Manifest(
                    "this platform has no updater artifact mapping".to_owned(),
                )
            })?;
            let artifact = manifest.artifacts.get(key).ok_or_else(|| {
                UpdateCoreError::Manifest(format!("signed update manifest has no '{key}' artifact"))
            })?;
            validate_v2_artifact(key, artifact)?;
            validate_platform_format(artifact.format, platform)?;
            validate_system_trust(artifact.system_trust, platform)?;
            Ok(SelectedArtifact {
                key: key.to_owned(),
                url: artifact.url.clone(),
                sha256: artifact.sha256.to_ascii_lowercase(),
                size: Some(artifact.size),
                format: Some(artifact.format),
                system_trust: Some(artifact.system_trust),
            })
        }
    }
}

/// Applies SemVer and rollout policy after the signed manifest has been checked.
pub fn evaluate_update(
    verified: &VerifiedManifest,
    current_version: &str,
    installation_id: Uuid,
    platform: &Platform,
) -> Result<UpdateCheckOutcome> {
    let ordering = compare_versions(current_version, manifest_version(&verified.manifest))?;
    let artifact = select_artifact(&verified.manifest, platform)?;
    let (version, published_at, notes, paused, rollout_percent, release_url) =
        manifest_fields(&verified.manifest);
    let eligible = rollout_eligible(installation_id, version, paused, rollout_percent);

    if ordering != std::cmp::Ordering::Greater || !eligible {
        return Ok(UpdateCheckOutcome::UpToDate {
            current_version: current_version.to_owned(),
            // A paused or withheld release must not disclose its newer version to the UI.
            latest_version: if ordering == std::cmp::Ordering::Greater {
                current_version.to_owned()
            } else {
                version.to_owned()
            },
        });
    }

    Ok(UpdateCheckOutcome::Available(UpdateRelease {
        current_version: current_version.to_owned(),
        version: version.to_owned(),
        published_at: published_at.to_owned(),
        notes: notes.to_owned(),
        release_url: release_url.to_owned(),
        artifact,
        signed_envelope: Arc::clone(&verified.envelope.raw_envelope),
    }))
}

fn validate_manifest(manifest: &SignedManifest) -> Result<()> {
    match manifest {
        SignedManifest::V1(manifest) => validate_manifest_v1(manifest),
        SignedManifest::V2(manifest) => validate_manifest_v2(manifest),
    }
}

fn validate_manifest_v1(manifest: &ManifestV1) -> Result<()> {
    if manifest.schema_version != 1 {
        return Err(UpdateCoreError::Manifest(format!(
            "unsupported update manifest schema {}",
            manifest.schema_version
        )));
    }
    if manifest.published_at.trim().is_empty() {
        return Err(UpdateCoreError::Manifest(
            "signed update manifest has no publication time".to_owned(),
        ));
    }
    validate_rollout_percent(manifest.rollout_percent)?;
    validate_official_release_url(&manifest.release_url, "release_url")?;
    if manifest.artifacts.is_empty() {
        return Err(UpdateCoreError::Manifest(
            "signed update manifest contains no artifacts".to_owned(),
        ));
    }
    for (key, artifact) in &manifest.artifacts {
        validate_official_release_url(&artifact.url, &format!("artifact '{key}' URL"))?;
        validate_sha256(&artifact.sha256, &format!("artifact '{key}'"))?;
    }
    Ok(())
}

fn validate_manifest_v2(manifest: &ManifestV2) -> Result<()> {
    if manifest.schema_version != 2 {
        return Err(UpdateCoreError::Manifest(format!(
            "unsupported update manifest schema {}",
            manifest.schema_version
        )));
    }
    if manifest.channel != "stable" {
        return Err(UpdateCoreError::Manifest(
            "update manifest channel must be stable".to_owned(),
        ));
    }
    if !is_rfc3339_utc(&manifest.published_at) {
        return Err(UpdateCoreError::Manifest(
            "update manifest publication time must be RFC3339 UTC".to_owned(),
        ));
    }
    if manifest.notes.len() > 32 * 1024 {
        return Err(UpdateCoreError::Manifest(
            "update release notes exceed the size limit".to_owned(),
        ));
    }
    validate_rollout_percent(manifest.rollout_percent)?;
    validate_official_release_url(&manifest.release_url, "release URL")?;
    if manifest.artifacts.is_empty() {
        return Err(UpdateCoreError::Manifest(
            "signed update manifest contains no artifacts".to_owned(),
        ));
    }
    for (key, artifact) in &manifest.artifacts {
        validate_v2_artifact(key, artifact)?;
    }
    Ok(())
}

fn validate_v2_artifact(key: &str, artifact: &ArtifactV2) -> Result<()> {
    validate_official_release_url(&artifact.url, &format!("artifact '{key}' URL"))?;
    if artifact.size == 0 || artifact.size > MAX_ARTIFACT_BYTES {
        return Err(UpdateCoreError::Manifest(format!(
            "artifact '{key}' has an invalid size"
        )));
    }
    validate_sha256(&artifact.sha256, &format!("artifact '{key}'"))
}

fn validate_rollout_percent(value: u8) -> Result<()> {
    if value > 100 {
        return Err(UpdateCoreError::Manifest(format!(
            "rollout_percent {value} exceeds 100"
        )));
    }
    Ok(())
}

fn manifest_version(manifest: &SignedManifest) -> &str {
    match manifest {
        SignedManifest::V1(manifest) => &manifest.version,
        SignedManifest::V2(manifest) => &manifest.version,
    }
}

fn manifest_fields(manifest: &SignedManifest) -> (&str, &str, &str, bool, u8, &str) {
    match manifest {
        SignedManifest::V1(manifest) => (
            &manifest.version,
            &manifest.published_at,
            "",
            manifest.paused,
            manifest.rollout_percent,
            &manifest.release_url,
        ),
        SignedManifest::V2(manifest) => (
            &manifest.version,
            &manifest.published_at,
            &manifest.notes,
            manifest.paused,
            manifest.rollout_percent,
            &manifest.release_url,
        ),
    }
}
