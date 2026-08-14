// @author kongweiguang

//! Velopack adapter fed only by Gmark's already verified update artifact.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use velopack::sources::UpdateSource;
use velopack::{UpdateInfo, UpdateManager, VelopackAsset, VelopackAssetFeed};

use crate::net::update_v2::{ArtifactFormat, UpdateRelease};

const VELOPACK_PACKAGE_ID: &str = "GMark";

#[derive(Clone)]
struct VerifiedPackageSource {
    artifact_path: PathBuf,
    asset: VelopackAsset,
}

impl UpdateSource for VerifiedPackageSource {
    /// 返回由签名 V2 清单构造的单资产 feed，避免 Velopack 再信任一个未签名的远端索引。
    fn get_release_feed(
        &self,
        _channel: &str,
        _app: &velopack::bundle::Manifest,
        _staged_user_id: &str,
    ) -> Result<VelopackAssetFeed, velopack::Error> {
        Ok(VelopackAssetFeed {
            Assets: vec![self.asset.clone()],
        })
    }

    /// 只把 Gmark 已完成 Ed25519、大小和 SHA-256 校验的文件交给 Velopack；
    /// Velopack 随后还会按同一 SHA-256 与大小再验证一次再进入安装缓存。
    fn download_release_entry(
        &self,
        _asset: &VelopackAsset,
        local_file: &Path,
        progress_sender: Option<Sender<i16>>,
    ) -> Result<(), velopack::Error> {
        if let Some(sender) = &progress_sender {
            let _ = sender.send(10);
        }
        std::fs::copy(&self.artifact_path, local_file)?;
        if let Some(sender) = &progress_sender {
            let _ = sender.send(100);
        }
        Ok(())
    }
}

/// 只有 Velopack 能从当前可执行文件定位到受管安装布局时才显示自安装入口，
/// 这样 DEB、源码构建和旧 Inno 直装不会在点击后才退出失败。
pub(super) fn is_managed_install() -> bool {
    UpdateManager::new(velopack::sources::NoneSource {}, None, None)
        .is_ok_and(|manager| manager.get_app_id() == VELOPACK_PACKAGE_ID)
}

/// 将已验签的 nupkg 导入 Velopack 缓存并启动其退出等待器；本函数成功返回后，
/// 调用方只需正常退出，目录替换、失败提示和新版拉起都由 Velopack 负责。
pub(super) fn prepare_install(release: &UpdateRelease, artifact_path: &Path) -> Result<(), String> {
    validate_platform_package(release.artifact_format)?;
    validate_verified_file(artifact_path)?;
    let filename = package_filename(&release.artifact_url)?;

    let asset = VelopackAsset {
        PackageId: VELOPACK_PACKAGE_ID.to_owned(),
        Version: release.version.clone(),
        Type: "Full".to_owned(),
        FileName: filename,
        SHA1: String::new(),
        SHA256: release.artifact_sha256.clone(),
        Size: release.artifact_size,
        NotesMarkdown: release.notes.clone(),
        NotesHtml: String::new(),
    };
    let source = VerifiedPackageSource {
        artifact_path: artifact_path.to_path_buf(),
        asset: asset.clone(),
    };
    let manager = UpdateManager::new(source, None, None)
        .map_err(|error| format!("当前安装不是可自更新的 Velopack 布局：{error}"))?;
    if manager.get_app_id() != VELOPACK_PACKAGE_ID {
        return Err("当前 Velopack 安装标识与 Gmark 不匹配".to_owned());
    }
    let update = UpdateInfo {
        TargetFullRelease: asset.clone(),
        BaseRelease: None,
        DeltasToTarget: Vec::new(),
        IsDowngrade: false,
    };
    manager
        .download_updates(&update, None)
        .map_err(|error| format!("无法把已验证更新交给 Velopack：{error}"))?;
    manager
        .wait_exit_then_apply_updates(&asset, false, true, std::env::args_os().skip(1))
        .map_err(|error| format!("无法启动 Velopack 更新进程：{error}"))
}

/// 限定每个平台只接受对应的 Velopack 包，避免兼容用 Inno/AppImage/Bundle
/// 误入新安装路径后再次产生两套安装语义。
fn validate_platform_package(format: ArtifactFormat) -> Result<(), String> {
    let expected = if cfg!(target_os = "windows") {
        ArtifactFormat::WindowsVelopackNupkg
    } else if cfg!(target_os = "macos") {
        ArtifactFormat::MacosVelopackNupkg
    } else if cfg!(target_os = "linux") {
        ArtifactFormat::LinuxVelopackNupkg
    } else {
        return Err("当前平台不支持应用内更新".to_owned());
    };
    if format == expected {
        Ok(())
    } else {
        Err("签名清单中的安装包格式不是当前平台的 Velopack 更新包".to_owned())
    }
}

/// 交接前再次拒绝链接或非普通文件，避免下载校验完成后路径被本地替换。
fn validate_verified_file(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("无法检查已验证更新包：{error}"))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err("已验证更新包必须是普通非链接文件".to_owned());
    }
    Ok(())
}

/// 从已限制为官方 GitHub Release 的 URL 取文件名，并再次约束为单段 nupkg 名称，
/// 防止远端字段参与 Velopack 本地缓存路径拼接。
fn package_filename(artifact_url: &str) -> Result<String, String> {
    let url = url::Url::parse(artifact_url).map_err(|error| format!("更新包 URL 无效：{error}"))?;
    let filename = url
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .ok_or_else(|| "更新包 URL 没有文件名".to_owned())?;
    if filename.len() > 180
        || filename == "."
        || filename == ".."
        || !filename.to_ascii_lowercase().ends_with(".nupkg")
        || filename.contains(['/', '\\'])
    {
        return Err("更新包 URL 文件名不是安全的 nupkg 名称".to_owned());
    }
    Ok(filename.to_owned())
}

#[cfg(test)]
#[path = "../../../tests/unit/updater/velopack.rs"]
mod tests;
