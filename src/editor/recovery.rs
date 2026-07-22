// @author kongweiguang

//! Editor 与崩溃恢复 journal 的 debounce、后台持久化和 checkpoint 接线。

use std::time::Duration;

#[cfg(test)]
use anyhow::Context as _;
use gpui::*;

use super::*;
use crate::recovery::RecoverySelection;

enum RecoveryTimerStep {
    Continue(u64),
    Persist {
        source: String,
        source_format: gmark_document::SourceFormatSnapshot,
        selection: RecoverySelection,
        view_mode: String,
    },
    Stop,
}

impl Editor {
    const RECOVERY_IDLE_DELAY: Duration = Duration::from_secs(2);

    pub(super) fn schedule_recovery_journal(&mut self, cx: &mut Context<Self>) {
        let Some(journal) = self.recovery_journal.clone() else {
            return;
        };
        self.recovery_generation = self.recovery_generation.wrapping_add(1);
        if self.recovery_task.is_some() {
            return;
        }
        let initial_generation = self.recovery_generation;
        self.recovery_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let mut observed_generation = initial_generation;
            loop {
                cx.background_executor()
                    .timer(Self::RECOVERY_IDLE_DELAY)
                    .await;
                let step = this.update(cx, |editor, cx| {
                    if !editor.is_document_dirty() {
                        editor.recovery_task = None;
                        return RecoveryTimerStep::Stop;
                    }
                    if editor.recovery_generation != observed_generation {
                        return RecoveryTimerStep::Continue(editor.recovery_generation);
                    }
                    let snapshot = editor.capture_source_selection_snapshot(cx);
                    RecoveryTimerStep::Persist {
                        source: editor.source_document.text(),
                        source_format: editor.source_document.source_format(),
                        selection: RecoverySelection::from_source_selection(
                            snapshot.source_selection(),
                        ),
                        view_mode: editor.recovery_view_mode_id().to_owned(),
                    }
                });
                let Ok(step) = step else {
                    return;
                };
                match step {
                    RecoveryTimerStep::Continue(generation) => {
                        observed_generation = generation;
                    }
                    RecoveryTimerStep::Stop => return,
                    RecoveryTimerStep::Persist {
                        source,
                        source_format,
                        selection,
                        view_mode,
                    } => {
                        let result = cx
                            .background_spawn(async move {
                                let mut journal = journal.lock().map_err(|_| {
                                    anyhow::anyhow!("recovery journal lock poisoned")
                                })?;
                                journal.record_formatted(
                                    &source,
                                    source_format,
                                    selection,
                                    &view_mode,
                                )
                            })
                            .await;
                        if let Err(error) = result {
                            eprintln!("failed to persist recovery journal: {error}");
                        }
                        // 已无后续 await；此时清 handle 不会取消刚完成的持久化。
                        let _ = this.update(cx, |editor, cx| {
                            editor.recovery_task = None;
                            if editor.is_document_dirty()
                                && editor.recovery_generation != observed_generation
                            {
                                editor.schedule_recovery_journal(cx);
                            }
                        });
                        return;
                    }
                }
            }
        }));
    }

    pub(super) fn checkpoint_recovery_journal(&mut self) {
        self.checkpoint_recovery_journal_with_snapshot(
            self.source_document.text(),
            self.source_document.source_format(),
        );
    }

    pub(super) fn checkpoint_recovery_journal_with_snapshot(
        &mut self,
        source: String,
        source_format: gmark_document::SourceFormatSnapshot,
    ) {
        self.recovery_task = None;
        let Some(journal) = self.recovery_journal.as_ref() else {
            return;
        };
        match journal.lock() {
            Ok(mut journal) => {
                if let Err(error) =
                    journal.checkpoint_formatted(self.file_path.clone(), source, source_format)
                {
                    eprintln!("failed to checkpoint recovery journal: {error}");
                }
            }
            Err(_) => eprintln!("failed to checkpoint recovery journal: lock poisoned"),
        }
    }

    #[cfg(test)]
    pub(super) fn flush_recovery_journal_now(&mut self, cx: &App) -> anyhow::Result<bool> {
        let journal = self
            .recovery_journal
            .as_ref()
            .context("test editor has no recovery journal")?;
        let snapshot = self.capture_source_selection_snapshot(cx);
        let source = self.source_document.text();
        let source_format = self.source_document.source_format();
        let selection = RecoverySelection::from_source_selection(snapshot.source_selection());
        journal
            .lock()
            .map_err(|_| anyhow::anyhow!("recovery journal lock poisoned"))?
            .record_formatted(
                &source,
                source_format,
                selection,
                self.recovery_view_mode_id(),
            )
    }

    fn recovery_view_mode_id(&self) -> &'static str {
        match self.view_mode {
            ViewMode::Rendered => "rendered",
            ViewMode::Source => "source",
            ViewMode::Split => "split",
            ViewMode::Preview => "preview",
        }
    }
}
