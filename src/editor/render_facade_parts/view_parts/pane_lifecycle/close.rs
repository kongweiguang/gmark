// @author kongweiguang

//! Pane save-and-close request lifecycle.
//!
//! The parent pane module owns canvas/model synchronization; this submodule
//! owns only the bounded save completion bridge and request identity checks.

use super::*;
use futures::channel::oneshot;
use std::sync::{Arc, Mutex};

const PANE_CLOSE_SAVE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneCloseSaveOutcome {
    Succeeded,
    Failed,
    /// Host saves publish only a queue transition.  The host entity is read
    /// once after this notification to distinguish a clean save from an error.
    Settled,
    TimedOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PaneCloseRequest {
    pub(super) generation: u64,
    pub(super) pane: crate::editor::panes::PaneId,
    pub(super) tab: crate::editor::panes::TabId,
}

impl PaneCloseRequest {
    /// Carries one immutable identity through the async save boundary so a
    /// completion cannot accidentally act on a later request for the same tab.
    fn new(
        generation: u64,
        pane: crate::editor::panes::PaneId,
        tab: crate::editor::panes::TabId,
    ) -> Self {
        Self {
            generation,
            pane,
            tab,
        }
    }
}

type PaneCloseSaveSignal = Arc<Mutex<Option<oneshot::Sender<u8>>>>;

/// Creates a one-shot bridge because save completion is an event, not a state
/// that needs a timer-driven observer.
fn pane_close_save_channel() -> (PaneCloseSaveSignal, oneshot::Receiver<u8>) {
    let (sender, receiver) = oneshot::channel();
    (Arc::new(Mutex::new(Some(sender))), receiver)
}

/// Sends at most one result; duplicate save callbacks are harmless and do not
/// wake a request that has already been consumed or cancelled.
fn complete_pane_close_save(signal: &PaneCloseSaveSignal, outcome: PaneCloseSaveOutcome) {
    let mut sender = match signal.lock() {
        Ok(sender) => sender,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(sender) = sender.take() {
        let _ = sender.send(match outcome {
            PaneCloseSaveOutcome::Succeeded => 1,
            PaneCloseSaveOutcome::Failed => 2,
            PaneCloseSaveOutcome::Settled => 3,
            PaneCloseSaveOutcome::TimedOut => 4,
        });
    }
}

/// Treats Host's repeated StateChanged notifications as terminal only after
/// saving stops; the save-start notification is intentionally still busy.
fn host_save_event_is_terminal(
    event: &crate::document_host::DocumentHostEvent,
    busy: bool,
) -> bool {
    matches!(event, crate::document_host::DocumentHostEvent::StateChanged) && !busy
}

/// Returns whether a completion is still allowed to close the exact tab that
/// started it.  Clearing the current request after the first close makes this
/// check idempotent even if an already queued callback arrives twice.
fn should_close_pane_tab(
    current: Option<PaneCloseRequest>,
    request: PaneCloseRequest,
    outcome: PaneCloseSaveOutcome,
    tab_exists: bool,
) -> bool {
    current == Some(request) && tab_exists && matches!(outcome, PaneCloseSaveOutcome::Succeeded)
}

impl Editor {
    /// Invalidates every previous save observer while retaining the visible
    /// target so a new Save action can install a fresh request generation.
    pub(in crate::editor) fn invalidate_pane_close_save(&mut self, cx: &mut Context<Self>) {
        self.pane_close_generation = self.pane_close_generation.wrapping_add(1).max(1);
        self.pane_close_save_task = None;
        self.pane_close_save_subscription = None;
        self.pane_close_save_signal = None;
        self.clear_pane_close_save_source(cx);
    }

    /// Bridges the existing Markdown save callbacks to the parent one-shot;
    /// numeric codes keep this method callable without exposing lifecycle
    /// internals to the tab module.
    pub(in crate::editor) fn signal_pane_close_save(&mut self, outcome: u8) {
        let Some(signal) = self.pane_close_save_signal.take() else {
            return;
        };
        let outcome = match outcome {
            1 => PaneCloseSaveOutcome::Succeeded,
            3 => PaneCloseSaveOutcome::Settled,
            4 => PaneCloseSaveOutcome::TimedOut,
            _ => PaneCloseSaveOutcome::Failed,
        };
        complete_pane_close_save(&signal, outcome);
    }

    /// Starts a new request identity after cancelling any older observer. The
    /// generation is the authority used by the completion task, not the
    /// mutable active-tab index.
    pub(super) fn begin_pane_close_request(
        &mut self,
        pane: crate::editor::panes::PaneId,
        tab: crate::editor::panes::TabId,
        cx: &mut Context<Self>,
    ) -> PaneCloseRequest {
        self.invalidate_pane_close_save(cx);
        self.pane_close_target = Some((pane, tab));
        PaneCloseRequest::new(self.pane_close_generation, pane, tab)
    }

    /// Clears the request only after the model close succeeds, so one-shot
    /// completion callbacks cannot close the replacement tab a second time.
    pub(super) fn finish_pane_close_request(&mut self, cx: &mut Context<Self>) {
        self.pane_close_generation = self.pane_close_generation.wrapping_add(1).max(1);
        self.pane_close_target = None;
        self.pane_close_save_subscription = None;
        self.pane_close_save_signal = None;
        self.clear_pane_close_save_source(cx);
    }

    /// Clears the child-side sender because dropping only the parent receiver
    /// would leave a stale pane-close mode in the Markdown child Editor.
    fn clear_pane_close_save_source(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.pane_close_save_markdown_editor.take() else {
            return;
        };
        editor.update(cx, |editor, _cx| {
            editor.pane_close_save_signal = None;
        });
    }

    /// Cancels a pending close when a pane operation changes the requested
    /// tab's identity or location; a late save result must not close after the
    /// user has selected or moved a different pane tab.
    pub(super) fn invalidate_pane_close_for_event(
        &mut self,
        event: &crate::editor::panes::PaneEvent,
        cx: &mut Context<Self>,
    ) {
        let Some((pending_pane, pending_tab)) = self.pane_close_target else {
            return;
        };
        let changed = match event {
            crate::editor::panes::PaneEvent::ActivateTab { pane, tab } => {
                *pane == pending_pane && *tab != pending_tab
            }
            crate::editor::panes::PaneEvent::Close { pane } => *pane == pending_pane,
            crate::editor::panes::PaneEvent::MoveTab { source, tab, .. } => {
                *source == pending_pane && *tab == pending_tab
            }
            _ => false,
        };
        if changed {
            self.invalidate_pane_close_save(cx);
            self.pane_close_target = None;
        }
    }

    /// Installs the one-shot completion bridge before dispatching Save, so both
    /// Markdown and Host saves have identical close semantics.
    pub(in crate::editor) fn start_pane_tab_save(
        &mut self,
        pane: crate::editor::panes::PaneId,
        tab: crate::editor::panes::TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.pane_workspace.clone() else {
            return;
        };
        let request = self.begin_pane_close_request(pane, tab, cx);
        let active = workspace
            .read(cx)
            .workspace()
            .pane(pane)
            .and_then(|state| state.active_tab_id());
        if active != Some(tab) {
            if workspace
                .update(cx, |workspace, _cx| {
                    workspace.workspace_mut().focus(pane)?;
                    workspace.workspace_mut().set_active_tab(pane, tab)
                })
                .is_err()
            {
                self.invalidate_pane_close_save(cx);
                self.pane_close_target = None;
                return;
            }
            self.detach_all_pane_host_canvases(cx);
            self.sync_pane_canvas_entities(cx);
        }
        let markdown_canvas =
            self.pane_canvas_entities
                .borrow()
                .get(&pane)
                .and_then(|(_, _, canvas)| match canvas {
                    crate::editor::panes::PaneCanvasEntity::Markdown(entity) => {
                        Some(entity.clone())
                    }
                    crate::editor::panes::PaneCanvasEntity::DocumentHost(_)
                    | crate::editor::panes::PaneCanvasEntity::ReadOnly(_) => None,
                });
        let host_canvas =
            self.pane_canvas_entities
                .borrow()
                .get(&pane)
                .and_then(|(_, _, canvas)| match canvas {
                    crate::editor::panes::PaneCanvasEntity::DocumentHost(entity) => {
                        Some(entity.clone())
                    }
                    crate::editor::panes::PaneCanvasEntity::Markdown(_)
                    | crate::editor::panes::PaneCanvasEntity::ReadOnly(_) => None,
                });
        if markdown_canvas.is_none() && host_canvas.is_none() {
            self.invalidate_pane_close_save(cx);
            self.pane_close_target = None;
            return;
        }
        let (signal, receiver) = pane_close_save_channel();
        self.pane_close_save_signal = Some(signal.clone());
        if let Some(entity) = markdown_canvas {
            let editor = entity.read(cx).editor();
            self.pane_close_save_markdown_editor = Some(editor.clone());
            editor.update(cx, |editor, cx| {
                editor.pane_close_save_signal = Some(signal.clone());
                editor.save_document(window, cx);
            });
            self.await_pane_tab_save(request, receiver, None, workspace, cx);
        } else if let Some(entity) = host_canvas {
            let host = entity.read(cx).host();
            let signal_for_host = signal.clone();
            // The Host already emits StateChanged at save start and terminal
            // completion; subscribing here avoids reaching into its private
            // Controller while still waking the close task exactly once.
            self.pane_close_save_subscription =
                Some(cx.subscribe(&host, move |_editor, view, event, cx| {
                    if host_save_event_is_terminal(
                        event,
                        view.read(cx).accessibility_snapshot(cx).busy,
                    ) {
                        complete_pane_close_save(&signal_for_host, PaneCloseSaveOutcome::Settled);
                    }
                }));
            host.update(cx, |host, cx| {
                host.on_save_document(&crate::components::SaveDocument, window, cx);
            });
            self.await_pane_tab_save(request, receiver, Some(host), workspace, cx);
        } else {
            self.invalidate_pane_close_save(cx);
            self.pane_close_target = None;
        }
    }

    /// Waits once for save completion or the bounded timeout; cancellation is
    /// provided by dropping this task when the request generation changes.
    pub(super) fn await_pane_tab_save(
        &mut self,
        request: PaneCloseRequest,
        receiver: oneshot::Receiver<u8>,
        host: Option<Entity<crate::document_host::DocumentHost>>,
        workspace: Entity<crate::editor::panes::PaneWorkspaceView>,
        cx: &mut Context<Self>,
    ) {
        let weak = cx.entity().downgrade();
        self.pane_close_save_task = Some(cx.spawn(async move |_, cx| {
            let completion = futures::future::select(
                Box::pin(receiver),
                Box::pin(cx.background_executor().timer(PANE_CLOSE_SAVE_TIMEOUT)),
            )
            .await;
            let outcome = match completion {
                futures::future::Either::Left((Ok(outcome), _)) => match outcome {
                    1 => PaneCloseSaveOutcome::Succeeded,
                    3 => PaneCloseSaveOutcome::Settled,
                    _ => PaneCloseSaveOutcome::Failed,
                },
                futures::future::Either::Left((Err(_), _))
                | futures::future::Either::Right((_, _)) => PaneCloseSaveOutcome::TimedOut,
            };
            let _ = weak.update(cx, |editor, cx| {
                let current = editor.pane_close_target.map(|(pane, tab)| {
                    PaneCloseRequest::new(editor.pane_close_generation, pane, tab)
                });
                let tab_exists = workspace
                    .read(cx)
                    .workspace()
                    .tab(request.pane, request.tab)
                    .is_some();
                let outcome = if matches!(outcome, PaneCloseSaveOutcome::Settled) {
                    host.as_ref()
                        .map(|host| {
                            let snapshot = host.read(cx).accessibility_snapshot(cx);
                            if !snapshot.busy && !snapshot.dirty {
                                PaneCloseSaveOutcome::Succeeded
                            } else {
                                PaneCloseSaveOutcome::Failed
                            }
                        })
                        .unwrap_or(PaneCloseSaveOutcome::Failed)
                } else {
                    outcome
                };
                if !should_close_pane_tab(current, request, outcome, tab_exists) {
                    if current == Some(request) {
                        editor.pane_close_target = None;
                        editor.pane_close_generation =
                            editor.pane_close_generation.wrapping_add(1).max(1);
                        editor.pane_close_save_subscription = None;
                        editor.pane_close_save_signal = None;
                        editor.clear_pane_close_save_source(cx);
                        // Keep the existing save/error surface visible; only
                        // the close request is abandoned after failure/timeout.
                        cx.notify();
                    }
                    return;
                }
                editor.pane_close_save_subscription = None;
                editor.pane_close_save_signal = None;
                editor.clear_pane_close_save_source(cx);
                editor.close_pane_tab_now(&workspace, request.pane, request.tab, cx);
            });
        }));
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/editor/pane_close_lifecycle.rs"]
mod tests;
