// @author kongweiguang

//! Watcher event loop and external-change classification.
//!
//! Keeping classification here gives file events one shared source of truth
//! regardless of whether the opened controller is resident or paged.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use futures::StreamExt as _;
use gmark_document_core::{
    DocumentBackendKind, DocumentProfile, DocumentRevision, LoadingLimits, LoadingPolicy,
    TextEncoding,
};
use gmark_document_runtime::{
    DocumentCommand, DocumentHandle, DocumentRegistryKey, FileIdentity, SaveStateNotification,
};
use gmark_paged_document::{LineIndex, OpenProbe};

use crate::editor::services::file_watch::FileWatchSignal;

use super::runtime::build_host_session;
use super::watcher::{ProbeKey, ProbeSlot, WatcherControl, WatcherEntry, WatcherRegistration};

/// The service sibling owns watcher threads, so the event loop is visible to
/// that sibling while remaining private to the document-service module.
pub(crate) async fn run_watcher(
    control: Arc<WatcherControl>,
    receiver: futures::channel::mpsc::UnboundedReceiver<FileWatchSignal>,
    save_receiver: futures::channel::mpsc::UnboundedReceiver<SaveStateNotification>,
    path: PathBuf,
    key: DocumentRegistryKey,
    handle: DocumentHandle,
    probe: Option<OpenProbe>,
    token: Arc<()>,
    watchers: Weak<Mutex<BTreeMap<DocumentRegistryKey, WatcherEntry>>>,
    probes: Weak<Mutex<BTreeMap<ProbeKey, Arc<ProbeSlot>>>>,
) {
    let mut receiver = receiver.fuse();
    let mut save_receiver = save_receiver.fuse();
    let mut save_closed = false;
    let mut pending_changed = false;
    loop {
        if watchers.upgrade().is_none() {
            clear_probe_entries(&probes, &key);
            return;
        }
        if save_closed {
            match receiver.next().await {
                Some(FileWatchSignal::Changed) => {
                    match process_external_change(&handle, &path, probe.as_ref()) {
                        Ok(deferred) => {
                            pending_changed = deferred;
                            control
                                .pending_changed
                                .store(pending_changed, std::sync::atomic::Ordering::Release);
                        }
                        Err(error) => {
                            pending_changed = false;
                            control
                                .pending_changed
                                .store(false, std::sync::atomic::Ordering::Release);
                            eprintln!(
                                "failed to process external change for '{}': {error}",
                                path.display()
                            );
                            dispatch_external_conflict(&handle, &path);
                        }
                    }
                }
                Some(FileWatchSignal::Error(detail)) => {
                    pending_changed = false;
                    control
                        .pending_changed
                        .store(false, std::sync::atomic::Ordering::Release);
                    eprintln!("file watcher error for '{}': {detail}", path.display());
                    dispatch_external_conflict(&handle, &path);
                }
                None => {
                    let removed = remove_watcher(&watchers, &key, &token);
                    if removed {
                        clear_probe_entries(&probes, &key);
                    }
                    return;
                }
            }
            continue;
        }

        futures::select! {
            signal = receiver.next() => match signal {
                Some(FileWatchSignal::Changed) => {
                    match process_external_change(&handle, &path, probe.as_ref()) {
                        Ok(deferred) => {
                            pending_changed = deferred;
                            control
                                .pending_changed
                                .store(pending_changed, std::sync::atomic::Ordering::Release);
                        }
                        Err(error) => {
                            pending_changed = false;
                            control.pending_changed.store(
                                false,
                                std::sync::atomic::Ordering::Release,
                            );
                            eprintln!(
                                "failed to process external change for '{}': {error}",
                                path.display()
                            );
                            dispatch_external_conflict(&handle, &path);
                        }
                    }
                }
                Some(FileWatchSignal::Error(detail)) => {
                    pending_changed = false;
                    control
                        .pending_changed
                        .store(false, std::sync::atomic::Ordering::Release);
                    eprintln!("file watcher error for '{}': {detail}", path.display());
                    dispatch_external_conflict(&handle, &path);
                }
                None => {
                    let removed = remove_watcher(&watchers, &key, &token);
                    if removed {
                        clear_probe_entries(&probes, &key);
                    }
                    return;
                }
            },
            notification = save_receiver.next() => match notification {
                Some(_) if pending_changed => {
                    match process_external_change(&handle, &path, probe.as_ref()) {
                        Ok(deferred) => {
                            pending_changed = deferred;
                            control
                                .pending_changed
                                .store(pending_changed, std::sync::atomic::Ordering::Release);
                        }
                        Err(error) => {
                            pending_changed = false;
                            control.pending_changed.store(
                                false,
                                std::sync::atomic::Ordering::Release,
                            );
                            eprintln!(
                                "failed to process deferred external change for '{}': {error}",
                                path.display()
                            );
                            dispatch_external_conflict(&handle, &path);
                        }
                    }
                }
                Some(_) => {}
                None => {
                    save_closed = true;
                }
            },
        }
    }
}

#[derive(Clone)]
struct WatchSnapshot {
    revision: DocumentRevision,
    save_in_flight_revision: Option<DocumentRevision>,
    identity: FileIdentity,
    dirty: bool,
    backend: DocumentBackendKind,
    profile: DocumentProfile,
    loading_limits: LoadingLimits,
}

fn capture_watch_snapshot(handle: &DocumentHandle) -> Result<WatchSnapshot, String> {
    let controller = handle.lock().map_err(|error| error.to_string())?;
    let session = controller.session();
    Ok(WatchSnapshot {
        revision: DocumentRevision(session.revision()),
        save_in_flight_revision: controller.save_in_flight_revision(),
        identity: session.file_identity.clone(),
        dirty: session.dirty,
        backend: session.store.kind(),
        profile: session.profile.clone(),
        loading_limits: session.loading_limits,
    })
}

/// Parent-level tests and wiring reuse the same classification path as workers
/// to keep external-change behavior consistent across all entry points.
pub(crate) fn process_external_change(
    handle: &DocumentHandle,
    path: &Path,
    _probe: Option<&OpenProbe>,
) -> Result<bool, String> {
    let snapshot = capture_watch_snapshot(handle)?;
    if snapshot.save_in_flight_revision.is_some() {
        // complete_save/fail_save will publish the final identity.  Defer
        // this debounced signal instead of classifying an atomic-save rename
        // as an external replacement while bytes are still in flight.
        return Ok(true);
    }
    let source = gmark_paged_document::FileSource::open(path).map_err(|error| error.to_string())?;
    let identity = FileIdentity::from(&source.identity().map_err(|error| error.to_string())?);

    if identity == snapshot.identity {
        return Ok(false);
    }
    let change = classify_external_change(&snapshot.identity, &identity);
    if matches!(
        change,
        gmark_paged_document::ExternalChange::Appended { .. }
    ) && !snapshot.dirty
        && matches!(snapshot.profile.encoding, TextEncoding::Utf8 { .. })
    {
        let index = LineIndex::build(&source).map_err(|error| error.to_string())?;
        return handle
            .accept_external_append(
                snapshot.revision,
                snapshot.identity,
                source,
                index,
                identity,
            )
            .map(|_| false)
            .map_err(|error| format!("failed to accept external append: {error}"));
    }

    if snapshot.dirty {
        dispatch_external_conflict_identity(handle, identity);
        return Ok(false);
    }

    let policy = LoadingPolicy {
        max_resident_bytes: Some(snapshot.loading_limits.max_resident_bytes),
        force_safe_source: snapshot.backend == DocumentBackendKind::Paged,
    };
    let probe = crate::document_io::probe_document_with_policy(path, policy)
        .map_err(|error| error.to_string())?;
    let prepared_source =
        gmark_paged_document::FileSource::open(path).map_err(|error| error.to_string())?;
    let prepared =
        gmark_paged_document::prepare_utf8_source(prepared_source, probe.encoding.clone())
            .map_err(|error| error.to_string())?;
    let session = build_host_session(probe, prepared).map_err(|error| error.to_string())?;
    handle
        .reload_prepared_document(snapshot.revision, snapshot.identity, session)
        .map(|_| false)
        .map_err(|error| format!("failed to reload externally changed document: {error}"))
}

fn classify_external_change(
    expected: &FileIdentity,
    current: &FileIdentity,
) -> gmark_paged_document::ExternalChange {
    if current.platform_id != expected.platform_id {
        return gmark_paged_document::ExternalChange::Replaced;
    }
    if current.len > expected.len {
        return gmark_paged_document::ExternalChange::Appended {
            from: expected.len,
            to: current.len,
        };
    }
    if current.len < expected.len {
        return gmark_paged_document::ExternalChange::Truncated { len: current.len };
    }
    if current.modified_nanos != expected.modified_nanos {
        gmark_paged_document::ExternalChange::Modified
    } else {
        gmark_paged_document::ExternalChange::Unchanged
    }
}

/// The parent module exposes this helper to focused tests without widening the
/// conflict dispatch contract beyond the document-service module.
pub(crate) fn dispatch_external_conflict(handle: &DocumentHandle, path: &Path) {
    let expected = match handle.lock() {
        Ok(controller) => controller.session().file_identity.clone(),
        Err(error) => {
            eprintln!("failed to inspect external file identity: {error}");
            return;
        }
    };
    let current = gmark_paged_document::FileSource::open(path)
        .and_then(|source| source.identity())
        .map(|identity| FileIdentity::from(&identity));
    let identity = match current {
        Ok(identity) if identity == expected => return,
        Ok(identity) => identity,
        Err(_) => expected,
    };
    dispatch_external_conflict_identity(handle, identity);
}

fn dispatch_external_conflict_identity(handle: &DocumentHandle, identity: FileIdentity) {
    let Ok(mut controller) = handle.lock() else {
        return;
    };
    let _ = controller.dispatch(DocumentCommand::ExternalConflict { identity });
}

/// Sibling service cleanup needs the token-checked removal operation, while
/// callers outside the document-service module must not depend on it.
pub(super) fn remove_watcher(
    watchers: &Weak<Mutex<BTreeMap<DocumentRegistryKey, WatcherEntry>>>,
    key: &DocumentRegistryKey,
    token: &Arc<()>,
) -> bool {
    let Some(watchers) = watchers.upgrade() else {
        return false;
    };
    let mut entries = match watchers.lock() {
        Ok(entries) => entries,
        Err(poisoned) => poisoned.into_inner(),
    };
    if entries.get(key).is_some_and(|entry| {
        let current_token = watcher_registration_parts(&entry.registration).1;
        Arc::ptr_eq(&current_token, token)
    }) {
        entries.remove(key);
        true
    } else {
        false
    }
}

/// Sibling cleanup callbacks read registration state through this narrow helper
/// so the registration tuple stays encapsulated in the watcher module.
pub(super) fn watcher_registration_parts(
    registration: &WatcherRegistration,
) -> (DocumentRegistryKey, Arc<()>, Weak<WatcherControl>) {
    let state = match registration.lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    (state.0.clone(), Arc::clone(&state.1), state.2.clone())
}

/// Service lifecycle paths clear probe state when watcher ownership ends; keep
/// that cache operation scoped to the document-service module.
pub(super) fn clear_probe_entries(
    probes: &Weak<Mutex<BTreeMap<ProbeKey, Arc<ProbeSlot>>>>,
    key: &DocumentRegistryKey,
) {
    let Some(probes) = probes.upgrade() else {
        return;
    };
    let mut entries = match probes.lock() {
        Ok(entries) => entries,
        Err(poisoned) => poisoned.into_inner(),
    };
    entries.retain(|probe_key, _| &probe_key.key != key);
}
