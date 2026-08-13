// @author kongweiguang

//! Durable identities, directions, and tab values for pane state.

use uuid::Uuid;

/// The maximum number of leaves a workspace may contain.
pub const MAX_PANES: usize = 8;
/// The ratio used by a newly-created split.
pub const DEFAULT_SPLIT_RATIO: f32 = 0.5;
/// The persisted/layout ratio lower bound.
pub const MIN_SPLIT_RATIO: f32 = 0.1;
/// The persisted/layout ratio upper bound.
pub const MAX_SPLIT_RATIO: f32 = 0.9;
/// The regular keyboard ratio increment.
pub const RATIO_KEYBOARD_DELTA: f32 = 0.02;
/// The shift-key keyboard ratio increment.
pub const RATIO_KEYBOARD_SHIFT_DELTA: f32 = 0.1;

/// Durable identity for a pane leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PaneId(Uuid);

impl PaneId {
    /// Allocate a durable UUID.  Persistence owns the UUID, not this model.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }

    pub const fn uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for PaneId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<PaneId> for Uuid {
    fn from(value: PaneId) -> Self {
        value.0
    }
}

/// Durable identity for a tab/view instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(Uuid);

impl TabId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }

    pub const fn uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for TabId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<TabId> for Uuid {
    fn from(value: TabId) -> Self {
        value.0
    }
}

/// The orientation of a split.
///
/// `Horizontal` lays the first and second child out left-to-right (a vertical
/// divider).  `Vertical` lays them out top-to-bottom (a horizontal divider).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SplitAxis {
    #[default]
    Horizontal,
    Vertical,
}

impl SplitAxis {
    pub const HORIZONTAL: Self = Self::Horizontal;
    pub const VERTICAL: Self = Self::Vertical;
}

/// Direction in which a new pane is inserted relative to an existing leaf.
///
/// Horizontal splits place `Left` before and `Right` after the target.  Vertical
/// splits use the same ordering for `Up` and `Down`, respectively.  The
/// first/second child ordering is the source of truth for both layout and
/// persistence, so callers do not need to reason about tree rewrites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaneSplitDirection {
    Left,
    Right,
    Up,
    Down,
}

impl PaneSplitDirection {
    pub const fn axis(self) -> SplitAxis {
        match self {
            Self::Left | Self::Right => SplitAxis::Horizontal,
            Self::Up | Self::Down => SplitAxis::Vertical,
        }
    }

    pub(super) const fn inserts_before(self) -> bool {
        matches!(self, Self::Left | Self::Up)
    }
}

/// Direction used for deterministic adjacent-pane focus movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FocusDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Alias for callers that use pane terminology.
pub type PaneDirection = FocusDirection;
/// Short alias for integrations that use a generic direction name.
pub type Direction = FocusDirection;

/// A tab's stable instance id, document identity, and runtime view handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabView<D, V = ()> {
    pub(super) id: TabId,
    pub(super) document: D,
    pub(super) view: V,
    pub(super) pinned: bool,
}

impl<D, V> TabView<D, V> {
    pub fn new(id: TabId, document: D, view: V) -> Self {
        Self {
            id,
            document,
            view,
            pinned: false,
        }
    }

    pub fn with_id(id: TabId, document: D, view: V) -> Self {
        Self::new(id, document, view)
    }

    pub fn with_pinned(id: TabId, document: D, view: V, pinned: bool) -> Self {
        Self {
            id,
            document,
            view,
            pinned,
        }
    }

    pub const fn id(&self) -> TabId {
        self.id
    }

    pub const fn document(&self) -> &D {
        &self.document
    }

    pub const fn document_id(&self) -> &D {
        &self.document
    }

    pub const fn view(&self) -> &V {
        &self.view
    }

    pub const fn view_id(&self) -> &V {
        &self.view
    }

    pub const fn is_pinned(&self) -> bool {
        self.pinned
    }

    pub fn set_pinned(&mut self, pinned: bool) {
        self.pinned = pinned;
    }

    pub fn document_mut(&mut self) -> &mut D {
        &mut self.document
    }

    pub fn view_mut(&mut self) -> &mut V {
        &mut self.view
    }

    pub fn into_parts(self) -> (TabId, D, V) {
        (self.id, self.document, self.view)
    }

    pub fn into_parts_with_pinned(self) -> (TabId, D, V, bool) {
        (self.id, self.document, self.view, self.pinned)
    }
}

impl<D> TabView<D, ()> {
    /// Construct a unit-view tab with a durable random instance id.
    pub fn from_document(document: D) -> Self {
        Self::new(TabId::new(), document, ())
    }

    pub fn new_document(document: D) -> Self {
        Self::from_document(document)
    }
}
