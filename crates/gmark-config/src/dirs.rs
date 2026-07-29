// @author kongweiguang

//! 配置根目录及其稳定子路径。

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use directories::ProjectDirs;

/// GMark 拥有的跨平台配置目录。
///
/// 系统目录遵循既有的 `com/kongweiguang/gmark` 标识；显式根目录便于测试和
/// 宿主程序隔离不同实例。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigDirs {
    root: PathBuf,
}

/// Wave 2 迁移期间保留旧领域名称。
pub type GmarkConfigDirs = ConfigDirs;

impl ConfigDirs {
    /// 解析系统配置目录，并尊重 UI 验收使用的根目录覆盖变量。
    pub fn from_system() -> Result<Self> {
        Self::from_system_with_override(
            std::env::var_os("GMARK_UI_CHECK_CONFIG_ROOT").map(PathBuf::from),
        )
    }

    /// 解析系统配置目录，或使用调用方提供的非空根目录。
    pub fn from_system_with_override(override_root: Option<PathBuf>) -> Result<Self> {
        // 验收实例必须隔离，避免恢复或历史记录影响用户正在使用的实例。
        if let Some(root) = override_root.filter(|root| !root.as_os_str().is_empty()) {
            return Ok(Self { root });
        }
        let dirs = ProjectDirs::from("com", "kongweiguang", "gmark")
            .context("failed to resolve the gmark config directory")?;
        Ok(Self {
            root: dirs.config_dir().to_path_buf(),
        })
    }

    /// 从调用方提供的根目录构造路径集合。
    #[must_use]
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 返回未创建的配置根目录。
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 返回导入语言包目录。
    #[must_use]
    pub fn languages_dir(&self) -> PathBuf {
        self.root.join("languages")
    }

    /// 返回 recent files 历史文件。
    #[must_use]
    pub fn history_file(&self) -> PathBuf {
        self.root.join(".history")
    }

    /// 返回应用偏好 TOML 文件。
    #[must_use]
    pub fn app_config_file(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    /// 返回恢复日志目录。
    #[must_use]
    pub fn recovery_dir(&self) -> PathBuf {
        self.root.join("recovery")
    }

    /// 返回崩溃报告目录。
    #[must_use]
    pub fn crash_reports_dir(&self) -> PathBuf {
        self.root.join("crash-reports")
    }

    /// 返回持久更新缓存目录。
    #[must_use]
    pub fn updates_dir(&self) -> PathBuf {
        self.root.join("updates")
    }

    /// 返回稳定安装 ID 文件。
    #[must_use]
    pub fn installation_id_file(&self) -> PathBuf {
        self.root.join("installation-id")
    }

    /// 返回工作区会话 registry 文件。
    #[must_use]
    pub fn workspace_session_file(&self) -> PathBuf {
        self.root.join("workspace-session.json")
    }

    /// 返回单实例锁文件路径。
    #[must_use]
    pub fn instance_lock_file(&self) -> PathBuf {
        self.root.join("instance.lock")
    }
}
