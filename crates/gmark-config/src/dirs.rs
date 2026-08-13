// @author kongweiguang

//! Gmark 配置、状态、缓存与运行时目录的解析和安全按需创建。

use std::{
    env,
    fs::{self, DirBuilder, Metadata},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use directories::BaseDirs;

const APP_HOME_DIR: &str = ".gmark";

/// Gmark 的跨平台目录集合。
///
/// 目录解析本身不产生文件系统副作用。调用方在真正需要写入时，应使用
/// `ensure_*_root` 或 `ensure_*_parent`；这些入口会拒绝已有的符号链接和非目录。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppDirs {
    config_root: PathBuf,
    state_root: PathBuf,
    cache_root: PathBuf,
    runtime_root: PathBuf,
}

impl AppDirs {
    /// 从用户主目录下的 `~/.gmark` 解析 Gmark 的四个目录根。
    ///
    /// `GMARK_UI_CHECK_ROOT` 只用于验收实例隔离；一旦设置，必须是非空绝对路径。
    /// 旧的单配置根验收变量不再读取。
    pub fn from_system() -> Result<Self> {
        Self::from_system_with_override(env::var_os("GMARK_UI_CHECK_ROOT").map(PathBuf::from))
    }

    /// 从系统目录解析，或使用验收/测试根覆盖。
    pub fn from_system_with_override(override_root: Option<PathBuf>) -> Result<Self> {
        if let Some(root) = override_root {
            validate_ui_check_root(&root)?;
            return Ok(Self::from_ui_check_root(root));
        }

        let dirs = BaseDirs::new().context("failed to resolve the user home directory")?;
        Ok(Self::from_app_root(dirs.home_dir().join(APP_HOME_DIR)))
    }

    /// 从验收根派生四个隔离子目录：`config`、`state`、`cache`、`runtime`。
    ///
    /// 该构造器不访问文件系统，也不创建任何目录；根路径校验只在调用时执行。
    pub fn from_ui_check_root(root: impl Into<PathBuf>) -> Self {
        Self::from_app_root(root.into())
    }

    fn from_app_root(root: PathBuf) -> Self {
        Self {
            config_root: root.join("config"),
            state_root: root.join("state"),
            cache_root: root.join("cache"),
            runtime_root: root.join("runtime"),
        }
    }

    /// 从显式测试根构造目录集合，四类数据共享该根。
    ///
    /// 需要验证正式四子目录布局时使用 [`Self::from_ui_check_root`]。
    #[must_use]
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let config_root = root.into();
        Self {
            state_root: config_root.clone(),
            cache_root: config_root.clone(),
            runtime_root: config_root.clone(),
            config_root,
        }
    }

    /// 构造一个完整目录集合，供纯单元测试和适配器注入使用。
    pub fn from_roots(
        config_root: impl Into<PathBuf>,
        state_root: impl Into<PathBuf>,
        cache_root: impl Into<PathBuf>,
        runtime_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            config_root: config_root.into(),
            state_root: state_root.into(),
            cache_root: cache_root.into(),
            runtime_root: runtime_root.into(),
        }
    }

    /// 返回配置根目录（不会创建）。
    #[must_use]
    pub fn config_root(&self) -> &Path {
        &self.config_root
    }

    /// 返回状态根目录（不会创建）。
    #[must_use]
    pub fn state_root(&self) -> &Path {
        &self.state_root
    }

    /// 返回缓存根目录（不会创建）。
    #[must_use]
    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    /// 返回运行时根目录（不会创建）。
    #[must_use]
    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    /// `config.toml` 应用偏好文件。
    #[must_use]
    pub fn config_toml_file(&self) -> PathBuf {
        self.config_root.join("config.toml")
    }

    /// 应用偏好文件。
    #[must_use]
    pub fn app_config_file(&self) -> PathBuf {
        self.config_toml_file()
    }

    /// 用户扩展语言包目录。
    #[must_use]
    pub fn languages_dir(&self) -> PathBuf {
        self.config_root.join("languages")
    }

    /// 最近文件历史状态文件。
    #[must_use]
    pub fn history_file(&self) -> PathBuf {
        self.state_root.join(".history")
    }

    /// 工作区会话状态文件。
    #[must_use]
    pub fn workspace_session_file(&self) -> PathBuf {
        self.state_root.join("workspace-session.json")
    }

    /// v10 迁移前工作区会话备份文件。
    #[must_use]
    pub fn workspace_session_pre_v10_file(&self) -> PathBuf {
        self.state_root.join("workspace-session.pre-v10.json")
    }

    /// 匿名安装标识文件。
    #[must_use]
    pub fn installation_id_file(&self) -> PathBuf {
        self.state_root.join("installation-id")
    }

    /// 恢复日志目录。
    #[must_use]
    pub fn recovery_dir(&self) -> PathBuf {
        self.state_root.join("recovery")
    }

    /// 崩溃报告目录。
    #[must_use]
    pub fn crash_reports_dir(&self) -> PathBuf {
        self.state_root.join("crash-reports")
    }

    /// 更新包缓存目录。
    #[must_use]
    pub fn updates_dir(&self) -> PathBuf {
        self.cache_root.join("updates")
    }

    /// 大文档索引缓存目录。
    #[must_use]
    pub fn large_document_indexes_dir(&self) -> PathBuf {
        self.cache_root.join("large-document-indexes")
    }

    /// LaTeX SVG 缓存目录。
    #[must_use]
    pub fn latex_svg_dir(&self) -> PathBuf {
        self.cache_root.join("latex-svg")
    }

    /// Mermaid SVG 缓存目录。
    #[must_use]
    pub fn mermaid_svg_dir(&self) -> PathBuf {
        self.cache_root.join("mermaid-svg")
    }

    /// 单实例锁文件。
    #[must_use]
    pub fn instance_lock_file(&self) -> PathBuf {
        self.runtime_root.join("instance.lock")
    }

    /// 按需创建并校验配置根目录。
    pub fn ensure_config_root(&self) -> Result<()> {
        ensure_directory_tree(&self.config_root, 0o755)
    }

    /// 按需创建并校验状态根目录；Unix 新目录权限为 `0700`。
    pub fn ensure_state_root(&self) -> Result<()> {
        ensure_directory_tree(&self.state_root, 0o700)
    }

    /// 按需创建并校验缓存根目录。
    pub fn ensure_cache_root(&self) -> Result<()> {
        ensure_directory_tree(&self.cache_root, 0o755)
    }

    /// 按需创建并校验运行时根目录；Unix 新目录权限为 `0700`。
    pub fn ensure_runtime_root(&self) -> Result<()> {
        ensure_directory_tree(&self.runtime_root, 0o700)
    }

    /// 确保配置文件的父目录存在且位于配置根内。
    pub fn ensure_config_parent(&self, path: &Path) -> Result<()> {
        self.ensure_parent_under(path, &self.config_root, 0o755)
    }

    /// 确保状态文件的父目录存在且位于状态根内。
    pub fn ensure_state_parent(&self, path: &Path) -> Result<()> {
        self.ensure_parent_under(path, &self.state_root, 0o700)
    }

    /// 确保缓存文件的父目录存在且位于缓存根内。
    pub fn ensure_cache_parent(&self, path: &Path) -> Result<()> {
        self.ensure_parent_under(path, &self.cache_root, 0o755)
    }

    /// 确保运行时文件的父目录存在且位于运行时根内。
    pub fn ensure_runtime_parent(&self, path: &Path) -> Result<()> {
        self.ensure_parent_under(path, &self.runtime_root, 0o700)
    }

    /// 校验已存在的配置根；缺失根保持未创建并视为可读取的空状态。
    pub fn validate_config_root(&self) -> Result<()> {
        validate_existing_directory(&self.config_root)
    }

    /// 校验已存在的状态根；缺失根保持未创建并视为可读取的空状态。
    pub fn validate_state_root(&self) -> Result<()> {
        validate_existing_directory(&self.state_root)
    }

    /// 校验已存在的缓存根；缺失根保持未创建并视为无缓存。
    pub fn validate_cache_root(&self) -> Result<()> {
        validate_existing_directory(&self.cache_root)
    }

    /// 校验已存在的运行时根；缺失根保持未创建。
    pub fn validate_runtime_root(&self) -> Result<()> {
        validate_existing_directory(&self.runtime_root)
    }

    /// 根据目录所属根确保文件父目录存在。
    pub fn ensure_parent(&self, path: &Path) -> Result<()> {
        if path.starts_with(&self.config_root) {
            self.ensure_config_parent(path)
        } else if path.starts_with(&self.state_root) {
            self.ensure_state_parent(path)
        } else if path.starts_with(&self.cache_root) {
            self.ensure_cache_parent(path)
        } else if path.starts_with(&self.runtime_root) {
            self.ensure_runtime_parent(path)
        } else {
            bail!(
                "path '{}' is outside the Gmark directory roots",
                path.display()
            )
        }
    }

    fn ensure_parent_under(&self, path: &Path, root: &Path, mode: u32) -> Result<()> {
        if path
            .components()
            .any(|component| component == Component::ParentDir)
        {
            bail!(
                "path '{}' must not contain parent traversal",
                path.display()
            );
        }
        if !path.starts_with(root) {
            bail!(
                "path '{}' is outside root '{}'",
                path.display(),
                root.display()
            );
        }
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(root);
        ensure_directory_tree(root, mode)?;
        ensure_directory_tree(parent, mode)
    }
}

fn validate_ui_check_root(root: &Path) -> Result<()> {
    if root.as_os_str().is_empty() {
        bail!("GMARK_UI_CHECK_ROOT must be a non-empty absolute path");
    }
    if !root.is_absolute() {
        bail!("GMARK_UI_CHECK_ROOT must be an absolute path");
    }
    Ok(())
}

fn ensure_directory_tree(path: &Path, mode: u32) -> Result<()> {
    if path.as_os_str().is_empty() {
        bail!("directory path must not be empty");
    }

    let mut current = path.to_path_buf();
    let mut pending = Vec::new();
    loop {
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                ensure_directory_metadata(&current, &metadata)?;
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                pending.push(current.clone());
                let Some(parent) = current.parent() else {
                    break;
                };
                if parent.as_os_str().is_empty() || parent == current {
                    break;
                }
                current = parent.to_path_buf();
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to inspect directory '{}'", current.display())
                });
            }
        }
    }
    for directory in pending.into_iter().rev() {
        match create_directory(&directory, mode) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to create directory '{}'", directory.display())
                });
            }
        }
        let metadata = fs::symlink_metadata(&directory)
            .with_context(|| format!("failed to inspect directory '{}'", directory.display()))?;
        ensure_directory_metadata(&directory, &metadata)?;
    }
    Ok(())
}

fn ensure_directory_metadata(path: &Path, metadata: &Metadata) -> Result<()> {
    if metadata.file_type().is_symlink() || is_reparse_point(metadata) {
        bail!("directory '{}' must not be a symbolic link", path.display());
    }
    if !metadata.is_dir() {
        bail!("directory '{}' is not a directory", path.display());
    }
    Ok(())
}

fn validate_existing_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => ensure_directory_metadata(path, &metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect directory '{}'", path.display()))
        }
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &Metadata) -> bool {
    false
}

fn create_directory(path: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        DirBuilder::new().mode(mode).create(path)
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        DirBuilder::new().create(path)
    }
}
