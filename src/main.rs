// @author kongweiguang

//! Gmark 进程入口；应用装配与平台生命周期由库门面负责。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// 兼容 Inno 迁移时旧 helper 对根 `gmark.exe --version` 的严格检查；正常启动立即
/// 转交 Velopack 的 `current` 版本，避免根入口在后续更新后继续运行旧业务二进制。
#[cfg(windows)]
fn dispatch_windows_bridge_root() -> Result<bool, String> {
    let executable =
        std::env::current_exe().map_err(|error| format!("无法定位 Windows 启动程序：{error}"))?;
    let Some(root) = executable.parent() else {
        return Ok(false);
    };
    if root
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("current"))
        || !root.join("Update.exe").is_file()
    {
        return Ok(false);
    }
    let current = root.join("current").join("gmark.exe");
    if !current.is_file() {
        return Ok(false);
    }
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() == 1
        && matches!(
            arguments[0].to_str(),
            Some("--version" | "-v" | "--help" | "-h")
        )
    {
        return Ok(false);
    }
    std::process::Command::new(&current)
        .args(arguments)
        .current_dir(root.join("current"))
        .spawn()
        .map_err(|error| format!("无法启动 Velopack 当前版本：{error}"))?;
    Ok(true)
}

/// 让 Velopack 在 GPUI、单实例锁和文档恢复之前处理安装生命周期参数，避免更新器
/// 为了替换正在运行的目录再次复制应用自己的退出与重启状态机。
fn main() {
    #[cfg(windows)]
    match dispatch_windows_bridge_root() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("Gmark 启动失败: {error}");
            std::process::exit(1);
        }
    }
    velopack::VelopackApp::build().run();
    if let Err(error) = gmark::run() {
        eprintln!("Gmark 启动失败: {error:#}");
        std::process::exit(1);
    }
}
