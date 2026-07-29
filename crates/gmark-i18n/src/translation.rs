// @author kongweiguang

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::builtins::TranslationSchema;
use crate::{I18nError, Result};

/// Complete translations for one active language.
///
/// Root entries preserve the historic language-pack keys. Nested maps use a
/// `group.key` lookup path, such as `slash_commands.table`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TranslationBundle {
    scalars: BTreeMap<String, String>,
    groups: BTreeMap<String, BTreeMap<String, String>>,
}

impl TranslationBundle {
    /// Creates a bundle from explicit root and grouped translations.
    pub fn from_parts(
        scalars: BTreeMap<String, String>,
        groups: BTreeMap<String, BTreeMap<String, String>>,
    ) -> Self {
        Self { scalars, groups }
    }

    /// Looks up a key without applying a missing-key fallback.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.scalars.get(key).map(String::as_str).or_else(|| {
            let (group, nested_key) = key.split_once('.')?;
            self.groups.get(group)?.get(nested_key).map(String::as_str)
        })
    }

    /// Looks up one entry in a nested legacy map.
    pub fn get_group(&self, group: &str, key: &str) -> Option<&str> {
        self.groups
            .get(group)
            .and_then(|entries| entries.get(key))
            .map(String::as_str)
    }

    /// Returns a copy of the complete lookup path when no translation exists.
    pub fn translate(&self, key: &str) -> String {
        self.get(key).unwrap_or(key).to_owned()
    }

    /// Returns a grouped translation, or the nested key when it is absent.
    ///
    /// This is the compatibility behavior of the legacy
    /// `large_document_text` lookup, where an unknown `large_document` key is
    /// rendered unchanged instead of disappearing from the UI.
    pub fn translate_group(&self, group: &str, key: &str) -> String {
        self.get_group(group, key).unwrap_or(key).to_owned()
    }

    /// Looks up and interpolates a template. Unknown parameters are harmless;
    /// missing placeholders remain visible in the result.
    pub fn format_text(&self, key: &str, parameters: &[(&str, &str)]) -> String {
        interpolate(&self.translate(key), parameters)
    }

    /// Formats a grouped translation while retaining grouped missing-key
    /// compatibility semantics.
    pub fn format_group_text(&self, group: &str, key: &str, parameters: &[(&str, &str)]) -> String {
        interpolate(&self.translate_group(group, key), parameters)
    }

    /// Returns the root scalar translations in stable key order.
    pub fn scalars(&self) -> &BTreeMap<String, String> {
        &self.scalars
    }

    /// Returns the grouped translations in stable key order.
    pub fn groups(&self) -> &BTreeMap<String, BTreeMap<String, String>> {
        &self.groups
    }

    /// Applies this partial bundle over a complete fallback bundle.
    pub(crate) fn merge_over(&self, fallback: &Self) -> Self {
        let mut merged = fallback.clone();
        merged.scalars.extend(self.scalars.clone());
        for (group, custom_entries) in &self.groups {
            merged
                .groups
                .entry(group.clone())
                .or_default()
                .extend(custom_entries.clone());
        }
        merged
    }

    pub(crate) fn from_complete_value(value: &Value) -> Result<Self> {
        let object = value.as_object().ok_or_else(|| {
            I18nError::invalid_field(
                "catalog",
                "expected an object containing string values or string maps",
            )
        })?;
        let mut scalars = BTreeMap::new();
        let mut groups = BTreeMap::new();

        for (key, value) in object {
            if let Some(text) = value.as_str() {
                scalars.insert(key.clone(), text.to_owned());
                continue;
            }
            let entries = parse_string_map(value, key)?;
            groups.insert(key.clone(), entries);
        }

        Ok(Self { scalars, groups })
    }

    pub(crate) fn from_partial_object(
        object: &Map<String, Value>,
        schema: &TranslationSchema,
    ) -> Result<Self> {
        let mut scalars = BTreeMap::new();
        let mut groups = BTreeMap::new();

        for (key, value) in object {
            if schema.scalar_keys.contains(key) {
                let text = value.as_str().ok_or_else(|| {
                    I18nError::invalid_field(format!("strings.{key}"), "expected a string")
                })?;
                scalars.insert(key.clone(), text.to_owned());
            } else if schema.group_keys.contains(key) {
                groups.insert(
                    key.clone(),
                    parse_string_map(value, &format!("strings.{key}"))?,
                );
            }
        }

        Ok(Self { scalars, groups })
    }

    pub(crate) fn root_keys(&self) -> BTreeSet<String> {
        self.scalars
            .keys()
            .chain(self.groups.keys())
            .cloned()
            .collect()
    }

    pub(crate) fn group_keys(&self) -> BTreeMap<String, BTreeSet<String>> {
        self.groups
            .iter()
            .map(|(group, entries)| (group.clone(), entries.keys().cloned().collect()))
            .collect()
    }
}

/// Replaces `{name}` placeholders in a template.
///
/// A parameter name may be supplied either as `name` or as `{name}`. Applying
/// replacements in slice order matches the legacy sequential `String::replace`
/// behavior used by UI error formatting.
pub fn interpolate(template: &str, parameters: &[(&str, &str)]) -> String {
    parameters
        .iter()
        .fold(template.to_owned(), |message, (name, value)| {
            let placeholder = if name.starts_with('{') && name.ends_with('}') {
                (*name).to_owned()
            } else {
                format!("{{{name}}}")
            };
            message.replace(&placeholder, value)
        })
}

fn parse_string_map(value: &Value, field: &str) -> Result<BTreeMap<String, String>> {
    let object = value
        .as_object()
        .ok_or_else(|| I18nError::invalid_field(field, "expected an object of string values"))?;
    let mut entries = BTreeMap::new();
    for (key, value) in object {
        let text = value.as_str().ok_or_else(|| {
            I18nError::invalid_field(format!("{field}.{key}"), "expected a string")
        })?;
        entries.insert(key.clone(), text.to_owned());
    }
    Ok(entries)
}
