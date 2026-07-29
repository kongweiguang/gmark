// @author kongweiguang

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer as _, SigningKey};
use gmark_update_core::{
    ArtifactFormat, MAX_ENVELOPE_BYTES, Platform, SystemTrust, UpdateCheckOutcome, UpdateCoreError,
    evaluate_update, parse_verified_manifest, rollout_bucket, rollout_eligible, select_artifact,
};
use serde_json::{Value, json};
use uuid::Uuid;

const RELEASE_URL: &str = "https://github.com/kongweiguang/gmark/releases/tag/v0.2.0";
const ARTIFACT_URL: &str =
    "https://github.com/kongweiguang/gmark/releases/download/v0.2.0/gmark.AppImage";
const SHA256: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn signed_envelope(payload: Vec<u8>, key: &SigningKey) -> Vec<u8> {
    let signature = key.sign(&payload);
    serde_json::to_vec(&json!({
        "schema_version": 1,
        "algorithm": "Ed25519",
        "payload": STANDARD.encode(payload),
        "signature": STANDARD.encode(signature.to_bytes()),
    }))
    .unwrap()
}

fn v2_payload(paused: bool, release_url: &str, extra_field: bool) -> Vec<u8> {
    let mut payload = json!({
        "schema_version": 2,
        "channel": "stable",
        "version": "0.2.0",
        "published_at": "2026-07-22T12:00:00Z",
        "notes": "fixture",
        "paused": paused,
        "rollout_percent": 100,
        "release_url": release_url,
        "artifacts": {
            "linux-x86_64": {
                "url": ARTIFACT_URL,
                "size": 9,
                "sha256": SHA256,
                "format": ArtifactFormat::LinuxAppImage,
                "system_trust": SystemTrust::NotApplicable,
            }
        }
    });
    if extra_field {
        payload["unknown"] = json!(true);
    }
    serde_json::to_vec(&payload).unwrap()
}

#[test]
fn verifies_v1_and_v2_without_reserializing_the_signed_payload() {
    let key = signing_key(7);
    let v1_payload = serde_json::to_vec(&json!({
        "schema_version": 1,
        "version": "0.2.0",
        "published_at": "2026-07-22T12:00:00Z",
        "paused": false,
        "rollout_percent": 100,
        "release_url": RELEASE_URL,
        "artifacts": {
            "linux-x86_64": { "url": ARTIFACT_URL, "sha256": SHA256 }
        }
    }))
    .unwrap();
    let v1 = parse_verified_manifest(
        &signed_envelope(v1_payload.clone(), &key),
        &key.verifying_key(),
    )
    .unwrap();
    assert_eq!(v1.envelope.raw_payload.as_ref(), v1_payload.as_slice());

    let v2_payload = v2_payload(false, RELEASE_URL, false);
    let v2 = parse_verified_manifest(
        &signed_envelope(v2_payload.clone(), &key),
        &key.verifying_key(),
    )
    .unwrap();
    assert_eq!(v2.envelope.raw_payload.as_ref(), v2_payload.as_slice());
    let outcome = evaluate_update(
        &v2,
        "0.1.0",
        Uuid::from_u128(0x2a),
        &Platform::new("linux", "x86_64"),
    )
    .unwrap();
    assert!(matches!(outcome, UpdateCheckOutcome::Available(_)));
}

#[test]
fn rejects_tampering_and_a_wrong_verification_key() {
    let signer = signing_key(11);
    let payload = v2_payload(false, RELEASE_URL, false);
    let envelope = signed_envelope(payload, &signer);
    let wrong_key = signing_key(12).verifying_key();
    assert!(matches!(
        parse_verified_manifest(&envelope, &wrong_key),
        Err(UpdateCoreError::Signature(_))
    ));

    let mut tampered: Value = serde_json::from_slice(&envelope).unwrap();
    tampered["payload"] = json!(STANDARD.encode(br#"{\"schema_version\":2}"#));
    let tampered = serde_json::to_vec(&tampered).unwrap();
    assert!(matches!(
        parse_verified_manifest(&tampered, &signer.verifying_key()),
        Err(UpdateCoreError::Signature(_))
    ));
}

#[test]
fn rejects_unknown_fields_and_oversized_envelopes() {
    let key = signing_key(13);
    let unknown_payload = signed_envelope(v2_payload(false, RELEASE_URL, true), &key);
    assert!(matches!(
        parse_verified_manifest(&unknown_payload, &key.verifying_key()),
        Err(UpdateCoreError::Manifest(_))
    ));

    let oversized = vec![b' '; MAX_ENVELOPE_BYTES + 1];
    assert!(matches!(
        parse_verified_manifest(&oversized, &key.verifying_key()),
        Err(UpdateCoreError::Envelope(_))
    ));
}

#[test]
fn rejects_unsafe_urls_and_missing_platform_artifacts() {
    let key = signing_key(17);
    let unsafe_envelope = signed_envelope(
        v2_payload(
            false,
            "http://github.com/kongweiguang/gmark/releases/tag/v0.2.0",
            false,
        ),
        &key,
    );
    assert!(matches!(
        parse_verified_manifest(&unsafe_envelope, &key.verifying_key()),
        Err(UpdateCoreError::Manifest(_))
    ));

    let verified = parse_verified_manifest(
        &signed_envelope(v2_payload(false, RELEASE_URL, false), &key),
        &key.verifying_key(),
    )
    .unwrap();
    assert!(matches!(
        select_artifact(&verified.manifest, &Platform::new("plan9", "amd64")),
        Err(UpdateCoreError::Manifest(_))
    ));
}

#[test]
fn paused_rollout_hides_a_newer_release() {
    let key = signing_key(19);
    let verified = parse_verified_manifest(
        &signed_envelope(v2_payload(true, RELEASE_URL, false), &key),
        &key.verifying_key(),
    )
    .unwrap();
    let outcome = evaluate_update(
        &verified,
        "0.1.0",
        Uuid::from_u128(99),
        &Platform::new("linux", "x86_64"),
    )
    .unwrap();
    assert_eq!(
        outcome,
        UpdateCheckOutcome::UpToDate {
            current_version: "0.1.0".to_owned(),
            latest_version: "0.1.0".to_owned(),
        }
    );
}

#[test]
fn rollout_bucket_matches_the_legacy_crc32_contract() {
    let installation_id = Uuid::nil();
    assert_eq!(rollout_bucket(installation_id, "1.2.3"), 47);
    assert!(rollout_eligible(installation_id, "1.2.3", false, 48));
    assert!(!rollout_eligible(installation_id, "1.2.3", false, 47));
    assert!(!rollout_eligible(installation_id, "1.2.3", true, 100));
}

#[test]
fn selected_v2_artifact_requires_matching_format_and_system_trust() {
    let key = signing_key(23);
    let mut payload: Value =
        serde_json::from_slice(&v2_payload(false, RELEASE_URL, false)).unwrap();
    payload["artifacts"]["linux-x86_64"]["format"] = json!("windows-setup-exe");
    let wrong_format = parse_verified_manifest(
        &signed_envelope(serde_json::to_vec(&payload).unwrap(), &key),
        &key.verifying_key(),
    )
    .unwrap();
    assert!(matches!(
        select_artifact(&wrong_format.manifest, &Platform::new("linux", "x86_64")),
        Err(UpdateCoreError::Manifest(_))
    ));

    payload["artifacts"]["linux-x86_64"]["format"] = json!("linux-app-image");
    payload["artifacts"]["linux-x86_64"]["system_trust"] = json!("unsigned");
    let wrong_trust = parse_verified_manifest(
        &signed_envelope(serde_json::to_vec(&payload).unwrap(), &key),
        &key.verifying_key(),
    )
    .unwrap();
    assert!(matches!(
        select_artifact(&wrong_trust.manifest, &Platform::new("linux", "x86_64")),
        Err(UpdateCoreError::Manifest(_))
    ));
}
