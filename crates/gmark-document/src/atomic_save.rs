// @author kongweiguang

use std::{
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use tempfile::Builder;
use thiserror::Error;

#[cfg(unix)]
use std::fs::File;

/// 原子写入失败时所处的稳定阶段，供 UI 映射可操作错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtomicWriteStage {
    /// 校验目标路径。
    ValidateTarget,
    /// 读取既有文件元数据。
    InspectTarget,
    /// 在目标目录创建临时文件。
    CreateTemporary,
    /// 继承既有文件权限。
    ApplyPermissions,
    /// 写入全部内容。
    WriteContents,
    /// 刷新用户态缓冲。
    FlushContents,
    /// 把临时文件内容同步到存储设备。
    SyncTemporary,
    /// 原子替换目标目录项。
    PersistTemporary,
    /// 同步替换后的目标文件。
    SyncPersisted,
    /// 同步父目录元数据。
    SyncDirectory,
}

impl fmt::Display for AtomicWriteStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::ValidateTarget => "validate-target",
            Self::InspectTarget => "inspect-target",
            Self::CreateTemporary => "create-temporary",
            Self::ApplyPermissions => "apply-permissions",
            Self::WriteContents => "write-contents",
            Self::FlushContents => "flush-contents",
            Self::SyncTemporary => "sync-temporary",
            Self::PersistTemporary => "persist-temporary",
            Self::SyncPersisted => "sync-persisted",
            Self::SyncDirectory => "sync-directory",
        };
        formatter.write_str(label)
    }
}

impl AtomicWriteStage {
    /// 原子替换完成后再失败时，目标文件可能已经包含新内容。
    pub fn target_may_have_changed(self) -> bool {
        matches!(self, Self::SyncPersisted | Self::SyncDirectory)
    }
}

/// 原子写入错误，保留失败阶段、目标路径和底层错误链。
#[derive(Debug, Error)]
#[error("原子写入 {path:?} 在 {stage} 阶段失败: {source}")]
pub struct AtomicWriteError {
    path: PathBuf,
    stage: AtomicWriteStage,
    #[source]
    source: io::Error,
}

impl AtomicWriteError {
    fn new(path: &Path, stage: AtomicWriteStage, source: io::Error) -> Self {
        Self {
            path: path.to_path_buf(),
            stage,
            source,
        }
    }

    /// 返回失败阶段。
    pub fn stage(&self) -> AtomicWriteStage {
        self.stage
    }

    /// 返回原始目标路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 返回失败发生时原子替换是否已经完成。
    pub fn target_may_have_changed(&self) -> bool {
        self.stage.target_may_have_changed()
    }
}

/// 在目标文件所在目录完成写入、同步和原子替换。
///
/// 临时文件与目标文件位于同一文件系统。`PersistTemporary` 之前失败时既有
/// 目标保持不变；替换后的持久化同步仍可能失败，调用方必须通过
/// [`AtomicWriteError::target_may_have_changed`] 区分并刷新磁盘基线。目标存在时
/// 继承其权限。
pub fn atomic_write(path: impl AsRef<Path>, contents: &[u8]) -> Result<(), AtomicWriteError> {
    atomic_write_with_stage_hook(path.as_ref(), contents, |_| Ok(()))
}

fn atomic_write_with_stage_hook(
    path: &Path,
    contents: &[u8],
    mut before_stage: impl FnMut(AtomicWriteStage) -> io::Result<()>,
) -> Result<(), AtomicWriteError> {
    run_stage_hook(path, AtomicWriteStage::ValidateTarget, &mut before_stage)?;
    if path.file_name().is_none() {
        return Err(AtomicWriteError::new(
            path,
            AtomicWriteStage::ValidateTarget,
            io::Error::new(io::ErrorKind::InvalidInput, "目标路径缺少文件名"),
        ));
    }

    let parent = path
        .parent()
        .filter(|candidate| !candidate.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    run_stage_hook(path, AtomicWriteStage::InspectTarget, &mut before_stage)?;
    let existing_permissions = match fs::metadata(path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(AtomicWriteError::new(
                path,
                AtomicWriteStage::InspectTarget,
                error,
            ));
        }
    };

    run_stage_hook(path, AtomicWriteStage::CreateTemporary, &mut before_stage)?;
    let mut temporary = Builder::new()
        .prefix(".gmark-save-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|error| AtomicWriteError::new(path, AtomicWriteStage::CreateTemporary, error))?;

    if let Some(permissions) = existing_permissions {
        run_stage_hook(path, AtomicWriteStage::ApplyPermissions, &mut before_stage)?;
        temporary
            .as_file()
            .set_permissions(permissions)
            .map_err(|error| {
                AtomicWriteError::new(path, AtomicWriteStage::ApplyPermissions, error)
            })?;
    }

    run_stage_hook(path, AtomicWriteStage::WriteContents, &mut before_stage)?;
    temporary
        .as_file_mut()
        .write_all(contents)
        .map_err(|error| AtomicWriteError::new(path, AtomicWriteStage::WriteContents, error))?;
    run_stage_hook(path, AtomicWriteStage::FlushContents, &mut before_stage)?;
    temporary
        .as_file_mut()
        .flush()
        .map_err(|error| AtomicWriteError::new(path, AtomicWriteStage::FlushContents, error))?;
    run_stage_hook(path, AtomicWriteStage::SyncTemporary, &mut before_stage)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| AtomicWriteError::new(path, AtomicWriteStage::SyncTemporary, error))?;

    run_stage_hook(path, AtomicWriteStage::PersistTemporary, &mut before_stage)?;
    let persisted = temporary.persist(path).map_err(|error| {
        AtomicWriteError::new(path, AtomicWriteStage::PersistTemporary, error.error)
    })?;
    run_stage_hook(path, AtomicWriteStage::SyncPersisted, &mut before_stage)?;
    persisted
        .sync_all()
        .map_err(|error| AtomicWriteError::new(path, AtomicWriteStage::SyncPersisted, error))?;

    run_stage_hook(path, AtomicWriteStage::SyncDirectory, &mut before_stage)?;
    sync_parent_directory(parent, path)
}

fn run_stage_hook(
    path: &Path,
    stage: AtomicWriteStage,
    before_stage: &mut impl FnMut(AtomicWriteStage) -> io::Result<()>,
) -> Result<(), AtomicWriteError> {
    before_stage(stage).map_err(|error| AtomicWriteError::new(path, stage, error))
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path, target: &Path) -> Result<(), AtomicWriteError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| AtomicWriteError::new(target, AtomicWriteStage::SyncDirectory, error))
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path, _target: &Path) -> Result<(), AtomicWriteError> {
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/atomic_save.rs"]
mod tests;
