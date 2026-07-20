// @author kongweiguang

//! gmark 进程入口；应用装配与平台生命周期由库门面负责。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = gmark::run() {
        eprintln!("gmark 启动失败: {error:#}");
        std::process::exit(1);
    }
}
