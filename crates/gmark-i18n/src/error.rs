// @author kongweiguang

use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::str::Utf8Error;

/// Result type returned by the pure i18n domain APIs.
pub type Result<T> = std::result::Result<T, I18nError>;

/// The on-disk encoding used by a language-pack file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanguagePackFormat {
    Json,
    Jsonc,
}

impl LanguagePackFormat {
    pub(crate) fn from_path(path: &std::path::Path) -> Result<Self> {
        let extension = path.extension().and_then(|extension| extension.to_str());
        match extension {
            Some(extension) if extension.eq_ignore_ascii_case("json") => Ok(Self::Json),
            Some(extension) if extension.eq_ignore_ascii_case("jsonc") => Ok(Self::Jsonc),
            _ => Err(I18nError::UnsupportedFileExtension {
                path: path.to_path_buf(),
            }),
        }
    }
}

impl fmt::Display for LanguagePackFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json => formatter.write_str("JSON"),
            Self::Jsonc => formatter.write_str("JSONC"),
        }
    }
}

/// Classified failures at file, syntax, validation, and catalog boundaries.
#[derive(Debug)]
pub enum I18nError {
    UnsupportedFileExtension {
        path: PathBuf,
    },
    NotRegularFile {
        path: PathBuf,
    },
    FileTooLarge {
        path: Option<PathBuf>,
        limit_bytes: usize,
        actual_bytes: usize,
    },
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    InvalidUtf8 {
        path: Option<PathBuf>,
        source: Utf8Error,
    },
    InvalidJson {
        format: LanguagePackFormat,
        message: String,
    },
    UnterminatedJsoncComment,
    InvalidDocumentRoot,
    MissingRequiredField {
        field: String,
    },
    InvalidField {
        field: String,
        reason: String,
    },
    BuiltinLanguageOverride {
        id: String,
    },
    InvalidLanguageId {
        id: String,
    },
    Serialization {
        message: String,
    },
    InvalidBuiltinCatalog {
        message: String,
    },
}

impl I18nError {
    pub(crate) fn invalid_field(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidField {
            field: field.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for I18nError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFileExtension { path } => write!(
                formatter,
                "language-pack file '{}' must use the .json or .jsonc extension",
                path.display()
            ),
            Self::NotRegularFile { path } => {
                write!(
                    formatter,
                    "language-pack path '{}' is not a file",
                    path.display()
                )
            }
            Self::FileTooLarge {
                path,
                limit_bytes,
                actual_bytes,
            } => match path {
                Some(path) => write!(
                    formatter,
                    "language-pack file '{}' is {actual_bytes} bytes; the limit is {limit_bytes} bytes",
                    path.display()
                ),
                None => write!(
                    formatter,
                    "language-pack input is {actual_bytes} bytes; the limit is {limit_bytes} bytes"
                ),
            },
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} '{}': {source}",
                path.display()
            ),
            Self::InvalidUtf8 { path, source } => match path {
                Some(path) => write!(
                    formatter,
                    "language-pack file '{}' is not valid UTF-8: {source}",
                    path.display()
                ),
                None => write!(
                    formatter,
                    "language-pack input is not valid UTF-8: {source}"
                ),
            },
            Self::InvalidJson { format, message } => {
                write!(formatter, "invalid {format} language pack: {message}")
            }
            Self::UnterminatedJsoncComment => {
                formatter.write_str("unterminated block comment in JSONC language pack")
            }
            Self::InvalidDocumentRoot => formatter.write_str("language pack must be a JSON object"),
            Self::MissingRequiredField { field } => {
                write!(formatter, "missing required language-pack field '{field}'")
            }
            Self::InvalidField { field, reason } => {
                write!(formatter, "invalid language-pack field '{field}': {reason}")
            }
            Self::BuiltinLanguageOverride { id } => {
                write!(
                    formatter,
                    "custom language id '{id}' would override a built-in language"
                )
            }
            Self::InvalidLanguageId { id } => {
                write!(
                    formatter,
                    "custom language id '{id}' contains unsupported characters"
                )
            }
            Self::Serialization { message } => {
                write!(
                    formatter,
                    "failed to serialize normalized language pack: {message}"
                )
            }
            Self::InvalidBuiltinCatalog { message } => {
                write!(formatter, "invalid built-in language catalog: {message}")
            }
        }
    }
}

impl Error for I18nError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::InvalidUtf8 { source, .. } => Some(source),
            _ => None,
        }
    }
}
