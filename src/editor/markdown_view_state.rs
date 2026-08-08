// @author kongweiguang

//! Process-local state for rendered Markdown affordances.
//!
//! Fold choices and table widths are presentation state, not document data.
//! This manager intentionally has no serialization or filesystem side effect;
//! closing the process drops every entry.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use gpui::Global;
use uuid::Uuid;

const DEFAULT_CAPACITY: usize = 128;

/// Identity of an open Markdown tab.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum MarkdownTabIdentity {
    /// Saved documents use their canonical path as the reusable snapshot key.
    Saved { path: PathBuf, tab_id: Uuid },
    /// Untitled documents are isolated by tab UUID.
    Untitled { tab_id: Uuid },
}

impl MarkdownTabIdentity {
    pub(crate) fn saved(path: &Path, tab_id: Uuid) -> Self {
        // A Save As target can be registered before the file is committed to
        // disk.  Canonicalization then fails, but using the caller's relative
        // spelling would make the same document receive a second state key
        // when another caller supplies an absolute spelling.  Prefer a
        // deterministic absolute path until the next open can resolve the
        // real filesystem identity.
        let canonical = dunce::canonicalize(path)
            .or_else(|_| std::path::absolute(path))
            .unwrap_or_else(|_| path.to_path_buf());
        Self::Saved {
            path: canonical,
            tab_id,
        }
    }

    pub(crate) fn untitled(tab_id: Uuid) -> Self {
        Self::Untitled { tab_id }
    }

    fn document_key(&self) -> DocumentViewKey {
        match self {
            Self::Saved { path, .. } => DocumentViewKey::Saved(path.clone()),
            Self::Untitled { tab_id } => DocumentViewKey::Untitled(*tab_id),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum DocumentViewKey {
    Saved(PathBuf),
    Untitled(Uuid),
}

/// Snapshot of rendered-only state for one tab.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MarkdownViewState {
    pub(crate) collapsed_headings: HashMap<String, bool>,
    pub(crate) collapsed_callouts: HashMap<String, bool>,
    pub(crate) table_column_widths: HashMap<String, Vec<f32>>,
}

/// Bounded process-local view-state manager.
#[derive(Debug)]
pub(crate) struct MarkdownViewStateManager {
    defaults: HashMap<DocumentViewKey, MarkdownViewState>,
    tabs: HashMap<Uuid, (DocumentViewKey, MarkdownViewState)>,
    lru: VecDeque<DocumentViewKey>,
    capacity: usize,
}

/// Process-wide handle for rendered-only Markdown state.
///
/// Editors are window entities, while the fold/column snapshot intentionally
/// survives opening a second window for the same canonical path.  Keeping the
/// mutable manager behind an `Arc<Mutex<_>>` lets every editor share that
/// lifetime without serializing presentation state into Markdown or a user
/// settings file.  The GPUI global is dropped with the application, so the
/// state is reset on process exit as required by the contract.
#[derive(Clone, Debug)]
pub(crate) struct SharedMarkdownViewState {
    inner: Arc<Mutex<MarkdownViewStateManager>>,
}

impl Global for SharedMarkdownViewState {}

impl Default for SharedMarkdownViewState {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MarkdownViewStateManager::default())),
        }
    }
}

impl SharedMarkdownViewState {
    fn lock(&self) -> MutexGuard<'_, MarkdownViewStateManager> {
        match self.inner.lock() {
            Ok(guard) => guard,
            // A poisoned presentation store must not crash the editor.  The
            // state is process-local and can safely continue from the last
            // in-memory snapshot after recovering the guard.
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(crate) fn open_tab(&self, identity: MarkdownTabIdentity) -> MarkdownViewState {
        self.lock().open_tab(identity)
    }

    pub(crate) fn close_tab(&self, tab_id: Uuid) -> Option<MarkdownViewState> {
        self.lock().close_tab(tab_id)
    }

    pub(crate) fn rekey_tab(
        &self,
        tab_id: Uuid,
        identity: MarkdownTabIdentity,
    ) -> MarkdownViewState {
        self.lock().rekey_tab(tab_id, identity)
    }

    pub(crate) fn state_for_tab(&self, tab_id: Uuid) -> Option<MarkdownViewState> {
        self.lock().state_for_tab(tab_id).cloned()
    }

    pub(crate) fn replace_tab_state(&self, tab_id: Uuid, state: MarkdownViewState) -> bool {
        let mut manager = self.lock();
        let Some(document_key) = manager.tabs.get_mut(&tab_id).map(|(key, tab_state)| {
            *tab_state = state.clone();
            key.clone()
        }) else {
            return false;
        };
        manager.publish_default(document_key, state);
        true
    }

    pub(crate) fn update_tab(
        &self,
        tab_id: Uuid,
        update: impl FnOnce(&mut MarkdownViewState),
    ) -> bool {
        let mut manager = self.lock();
        let Some((document_key, snapshot)) =
            manager
                .tabs
                .get_mut(&tab_id)
                .map(|(document_key, tab_state)| {
                    update(tab_state);
                    (document_key.clone(), tab_state.clone())
                })
        else {
            return false;
        };
        manager.publish_default(document_key, snapshot);
        true
    }

    pub(crate) fn set_heading_collapsed(
        &self,
        tab_id: Uuid,
        key: impl Into<String>,
        collapsed: bool,
    ) {
        self.lock().set_heading_collapsed(tab_id, key, collapsed);
    }

    pub(crate) fn set_callout_collapsed(
        &self,
        tab_id: Uuid,
        key: impl Into<String>,
        collapsed: bool,
    ) {
        self.lock().set_callout_collapsed(tab_id, key, collapsed);
    }

    pub(crate) fn set_table_layout(
        &self,
        tab_id: Uuid,
        key: impl Into<String>,
        fractions: Vec<f32>,
    ) {
        self.lock().set_table_layout(tab_id, key, fractions);
    }

    pub(crate) fn remove_table_layout(&self, tab_id: Uuid, key: &str) {
        self.lock().remove_table_layout(tab_id, key);
    }
}

impl Default for MarkdownViewStateManager {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}

impl MarkdownViewStateManager {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            defaults: HashMap::new(),
            tabs: HashMap::new(),
            lru: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    /// Opens a tab. A saved tab receives a clone of the latest path snapshot;
    /// subsequent mutations stay local to already-open tabs while the latest
    /// snapshot remains available to future duplicates.
    pub(crate) fn open_tab(&mut self, identity: MarkdownTabIdentity) -> MarkdownViewState {
        let tab_id = match &identity {
            MarkdownTabIdentity::Saved { tab_id, .. }
            | MarkdownTabIdentity::Untitled { tab_id } => *tab_id,
        };
        let document_key = identity.document_key();
        let state = self
            .defaults
            .get(&document_key)
            .cloned()
            .unwrap_or_default();
        self.tabs
            .insert(tab_id, (document_key.clone(), state.clone()));
        self.touch(&document_key);
        self.evict();
        state
    }

    /// Changes the document identity of an already-open tab while preserving
    /// its current presentation choices.  Save As uses this path: the tab is
    /// still showing the same document, but future reopen/duplicate tabs must
    /// address the new canonical path instead of the old untitled/saved key.
    pub(crate) fn rekey_tab(
        &mut self,
        tab_id: Uuid,
        identity: MarkdownTabIdentity,
    ) -> MarkdownViewState {
        let key = identity.document_key();
        let old = self.tabs.remove(&tab_id);
        let old_key = old.as_ref().map(|(key, _)| key.clone());
        let state = old
            .map(|(_, state)| state)
            .or_else(|| self.defaults.get(&key).cloned())
            .unwrap_or_default();
        if let Some(old_key) = old_key
            && !self.tabs.values().any(|(tab_key, _)| tab_key == &old_key)
        {
            self.defaults.remove(&old_key);
            self.lru.retain(|candidate| candidate != &old_key);
        }
        self.tabs.insert(tab_id, (key.clone(), state.clone()));
        self.publish_default(key, state.clone());
        state
    }

    pub(crate) fn state_for_tab(&self, tab_id: Uuid) -> Option<&MarkdownViewState> {
        self.tabs.get(&tab_id).map(|(_, state)| state)
    }

    #[cfg(test)]
    pub(crate) fn state_for_tab_mut(&mut self, tab_id: Uuid) -> Option<&mut MarkdownViewState> {
        self.tabs.get_mut(&tab_id).map(|(_, state)| state)
    }

    /// Updates one fold choice without serializing it into the Markdown
    /// document. The tab-local copy remains authoritative for already-open
    /// tabs, while the latest snapshot is published for a future duplicate or
    /// reopen without broadcasting into those existing tabs.
    pub(crate) fn set_heading_collapsed(
        &mut self,
        tab_id: Uuid,
        key: impl Into<String>,
        collapsed: bool,
    ) {
        let fold_key = key.into();
        let Some((document_key, state_snapshot)) =
            self.tabs.get_mut(&tab_id).map(|(document_key, state)| {
                state.collapsed_headings.insert(fold_key, collapsed);
                (document_key.clone(), state.clone())
            })
        else {
            return;
        };
        self.publish_default(document_key, state_snapshot);
    }

    pub(crate) fn set_callout_collapsed(
        &mut self,
        tab_id: Uuid,
        key: impl Into<String>,
        collapsed: bool,
    ) {
        let fold_key = key.into();
        let Some((document_key, state_snapshot)) =
            self.tabs.get_mut(&tab_id).map(|(document_key, state)| {
                state.collapsed_callouts.insert(fold_key, collapsed);
                (document_key.clone(), state.clone())
            })
        else {
            return;
        };
        self.publish_default(document_key, state_snapshot);
    }

    pub(crate) fn remove_table_layout(&mut self, tab_id: Uuid, key: &str) {
        let layout_key = key.to_owned();
        let Some((document_key, state_snapshot)) =
            self.tabs.get_mut(&tab_id).map(|(document_key, state)| {
                state.table_column_widths.remove(&layout_key);
                (document_key.clone(), state.clone())
            })
        else {
            return;
        };
        self.publish_default(document_key, state_snapshot);
    }

    pub(crate) fn set_table_layout(
        &mut self,
        tab_id: Uuid,
        key: impl Into<String>,
        fractions: Vec<f32>,
    ) {
        let layout_key = key.into();
        let Some((document_key, state_snapshot)) =
            self.tabs.get_mut(&tab_id).map(|(document_key, state)| {
                let mut fractions = fractions
                    .into_iter()
                    .map(|fraction| fraction.max(0.0))
                    .collect::<Vec<_>>();
                let sum = fractions.iter().sum::<f32>();
                if sum > f32::EPSILON {
                    for fraction in &mut fractions {
                        *fraction /= sum;
                    }
                    state.table_column_widths.insert(layout_key, fractions);
                }
                (document_key.clone(), state.clone())
            })
        else {
            return;
        };
        self.publish_default(document_key, state_snapshot);
    }

    /// Publishes the tab's last snapshot as the next saved-tab default.
    pub(crate) fn close_tab(&mut self, tab_id: Uuid) -> Option<MarkdownViewState> {
        let (document_key, state) = self.tabs.remove(&tab_id)?;
        // If another tab for the same path is still open, its most recent
        // mutation already owns the reusable default. Do not let closing an
        // older tab regress that snapshot.
        let another_tab_open = self.tabs.values().any(|(key, _)| key == &document_key);
        if !another_tab_open {
            self.publish_default(document_key.clone(), state.clone());
        }
        self.evict();
        Some(state)
    }

    #[expect(
        dead_code,
        reason = "state reset is reserved for workspace lifecycle cleanup"
    )]
    pub(crate) fn clear(&mut self) {
        self.defaults.clear();
        self.tabs.clear();
        self.lru.clear();
    }

    fn touch(&mut self, key: &DocumentViewKey) {
        self.lru.retain(|candidate| candidate != key);
        self.lru.push_back(key.clone());
    }

    fn publish_default(&mut self, key: DocumentViewKey, state: MarkdownViewState) {
        self.defaults.insert(key.clone(), state);
        self.touch(&key);
        self.evict();
    }

    fn evict(&mut self) {
        while self.document_count() > self.capacity {
            let Some(key) = self.lru.pop_front() else {
                break;
            };
            self.defaults.remove(&key);
            let tab_ids = self
                .tabs
                .iter()
                .filter_map(|(tab_id, (document_key, _))| (document_key == &key).then_some(*tab_id))
                .collect::<Vec<_>>();
            for tab_id in tab_ids {
                self.tabs.remove(&tab_id);
            }
        }
    }

    fn document_count(&self) -> usize {
        let mut keys =
            std::collections::HashSet::with_capacity(self.defaults.len() + self.tabs.len());
        keys.extend(self.defaults.keys().cloned());
        keys.extend(self.tabs.values().map(|(key, _)| key.clone()));
        keys.len()
    }
}

/// Stable heading identity from hierarchy path, normalized title and ordinal.
pub(crate) fn heading_view_key(ancestor_path: &[&str], title: &str, ordinal: usize) -> String {
    format!(
        "heading/{}/{}#{}",
        ancestor_path
            .iter()
            .map(|value| normalize_key_part(value))
            .collect::<Vec<_>>()
            .join("/"),
        normalize_key_part(title),
        ordinal
    )
}

/// Stable table identity from heading path, normalized headers and ordinal.
pub(crate) fn table_view_key(ancestor_path: &[&str], headers: &[&str], ordinal: usize) -> String {
    format!(
        "table/{}/{}#{}",
        ancestor_path
            .iter()
            .map(|value| normalize_key_part(value))
            .collect::<Vec<_>>()
            .join("/"),
        headers
            .iter()
            .map(|value| normalize_key_part(value))
            .collect::<Vec<_>>()
            .join("|"),
        ordinal
    )
}

/// Stable Callout identity from heading path, kind/title and ordinal.
pub(crate) fn callout_view_key(
    ancestor_path: &[&str],
    kind: &str,
    title: &str,
    ordinal: usize,
) -> String {
    format!(
        "callout/{}/{}/{}#{}",
        ancestor_path
            .iter()
            .map(|value| normalize_key_part(value))
            .collect::<Vec<_>>()
            .join("/"),
        normalize_key_part(kind),
        normalize_key_part(title),
        ordinal
    )
}

fn normalize_key_part(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
#[path = "../../tests/unit/editor/markdown_view_state.rs"]
mod tests;
