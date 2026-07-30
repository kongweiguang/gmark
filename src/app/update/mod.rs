// @author kongweiguang

//! Application-wide update coordinator.
//!
//! 多窗口共享一个权威状态与一个后台 worker；UI 只能发送命令，网络、缓存和状态转换
//! 都集中在这里，避免两个窗口同时下载或启动两次安装事务。

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::StreamExt as _;
use futures::channel::{mpsc, oneshot};
use gmark_update_core::{
    ApplyPlanV1, CancellationV1, HelperSignalV1, clear_helper_signal, parse_apply_result,
    write_apply_plan,
};
use gpui::{App, AppContext as _, AsyncApp, Context, Entity, Global, Task};

use crate::net::update_v2::{
    self, CheckOrigin, CheckOutcome, DownloadControl, DownloadEvent, UpdateRelease,
};

const AUTO_CHECK_DELAY: Duration = Duration::from_secs(10);
const AUTO_CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

mod coordinator;
mod install;
mod service;
mod state;

pub(crate) use coordinator::UpdateCoordinator;
use install::*;
use service::UpdateService;
use state::UpdateCommand;
pub(crate) use state::UpdateState;

#[cfg(test)]
#[path = "../../../tests/unit/updater/mod.rs"]
mod tests;
