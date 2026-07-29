// @author kongweiguang

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::builtins::builtin_catalog;
use crate::jsonc::parse_jsonc;
use crate::locale::is_builtin_language_id;
use crate::{I18nError, LanguagePackFormat, Result, TranslationBundle};

/// Maximum accepted source size for a language pack (1 MiB).
pub const MAX_LANGUAGE_PACK_BYTES: usize = 1024 * 1024;

/// A parsed language pack with its fallback-completed translations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguagePack {
    pub id: String,
    pub name: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub strings: TranslationBundle,
}

/// A custom pack that passed import validation and has a persistence-safe form.
#[derive(Clone, Debug)]
pub struct CustomLanguagePack {
    pack: LanguagePack,
    normalized: Value,
}

impl LanguagePack {
    /// Parses a JSON language pack. Missing strings use the id-specific
    /// built-in fallback, or English for an unknown id.
    pub fn from_json(input: &str) -> Result<Self> {
        Self::parse_text(input, LanguagePackFormat::Json)
    }

    /// Parses a JSONC language pack with line and block comments.
    pub fn from_jsonc(input: &str) -> Result<Self> {
        Self::parse_text(input, LanguagePackFormat::Jsonc)
    }

    /// Reads a `.json` or `.jsonc` language pack, enforcing the size limit
    /// before deserializing untrusted content.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let (value, _format) = read_value_from_file(path.as_ref())?;
        Self::from_value(value)
    }

    fn parse_text(input: &str, format: LanguagePackFormat) -> Result<Self> {
        ensure_size(input.len(), None)?;
        Self::from_value(parse_value(input, format)?)
    }

    pub(crate) fn from_value(mut value: Value) -> Result<Self> {
        prune_empty_json_values(&mut value);
        let object = value.as_object().ok_or(I18nError::InvalidDocumentRoot)?;
        let id = required_raw_text(object, "id")?;
        let name = optional_text(object, "name")?
            .or_else(|| builtin_catalog().language_name(&id).map(str::to_owned))
            .unwrap_or_else(|| id.clone());
        let partial = match object.get("strings") {
            Some(value) => {
                let strings = value
                    .as_object()
                    .ok_or_else(|| I18nError::invalid_field("strings", "expected an object"))?;
                TranslationBundle::from_partial_object(strings, builtin_catalog().schema())?
            }
            None => TranslationBundle::default(),
        };

        Ok(Self {
            id: id.clone(),
            name,
            author: optional_text(object, "author")?,
            description: optional_text(object, "description")?,
            version: optional_text(object, "version")?,
            homepage: optional_text(object, "homepage")?,
            license: optional_text(object, "license")?,
            strings: partial.merge_over(&builtin_catalog().fallback_bundle(&id)),
        })
    }
}

impl CustomLanguagePack {
    /// Parses JSON and applies the custom-id, required-name, and field
    /// whitelisting rules used for importable language packs.
    pub fn from_json(input: &str) -> Result<Self> {
        Self::parse_text(input, LanguagePackFormat::Json)
    }

    /// Parses JSONC and applies the custom-id, required-name, and field
    /// whitelisting rules used for importable language packs.
    pub fn from_jsonc(input: &str) -> Result<Self> {
        Self::parse_text(input, LanguagePackFormat::Jsonc)
    }

    /// Reads and validates an importable custom language pack from disk.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let (value, _format) = read_value_from_file(path.as_ref())?;
        Self::from_value(value)
    }

    /// Returns the fallback-completed pack ready for catalog installation.
    pub fn pack(&self) -> &LanguagePack {
        &self.pack
    }

    /// Consumes the validated wrapper and returns its language pack.
    pub fn into_pack(self) -> LanguagePack {
        self.pack
    }

    /// Returns the normalized, whitelist-only value written during import.
    pub fn normalized_value(&self) -> &Value {
        &self.normalized
    }

    /// Serializes the normalized persistence format with stable indentation.
    pub fn normalized_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.normalized).map_err(|error| I18nError::Serialization {
            message: error.to_string(),
        })
    }

    /// Persists the normalized pack under its sanitized id in `directory`.
    pub fn write_to_directory(&self, directory: impl AsRef<Path>) -> Result<PathBuf> {
        let directory = directory.as_ref();
        fs::create_dir_all(directory).map_err(|source| I18nError::Io {
            operation: "create language-pack directory",
            path: directory.to_path_buf(),
            source,
        })?;
        let path = directory.join(format!(
            "{}.json",
            sanitize_language_file_stem(&self.pack.id)
        ));
        let contents = self.normalized_json_pretty()?;
        fs::write(&path, contents).map_err(|source| I18nError::Io {
            operation: "write normalized language pack",
            path: path.clone(),
            source,
        })?;
        Ok(path)
    }

    fn parse_text(input: &str, format: LanguagePackFormat) -> Result<Self> {
        ensure_size(input.len(), None)?;
        Self::from_value(parse_value(input, format)?)
    }

    fn from_value(mut value: Value) -> Result<Self> {
        prune_empty_json_values(&mut value);
        let object = value.as_object().ok_or(I18nError::InvalidDocumentRoot)?;
        let id = required_custom_id(object)?;
        let name = required_text(object, "name")?;
        let mut normalized = Map::new();
        normalized.insert("id".to_owned(), Value::String(id));
        normalized.insert("name".to_owned(), Value::String(name));
        for key in ["author", "description", "version", "homepage", "license"] {
            if let Some(value) = object.get(key) {
                normalized.insert(key.to_owned(), value.clone());
            }
        }
        if let Some(strings) = object.get("strings").and_then(Value::as_object) {
            let mut normalized_strings = Map::new();
            let schema = builtin_catalog().schema();
            for (key, value) in strings {
                if schema.scalar_keys.contains(key) || schema.group_keys.contains(key) {
                    normalized_strings.insert(key.clone(), value.clone());
                }
            }
            if !normalized_strings.is_empty() {
                normalized.insert("strings".to_owned(), Value::Object(normalized_strings));
            }
        }

        let normalized = Value::Object(normalized);
        let pack = LanguagePack::from_value(normalized.clone())?;
        Ok(Self { pack, normalized })
    }
}

/// Returns whether an id can safely name an imported custom language.
pub fn is_valid_custom_language_id(language_id: &str) -> bool {
    !language_id.trim().is_empty()
        && language_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        && language_id
            .chars()
            .any(|character| character.is_ascii_alphabetic())
}

/// Produces the legacy safe persistence stem for a language id.
pub fn sanitize_language_file_stem(value: &str) -> String {
    let mut output = String::new();
    let mut last_was_separator = false;
    for character in value.trim().chars() {
        if character.is_whitespace() {
            if !last_was_separator && !output.is_empty() {
                output.push('_');
                last_was_separator = true;
            }
        } else if character.is_alphanumeric() || matches!(character, '-' | '_' | '.') {
            output.push(character);
            last_was_separator = false;
        }
    }
    let output = output.trim_matches(['_', '.']).to_owned();
    if output.is_empty() {
        "custom".to_owned()
    } else {
        output
    }
}

fn read_value_from_file(path: &Path) -> Result<(Value, LanguagePackFormat)> {
    let format = LanguagePackFormat::from_path(path)?;
    let metadata = fs::metadata(path).map_err(|source| I18nError::Io {
        operation: "read language-pack metadata",
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(I18nError::NotRegularFile {
            path: path.to_path_buf(),
        });
    }
    ensure_file_size(metadata.len(), Some(path.to_path_buf()))?;

    let mut file = fs::File::open(path).map_err(|source| I18nError::Io {
        operation: "open language pack",
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::with_capacity(metadata.len().min(MAX_LANGUAGE_PACK_BYTES as u64) as usize);
    file.by_ref()
        .take((MAX_LANGUAGE_PACK_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| I18nError::Io {
            operation: "read language pack",
            path: path.to_path_buf(),
            source,
        })?;
    ensure_size(bytes.len(), Some(path.to_path_buf()))?;
    let input = std::str::from_utf8(&bytes).map_err(|source| I18nError::InvalidUtf8 {
        path: Some(path.to_path_buf()),
        source,
    })?;
    Ok((parse_value(input, format)?, format))
}

fn parse_value(input: &str, format: LanguagePackFormat) -> Result<Value> {
    match format {
        LanguagePackFormat::Json => {
            serde_json::from_str(input).map_err(|error| I18nError::InvalidJson {
                format,
                message: error.to_string(),
            })
        }
        LanguagePackFormat::Jsonc => parse_jsonc(input),
    }
}

fn ensure_size(actual_bytes: usize, path: Option<PathBuf>) -> Result<()> {
    if actual_bytes > MAX_LANGUAGE_PACK_BYTES {
        return Err(I18nError::FileTooLarge {
            path,
            limit_bytes: MAX_LANGUAGE_PACK_BYTES,
            actual_bytes,
        });
    }
    Ok(())
}

fn ensure_file_size(actual_bytes: u64, path: Option<PathBuf>) -> Result<()> {
    if actual_bytes > MAX_LANGUAGE_PACK_BYTES as u64 {
        return Err(I18nError::FileTooLarge {
            path,
            limit_bytes: MAX_LANGUAGE_PACK_BYTES,
            actual_bytes: usize::try_from(actual_bytes).unwrap_or(usize::MAX),
        });
    }
    Ok(())
}

fn required_custom_id(object: &Map<String, Value>) -> Result<String> {
    let id = required_text(object, "id")?;
    if is_builtin_language_id(&id) {
        return Err(I18nError::BuiltinLanguageOverride { id });
    }
    if !is_valid_custom_language_id(&id) {
        return Err(I18nError::InvalidLanguageId { id });
    }
    Ok(id)
}

fn required_raw_text(object: &Map<String, Value>, key: &str) -> Result<String> {
    let value = object
        .get(key)
        .ok_or_else(|| I18nError::MissingRequiredField {
            field: key.to_owned(),
        })?;
    let text = value
        .as_str()
        .ok_or_else(|| I18nError::invalid_field(key, "expected a non-empty string"))?;
    if text.trim().is_empty() {
        return Err(I18nError::invalid_field(key, "expected a non-empty string"));
    }
    Ok(text.to_owned())
}

fn required_text(object: &Map<String, Value>, key: &str) -> Result<String> {
    let value = object
        .get(key)
        .ok_or_else(|| I18nError::MissingRequiredField {
            field: key.to_owned(),
        })?;
    let text = value
        .as_str()
        .ok_or_else(|| I18nError::invalid_field(key, "expected a non-empty string"))?;
    let text = text.trim();
    if text.is_empty() {
        return Err(I18nError::invalid_field(key, "expected a non-empty string"));
    }
    Ok(text.to_owned())
}

fn optional_text(object: &Map<String, Value>, key: &str) -> Result<Option<String>> {
    object
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| I18nError::invalid_field(key, "expected a string"))
        })
        .transpose()
}

fn prune_empty_json_values(value: &mut Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        Value::Array(items) => {
            items.retain_mut(|item| !prune_empty_json_values(item));
            items.is_empty()
        }
        Value::Object(object) => {
            object.retain(|_, item| !prune_empty_json_values(item));
            object.is_empty()
        }
        Value::Bool(_) | Value::Number(_) => false,
    }
}
