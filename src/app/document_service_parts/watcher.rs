// @author kongweiguang

//! Process-level watcher state.
//!
//! Keeping watcher state small and independent lets the service own setup while
//! the event loop owns classification and cleanup helpers.

use std::sync::{Arc, Mutex, Weak};

use gmark_document_runtime::{DocumentHandle, DocumentRegistryKey, SaveStateCallbackRegistration};
use gmark_paged_document::OpenProbe;

use crate::editor::services::file_watch::FileWatchGuard;

pub(super) struct WatcherEntry {
    pub(super) handle: DocumentHandle,
    pub(super) registration: WatcherRegistration,
    pub(super) control: Arc<WatcherControl>,
}

pub(super) type WatcherRegistration =
    Arc<Mutex<(DocumentRegistryKey, Arc<()>, Weak<WatcherControl>)>>;

pub(super) struct WatcherControl {
    pub(super) guard: Mutex<Option<FileWatchGuard>>,
    pub(super) save_registration: Mutex<Option<SaveStateCallbackRegistration>>,
    pub(super) pending_changed: std::sync::atomic::AtomicBool,
}

impl WatcherControl {
    /// Releases both callback and OS watcher ownership so the worker can end
    /// without leaving a process-level resource behind.
    pub(super) fn stop(&self) {
        let registration = match self.save_registration.lock() {
            Ok(mut registration) => registration.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        drop(registration);
        let guard = match self.guard.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        drop(guard);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ProbeKey {
    pub(super) key: DocumentRegistryKey,
    pub(super) max_resident_bytes: u64,
    pub(super) force_safe_source: bool,
}

pub(super) enum ProbeSlotState {
    Opening,
    Ready(OpenProbe),
    Failed(String),
    /// The owner may still finish, but no later generation may reuse its result.
    Abandoned,
}

pub(super) struct ProbeSlot {
    pub(super) state: Mutex<ProbeSlotState>,
    pub(super) ready: std::sync::Condvar,
}
