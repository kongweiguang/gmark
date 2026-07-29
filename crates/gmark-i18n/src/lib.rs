// @author kongweiguang

//! 与 UI runtime 无关的语言目录、翻译回退与外部语言包加载。

#![forbid(unsafe_code)]

mod builtins;
mod catalog;
mod error;
mod jsonc;
mod locale;
mod pack;
mod translation;

pub use builtins::LanguageCatalogEntry;
pub use catalog::{
    I18nCatalog, ImportedLanguagePack, LanguageDirectoryLoad, LanguagePackRejection,
    LanguageSelection,
};
pub use error::{I18nError, LanguagePackFormat, Result};
pub use locale::{
    BUILTIN_LANGUAGE_EN_US_ID, BUILTIN_LANGUAGE_ZH_CN_ID, DEFAULT_LANGUAGE_ID,
    language_id_for_locale_preferences, normalize_locale,
};
pub use pack::{
    CustomLanguagePack, LanguagePack, MAX_LANGUAGE_PACK_BYTES, is_valid_custom_language_id,
    sanitize_language_file_stem,
};
pub use translation::{TranslationBundle, interpolate};
