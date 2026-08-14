// @author kongweiguang

use super::*;

/// 锁定远端 URL 到本地缓存名的最小边界，避免未来放宽解析时重新引入目录穿越。
#[test]
fn package_filename_accepts_only_single_nupkg_segment() {
    assert_eq!(
        package_filename(
            "https://github.com/kongweiguang/gmark/releases/download/v0.2.2/gmark-v0.2.2-windows-x86_64-full.nupkg"
        ),
        Ok("gmark-v0.2.2-windows-x86_64-full.nupkg".to_owned())
    );
    assert!(package_filename("https://example.com/releases/v0.2.2/").is_err());
    assert!(package_filename("https://example.com/releases/v0.2.2/update.exe").is_err());
    assert!(package_filename("not a url").is_err());
}

/// 锁定当前编译平台只能消费自己的 Velopack 格式，防止三个 nupkg 变体被误当成通用包。
#[test]
fn platform_package_format_is_not_interchangeable() {
    let current = if cfg!(target_os = "windows") {
        ArtifactFormat::WindowsVelopackNupkg
    } else if cfg!(target_os = "macos") {
        ArtifactFormat::MacosVelopackNupkg
    } else {
        ArtifactFormat::LinuxVelopackNupkg
    };
    assert!(validate_platform_package(current).is_ok());
    assert!(validate_platform_package(ArtifactFormat::WindowsSetupExe).is_err());
}
