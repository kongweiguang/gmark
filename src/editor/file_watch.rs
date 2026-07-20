// @author kongweiguang

//! Debounced external-file monitoring that survives atomic replacement.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use gpui::*;
use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use super::Editor;

const FILE_WATCH_DEBOUNCE: Duration = Duration::from_millis(250);

pub(super) enum FileWatchSignal {
    Changed,
    Error(String),
}

/// Drop 时只发停止信号，不在 GPUI 线程等待 debouncer thread 的 tick。
pub(super) struct FileWatchGuard {
    debouncer: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
}

impl Drop for FileWatchGuard {
    fn drop(&mut self) {
        if let Some(debouncer) = self.debouncer.take() {
            debouncer.stop_nonblocking();
        }
    }
}

pub(super) fn start_file_watch(
    path: PathBuf,
) -> anyhow::Result<(FileWatchGuard, UnboundedReceiver<FileWatchSignal>)> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let watched_path = path.clone();
    let (sender, receiver) = unbounded();
    let mut debouncer = new_debouncer(
        FILE_WATCH_DEBOUNCE,
        None,
        move |result: DebounceEventResult| match result {
            Ok(events)
                if events.iter().any(|event| {
                    event
                        .event
                        .paths
                        .iter()
                        .any(|event_path| same_watch_path(&watched_path, event_path))
                }) =>
            {
                let _ = sender.unbounded_send(FileWatchSignal::Changed);
            }
            Ok(_) => {}
            Err(errors) => {
                let mut detail = format!("{errors:?}");
                detail.truncate(512);
                let _ = sender.unbounded_send(FileWatchSignal::Error(detail));
            }
        },
    )?;
    debouncer.watch(directory, RecursiveMode::NonRecursive)?;
    Ok((
        FileWatchGuard {
            debouncer: Some(debouncer),
        },
        receiver,
    ))
}

fn same_watch_path(expected: &Path, event_path: &Path) -> bool {
    if expected == event_path {
        return true;
    }
    if let (Ok(expected), Ok(event)) = (expected.canonicalize(), event_path.canonicalize()) {
        if expected == event {
            return true;
        }
    }
    #[cfg(target_os = "windows")]
    {
        expected
            .to_string_lossy()
            .eq_ignore_ascii_case(&event_path.to_string_lossy())
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

impl Editor {
    pub(super) fn restart_file_watcher(&mut self, cx: &mut Context<Self>) {
        self.file_watch_task = None;
        self.file_watch_guard = None;
        if self.source_surface.is_some() {
            // 大文档以 file id、抽样前缀和 generation 区分追加、截断与替换；普通
            // fingerprint watcher 会把合法 tail 追加误报成冲突，因此同一 Tab 只能保留
            // 大文档监控这一条事实来源。
            self.external_file_conflict = false;
            return;
        }
        if cfg!(test) {
            return;
        }
        let Some(path) = self.file_path.clone() else {
            return;
        };
        let (guard, mut receiver) = match start_file_watch(path.clone()) {
            Ok(watch) => watch,
            Err(error) => {
                eprintln!("failed to watch '{}': {error}", path.display());
                return;
            }
        };
        self.file_watch_guard = Some(guard);
        self.file_watch_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            while let Some(signal) = receiver.next().await {
                match signal {
                    FileWatchSignal::Error(detail) => {
                        eprintln!("file watcher error for '{}': {detail}", path.display());
                        let _ = this.update(cx, |editor, cx| {
                            if editor.file_path.as_deref() == Some(path.as_path())
                                && !editor.external_file_conflict
                            {
                                editor.external_file_conflict = true;
                                cx.notify();
                            }
                        });
                    }
                    FileWatchSignal::Changed => {
                        let snapshot = this.update(cx, |editor, _cx| {
                            (
                                editor.file_path.clone(),
                                editor.saved_file_fingerprint.clone(),
                            )
                        });
                        let Ok((Some(current_path), Some(expected))) = snapshot else {
                            continue;
                        };
                        if current_path != path {
                            continue;
                        }
                        let fingerprint_path = current_path.clone();
                        let current = cx
                            .background_spawn(async move {
                                crate::recovery::fingerprint_file(&fingerprint_path)
                            })
                            .await;
                        let _ = this.update(cx, |editor, cx| {
                            // 保存、另存或换文件会替换 expected；旧 fingerprint 结果不得回写。
                            let Some(conflict) = external_conflict_for_snapshot(
                                editor.file_path.as_deref(),
                                editor.saved_file_fingerprint.as_ref(),
                                &current_path,
                                &expected,
                                current.as_ref().map_err(|_| ()),
                            ) else {
                                return;
                            };
                            if editor.external_file_conflict != conflict {
                                editor.external_file_conflict = conflict;
                                cx.notify();
                            }
                        });
                    }
                }
            }
        }));
    }
}

fn external_conflict_for_snapshot(
    active_path: Option<&Path>,
    active_expected: Option<&crate::recovery::FileFingerprint>,
    watched_path: &Path,
    watched_expected: &crate::recovery::FileFingerprint,
    current: Result<&crate::recovery::FileFingerprint, ()>,
) -> Option<bool> {
    if active_path != Some(watched_path) || active_expected != Some(watched_expected) {
        return None;
    }
    Some(match current {
        Ok(value) => value != watched_expected,
        Err(_) => true,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/editor/file_watch.rs"]
mod tests;
