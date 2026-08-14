// @author kongweiguang

//! Recent-file persistence and menu refresh.

use super::*;

use std::time::Duration;

use futures::future::{Either, select};

const RECENT_HISTORY_DEADLINE: Duration = Duration::from_secs(30);

/// 启动菜单先使用空快照，再由 worker 读取有界历史并回到 UI 安装结果。
// 原因：菜单初始化发生在首帧前，不能让慢盘历史读取占用 GPUI 回调。
pub(crate) fn load_recent_files_in_background(cx: &mut App) {
    cx.spawn(async move |cx| {
        let result = match select(
            cx.background_spawn(async { crate::config::read_recent_files() }),
            cx.background_executor().timer(RECENT_HISTORY_DEADLINE),
        )
        .await
        {
            Either::Left((result, _timer)) => result,
            Either::Right((_elapsed, _reading)) => {
                eprintln!("timed out reading recent file history");
                return;
            }
        };
        match result {
            Ok(recent_files) => {
                let _ = cx.update(|cx| {
                    install_menus_with_recent_files(cx, recent_files);
                    cx.refresh_windows();
                });
            }
            Err(error) => eprintln!("failed to read recent file history: {error}"),
        }
    })
    .detach();
}

/// 成功的文件操作把历史读改写交给单独 worker，并以返回快照刷新菜单。
// 原因：历史文件既可能位于慢盘，也必须在配置层锁住整个读改写事务。
pub(crate) fn record_recent_file_and_refresh(path: &Path, cx: &mut App) {
    let path = path.to_path_buf();
    cx.spawn(async move |cx| {
        let result = match select(
            cx.background_spawn(async move { crate::config::record_recent_file(&path) }),
            cx.background_executor().timer(RECENT_HISTORY_DEADLINE),
        )
        .await
        {
            Either::Left((result, _timer)) => result,
            Either::Right((_elapsed, _writing)) => {
                eprintln!("timed out updating recent file history");
                return;
            }
        };
        match result {
            Ok(recent_files) => {
                let _ = cx.update(|cx| {
                    install_menus_with_recent_files(cx, recent_files);
                    cx.refresh_windows();
                });
            }
            Err(error) => eprintln!("failed to update recent file history: {error}"),
        }
    })
    .detach();
}

/// 缺失的历史项在 worker 中移除，保持 UI 提示与持久化清理的先后关系。
// 原因：即使只是删除一行历史，也会读改写整个文件，不能在菜单动作线程执行。
pub(crate) fn remove_recent_file_and_refresh(path: &Path, cx: &mut App) {
    let path = path.to_path_buf();
    cx.spawn(async move |cx| {
        let result = match select(
            cx.background_spawn(async move { crate::config::remove_recent_file(&path) }),
            cx.background_executor().timer(RECENT_HISTORY_DEADLINE),
        )
        .await
        {
            Either::Left((result, _timer)) => result,
            Either::Right((_elapsed, _writing)) => {
                eprintln!("timed out removing recent file history");
                return;
            }
        };
        match result {
            Ok(recent_files) => {
                let _ = cx.update(|cx| {
                    install_menus_with_recent_files(cx, recent_files);
                    cx.refresh_windows();
                });
            }
            Err(error) => eprintln!("failed to remove recent file history: {error}"),
        }
    })
    .detach();
}
