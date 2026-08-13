// @author kongweiguang

//! Per-leaf tab state kept separate from the durable pane tree.

use super::{PaneError, TabId, TabView};

/// State associated with one pane id in [`PaneWorkspace::panes`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneState<D, V = ()> {
    pub(super) tabs: Vec<TabView<D, V>>,
    pub(super) active: Option<TabId>,
}

impl<D, V> PaneState<D, V> {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
        }
    }

    pub fn with_tabs(tabs: Vec<TabView<D, V>>) -> Self {
        let active = tabs.first().map(TabView::id);
        Self { tabs, active }
    }

    pub fn tabs(&self) -> &[TabView<D, V>] {
        &self.tabs
    }

    pub fn tab(&self, id: TabId) -> Option<&TabView<D, V>> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    pub fn tab_mut(&mut self, id: TabId) -> Option<&mut TabView<D, V>> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    pub fn active_tab_id(&self) -> Option<TabId> {
        self.active
    }

    pub fn active_tab(&self) -> Option<&TabView<D, V>> {
        self.active.and_then(|id| self.tab(id))
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn set_active_tab(&mut self, id: TabId) -> Result<(), PaneError> {
        if self.tab(id).is_none() {
            return Err(PaneError::TabNotFound(id));
        }
        self.active = Some(id);
        Ok(())
    }

    pub(super) fn push_tab(&mut self, tab: TabView<D, V>) {
        let id = tab.id;
        self.tabs.push(tab);
        if self.active.is_none() {
            self.active = Some(id);
        }
    }

    pub(super) fn remove_tab(&mut self, id: TabId) -> Option<TabView<D, V>> {
        let index = self.tabs.iter().position(|tab| tab.id == id)?;
        let removed_active = self.active == Some(id);
        let tab = self.tabs.remove(index);
        if removed_active {
            self.active = self
                .tabs
                .get(index)
                .or_else(|| self.tabs.last())
                .map(TabView::id);
        } else if self
            .active
            .is_some_and(|active| !self.tabs.iter().any(|candidate| candidate.id == active))
        {
            self.active = self.tabs.first().map(TabView::id);
        }
        Some(tab)
    }

    pub(super) fn take_tabs(&mut self) -> (Vec<TabView<D, V>>, Option<TabId>) {
        (std::mem::take(&mut self.tabs), self.active.take())
    }
}

impl<D, V> Default for PaneState<D, V> {
    fn default() -> Self {
        Self::new()
    }
}
