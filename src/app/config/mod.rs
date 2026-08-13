// @author kongweiguang

//! Thin application adapters over the GPUI-independent configuration domain.

pub(crate) mod workspace_session;

pub(crate) use crate::preferences::{
    AutoSavePreference, EditorSettings, ImagePasteBehavior, StartupOpenPreference,
    apply_configured_language, first_existing_recent_markdown_file,
    import_language_config_and_select, load_or_create_app_preferences, open_preferences_window,
    read_app_preferences,
};
pub(crate) use gmark_config::{
    AppDirs, load_or_create_installation_id, read_recent_files, record_recent_file,
    remove_recent_file,
};

#[cfg(test)]
pub(crate) use gmark_config::{
    RECENT_FILES_LIMIT, load_or_create_installation_id_with_dirs, read_recent_files_with_dirs,
    record_recent_file_with_dirs, remove_recent_file_with_dirs,
};

#[cfg(test)]
#[path = "../../../tests/unit/config/tests.rs"]
mod tests;
