// @author kongweiguang

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::builtins::builtin_catalog;
use crate::locale::{DEFAULT_LANGUAGE_ID, is_builtin_language_id};
use crate::{
    CustomLanguagePack, I18nError, LanguageCatalogEntry, LanguagePack, Result, TranslationBundle,
};

/// Result of selecting a language in an [`I18nCatalog`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanguageSelection {
    Changed,
    Unchanged,
    NotFound,
}

/// A pack rejected while scanning an external language-pack directory.
#[derive(Debug)]
pub struct LanguagePackRejection {
    pub path: PathBuf,
    pub error: I18nError,
}

/// Observable result of loading a directory while retaining valid packs.
#[derive(Debug, Default)]
pub struct LanguageDirectoryLoad {
    pub loaded: Vec<LanguageCatalogEntry>,
    pub rejected: Vec<LanguagePackRejection>,
}

/// Result of importing a pack: it is persisted, installed, and activated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedLanguagePack {
    pub id: String,
    pub path: PathBuf,
}

/// Mutable, UI-runtime-independent language selection and custom-pack catalog.
pub struct I18nCatalog {
    current_language_id: String,
    strings: Arc<TranslationBundle>,
    custom_languages: Vec<LanguagePack>,
    language_catalog: Vec<LanguageCatalogEntry>,
}

impl Default for I18nCatalog {
    fn default() -> Self {
        Self::new_with_language_id(DEFAULT_LANGUAGE_ID)
    }
}

impl I18nCatalog {
    /// Creates a catalog using a built-in id, with English as the default.
    pub fn new_with_language_id(language_id: &str) -> Self {
        let catalog = builtin_catalog();
        let current_language_id = if catalog.bundle(language_id).is_some() {
            language_id
        } else {
            DEFAULT_LANGUAGE_ID
        };
        Self {
            current_language_id: current_language_id.to_owned(),
            strings: Arc::new(catalog.fallback_bundle(current_language_id)),
            custom_languages: Vec::new(),
            language_catalog: catalog.entries().to_vec(),
        }
    }

    /// Returns the exact id currently selected by the catalog.
    pub fn current_language_id(&self) -> &str {
        &self.current_language_id
    }

    /// Returns the complete active translation bundle.
    pub fn strings(&self) -> &TranslationBundle {
        self.strings.as_ref()
    }

    /// Returns a deep clone suitable for value snapshots.
    pub fn strings_clone(&self) -> TranslationBundle {
        self.strings.as_ref().clone()
    }

    /// Returns an O(1) snapshot handle for thin UI adapters and hot paths.
    pub fn strings_arc(&self) -> Arc<TranslationBundle> {
        self.strings.clone()
    }

    /// Returns built-in and imported languages in menu display order.
    pub fn available_languages(&self) -> &[LanguageCatalogEntry] {
        &self.language_catalog
    }

    /// Returns installed custom packs in their catalog order.
    pub fn custom_languages(&self) -> &[LanguagePack] {
        &self.custom_languages
    }

    /// Selects a language and reports whether the active bundle changed.
    pub fn select_language(&mut self, language_id: &str) -> LanguageSelection {
        let strings = builtin_catalog().bundle(language_id).or_else(|| {
            self.custom_languages
                .iter()
                .find(|pack| pack.id == language_id)
                .map(|pack| pack.strings.clone())
        });
        let Some(strings) = strings else {
            return LanguageSelection::NotFound;
        };
        let changed = self.current_language_id != language_id;
        if changed {
            self.current_language_id = language_id.to_owned();
        }
        self.strings = Arc::new(strings);
        if changed {
            LanguageSelection::Changed
        } else {
            LanguageSelection::Unchanged
        }
    }

    /// Compatibility helper with legacy `I18nManager` semantics: only a real
    /// selection change returns `true`; an unknown or already-active id is false.
    pub fn set_language_by_id(&mut self, language_id: &str) -> bool {
        self.select_language(language_id) == LanguageSelection::Changed
    }

    /// Adds or replaces an imported custom language. Built-in IDs stay
    /// immutable, even when a caller constructed a pack programmatically.
    pub fn upsert_custom_language(&mut self, pack: LanguagePack) -> Result<()> {
        if is_builtin_language_id(&pack.id) {
            return Err(I18nError::BuiltinLanguageOverride {
                id: pack.id.clone(),
            });
        }
        if let Some(existing) = self
            .custom_languages
            .iter_mut()
            .find(|existing| existing.id == pack.id)
        {
            *existing = pack;
        } else {
            self.custom_languages.push(pack);
        }
        self.rebuild_language_catalog();
        Ok(())
    }

    /// Imports a pack from `source`, writes its normalized copy to `directory`,
    /// installs it, and makes it active.
    pub fn import_language_file(
        &mut self,
        source: impl AsRef<Path>,
        directory: impl AsRef<Path>,
    ) -> Result<ImportedLanguagePack> {
        let custom = CustomLanguagePack::from_file(source)?;
        let path = custom.write_to_directory(directory)?;
        let id = custom.pack().id.clone();
        self.upsert_custom_language(custom.into_pack())?;
        let _ = self.select_language(&id);
        Ok(ImportedLanguagePack { id, path })
    }

    /// Loads each regular file from a directory. Malformed or unsupported packs
    /// are reported individually so a valid neighboring pack remains usable.
    pub fn load_language_directory(
        &mut self,
        directory: impl AsRef<Path>,
    ) -> Result<LanguageDirectoryLoad> {
        let directory = directory.as_ref();
        if !directory.exists() {
            return Ok(LanguageDirectoryLoad::default());
        }
        let entries = fs::read_dir(directory).map_err(|source| I18nError::Io {
            operation: "read language-pack directory",
            path: directory.to_path_buf(),
            source,
        })?;
        let mut loaded_packs = Vec::new();
        let mut rejected = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| I18nError::Io {
                operation: "read language-pack directory entry",
                path: directory.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            match CustomLanguagePack::from_file(&path) {
                Ok(pack) => loaded_packs.push(pack.into_pack()),
                Err(error) => rejected.push(LanguagePackRejection { path, error }),
            }
        }
        loaded_packs.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        let mut loaded = Vec::with_capacity(loaded_packs.len());
        for pack in loaded_packs {
            loaded.push(LanguageCatalogEntry {
                id: pack.id.clone(),
                name: pack.name.clone(),
            });
            self.upsert_custom_language(pack)?;
        }
        Ok(LanguageDirectoryLoad { loaded, rejected })
    }

    fn rebuild_language_catalog(&mut self) {
        let mut catalog = builtin_catalog().entries().to_vec();
        catalog.extend(
            self.custom_languages
                .iter()
                .map(|pack| LanguageCatalogEntry {
                    id: pack.id.clone(),
                    name: pack.name.clone(),
                }),
        );
        self.language_catalog = catalog;
    }
}
