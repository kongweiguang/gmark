// @author kongweiguang

//! Shared user-configuration helpers for app preferences and imported packs.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use directories::ProjectDirs;
use serde_json::{Map, Value};

pub(crate) mod workspace_session;

pub(crate) use crate::preferences::{
    AutoSavePreference, EditorSettings, ImagePasteBehavior, StartupOpenPreference,
    WorkspaceSidebarPosition, apply_configured_language, apply_configured_theme,
    first_existing_recent_markdown_file, import_language_config_and_select,
    import_theme_config_and_select, load_or_create_app_preferences, open_preferences_window,
    read_app_preferences,
};

pub(crate) const RECENT_FILES_LIMIT: usize = 20;

/// Cross-platform configuration directories owned by gmark.
#[derive(Debug, Clone)]
pub(crate) struct GmarkConfigDirs {
    root: PathBuf,
}

impl GmarkConfigDirs {
    /// Resolves the platform-specific app config directory.
    ///
    /// GPUI does not currently expose an app config path, so user-imported
    /// language and theme packs are stored under the OS location returned by
    /// `directories::ProjectDirs`.
    pub(crate) fn from_system() -> anyhow::Result<Self> {
        Self::from_system_override(
            std::env::var_os("GMARK_UI_CHECK_CONFIG_ROOT").map(PathBuf::from),
        )
    }

    fn from_system_override(override_root: Option<PathBuf>) -> anyhow::Result<Self> {
        // UI 真机验收必须与用户实例隔离，避免截图进程接管其未保存标签或恢复日志。
        if let Some(root) = override_root.filter(|root| !root.as_os_str().is_empty()) {
            return Ok(Self { root });
        }
        let dirs = ProjectDirs::from("com", "kongweiguang", "gmark")
            .context("failed to resolve the gmark config directory")?;
        Ok(Self {
            root: dirs.config_dir().to_path_buf(),
        })
    }

    /// Creates a directory set from a caller-provided root for tests.
    #[cfg(test)]
    pub(crate) fn from_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub(crate) fn languages_dir(&self) -> PathBuf {
        self.root.join("languages")
    }

    pub(crate) fn themes_dir(&self) -> PathBuf {
        self.root.join("themes")
    }

    pub(crate) fn history_file(&self) -> PathBuf {
        self.root.join(".history")
    }

    pub(crate) fn app_config_file(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub(crate) fn recovery_dir(&self) -> PathBuf {
        self.root.join("recovery")
    }

    pub(crate) fn crash_reports_dir(&self) -> PathBuf {
        self.root.join("crash-reports")
    }

    pub(crate) fn installation_id_file(&self) -> PathBuf {
        self.root.join("installation-id")
    }

    pub(crate) fn workspace_session_file(&self) -> PathBuf {
        self.root.join("workspace-session.json")
    }

    #[cfg(any(target_os = "windows", test))]
    pub(crate) fn instance_lock_file(&self) -> PathBuf {
        self.root.join("instance.lock")
    }
}

/// 返回稳定、匿名的发布分桶标识；该值只在本地参与 hash，不随更新请求上传。
pub(crate) fn load_or_create_installation_id() -> anyhow::Result<uuid::Uuid> {
    load_or_create_installation_id_with_dirs(&GmarkConfigDirs::from_system()?)
}

fn load_or_create_installation_id_with_dirs(dirs: &GmarkConfigDirs) -> anyhow::Result<uuid::Uuid> {
    let path = dirs.installation_id_file();
    match std::fs::read_to_string(&path) {
        Ok(value) => return parse_installation_id(&path, &value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read '{}'", path.display()));
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }
    let id = uuid::Uuid::new_v4();
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            if let Err(error) = writeln!(file, "{id}").and_then(|_| file.sync_all()) {
                drop(file);
                // 仅清理由本次 create_new 拥有的半成品；已有文件绝不在此路径替换。
                let _ = std::fs::remove_file(&path);
                return Err(error)
                    .with_context(|| format!("failed to persist '{}'", path.display()));
            }
            Ok(id)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let value = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read raced '{}'", path.display()))?;
            parse_installation_id(&path, &value)
        }
        Err(error) => Err(error).with_context(|| format!("failed to create '{}'", path.display())),
    }
}

fn parse_installation_id(path: &Path, value: &str) -> anyhow::Result<uuid::Uuid> {
    uuid::Uuid::parse_str(value.trim())
        .with_context(|| format!("'{}' contains an invalid installation id", path.display()))
}

pub(crate) fn read_recent_files() -> anyhow::Result<Vec<PathBuf>> {
    read_recent_files_with_dirs(&GmarkConfigDirs::from_system()?)
}

pub(crate) fn record_recent_file(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    record_recent_file_with_dirs(path, &GmarkConfigDirs::from_system()?)
}

pub(crate) fn remove_recent_file(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    remove_recent_file_with_dirs(path, &GmarkConfigDirs::from_system()?)
}

pub(crate) fn read_recent_files_with_dirs(dirs: &GmarkConfigDirs) -> anyhow::Result<Vec<PathBuf>> {
    let path = dirs.history_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read '{}'", path.display()));
        }
    };

    Ok(normalize_recent_files(text.lines().map(PathBuf::from)))
}

pub(crate) fn record_recent_file_with_dirs(
    path: &Path,
    dirs: &GmarkConfigDirs,
) -> anyhow::Result<Vec<PathBuf>> {
    if path.to_string_lossy().trim().is_empty() {
        bail!("recent file path cannot be empty");
    }
    if !is_recordable_recent_file_path(path) {
        return read_recent_files_with_dirs(dirs);
    }

    let mut paths = read_recent_files_with_dirs(dirs)?;
    let path = path.to_path_buf();
    paths.retain(|existing| !same_recent_path(existing, &path));
    paths.insert(0, path);
    paths.truncate(RECENT_FILES_LIMIT);
    write_recent_files_with_dirs(&paths, dirs)?;
    Ok(paths)
}

pub(crate) fn remove_recent_file_with_dirs(
    path: &Path,
    dirs: &GmarkConfigDirs,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths = read_recent_files_with_dirs(dirs)?;
    paths.retain(|existing| !same_recent_path(existing, path));
    write_recent_files_with_dirs(&paths, dirs)?;
    Ok(paths)
}

fn write_recent_files_with_dirs(paths: &[PathBuf], dirs: &GmarkConfigDirs) -> anyhow::Result<()> {
    let history_file = dirs.history_file();
    let normalized = normalize_recent_files(paths.iter().cloned());
    if normalized.is_empty() {
        match std::fs::remove_file(&history_file) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to remove '{}'", history_file.display()));
            }
        }
        return Ok(());
    }

    if let Some(parent) = history_file.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }
    let mut content = String::new();
    for path in normalized {
        content.push_str(&path.to_string_lossy());
        content.push('\n');
    }
    std::fs::write(&history_file, content)
        .with_context(|| format!("failed to write '{}'", history_file.display()))
}

fn normalize_recent_files(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut normalized: Vec<PathBuf> = Vec::new();
    for path in paths {
        let text = path.to_string_lossy();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = PathBuf::from(trimmed);
        if !is_recordable_recent_file_path(&path) {
            continue;
        }
        if normalized
            .iter()
            .any(|existing| same_recent_path(existing, &path))
        {
            continue;
        }
        normalized.push(path);
        if normalized.len() == RECENT_FILES_LIMIT {
            break;
        }
    }
    normalized
}

fn is_recordable_recent_file_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    if text.trim().is_empty() {
        return false;
    }

    !(is_inside_system_temp_dir(path) && has_gmark_temp_fixture_name(path))
}

fn is_inside_system_temp_dir(path: &Path) -> bool {
    let temp_dir = std::env::temp_dir();
    if cfg!(windows) {
        let path_text = normalize_windows_path_text(path);
        let mut temp_text = normalize_windows_path_text(&temp_dir);
        if !temp_text.ends_with('\\') {
            temp_text.push('\\');
        }
        return path_text.starts_with(&temp_text);
    }

    path.starts_with(temp_dir)
}

fn normalize_windows_path_text(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn has_gmark_temp_fixture_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            let name = name.to_ascii_lowercase();
            name.starts_with("gmark-drop-") || name.starts_with("velotypre-drop-")
        })
        .unwrap_or(false)
}

fn same_recent_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

pub(crate) fn is_supported_config_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extension.eq_ignore_ascii_case("json") || extension.eq_ignore_ascii_case("jsonc")
        })
        .unwrap_or(false)
}

pub(crate) fn read_json_or_jsonc(path: &Path) -> anyhow::Result<Value> {
    if !is_supported_config_file(path) {
        bail!("configuration files must use the .json or .jsonc extension");
    }

    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read '{}'", path.display()))?;
    let parsed = if path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("jsonc"))
        .unwrap_or(false)
    {
        parse_jsonc_value(&text)?
    } else {
        serde_json::from_str(&text)?
    };
    Ok(parsed)
}

pub(crate) fn parse_jsonc_value(text: &str) -> anyhow::Result<Value> {
    let stripped = strip_jsonc_comments(text)?;
    Ok(serde_json::from_str(&stripped)?)
}

pub(crate) fn strip_jsonc_comments(input: &str) -> anyhow::Result<String> {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == '/' {
            match chars.peek().copied() {
                Some('/') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next == '\n' {
                            output.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    chars.next();
                    let mut closed = false;
                    let mut previous = '\0';
                    for next in chars.by_ref() {
                        if next == '\n' {
                            output.push('\n');
                        }
                        if previous == '*' && next == '/' {
                            closed = true;
                            break;
                        }
                        previous = next;
                    }
                    if !closed {
                        bail!("unterminated block comment in JSONC file");
                    }
                    continue;
                }
                _ => {}
            }
        }

        output.push(ch);
    }

    Ok(output)
}

pub(crate) fn sanitize_config_file_stem(value: &str) -> String {
    let mut output = String::new();
    let mut last_was_separator = false;
    for ch in value.trim().chars() {
        if ch.is_whitespace() {
            if !last_was_separator && !output.is_empty() {
                output.push('_');
                last_was_separator = true;
            }
        } else if ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            output.push(ch);
            last_was_separator = false;
        }
    }

    let output = output.trim_matches(['_', '.']).to_string();
    if output.is_empty() {
        "custom".into()
    } else {
        output
    }
}

pub(crate) fn prune_empty_json_values(value: &mut Value) -> bool {
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

pub(crate) fn merge_non_empty_json_values(base: &mut Value, patch: &Value) {
    if is_empty_json_value(patch) {
        return;
    }

    match (base, patch) {
        (Value::Object(base_object), Value::Object(patch_object)) => {
            for (key, patch_value) in patch_object {
                if is_empty_json_value(patch_value) {
                    continue;
                }
                match base_object.get_mut(key) {
                    Some(base_value) => merge_non_empty_json_values(base_value, patch_value),
                    None => {
                        base_object.insert(key.clone(), patch_value.clone());
                    }
                }
            }
        }
        (base_value, patch_value) => {
            *base_value = patch_value.clone();
        }
    }
}

pub(crate) fn object_without_empty_values(mut object: Map<String, Value>) -> Map<String, Value> {
    object.retain(|_, value| !prune_empty_json_values(value));
    object
}

fn is_empty_json_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        Value::Array(items) => items.iter().all(is_empty_json_value),
        Value::Object(object) => object.values().all(is_empty_json_value),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/config/tests.rs"]
mod tests;
