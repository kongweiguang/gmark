// @author kongweiguang

use super::*;
use ed25519_dalek::{Signer as _, SigningKey};
use serde_json::{Value, json};
use std::cell::RefCell;

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn manifest(version: &str) -> Value {
    let artifact = current_artifact_key().unwrap();
    json!({
        "schema_version": 1,
        "version": version,
        "published_at": "2026-07-16T00:00:00Z",
        "paused": false,
        "rollout_percent": 100,
        "release_url": format!("https://github.com/kongweiguang/gmark/releases/tag/v{version}"),
        "artifacts": {
            artifact: {
                "url": format!("https://github.com/kongweiguang/gmark/releases/download/v{version}/gmark.zip"),
                "sha256": "ab".repeat(32)
            }
        }
    })
}

fn signed_envelope(payload: &Value, key: &SigningKey) -> Vec<u8> {
    let payload = serde_json::to_vec(payload).unwrap();
    let signature = key.sign(&payload);
    serde_json::to_vec(&json!({
        "schema_version": 1,
        "algorithm": "Ed25519",
        "payload": BASE64.encode(payload),
        "signature": BASE64.encode(signature.to_bytes())
    }))
    .unwrap()
}

#[test]
fn verifies_signed_manifest_before_reporting_update() {
    let key = signing_key(7);
    let envelope = signed_envelope(&manifest("0.2.0"), &key);
    let result =
        check_latest_version_with("0.1.0", uuid::Uuid::nil(), &key.verifying_key(), |_| {
            Ok(envelope.clone())
        })
        .unwrap();

    match result {
        UpdateCheckResult::UpdateAvailable(info) => {
            assert_eq!(info.latest_version, "0.2.0");
            assert!(info.release_url.ends_with("/tag/v0.2.0"));
            assert!(info.artifact_url.ends_with("/gmark.zip"));
            assert_eq!(info.artifact_sha256, "ab".repeat(32));
        }
        UpdateCheckResult::UpToDate(_) => panic!("expected signed update"),
    }
}

#[test]
fn rejects_tampered_payload_wrong_key_and_signature() {
    let key = signing_key(7);
    let envelope = signed_envelope(&manifest("0.2.0"), &key);
    assert!(matches!(
        verify_signed_manifest(&envelope, &signing_key(8).verifying_key()),
        Err(UpdateCheckError::Signature(_))
    ));

    let mut parsed: Value = serde_json::from_slice(&envelope).unwrap();
    parsed["payload"] = Value::String(BASE64.encode(b"{}"));
    assert!(matches!(
        verify_signed_manifest(&serde_json::to_vec(&parsed).unwrap(), &key.verifying_key()),
        Err(UpdateCheckError::Signature(_))
    ));

    parsed["signature"] = Value::String(BASE64.encode([0_u8; 64]));
    assert!(matches!(
        verify_signed_manifest(&serde_json::to_vec(&parsed).unwrap(), &key.verifying_key()),
        Err(UpdateCheckError::Signature(_))
    ));
}

#[test]
fn rejects_oversized_unknown_or_unsafe_signed_manifests() {
    let key = signing_key(7);
    assert!(matches!(
        verify_signed_manifest(&vec![b' '; MAX_ENVELOPE_BYTES + 1], &key.verifying_key()),
        Err(UpdateCheckError::Envelope(_))
    ));

    for mutation in [
        ("schema_version", json!(2)),
        ("release_url", json!("http://evil.example/update")),
        ("rollout_percent", json!(101)),
    ] {
        let mut payload = manifest("0.2.0");
        payload[mutation.0] = mutation.1;
        let envelope = signed_envelope(&payload, &key);
        let error = compare_signed_manifest(
            "0.1.0",
            uuid::Uuid::nil(),
            &envelope,
            UpdateSource::GitHub,
            &key.verifying_key(),
        )
        .unwrap_err();
        assert!(matches!(error, UpdateCheckError::Manifest(_)));
    }

    let artifact = current_artifact_key().unwrap();
    let mut invalid_hash = manifest("0.2.0");
    invalid_hash["artifacts"][artifact]["sha256"] = json!("not-a-sha256");
    let envelope = signed_envelope(&invalid_hash, &key);
    assert!(matches!(
        compare_signed_manifest(
            "0.1.0",
            uuid::Uuid::nil(),
            &envelope,
            UpdateSource::GitHub,
            &key.verifying_key(),
        ),
        Err(UpdateCheckError::Manifest(_))
    ));

    let mut missing_platform = manifest("0.2.0");
    missing_platform["artifacts"]
        .as_object_mut()
        .unwrap()
        .remove(artifact);
    let envelope = signed_envelope(&missing_platform, &key);
    assert!(matches!(
        compare_signed_manifest(
            "0.1.0",
            uuid::Uuid::nil(),
            &envelope,
            UpdateSource::GitHub,
            &key.verifying_key(),
        ),
        Err(UpdateCheckError::Manifest(_))
    ));
}

#[test]
fn paused_or_zero_percent_rollout_does_not_expose_update() {
    let key = signing_key(7);
    for (field, value) in [("paused", json!(true)), ("rollout_percent", json!(0))] {
        let mut payload = manifest("0.2.0");
        payload[field] = value;
        let envelope = signed_envelope(&payload, &key);
        let result = compare_signed_manifest(
            "0.1.0",
            uuid::Uuid::nil(),
            &envelope,
            UpdateSource::GitHub,
            &key.verifying_key(),
        )
        .unwrap();
        match result {
            UpdateCheckResult::UpToDate(info) => assert_eq!(info.latest_version, "0.1.0"),
            UpdateCheckResult::UpdateAvailable(_) => panic!("rollout must be deferred"),
        }
    }
}

#[test]
fn falls_back_to_gitee_only_after_timeout_and_reverifies() {
    let key = signing_key(7);
    let envelope = signed_envelope(&manifest("0.2.0"), &key);
    let calls = RefCell::new(Vec::new());
    let result =
        check_latest_version_with("0.1.0", uuid::Uuid::nil(), &key.verifying_key(), |source| {
            calls.borrow_mut().push(source);
            match source {
                UpdateSource::GitHub => Err(RemoteFetchFailure::timeout(source, "timeout")),
                UpdateSource::Gitee => Ok(envelope.clone()),
            }
        })
        .unwrap();
    assert_eq!(
        calls.into_inner(),
        vec![UpdateSource::GitHub, UpdateSource::Gitee]
    );
    assert!(matches!(result, UpdateCheckResult::UpdateAvailable(_)));
}

#[test]
fn does_not_fallback_after_http_or_signature_failure() {
    let key = signing_key(7);
    let calls = RefCell::new(Vec::new());
    let error =
        check_latest_version_with("0.1.0", uuid::Uuid::nil(), &key.verifying_key(), |source| {
            calls.borrow_mut().push(source);
            Err(RemoteFetchFailure::new(
                source,
                RemoteFetchFailureKind::HttpStatus,
                "HTTP 404",
            ))
        })
        .unwrap_err();
    assert!(matches!(error, UpdateCheckError::Fetch(_)));
    assert_eq!(calls.into_inner(), vec![UpdateSource::GitHub]);

    let invalid = signed_envelope(&manifest("0.2.0"), &signing_key(9));
    let calls = RefCell::new(Vec::new());
    assert!(matches!(
        check_latest_version_with("0.1.0", uuid::Uuid::nil(), &key.verifying_key(), |source| {
            calls.borrow_mut().push(source);
            Ok(invalid.clone())
        }),
        Err(UpdateCheckError::Signature(_))
    ));
    assert_eq!(calls.into_inner(), vec![UpdateSource::GitHub]);
}

#[test]
fn public_key_parser_requires_exact_ed25519_length() {
    let key = signing_key(7).verifying_key();
    assert_eq!(
        verifying_key_from_base64(&BASE64.encode(key.to_bytes())).unwrap(),
        key
    );
    assert!(verifying_key_from_base64(&BASE64.encode([0_u8; 31])).is_err());
    assert!(verifying_key_from_base64("not base64").is_err());
}

#[test]
fn sha256_matches_standard_vectors_across_chunk_boundaries() {
    let mut empty = Sha256::new();
    empty.update(b"");
    assert_eq!(
        hex_sha256(empty.finalize().into()),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );

    let mut abc = Sha256::new();
    abc.update(b"a");
    abc.update(b"bc");
    assert_eq!(
        hex_sha256(abc.finalize().into()),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn verified_copy_rejects_oversize_and_hash_mismatch() {
    let payload = b"verified installer";
    let expected = "2b3259d45e6f60a32d17ab3a9f417da8c68726318ef5dd738e837a51d5a91586";
    let mut output = Vec::new();
    assert_eq!(
        copy_and_verify(&payload[..], &mut output, payload.len() as u64, expected).unwrap(),
        payload.len() as u64
    );
    assert_eq!(output, payload);

    assert!(matches!(
        copy_and_verify(&payload[..], &mut Vec::new(), 4, expected),
        Err(UpdateInstallError::TooLarge)
    ));
    assert!(matches!(
        copy_and_verify(&payload[..], &mut Vec::new(), 100, &"00".repeat(32)),
        Err(UpdateInstallError::HashMismatch { .. })
    ));
}
