// @author kongweiguang

//! Update domain errors. Adapters may attach transport or UI context without
//! coupling this crate to a particular runtime.

use std::fmt;

/// Result type used by the update domain APIs.
pub type Result<T> = std::result::Result<T, UpdateCoreError>;

/// Stable error categories for update protocol boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateCoreError {
    Configuration(String),
    Envelope(String),
    Signature(String),
    Manifest(String),
    Policy(String),
    Download(String),
    Io(String),
    Protocol(String),
    TooLarge,
    Truncated { expected: u64, actual: u64 },
    HashMismatch { expected: String, actual: String },
}

impl fmt::Display for UpdateCoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message)
            | Self::Envelope(message)
            | Self::Signature(message)
            | Self::Manifest(message)
            | Self::Policy(message)
            | Self::Download(message)
            | Self::Io(message)
            | Self::Protocol(message) => formatter.write_str(message),
            Self::TooLarge => formatter.write_str("update artifact exceeds its size limit"),
            Self::Truncated { expected, actual } => {
                write!(
                    formatter,
                    "update artifact is truncated: expected {expected} bytes, got {actual}"
                )
            }
            Self::HashMismatch { expected, actual } => {
                write!(
                    formatter,
                    "update artifact hash mismatch: expected {expected}, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for UpdateCoreError {}
