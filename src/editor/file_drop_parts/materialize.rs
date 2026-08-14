// @author kongweiguang

//! Background resource materialization shared by external drops and replacements.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context as AnyhowContext, Result, anyhow};

use super::Editor;
use crate::preferences::ResourceInsertBehavior;

pub(super) const MAX_RESOURCE_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const MAX_RESOURCE_NAME_ATTEMPTS: usize = 1_000;

pub(in crate::editor) struct ResourceCleanupGuard(Option<crate::resource_io::MaterializedResource>);

impl ResourceCleanupGuard {
    /// 为后台结果建立失败清理边界，确保实体销毁或 gate 失败不会遗留新副本。
    pub(in crate::editor) fn new(materialized: crate::resource_io::MaterializedResource) -> Self {
        Self(Some(materialized))
    }

    /// 让已写入文档的副本脱离失败清理 guard，避免成功资源被误删。
    pub(in crate::editor) fn disarm(&mut self) {
        self.0 = None;
    }

    /// 取出待提交副本，使失败分支可以明确决定是否删除它。
    pub(in crate::editor) fn take(&mut self) -> Option<crate::resource_io::MaterializedResource> {
        self.0.take()
    }
}

impl Drop for ResourceCleanupGuard {
    /// 任务被取消、实体消失或 gate 失败时回收本次创建的副本，既有源文件不在 guard 内。
    fn drop(&mut self) {
        if let Some(materialized) = self.0.take() {
            materialized.cleanup_if_created();
        }
    }
}

/// 检查资源源文件的稳定元数据上限，确保 UNC 或超大本地文件尚未进入复制循环。
pub(super) fn checked_resource_input_size(source: &Path) -> Result<u64> {
    let metadata = match fs::metadata(source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(
                ResourceMaterializationFailure::SourceNotFound(source.to_path_buf()).into(),
            );
        }
        Err(error) => {
            return Err(error).with_context(|| format!("无法读取资源文件 '{}'", source.display()));
        }
    };
    if !metadata.is_file() {
        return Err(ResourceMaterializationFailure::SourceNotFile(source.to_path_buf()).into());
    }
    let size = metadata.len();
    if size > MAX_RESOURCE_BYTES {
        return Err(anyhow!("资源文件超过 64 MiB 安全限制"));
    }
    Ok(size)
}

/// 仅把源路径不存在或不是普通文件标记为可回退文本；权限、超限和复制错误必须显式失败。
pub(in crate::editor) fn resource_materialization_is_missing(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<ResourceMaterializationFailure>()
            .is_some()
    })
}

/// 对生成的 Markdown 字节数复用同一硬上限，避免字符串已构造后才进入 UI 事务。
pub(super) fn checked_resource_output_size(size: u64) -> Result<()> {
    if size > MAX_RESOURCE_BYTES {
        return Err(anyhow!("生成的资源 Markdown 超过 64 MiB 安全限制"));
    }
    Ok(())
}

/// 重复既有资源 IO 的确定性候选命名，但把冲突搜索限制在 1,000 次以内。
pub(super) fn bounded_resource_candidate_path(
    dir: &Path,
    preferred_name: &str,
    index: usize,
) -> PathBuf {
    let preferred = Path::new(preferred_name);
    let stem = preferred
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("resource");
    let extension = preferred
        .extension()
        .and_then(|extension| extension.to_str());
    let name = if index == 0 {
        preferred_name.to_owned()
    } else if let Some(extension) = extension {
        format!("{stem}-{index}.{extension}")
    } else {
        format!("{stem}-{index}")
    };
    dir.join(name)
}

struct ResourceCopyGuard {
    path: PathBuf,
    output: Option<File>,
    committed: bool,
}

#[derive(Debug)]
enum ResourceMaterializationFailure {
    SourceNotFound(PathBuf),
    SourceNotFile(PathBuf),
}

impl fmt::Display for ResourceMaterializationFailure {
    /// 保留源路径错误类别，后台粘贴才能只对缺失/目录执行兼容文本回退。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceNotFound(path) => {
                write!(formatter, "资源源不存在: '{}'", path.display())
            }
            Self::SourceNotFile(path) => {
                write!(formatter, "资源源不是普通文件: '{}'", path.display())
            }
        }
    }
}

impl std::error::Error for ResourceMaterializationFailure {}

impl ResourceCopyGuard {
    /// 绑定 create_new 返回的目标文件，使任务取消于 io::copy 中途时也能删除半成品。
    fn new(path: PathBuf, output: File) -> Self {
        Self {
            path,
            output: Some(output),
            committed: false,
        }
    }

    /// 向已创建目标写入受限输入，避免调用方在文件句柄外遗漏失败清理。
    fn copy_from(&mut self, input: &mut impl Read, limit: u64) -> Result<u64> {
        let Some(output) = self.output.as_mut() else {
            return Err(anyhow!("资源副本写入句柄已关闭"));
        };
        Ok(io::copy(&mut input.take(limit), output)?)
    }

    /// 刷新当前副本后标记提交，只有此时 guard 才会放弃删除目标。
    fn commit(mut self) -> PathBuf {
        self.committed = true;
        drop(self.output.take());
        std::mem::take(&mut self.path)
    }
}

impl Drop for ResourceCopyGuard {
    /// 关闭句柄后删除未提交文件，覆盖取消、读取失败和超限三类半成品路径。
    fn drop(&mut self) {
        if !self.committed {
            drop(self.output.take());
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// 只复制当前任务创建的文件，并在文件增长或重名冲突超过预算时整体失败。
pub(super) fn copy_resource_without_overwrite(
    source: &Path,
    dir: &Path,
    preferred_name: &str,
) -> Result<PathBuf> {
    let mut input = match File::open(source) {
        Ok(input) => input,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(
                ResourceMaterializationFailure::SourceNotFound(source.to_path_buf()).into(),
            );
        }
        Err(error) => {
            return Err(error).with_context(|| format!("无法打开资源文件 '{}'", source.display()));
        }
    };
    for index in 0..MAX_RESOURCE_NAME_ATTEMPTS {
        let target = bounded_resource_candidate_path(dir, preferred_name, index);
        let output = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(output) => output,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("无法创建资源副本 '{}'", target.display()));
            }
        };
        let mut copy_guard = ResourceCopyGuard::new(target.clone(), output);

        let read_limit = MAX_RESOURCE_BYTES
            .checked_add(1)
            .ok_or_else(|| anyhow!("资源读取上限计算溢出"))?;
        let copied = copy_guard
            .copy_from(&mut input, read_limit)
            .with_context(|| format!("无法复制资源 '{}'", source.display()))?;
        if let Some(output) = copy_guard.output.as_mut() {
            output
                .flush()
                .with_context(|| format!("无法刷新资源副本 '{}'", target.display()))?;
        }
        if copied > MAX_RESOURCE_BYTES {
            return Err(anyhow!("资源文件在读取期间超过 64 MiB 安全限制"));
        }
        return Ok(copy_guard.commit());
    }
    Err(anyhow!("资源冲突重命名已达到 1,000 次上限"))
}

/// 解析复制策略对应的目标目录；目录 canonicalize 在后台调用以避免 UNC 阻塞 UI。
fn resource_copy_target_dir(
    source: &Path,
    document_path: Option<&Path>,
    behavior: ResourceInsertBehavior,
) -> Result<Option<PathBuf>> {
    let Some(document_path) = document_path else {
        if behavior != ResourceInsertBehavior::None {
            return Err(anyhow!("请先保存 Markdown 文档再复制资源"));
        }
        return Ok(None);
    };
    let root = document_path
        .parent()
        .ok_or_else(|| anyhow!("Markdown 文档没有可用父目录"))?;
    let target_dir = match behavior {
        ResourceInsertBehavior::None | ResourceInsertBehavior::CopyToDocumentFolder => {
            root.to_path_buf()
        }
        ResourceInsertBehavior::CopyToAssetsFolder => root.join("assets"),
        ResourceInsertBehavior::CopyToNamedAssetsFolder => {
            let stem = document_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.trim().is_empty())
                .unwrap_or("untitled");
            root.join(format!("{stem}.assets"))
        }
    };
    if behavior == ResourceInsertBehavior::None || same_resource_directory(source, &target_dir) {
        return Ok(None);
    }
    Ok(Some(target_dir))
}

/// 比较源文件父目录与目标目录，失败时保留原路径比较以兼容 UNC 和尚未创建的目录。
fn same_resource_directory(source: &Path, target_dir: &Path) -> bool {
    let Some(parent) = source.parent() else {
        return false;
    };
    parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf())
        == target_dir
            .canonicalize()
            .unwrap_or_else(|_| target_dir.to_path_buf())
}

pub(super) enum DroppedPathKind {
    Open(PathBuf),
    Resource(PathBuf),
    Invalid,
}

/// 把拖放路径的 UNC/网络文件属性读取放到后台，UI 线程只消费已分类的轻量路径结果。
pub(super) fn classify_dropped_paths(paths: &[PathBuf]) -> DroppedPathKind {
    let mut first_resource = None;
    for path in paths {
        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        if crate::document_io::is_markdown_path(path) || crate::document_io::is_image_path(path) {
            return DroppedPathKind::Open(path.clone());
        }
        if first_resource.is_none() {
            first_resource = Some(path.clone());
        }
    }
    first_resource
        .map(DroppedPathKind::Resource)
        .unwrap_or(DroppedPathKind::Invalid)
}

/// 统一判断后台资源结果是否仍属于当前文档，避免仅靠实体存活误提交到新 revision。
pub(in crate::editor) fn resource_materialization_is_current(
    expected_epoch: u64,
    expected_revision: gmark_document::Revision,
    current_epoch: u64,
    current_revision: gmark_document::Revision,
) -> bool {
    expected_epoch == current_epoch && expected_revision == current_revision
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::editor) struct ResourceDropTarget {
    pub(in crate::editor) document_epoch: u64,
    pub(in crate::editor) generation: u64,
    pub(in crate::editor) revision: gmark_document::Revision,
    pub(in crate::editor) tab_id: uuid::Uuid,
    pub(in crate::editor) block_id: gpui::EntityId,
    pub(in crate::editor) selection: std::ops::Range<usize>,
    pub(in crate::editor) selection_reversed: bool,
}

/// 拖放任务同时绑定文档、标签、块和选区；任一变化都说明用户意图已改变，必须丢弃后台结果。
pub(in crate::editor) fn resource_drop_target_is_current(
    expected: &ResourceDropTarget,
    current: &ResourceDropTarget,
) -> bool {
    expected == current
}

/// 资源替换的错误提示也沿用 tab/revision gate，避免迟到失败覆盖新文档状态。
pub(in crate::editor) fn resource_materialization_is_current_for_tab(
    expected_epoch: u64,
    expected_revision: gmark_document::Revision,
    expected_tab_id: uuid::Uuid,
    current_epoch: u64,
    current_revision: gmark_document::Revision,
    current_tab_id: uuid::Uuid,
) -> bool {
    expected_epoch == current_epoch
        && expected_revision == current_revision
        && expected_tab_id == current_tab_id
}

impl Editor {
    /// 在后台完成资源复制与 Markdown 生成，统一限制输入、输出和冲突命名，避免 UNC
    /// canonicalize、文件读取或 create_new 重试占用 GPUI 线程；调用方负责在提交前做 gate。
    pub(in crate::editor) fn materialize_resource_with_limits(
        label: &str,
        source: &Path,
        document_path: Option<&Path>,
        behavior: ResourceInsertBehavior,
        explicit_kind: Option<crate::components::ResourceKind>,
    ) -> Result<(String, crate::resource_io::MaterializedResource)> {
        checked_resource_input_size(source)?;
        let target_dir = resource_copy_target_dir(source, document_path, behavior)?;
        let (target, created) = if let Some(target_dir) = target_dir {
            fs::create_dir_all(&target_dir)
                .with_context(|| format!("无法创建资源目录 '{}'", target_dir.display()))?;
            let name = source
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .ok_or_else(|| anyhow!("资源源没有有效文件名"))?;
            (
                copy_resource_without_overwrite(source, &target_dir, name)?,
                true,
            )
        } else {
            (source.to_path_buf(), false)
        };

        let materialized = crate::resource_io::MaterializedResource {
            path: target,
            created,
        };
        let effective_label = if label.trim().is_empty() {
            source
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .unwrap_or("resource")
        } else {
            label
        };
        let label_size = match u64::try_from(effective_label.len()) {
            Ok(size) => size,
            Err(_) => {
                materialized.cleanup_if_created();
                return Err(anyhow!("资源标签长度溢出"));
            }
        };
        if let Err(error) = checked_resource_output_size(label_size) {
            materialized.cleanup_if_created();
            return Err(error);
        }
        let markdown = match crate::resource_io::resource_markdown_for_path(
            effective_label,
            &materialized.path,
            document_path,
            ResourceInsertBehavior::None,
            explicit_kind,
        ) {
            Ok((markdown, _)) => markdown,
            Err(error) => {
                materialized.cleanup_if_created();
                return Err(error);
            }
        };
        let markdown_size = match u64::try_from(markdown.len()) {
            Ok(size) => size,
            Err(_) => {
                materialized.cleanup_if_created();
                return Err(anyhow!("生成的资源 Markdown 长度溢出"));
            }
        };
        if let Err(error) = checked_resource_output_size(markdown_size) {
            materialized.cleanup_if_created();
            return Err(error);
        }
        Ok((markdown, materialized))
    }
}
