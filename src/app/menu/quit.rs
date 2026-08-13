// @author kongweiguang

//! Application quit coordination.
//!
//! GPUI invokes action handlers while an editor entity is being updated.  A
//! quit request made from that callback must therefore be scheduled for the
//! next application turn before it starts updating editor windows.  This
//! coordinator owns that hand-off and keeps a single, idempotent quit intent
//! for all menu, shortcut, title-bar, and update-restart entry points.

use std::collections::BTreeSet;

use gmark_document_runtime::DocumentId;
use gpui::{App, Global};

/// Why the application is being asked to close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuitIntent {
    UserQuit,
    ApplyUpdate,
}

/// Observable lifecycle of the one in-flight application quit request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuitPhase {
    Idle,
    Scheduled,
    Evaluating,
    AwaitingUser,
    Handoff,
    Completed,
}

/// Result recorded for the most recent request or lifecycle transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuitRequestOutcome {
    Scheduled,
    AlreadyInProgress,
    Vetoed,
    Approved,
    Aborted,
}

/// Process-wide quit state.  Keeping this separate from editor entities makes
/// multi-window requests idempotent and lets tests inspect the decision without
/// actually terminating their test process.
#[derive(Clone, Debug)]
pub(crate) struct QuitCoordinator {
    intent: Option<QuitIntent>,
    phase: QuitPhase,
    last_outcome: Option<QuitRequestOutcome>,
    /// Documents that already received a decision during this process-wide
    /// quit attempt.  DocumentId is deliberately used instead of path/UUID
    /// plumbing so shared views cannot prompt more than once.
    handled_documents: BTreeSet<DocumentId>,
    /// The document(s) currently represented by an unsaved-changes dialog.
    /// They become handled when the dialog resolves and continuation is
    /// scheduled; cancellation clears both sets with the quit intent.
    pending_documents: BTreeSet<DocumentId>,
}

impl Default for QuitCoordinator {
    fn default() -> Self {
        Self {
            intent: None,
            phase: QuitPhase::Idle,
            last_outcome: None,
            handled_documents: BTreeSet::new(),
            pending_documents: BTreeSet::new(),
        }
    }
}

impl Global for QuitCoordinator {}

impl QuitCoordinator {
    pub(crate) fn ensure(cx: &mut App) {
        if cx.try_global::<Self>().is_none() {
            cx.set_global(Self::default());
        }
    }

    pub(crate) fn begin(cx: &mut App, intent: QuitIntent) -> QuitRequestOutcome {
        Self::ensure(cx);
        let coordinator = cx.global_mut::<Self>();
        if coordinator.phase != QuitPhase::Idle {
            coordinator.last_outcome = Some(QuitRequestOutcome::AlreadyInProgress);
            return QuitRequestOutcome::AlreadyInProgress;
        }

        coordinator.intent = Some(intent);
        coordinator.phase = QuitPhase::Scheduled;
        coordinator.last_outcome = Some(QuitRequestOutcome::Scheduled);
        coordinator.handled_documents.clear();
        coordinator.pending_documents.clear();
        QuitRequestOutcome::Scheduled
    }

    pub(crate) fn schedule_continuation(cx: &mut App) -> bool {
        Self::ensure(cx);
        let coordinator = cx.global_mut::<Self>();
        if coordinator.intent.is_none() || !matches!(coordinator.phase, QuitPhase::AwaitingUser) {
            return false;
        }
        // A save/discard callback schedules the next evaluation only after its
        // pending document has been resolved.  Moving the ids here keeps the
        // operation idempotent when multiple windows share one Controller.
        coordinator.resolve_pending_documents();
        coordinator.phase = QuitPhase::Scheduled;
        true
    }

    pub(crate) fn begin_evaluation(cx: &mut App) -> Option<QuitIntent> {
        Self::ensure(cx);
        let coordinator = cx.global_mut::<Self>();
        if coordinator.phase != QuitPhase::Scheduled {
            return None;
        }
        let intent = coordinator.intent?;
        coordinator.phase = QuitPhase::Evaluating;
        Some(intent)
    }

    pub(crate) fn veto(cx: &mut App) {
        Self::ensure(cx);
        let coordinator = cx.global_mut::<Self>();
        if coordinator.intent.is_some() {
            coordinator.phase = QuitPhase::AwaitingUser;
            coordinator.last_outcome = Some(QuitRequestOutcome::Vetoed);
        }
    }

    pub(crate) fn mark_handoff(cx: &mut App) {
        Self::ensure(cx);
        let coordinator = cx.global_mut::<Self>();
        coordinator.phase = QuitPhase::Handoff;
        coordinator.last_outcome = Some(QuitRequestOutcome::Approved);
    }

    pub(crate) fn complete(cx: &mut App) {
        Self::ensure(cx);
        let coordinator = cx.global_mut::<Self>();
        coordinator.intent = None;
        coordinator.phase = QuitPhase::Completed;
        coordinator.last_outcome = Some(QuitRequestOutcome::Approved);
        coordinator.handled_documents.clear();
        coordinator.pending_documents.clear();
    }

    /// Aborts a pending request after Keep Editing, a save failure, or an
    /// unresolved external-file conflict.  No helper is started by this
    /// operation; a later update click may create a fresh intent.
    pub(crate) fn abort(cx: &mut App) -> bool {
        Self::ensure(cx);
        let coordinator = cx.global_mut::<Self>();
        if coordinator.phase == QuitPhase::Idle {
            return false;
        }
        coordinator.intent = None;
        coordinator.phase = QuitPhase::Idle;
        coordinator.last_outcome = Some(QuitRequestOutcome::Aborted);
        coordinator.handled_documents.clear();
        coordinator.pending_documents.clear();
        true
    }

    /// Registers the document shown by an unsaved-changes dialog.  A second
    /// view over the same Controller is not allowed to open another dialog.
    pub(crate) fn mark_document_pending(cx: &mut App, document_id: DocumentId) -> bool {
        Self::ensure(cx);
        cx.global_mut::<Self>().mark_pending(document_id)
    }

    pub(crate) fn is_document_handled(cx: &App, document_id: DocumentId) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|coordinator| coordinator.handled_documents.contains(&document_id))
    }

    #[cfg(test)]
    pub(crate) fn pending_document_count(&self) -> usize {
        self.pending_documents.len()
    }

    #[cfg(test)]
    pub(crate) fn handled_document_count(&self) -> usize {
        self.handled_documents.len()
    }

    fn mark_pending(&mut self, document_id: DocumentId) -> bool {
        if self.handled_documents.contains(&document_id) {
            return false;
        }
        self.pending_documents.insert(document_id)
    }

    fn resolve_pending_documents(&mut self) {
        let pending = std::mem::take(&mut self.pending_documents);
        self.handled_documents.extend(pending);
    }

    pub(crate) fn is_pending(cx: &App) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|coordinator| coordinator.intent.is_some())
    }

    // 原因：更新服务与回归测试共享此只读诊断入口；当状态查询统一经事件接口后移除。
    #[allow(dead_code)]
    pub(crate) fn is_pending_apply_update(cx: &App) -> bool {
        cx.try_global::<Self>().is_some_and(|coordinator| {
            coordinator.intent == Some(QuitIntent::ApplyUpdate)
                && !matches!(coordinator.phase, QuitPhase::Completed | QuitPhase::Idle)
        })
    }

    // 原因：退出协调回归测试需要观察当前意图；当提供稳定测试快照接口后移除。
    #[allow(dead_code)]
    pub(crate) fn intent(cx: &App) -> Option<QuitIntent> {
        cx.try_global::<Self>()
            .and_then(|coordinator| coordinator.intent)
    }

    // 原因：退出协调回归测试需要观察阶段；当提供稳定测试快照接口后移除。
    #[allow(dead_code)]
    pub(crate) fn phase(cx: &App) -> QuitPhase {
        cx.try_global::<Self>()
            .map_or(QuitPhase::Idle, |coordinator| coordinator.phase)
    }

    // 原因：退出协调回归测试需要核对取消语义；当提供稳定测试快照接口后移除。
    #[allow(dead_code)]
    pub(crate) fn last_outcome(cx: &App) -> Option<QuitRequestOutcome> {
        cx.try_global::<Self>()
            .and_then(|coordinator| coordinator.last_outcome)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/app/menu/quit.rs"]
mod tests;
