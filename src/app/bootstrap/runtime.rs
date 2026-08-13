// @author kongweiguang

//! Platform lifecycle, startup restoration, and helper acknowledgement wiring.

use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
};

#[cfg(target_os = "macos")]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tempfile::NamedTempFile;
use uuid::Uuid;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use futures::StreamExt;
#[cfg(target_os = "macos")]
use futures::channel::mpsc;
#[cfg(target_os = "windows")]
use gpui::Global;
use gpui::{App, Application, BorrowAppContext};

#[cfg(target_os = "macos")]
use crate::file_url::parse_file_url;
#[cfg(target_os = "windows")]
use crate::single_instance;
use crate::{
    app::document_service::DocumentService,
    app_menu::{
        self, init as init_app_menu, open_editor_window, open_paged_recovery_window,
        open_recovered_editor_tabs_window, open_workspace_session_window,
    },
    components::init_with_keybindings as init_editor,
    config, crash_report, editor,
    i18n::I18nManager,
    net, recovery,
    theme::ThemeManager,
    ui::visual_preferences::VisualPreferencesManager,
    updater,
};

use super::assets::GmarkAssets;

const UPDATE_ACK_CAPABILITY_ENV: &str = "GMARK_UPDATE_ACK_CAPABILITY";
const ACKNOWLEDGEMENT_FILE_NAME: &str = "startup-ack";
const ACK_CAPABILITY_FILE_PREFIX: &str = "startup-ack-capability-";
const MAX_ACK_CAPABILITY_BYTES: usize = 128;

// Keep platform startup and acknowledgement code in bounded files so the
// bootstrap module remains easy to audit without changing its crate API.
#[path = "runtime_parts/acknowledgement.rs"]
mod acknowledgement;
#[path = "runtime_parts/startup.rs"]
mod startup;

pub(crate) use acknowledgement::{take_update_acknowledgement, write_update_acknowledgement};
#[cfg(test)]
pub(crate) use acknowledgement::{
    validate_external_v2_install_binding_against, write_update_acknowledgement_for_target,
};
pub(crate) use startup::run_app;

#[cfg(test)]
#[path = "../../../tests/unit/app/bootstrap/runtime.rs"]
mod tests;
