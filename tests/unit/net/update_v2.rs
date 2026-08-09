// @author kongweiguang

use super::*;

#[cfg(feature = "updater-e2e")]
#[test]
fn updater_e2e_manifest_override_is_loopback_only() {
    assert_eq!(
        validate_updater_e2e_manifest_url("http://127.0.0.1:48123/update-manifest-v2.json")
            .unwrap(),
        "http://127.0.0.1:48123/update-manifest-v2.json"
    );
    assert!(validate_updater_e2e_manifest_url("https://example.com/update.json").is_err());
    assert!(validate_updater_e2e_manifest_url("http://user@localhost/update.json").is_err());
}
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer as _, SigningKey};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::io::Write as _;
use std::net::TcpListener;
use std::sync::Arc;

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[19; 32])
}

fn platform_format() -> ArtifactFormat {
    match std::env::consts::OS {
        "windows" => ArtifactFormat::WindowsSetupExe,
        "macos" => ArtifactFormat::MacosAppTarGz,
        "linux" => ArtifactFormat::LinuxAppImage,
        other => panic!("unsupported test platform {other}"),
    }
}

fn platform_system_trust() -> SystemTrust {
    if cfg!(target_os = "linux") {
        SystemTrust::NotApplicable
    } else {
        SystemTrust::Unsigned
    }
}

fn manifest(version: &str) -> Value {
    let artifact = current_artifact_key().unwrap();
    json!({
        "schema_version": 2,
        "channel": "stable",
        "version": version,
        "published_at": "2026-07-22T12:00:00Z",
        "notes": "Reliable updater",
        "paused": false,
        "rollout_percent": 100,
        "release_url": format!("https://github.com/kongweiguang/gmark/releases/tag/v{version}"),
        "artifacts": {
            artifact: {
                "url": format!("https://github.com/kongweiguang/gmark/releases/download/v{version}/artifact"),
                "size": 16,
                "sha256": "ab".repeat(32),
                "format": platform_format(),
                "system_trust": platform_system_trust()
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
fn signed_v2_manifest_selects_current_platform() {
    let key = signing_key();
    let envelope = signed_envelope(&manifest("0.2.0"), &key);
    let result = compare_signed_manifest_v2(
        "0.1.0",
        uuid::Uuid::nil(),
        &envelope,
        "fixture",
        &key.verifying_key(),
    )
    .unwrap();
    let CheckOutcome::Available(release) = result else {
        panic!("expected update");
    };
    assert_eq!(release.version, "0.2.0");
    assert_eq!(release.artifact_size, 16);
    assert_eq!(release.artifact_format, platform_format());
    assert_eq!(release.signed_envelope.as_ref(), envelope);
}

#[test]
fn v2_manifest_rejects_wrong_channel_time_and_signature() {
    let key = signing_key();
    for (field, value) in [
        ("channel", json!("beta")),
        ("published_at", json!("yesterday")),
    ] {
        let mut payload = manifest("0.2.0");
        payload[field] = value;
        let envelope = signed_envelope(&payload, &key);
        assert!(matches!(
            compare_signed_manifest_v2(
                "0.1.0",
                uuid::Uuid::nil(),
                &envelope,
                "fixture",
                &key.verifying_key()
            ),
            Err(UpdateV2Error::Manifest(_))
        ));
    }

    let envelope = signed_envelope(&manifest("0.2.0"), &key);
    assert!(matches!(
        verify_signed_manifest_v2(
            &envelope,
            &SigningKey::from_bytes(&[20; 32]).verifying_key()
        ),
        Err(UpdateV2Error::Signature(_))
    ));
}

#[test]
fn content_range_parser_is_strict_about_unit_and_start() {
    assert_eq!(
        parse_content_range_start("bytes 4096-8191/16384"),
        Some(4096)
    );
    assert_eq!(parse_content_range_start("items 1-2/3"), None);
    assert_eq!(parse_content_range_start("bytes */16384"), None);
}

#[test]
fn atomic_metadata_commit_replaces_an_existing_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("partial.json");
    write_partial_metadata(
        &path,
        &gmark_update_core::PartialMetadata {
            etag: Some("old".to_owned()),
            last_modified: None,
        },
    )
    .unwrap();
    write_partial_metadata(
        &path,
        &gmark_update_core::PartialMetadata {
            etag: Some("new".to_owned()),
            last_modified: None,
        },
    )
    .unwrap();
    assert_eq!(
        read_partial_metadata(&path).unwrap().etag.as_deref(),
        Some("new")
    );
}

#[test]
fn oversized_cached_envelope_is_rejected_before_manifest_parsing() {
    let root = tempfile::tempdir().unwrap();
    let envelope = root.path().join("manifest.envelope.json");
    std::fs::write(
        &envelope,
        vec![b'x'; gmark_update_core::MAX_ENVELOPE_BYTES + 1],
    )
    .unwrap();

    assert!(read_cached_envelope(&envelope).is_err());
}

#[test]
fn verified_ready_file_requires_exact_size_and_hash() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("artifact");
    std::fs::write(&path, b"verified updater").unwrap();
    let release = UpdateRelease {
        current_version: "0.1.0".into(),
        version: "0.2.0".into(),
        published_at: "2026-07-22T12:00:00Z".into(),
        notes: String::new(),
        release_url: "https://github.com/kongweiguang/gmark/releases/tag/v0.2.0".into(),
        artifact_url: "https://github.com/kongweiguang/gmark/releases/download/v0.2.0/artifact"
            .into(),
        artifact_size: 16,
        artifact_sha256: "72b322780b6f8f966c1c24a2a05e383e8ae3beba4d34bd02d4cf616c1d9395d0".into(),
        artifact_format: platform_format(),
        system_trust: SystemTrust::Unsigned,
        signed_envelope: Arc::from([]),
    };
    assert!(verify_file(&path, &release).unwrap());
    std::fs::write(&path, b"tampered updater").unwrap();
    assert!(!verify_file(&path, &release).unwrap());
}

fn download_fixture_release(url: String, payload: &[u8]) -> UpdateRelease {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    UpdateRelease {
        current_version: "0.1.0".into(),
        version: "0.2.0".into(),
        published_at: "2026-07-22T12:00:00Z".into(),
        notes: String::new(),
        release_url: "https://github.com/kongweiguang/gmark/releases/tag/v0.2.0".into(),
        artifact_url: url,
        artifact_size: payload.len() as u64,
        artifact_sha256: format!("{:x}", hasher.finalize()),
        artifact_format: platform_format(),
        system_trust: SystemTrust::Unsigned,
        signed_envelope: Arc::from(b"signed-envelope".as_slice()),
    }
}

#[test]
fn resumable_download_requests_the_missing_range_and_reports_monotonic_progress() {
    let payload = b"production updater payload".to_vec();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let tail = payload[7..].to_vec();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        assert!(request.contains("Range: bytes=7-") || request.contains("range: bytes=7-"));
        let response = format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes 7-{}/{}\r\nETag: \"fixture\"\r\nConnection: close\r\n\r\n",
            tail.len(),
            7 + tail.len() - 1,
            7 + tail.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.write_all(&tail).unwrap();
    });

    let root = tempfile::tempdir().unwrap();
    let version_dir = root.path().join("v0.2.0");
    std::fs::create_dir_all(&version_dir).unwrap();
    std::fs::write(version_dir.join("artifact.part"), &payload[..7]).unwrap();
    std::fs::write(
        version_dir.join("partial.json"),
        br#"{"etag":"\"fixture\"","last_modified":null}"#,
    )
    .unwrap();
    let release = download_fixture_release(format!("http://{address}/artifact"), &payload);
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let mut observed = Vec::new();
    let ready = download_release_with_client(
        &release,
        root.path(),
        &DownloadControl::default(),
        &client,
        true,
        |event| match event {
            DownloadEvent::Started { downloaded, .. }
            | DownloadEvent::Progress { downloaded, .. } => observed.push(downloaded),
            _ => {}
        },
    )
    .unwrap();
    server.join().unwrap();
    assert_eq!(std::fs::read(ready).unwrap(), payload);
    assert_eq!(observed.first().copied(), Some(7));
    assert_eq!(observed.last().copied(), Some(release.artifact_size));
    assert!(observed.windows(2).all(|pair| pair[0] <= pair[1]));
}
