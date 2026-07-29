// @author kongweiguang

//! Signed update-envelope parsing while preserving the signed payload bytes.

use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::{
    Result, UpdateCoreError,
    policy::{MAX_ENVELOPE_BYTES, MAX_PAYLOAD_BYTES},
};

/// The versioned JSON envelope shared by v1 and v2 update manifests.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedEnvelopeV1 {
    pub schema_version: u8,
    pub algorithm: String,
    pub payload: String,
    pub signature: String,
}

/// A verified envelope together with byte-for-byte source data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedEnvelope {
    /// The original envelope is retained for helper re-verification.
    pub raw_envelope: Arc<[u8]>,
    /// Verification is always over these original decoded bytes, never JSON reserialization.
    pub raw_payload: Arc<[u8]>,
}

/// Parses a base64 Ed25519 public key used by a production update channel.
pub fn verifying_key_from_base64(encoded: &str) -> Result<VerifyingKey> {
    let bytes = STANDARD.decode(encoded).map_err(|error| {
        UpdateCoreError::Configuration(format!("invalid update public key base64: {error}"))
    })?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
        UpdateCoreError::Configuration(format!(
            "update public key must be 32 bytes, got {}",
            bytes.len()
        ))
    })?;
    VerifyingKey::from_bytes(&bytes).map_err(|error| {
        UpdateCoreError::Configuration(format!("invalid Ed25519 update public key: {error}"))
    })
}

/// Strictly parses and verifies an Ed25519 update envelope.
pub fn parse_and_verify_envelope(
    envelope_bytes: &[u8],
    key: &VerifyingKey,
) -> Result<VerifiedEnvelope> {
    if envelope_bytes.is_empty() {
        return Err(UpdateCoreError::Envelope(
            "signed update envelope is empty".to_owned(),
        ));
    }
    if envelope_bytes.len() > MAX_ENVELOPE_BYTES {
        return Err(UpdateCoreError::Envelope(format!(
            "signed update envelope exceeds {MAX_ENVELOPE_BYTES} bytes"
        )));
    }

    let envelope: SignedEnvelopeV1 = serde_json::from_slice(envelope_bytes).map_err(|error| {
        UpdateCoreError::Envelope(format!("invalid signed update envelope: {error}"))
    })?;
    if envelope.schema_version != 1 {
        return Err(UpdateCoreError::Envelope(format!(
            "unsupported update envelope schema {}",
            envelope.schema_version
        )));
    }
    if envelope.algorithm != "Ed25519" {
        return Err(UpdateCoreError::Envelope(format!(
            "unsupported update signature algorithm '{}'",
            envelope.algorithm
        )));
    }

    let payload = STANDARD.decode(envelope.payload).map_err(|error| {
        UpdateCoreError::Envelope(format!("invalid update payload base64: {error}"))
    })?;
    if payload.is_empty() || payload.len() > MAX_PAYLOAD_BYTES {
        return Err(UpdateCoreError::Envelope(format!(
            "signed update payload exceeds {MAX_PAYLOAD_BYTES} bytes"
        )));
    }
    let signature = STANDARD.decode(envelope.signature).map_err(|error| {
        UpdateCoreError::Envelope(format!("invalid update signature base64: {error}"))
    })?;
    let signature = Signature::from_slice(&signature).map_err(|error| {
        UpdateCoreError::Signature(format!("invalid Ed25519 signature bytes: {error}"))
    })?;
    key.verify_strict(&payload, &signature).map_err(|_| {
        UpdateCoreError::Signature("update manifest signature verification failed".to_owned())
    })?;

    Ok(VerifiedEnvelope {
        raw_envelope: Arc::from(envelope_bytes.to_vec()),
        raw_payload: Arc::from(payload),
    })
}
