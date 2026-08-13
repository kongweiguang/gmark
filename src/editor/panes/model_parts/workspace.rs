// @author kongweiguang

//! Mutating workspace operations over the pane tree and state map.

use std::collections::BTreeMap;

use super::helpers::*;
use super::*;

/// Authoritative pane tree, pane-state map, and focused pane.
#[derive(Clone, Debug, PartialEq)]
pub struct PaneWorkspace<D = String, V = ()> {
    root: PaneNode,
    panes: BTreeMap<PaneId, PaneState<D, V>>,
    focused_pane: PaneId,
}

impl<D, V> Default for PaneWorkspace<D, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D, V> PaneWorkspace<D, V> {
    pub fn new() -> Self {
        Self::with_root_id(PaneId::new())
    }

    pub fn with_root_id(root_id: PaneId) -> Self {
        let mut panes = BTreeMap::new();
        panes.insert(root_id, PaneState::new());
        Self {
            root: PaneNode::Leaf(root_id),
            panes,
            focused_pane: root_id,
        }
    }

    /// Restore already-validated durable state without copying pane states
    /// into the tree.  Invalid map/tree/focus combinations are rejected.
    pub fn from_parts(
        root: PaneNode,
        panes: BTreeMap<PaneId, PaneState<D, V>>,
        focused_pane: PaneId,
    ) -> Result<Self, PaneError> {
        let ids = collect_ids(&root);
        if root.leaf_count() != ids.len()
            || ids.len() > MAX_PANES
            || ids.len() != panes.len()
            || !ids.iter().all(|id| panes.contains_key(id))
            || !panes.contains_key(&focused_pane)
        {
            return Err(PaneError::InvalidTree);
        }
        Ok(Self {
            root,
            panes,
            focused_pane,
        })
    }

    pub fn root(&self) -> &PaneNode {
        &self.root
    }

    pub fn panes(&self) -> &BTreeMap<PaneId, PaneState<D, V>> {
        &self.panes
    }

    pub fn pane_states(&self) -> impl Iterator<Item = (PaneId, &PaneState<D, V>)> {
        self.panes.iter().map(|(id, state)| (*id, state))
    }

    pub fn focused(&self) -> PaneId {
        self.focused_pane
    }

    pub fn focused_pane(&self) -> PaneId {
        self.focused_pane
    }

    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        let mut ids = Vec::with_capacity(self.panes.len());
        collect_ids_in_order(&self.root, &mut ids);
        ids
    }

    pub fn pane(&self, id: PaneId) -> Option<&PaneState<D, V>> {
        self.panes.get(&id)
    }

    /// Mutable access used by the view adapter when an active runtime view is
    /// detached before a pane operation transfers its tab state.
    pub fn pane_mut(&mut self, id: PaneId) -> Option<&mut PaneState<D, V>> {
        self.panes.get_mut(&id)
    }

    pub fn tab(&self, pane: PaneId, tab: TabId) -> Option<&TabView<D, V>> {
        self.pane(pane).and_then(|state| state.tab(tab))
    }

    /// Mutable tab access for lifecycle adapters.  The model still owns the
    /// tab value; callers cannot replace it without going through model
    /// operations such as move/copy/close.
    pub fn tab_mut(&mut self, pane: PaneId, tab: TabId) -> Option<&mut TabView<D, V>> {
        self.pane_mut(pane).and_then(|state| state.tab_mut(tab))
    }

    /// Find a tab by durable instance id without exposing the internal pane
    /// map.  This is used after a move/close operation when the source leaf
    /// may have collapsed and the tab now lives in a sibling boundary leaf.
    pub fn find_tab_mut(&mut self, tab: TabId) -> Option<&mut TabView<D, V>> {
        self.panes.values_mut().find_map(|pane| pane.tab_mut(tab))
    }

    pub fn focus(&mut self, pane: PaneId) -> Result<(), PaneError> {
        if !self.panes.contains_key(&pane) {
            return Err(PaneError::PaneNotFound(pane));
        }
        self.focused_pane = pane;
        Ok(())
    }

    /// Close one tab and return its owned view state to the lifecycle layer.
    ///
    /// Validation happens before removing anything so an invalid pane/tab
    /// request leaves the workspace untouched.  Removing an active tab picks
    /// the tab that shifts into its position (the right neighbour), falling
    /// back to the new last tab on the left.  An empty non-root pane is then
    /// collapsed into the geometric boundary leaf of its sibling subtree.
    pub fn close_tab(&mut self, pane: PaneId, tab: TabId) -> Result<TabView<D, V>, PaneError> {
        let state = self.panes.get(&pane).ok_or(PaneError::PaneNotFound(pane))?;
        if state.tab(tab).is_none() {
            return Err(PaneError::TabNotFound(tab));
        }

        let removed = match self
            .panes
            .get_mut(&pane)
            .and_then(|state| state.remove_tab(tab))
        {
            Some(tab) => tab,
            None => return Err(PaneError::TabNotFound(tab)),
        };

        if let Some(target) = self.collapse_empty_source(pane) {
            self.focused_pane = target;
        }
        Ok(removed)
    }

    pub fn split(&mut self, pane: PaneId, axis: SplitAxis) -> Result<PaneId, PaneError> {
        self.split_with_ratio(pane, axis, DEFAULT_SPLIT_RATIO)
    }

    pub fn split_right(&mut self, pane: PaneId) -> Result<PaneId, PaneError> {
        self.split(pane, SplitAxis::Horizontal)
    }

    pub fn split_down(&mut self, pane: PaneId) -> Result<PaneId, PaneError> {
        self.split(pane, SplitAxis::Vertical)
    }

    /// Split `pane` in a compass direction, focusing the newly-created pane.
    ///
    /// The direction controls both the split axis and whether the new leaf is
    /// the first or second child.  Existing `split`, `split_right`, and
    /// `split_down` calls retain their historical second-child behavior.
    pub fn split_toward(
        &mut self,
        pane: PaneId,
        direction: PaneSplitDirection,
    ) -> Result<PaneId, PaneError> {
        self.split_with_ratio_and_position(
            pane,
            direction.axis(),
            DEFAULT_SPLIT_RATIO,
            direction.inserts_before(),
        )
    }

    pub fn split_toward_focused(
        &mut self,
        direction: PaneSplitDirection,
    ) -> Result<PaneId, PaneError> {
        self.split_toward(self.focused_pane, direction)
    }

    pub fn split_focused(&mut self, axis: SplitAxis) -> Result<PaneId, PaneError> {
        self.split(self.focused_pane, axis)
    }

    pub fn split_right_focused(&mut self) -> Result<PaneId, PaneError> {
        self.split_right(self.focused_pane)
    }

    pub fn split_down_focused(&mut self) -> Result<PaneId, PaneError> {
        self.split_down(self.focused_pane)
    }

    pub fn split_with_ratio(
        &mut self,
        pane: PaneId,
        axis: SplitAxis,
        ratio: f32,
    ) -> Result<PaneId, PaneError> {
        self.split_with_ratio_and_position(pane, axis, ratio, false)
    }

    fn split_with_ratio_and_position(
        &mut self,
        pane: PaneId,
        axis: SplitAxis,
        ratio: f32,
        new_is_first: bool,
    ) -> Result<PaneId, PaneError> {
        let ratio = normalize_ratio(ratio)?;
        if self.panes.len() >= MAX_PANES {
            return Err(PaneError::TooManyPanes);
        }
        if !self.panes.contains_key(&pane) {
            return Err(PaneError::PaneNotFound(pane));
        }
        let new_id = self.unique_pane_id()?;
        if !split_leaf(&mut self.root, pane, new_id, axis, ratio, new_is_first) {
            return Err(PaneError::PaneNotFound(pane));
        }
        self.panes.insert(new_id, PaneState::new());
        self.focused_pane = new_id;
        Ok(new_id)
    }

    /// Set the ratio of the split directly enclosing `pane`.
    pub fn set_split_ratio(&mut self, pane: PaneId, ratio: f32) -> Result<f32, PaneError> {
        let ratio = normalize_ratio(ratio)?;
        let path = self.path_to_pane(pane)?;
        if path.is_empty() {
            return Err(PaneError::NoSplitForPane(pane));
        }
        let split_path = &path[..path.len() - 1];
        let old = ratio_at_path(&self.root, split_path).ok_or(PaneError::NoSplitForPane(pane))?;
        if !set_ratio_at_path(&mut self.root, split_path, ratio) {
            return Err(PaneError::NoSplitForPane(pane));
        }
        Ok(old)
    }

    /// Set the ratio of the split addressed by a root-to-child path.
    ///
    /// A path contains one entry per ancestor split (`false` selects the
    /// first child and `true` the second).  This form is used by recursive
    /// renderers because a split may contain another split on either side and
    /// therefore cannot be identified by the id of one descendant leaf.
    pub fn set_split_ratio_at_path(&mut self, path: &[bool], ratio: f32) -> Result<f32, PaneError> {
        let ratio = normalize_ratio(ratio)?;
        let old = ratio_at_path(&self.root, path).ok_or(PaneError::InvalidTree)?;
        if !set_ratio_at_path(&mut self.root, path, ratio) {
            return Err(PaneError::InvalidTree);
        }
        Ok(old)
    }

    /// Keyboard-friendly counterpart to [`Self::set_split_ratio_at_path`].
    pub fn adjust_split_ratio_at_path(
        &mut self,
        path: &[bool],
        increase: bool,
        shift: bool,
    ) -> Result<f32, PaneError> {
        let current = ratio_at_path(&self.root, path).ok_or(PaneError::InvalidTree)?;
        let delta = if shift {
            RATIO_KEYBOARD_SHIFT_DELTA
        } else {
            RATIO_KEYBOARD_DELTA
        };
        let next = normalize_ratio(if increase {
            current + delta
        } else {
            current - delta
        })?;
        self.set_split_ratio_at_path(path, next).map(|_| next)
    }

    pub fn adjust_split_ratio(
        &mut self,
        pane: PaneId,
        increase: bool,
        shift: bool,
    ) -> Result<f32, PaneError> {
        let path = self.path_to_pane(pane)?;
        if path.is_empty() {
            return Err(PaneError::NoSplitForPane(pane));
        }
        let split_path = &path[..path.len() - 1];
        let current =
            ratio_at_path(&self.root, split_path).ok_or(PaneError::NoSplitForPane(pane))?;
        let delta = if shift {
            RATIO_KEYBOARD_SHIFT_DELTA
        } else {
            RATIO_KEYBOARD_DELTA
        };
        let next = normalize_ratio(if increase {
            current + delta
        } else {
            current - delta
        })?;
        if !set_ratio_at_path(&mut self.root, split_path, next) {
            return Err(PaneError::NoSplitForPane(pane));
        }
        Ok(next)
    }

    pub fn adjust_ratio(
        &mut self,
        pane: PaneId,
        increase: bool,
        shift: bool,
    ) -> Result<f32, PaneError> {
        self.adjust_split_ratio(pane, increase, shift)
    }

    pub fn set_ratio(&mut self, pane: PaneId, ratio: f32) -> Result<(), PaneError> {
        self.set_split_ratio(pane, ratio).map(|_| ())
    }

    /// Balance every split according to the number of leaves in each child.
    pub fn balance(&mut self) {
        balance_node(&mut self.root);
    }

    pub fn balance_pane(&mut self, pane: PaneId) -> Result<f32, PaneError> {
        let path = self.path_to_pane(pane)?;
        if path.is_empty() {
            return Err(PaneError::NoSplitForPane(pane));
        }
        let split_path = &path[..path.len() - 1];
        let ratio = balanced_ratio_at_path(&self.root, split_path)
            .ok_or(PaneError::NoSplitForPane(pane))?;
        if !set_ratio_at_path(&mut self.root, split_path, ratio) {
            return Err(PaneError::NoSplitForPane(pane));
        }
        Ok(ratio)
    }

    pub fn focus_adjacent(&mut self, direction: FocusDirection) -> Result<PaneId, PaneError> {
        self.focus_adjacent_from(self.focused_pane, direction)
    }

    pub fn focus_adjacent_from(
        &mut self,
        from: PaneId,
        direction: FocusDirection,
    ) -> Result<PaneId, PaneError> {
        if !self.panes.contains_key(&from) {
            return Err(PaneError::PaneNotFound(from));
        }
        let target = self.adjacent_pane(from, direction)?;
        self.focused_pane = target;
        Ok(target)
    }

    /// Return the deterministic geometric neighbor without changing focus.
    pub fn adjacent_pane(
        &self,
        from: PaneId,
        direction: FocusDirection,
    ) -> Result<PaneId, PaneError> {
        if !self.panes.contains_key(&from) {
            return Err(PaneError::PaneNotFound(from));
        }
        let mut rects = Vec::with_capacity(self.panes.len());
        collect_rects(&self.root, Rect::ROOT, &mut rects);
        let current = rects
            .iter()
            .find(|(id, _)| *id == from)
            .map(|(_, rect)| *rect)
            .ok_or(PaneError::PaneNotFound(from))?;
        choose_adjacent(from, current, direction, &rects)
            .ok_or(PaneError::NoAdjacentPane { from, direction })
    }

    fn path_to_pane(&self, pane: PaneId) -> Result<Vec<bool>, PaneError> {
        if !self.panes.contains_key(&pane) {
            return Err(PaneError::PaneNotFound(pane));
        }
        find_path(&self.root, pane).ok_or(PaneError::PaneNotFound(pane))
    }

    fn collapse_empty_source(&mut self, source: PaneId) -> Option<PaneId> {
        if !self.panes.get(&source).is_some_and(PaneState::is_empty) || self.panes.len() <= 1 {
            return None;
        }
        let path = find_path(&self.root, source)?;
        if path.is_empty() {
            return None;
        }
        let state = self.panes.remove(&source)?;
        let parent_path = &path[..path.len() - 1];
        let source_is_first = !path[path.len() - 1];
        let (root, target) = collapse_at_path(
            std::mem::replace(&mut self.root, PaneNode::Leaf(source)),
            parent_path,
            source_is_first,
            state.tabs,
            state.active,
            &mut self.panes,
        );
        self.root = root;
        Some(target)
    }

    fn unique_pane_id(&self) -> Result<PaneId, PaneError> {
        for _ in 0..16 {
            let id = PaneId::new();
            if !self.panes.contains_key(&id) {
                return Ok(id);
            }
        }
        Err(PaneError::IdCollision)
    }
}

impl<D, V> PaneWorkspace<D, V>
where
    D: PartialEq,
{
    /// Insert a caller-constructed tab and focus the target pane.
    pub fn insert_tab(&mut self, pane: PaneId, tab: TabView<D, V>) -> Result<TabId, PaneError> {
        if !self.panes.contains_key(&pane) {
            return Err(PaneError::PaneNotFound(pane));
        }
        if self.pane(pane).is_some_and(|state| {
            state
                .tabs
                .iter()
                .any(|candidate| candidate.document == tab.document)
        }) {
            return Err(PaneError::DuplicateDocument);
        }
        if self.find_tab(tab.id()).is_some() {
            return Err(PaneError::DuplicateTabId(tab.id()));
        }
        let id = tab.id();
        if let Some(state) = self.panes.get_mut(&pane) {
            state.push_tab(tab);
        }
        self.focused_pane = pane;
        Ok(id)
    }

    pub fn add_tab(&mut self, pane: PaneId, tab: TabView<D, V>) -> Result<TabId, PaneError> {
        self.insert_tab(pane, tab)
    }

    pub fn open_tab(&mut self, pane: PaneId, tab: TabView<D, V>) -> Result<TabId, PaneError> {
        self.insert_tab(pane, tab)
    }

    pub fn open_document(&mut self, pane: PaneId, document: D) -> Result<TabId, PaneError>
    where
        V: Default,
    {
        let id = self.unique_tab_id()?;
        self.insert_tab(pane, TabView::new(id, document, V::default()))
    }

    pub fn set_active_tab(&mut self, pane: PaneId, tab: TabId) -> Result<(), PaneError> {
        self.panes
            .get_mut(&pane)
            .ok_or(PaneError::PaneNotFound(pane))?
            .set_active_tab(tab)
    }

    pub fn move_tab(
        &mut self,
        source: PaneId,
        target: PaneId,
        tab: TabId,
    ) -> Result<(), PaneError> {
        self.validate_tab_transfer(source, target, tab)?;
        let moved = self
            .panes
            .get_mut(&source)
            .and_then(|state| state.remove_tab(tab))
            .ok_or(PaneError::TabNotFound(tab))?;
        self.panes
            .get_mut(&target)
            .ok_or(PaneError::PaneNotFound(target))?
            .push_tab(moved);
        let _ = self.collapse_empty_source(source);
        self.focused_pane = target;
        Ok(())
    }

    pub fn copy_tab(
        &mut self,
        source: PaneId,
        target: PaneId,
        tab: TabId,
    ) -> Result<TabId, PaneError>
    where
        D: Clone,
        V: Clone,
    {
        self.validate_tab_transfer(source, target, tab)?;
        let mut copied = self
            .tab(source, tab)
            .ok_or(PaneError::TabNotFound(tab))?
            .clone();
        let new_id = self.unique_tab_id()?;
        copied.id = new_id;
        self.panes
            .get_mut(&target)
            .ok_or(PaneError::PaneNotFound(target))?
            .push_tab(copied);
        self.focused_pane = target;
        Ok(new_id)
    }

    pub fn move_tab_by_index(
        &mut self,
        source: PaneId,
        target: PaneId,
        index: usize,
    ) -> Result<(), PaneError> {
        let id = self
            .pane(source)
            .ok_or(PaneError::PaneNotFound(source))?
            .tabs
            .get(index)
            .map(TabView::id)
            .ok_or(PaneError::TabNotFound(TabId::new()))?;
        self.move_tab(source, target, id)
    }

    pub fn copy_tab_by_index(
        &mut self,
        source: PaneId,
        target: PaneId,
        index: usize,
    ) -> Result<TabId, PaneError>
    where
        D: Clone,
        V: Clone,
    {
        let id = self
            .pane(source)
            .ok_or(PaneError::PaneNotFound(source))?
            .tabs
            .get(index)
            .map(TabView::id)
            .ok_or(PaneError::TabNotFound(TabId::new()))?;
        self.copy_tab(source, target, id)
    }

    /// Close a leaf and append its tabs to the boundary leaf adjacent in the
    /// sibling subtree.  The closed pane's active tab remains active.
    pub fn close_pane(&mut self, pane: PaneId) -> Result<PaneId, PaneError> {
        let path = self.path_to_pane(pane)?;
        if path.is_empty() {
            return Err(PaneError::CannotCloseLastPane);
        }
        let mut state = self
            .panes
            .remove(&pane)
            .ok_or(PaneError::PaneNotFound(pane))?;
        let (tabs, active) = state.take_tabs();
        let parent_path = &path[..path.len() - 1];
        let source_is_first = !path[path.len() - 1];
        let (root, target) = collapse_at_path(
            std::mem::replace(&mut self.root, PaneNode::Leaf(pane)),
            parent_path,
            source_is_first,
            tabs,
            active,
            &mut self.panes,
        );
        self.root = root;
        self.focused_pane = target;
        Ok(target)
    }

    pub fn close_focused_pane(&mut self) -> Result<PaneId, PaneError> {
        self.close_pane(self.focused_pane)
    }

    fn validate_tab_transfer(
        &self,
        source: PaneId,
        target: PaneId,
        tab: TabId,
    ) -> Result<(), PaneError> {
        if source == target {
            return Err(PaneError::SamePane);
        }
        let source_tab = self.tab(source, tab).ok_or_else(|| {
            if self.pane(source).is_none() {
                PaneError::PaneNotFound(source)
            } else {
                PaneError::TabNotFound(tab)
            }
        })?;
        let target_state = self.pane(target).ok_or(PaneError::PaneNotFound(target))?;
        if target_state
            .tabs
            .iter()
            .any(|candidate| candidate.document == source_tab.document)
        {
            return Err(PaneError::DuplicateDocument);
        }
        Ok(())
    }

    fn unique_tab_id(&self) -> Result<TabId, PaneError> {
        for _ in 0..16 {
            let id = TabId::new();
            if self.find_tab(id).is_none() {
                return Ok(id);
            }
        }
        Err(PaneError::IdCollision)
    }

    fn find_tab(&self, id: TabId) -> Option<&TabView<D, V>> {
        self.panes.values().find_map(|pane| pane.tab(id))
    }
}
