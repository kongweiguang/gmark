// @author kongweiguang

//! Debounced active-block spelling checks. Harper runs on the background executor.

use std::sync::Arc;
use std::time::Duration;

use gpui::*;

use super::Editor;

impl Editor {
    const SPELLCHECK_IDLE_DELAY: Duration = Duration::from_millis(250);

    pub(super) fn schedule_active_block_spellcheck(&mut self, cx: &mut Context<Self>) {
        self.spellcheck_task = None;
        let Some(block) = self.current_edit_target_from_state(cx) else {
            return;
        };
        if !crate::config::EditorSettings::spell_check(cx)
            || matches!(
                self.view_mode,
                super::ViewMode::Source | super::ViewMode::Split
            )
        {
            block.update(cx, |block, cx| {
                if !block.spelling_diagnostics.is_empty() {
                    block.spelling_diagnostics = Arc::default();
                    cx.notify();
                }
            });
            return;
        }
        let (text, eligible) = block.read_with(cx, |block, _cx| {
            (
                block.display_text().to_owned(),
                !block.is_source_raw_mode()
                    && !block.kind().is_atomic_structural()
                    && !block.kind().is_code_block(),
            )
        });
        if !eligible || text.trim().is_empty() {
            block.update(cx, |block, cx| {
                if !block.spelling_diagnostics.is_empty() {
                    block.spelling_diagnostics = Arc::default();
                    cx.notify();
                }
            });
            return;
        }

        let weak_block = block.downgrade();
        self.spellcheck_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor()
                .timer(Self::SPELLCHECK_IDLE_DELAY)
                .await;
            let checked_text = text.clone();
            let diagnostics = cx
                .background_spawn(async move { crate::spellcheck::check_spelling(&checked_text) })
                .await;
            let _ = weak_block.update(cx, |block, cx| {
                // 新输入可能在后台检查期间到达；旧范围不得覆盖新 display text。
                if crate::config::EditorSettings::spell_check(cx) && block.display_text() == text {
                    block.spelling_diagnostics = diagnostics.into();
                    cx.notify();
                }
            });
            let _ = this.update(cx, |editor, _cx| editor.spellcheck_task = None);
        }));
    }
}
