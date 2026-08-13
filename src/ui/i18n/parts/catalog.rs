// @author kongweiguang

//! GPUI adapter for the pure `gmark-i18n` catalog and language-pack domain.

use std::{path::Path, sync::Arc};

use gmark_config::AppDirs;
#[cfg(test)]
use gmark_config::read_app_preferences;
use gpui::{App, Global};
use serde_json::{Map, Value};

use super::super::I18nStrings;

pub struct I18nManager {
    catalog: gmark_i18n::I18nCatalog,
    strings: Arc<I18nStrings>,
}

impl Global for I18nManager {}

impl Default for I18nManager {
    fn default() -> Self {
        Self::new_with_language_id(gmark_i18n::DEFAULT_LANGUAGE_ID)
    }
}

impl I18nManager {
    /// Installs the configured UI language into GPUI's global state.
    #[cfg(test)]
    pub fn init(cx: &mut App) {
        let language_id = read_app_preferences()
            .map(|preferences| preferences.default_language_id)
            .unwrap_or_else(|_| gmark_i18n::DEFAULT_LANGUAGE_ID.into());
        Self::init_with_language_id(cx, &language_id);
    }

    /// Installs a language selection and all persisted custom packs into GPUI.
    pub fn init_with_language_id(cx: &mut App, language_id: &str) {
        let mut manager = Self::new_with_language_id(gmark_i18n::DEFAULT_LANGUAGE_ID);
        if let Ok(dirs) = AppDirs::from_system()
            && let Err(error) = manager.load_custom_languages_from_dirs(&dirs)
        {
            eprintln!("failed to load custom languages: {error}");
        }
        let _ = manager.set_language_by_id(language_id);
        cx.set_global(manager);
    }

    /// Creates a manager with a supported built-in language or English fallback.
    pub fn new_with_language_id(language_id: &str) -> Self {
        let catalog = gmark_i18n::I18nCatalog::new_with_language_id(language_id);
        let strings = Arc::new(I18nStrings::from_translation_bundle(catalog.strings()));
        Self { catalog, strings }
    }

    /// Returns the currently selected language identifier.
    pub fn current_language_id(&self) -> &str {
        self.catalog.current_language_id()
    }

    /// Returns the strong UI projection used by existing GPUI render code.
    pub fn strings(&self) -> &I18nStrings {
        &self.strings
    }

    /// Returns an O(1) snapshot for render paths that outlive a read borrow.
    pub fn strings_arc(&self) -> Arc<I18nStrings> {
        self.strings.clone()
    }

    /// Returns built-in and imported UI languages in legacy menu order.
    pub fn available_languages(&self) -> &[gmark_i18n::LanguageCatalogEntry] {
        self.catalog.available_languages()
    }

    /// Selects a language and updates the GPUI projection on any successful lookup.
    pub fn set_language_by_id(&mut self, language_id: &str) -> bool {
        let changed = self.catalog.set_language_by_id(language_id);
        if changed || self.catalog.current_language_id() == language_id {
            self.refresh_strings();
        }
        changed
    }

    /// Imports, persists, activates, and projects a custom JSON or JSONC pack.
    pub fn import_language_config(&mut self, path: impl AsRef<Path>) -> anyhow::Result<String> {
        let dirs = AppDirs::from_system()?;
        self.import_language_config_with_dirs(path, &dirs)
    }

    pub(in crate::ui::i18n) fn import_language_config_with_dirs(
        &mut self,
        path: impl AsRef<Path>,
        dirs: &AppDirs,
    ) -> anyhow::Result<String> {
        let languages_dir = dirs.languages_dir();
        dirs.ensure_config_parent(&languages_dir.join(".gmark-languages-root"))?;
        let imported = self.catalog.import_language_file(path, languages_dir)?;
        self.refresh_strings();
        Ok(imported.id)
    }

    fn load_custom_languages_from_dirs(&mut self, dirs: &AppDirs) -> anyhow::Result<()> {
        dirs.validate_config_root()?;
        let loaded = self.catalog.load_language_directory(dirs.languages_dir())?;
        for rejected in loaded.rejected {
            eprintln!(
                "skipping custom language config '{}': {}",
                rejected.path.display(),
                rejected.error
            );
        }
        self.refresh_strings();
        Ok(())
    }

    fn refresh_strings(&mut self) {
        self.strings = Arc::new(I18nStrings::from_translation_bundle(self.catalog.strings()));
    }
}

impl I18nStrings {
    /// Projects a complete domain bundle onto the existing strongly typed UI API.
    pub(in crate::ui::i18n) fn from_translation_bundle(
        bundle: &gmark_i18n::TranslationBundle,
    ) -> Self {
        let mut values = Map::new();
        for (key, text) in bundle.scalars() {
            values.insert(key.clone(), Value::String(text.clone()));
        }
        for (group, entries) in bundle.groups() {
            values.insert(
                group.clone(),
                Value::Object(
                    entries
                        .iter()
                        .map(|(key, text)| (key.clone(), Value::String(text.clone())))
                        .collect(),
                ),
            );
        }
        serde_json::from_value(Value::Object(values))
            .expect("gmark-i18n translation schema must match the UI string adapter")
    }

    /// Built-in Simplified Chinese strings, sourced from `gmark-i18n`.
    pub fn zh_cn() -> Self {
        Self::for_language_id(gmark_i18n::BUILTIN_LANGUAGE_ZH_CN_ID)
            .expect("the built-in Chinese catalog must exist")
    }

    /// Built-in English strings, sourced from `gmark-i18n`.
    pub fn en_us() -> Self {
        Self::for_language_id(gmark_i18n::BUILTIN_LANGUAGE_EN_US_ID)
            .expect("the built-in English catalog must exist")
    }

    /// Returns the typed projection of a built-in language.
    pub fn for_language_id(language_id: &str) -> Option<Self> {
        match language_id {
            gmark_i18n::BUILTIN_LANGUAGE_ZH_CN_ID | gmark_i18n::BUILTIN_LANGUAGE_EN_US_ID => {
                let catalog = gmark_i18n::I18nCatalog::new_with_language_id(language_id);
                Some(Self::from_translation_bundle(catalog.strings()))
            }
            _ => None,
        }
    }

    /// Returns an owned large-document translation, preserving missing-key behavior.
    pub fn large_document_text(&self, key: &str) -> String {
        self.large_document
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_owned())
    }

    /// Returns an owned formula-palette translation, preserving missing-key behavior.
    pub fn math_palette_text(&self, key: &str) -> String {
        self.math_palette
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_owned())
    }

    /// Localizes a paged-document error for the current UI language.
    pub fn large_document_error(&self, error: &gmark_paged_document::PagedDocumentError) -> String {
        use gmark_paged_document::PagedDocumentError;

        let (key, replacements): (&str, Vec<(&str, String)>) = match error {
            PagedDocumentError::Io { path, .. } => {
                ("error_io", vec![("{path}", path.display().to_string())])
            }
            PagedDocumentError::InvalidRange { start, end, len } => (
                "error_invalid_range",
                vec![
                    ("{start}", start.to_string()),
                    ("{end}", end.to_string()),
                    ("{len}", len.to_string()),
                ],
            ),
            PagedDocumentError::RangeTooLarge => ("error_range_too_large", vec![]),
            PagedDocumentError::InvalidUtf8Boundary => ("error_utf8_boundary", vec![]),
            PagedDocumentError::Binary => ("error_binary", vec![]),
            PagedDocumentError::UnsupportedEncoding(encoding) => (
                "error_unsupported_encoding",
                vec![("{encoding}", encoding.clone())],
            ),
            PagedDocumentError::UnrepresentableEncoding { encoding } => (
                "error_unrepresentable_encoding",
                vec![("{encoding}", encoding.clone())],
            ),
            PagedDocumentError::Cancelled => ("error_cancelled", vec![]),
            PagedDocumentError::InvalidJson { offset, .. } => {
                ("error_invalid_json", vec![("{offset}", offset.to_string())])
            }
            PagedDocumentError::InvalidDelimited { offset, .. } => (
                "error_invalid_delimited",
                vec![("{offset}", offset.to_string())],
            ),
            PagedDocumentError::InvalidRegex(_) => ("error_invalid_regex", vec![]),
            PagedDocumentError::Search(_) => ("error_search", vec![]),
            PagedDocumentError::InvalidTransaction(_) => ("error_invalid_transaction", vec![]),
            PagedDocumentError::SourceChanged => ("error_source_changed", vec![]),
            PagedDocumentError::Persist { path, .. } => (
                "error_persist",
                vec![("{path}", path.display().to_string())],
            ),
            PagedDocumentError::Recovery(_) => ("error_recovery", vec![]),
        };
        replacements.into_iter().fold(
            self.large_document_text(key),
            |message, (placeholder, value)| message.replace(placeholder, &value),
        )
    }
}
