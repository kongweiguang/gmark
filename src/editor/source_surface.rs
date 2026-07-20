// @author kongweiguang

//! Editor 内统一的 Source 画布 owner。

use gmark_large_document::SourceViewState;
use gpui::prelude::*;
use gpui::{AnyElement, Entity, div};
use std::cell::RefCell;

/// Resident Rope 与磁盘后端不能再由独立可选画布决定两条互不相干的
/// 主画布生命周期。此类型是 Tab/Editor 唯一持有的 Source backend 身份：Resident
/// 直接保存 Source 状态，Disk 只保存有界磁盘适配器实体。
#[derive(Clone)]
pub(super) struct SourceSurface {
    backend: SourceSurfaceBackend,
}

#[derive(Clone)]
enum SourceSurfaceBackend {
    Resident(RefCell<SourceViewState>),
    Disk(Entity<crate::large_file::DiskSourceAdapter>),
}

impl SourceSurface {
    pub(super) fn resident() -> Self {
        Self {
            backend: SourceSurfaceBackend::Resident(RefCell::new(SourceViewState::default())),
        }
    }

    pub(super) fn disk(view: Entity<crate::large_file::DiskSourceAdapter>) -> Self {
        Self {
            backend: SourceSurfaceBackend::Disk(view),
        }
    }

    pub(super) fn disk_view(&self) -> Option<&Entity<crate::large_file::DiskSourceAdapter>> {
        match &self.backend {
            SourceSurfaceBackend::Resident(_) => None,
            SourceSurfaceBackend::Disk(view) => Some(view),
        }
    }

    pub(super) fn as_ref(&self) -> Option<&Entity<crate::large_file::DiskSourceAdapter>> {
        self.disk_view()
    }

    pub(super) fn disk_view_cloned(&self) -> Option<Entity<crate::large_file::DiskSourceAdapter>> {
        self.disk_view().cloned()
    }

    /// Editor shell 始终把 Resident 内容交给唯一 SourceSurface owner；Disk backend
    /// 在这里替换 adapter 内容，渲染层不再维护 `if large_file { ... }` 主画布分支。
    pub(super) fn render_content(&self, resident: AnyElement) -> AnyElement {
        match &self.backend {
            SourceSurfaceBackend::Resident(_) => resident,
            SourceSurfaceBackend::Disk(view) => div()
                .id("large-document-tab-content")
                .debug_selector(|| "large-document-tab-content".to_owned())
                .size_full()
                .child(view.clone())
                .into_any_element(),
        }
    }

    pub(super) fn is_disk_backed(&self) -> bool {
        matches!(self.backend, SourceSurfaceBackend::Disk(_))
    }

    pub(super) fn is_some(&self) -> bool {
        self.is_disk_backed()
    }

    #[cfg(test)]
    pub(super) fn is_none(&self) -> bool {
        !self.is_disk_backed()
    }

    #[cfg(test)]
    pub(super) fn expect(self, message: &str) -> Entity<crate::large_file::DiskSourceAdapter> {
        match self.backend {
            SourceSurfaceBackend::Disk(view) => view,
            SourceSurfaceBackend::Resident(_) => panic!("{message}"),
        }
    }

    pub(super) fn sync_resident_selection(&self, selection: gmark_large_document::SourceSelection) {
        match &self.backend {
            SourceSurfaceBackend::Resident(state) => state.borrow_mut().selection = selection,
            SourceSurfaceBackend::Disk(_) => {}
        }
    }

    #[cfg(test)]
    pub(super) fn resident_state(&self) -> Option<SourceViewState> {
        match &self.backend {
            SourceSurfaceBackend::Resident(state) => Some(state.borrow().clone()),
            SourceSurfaceBackend::Disk(_) => None,
        }
    }

    /// Tab snapshot 移走完整 owner 后，活动 Editor 立即回到新的 Resident surface；
    /// 不留下 `None` 窗口，避免渲染、命令和恢复逻辑重新分叉。
    pub(super) fn take(&mut self) -> Self {
        std::mem::replace(self, Self::resident())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmark_large_document::{SourceAffinity, SourceAnchor, SourceSelection};

    #[test]
    fn resident_surface_owns_directional_source_selection() {
        let surface = SourceSurface::resident();
        let selection = SourceSelection::from_range(3..11, true);
        surface.sync_resident_selection(selection);

        assert!(surface.is_none());
        assert_eq!(surface.resident_state().unwrap().selection, selection);
        assert_eq!(
            surface.resident_state().unwrap().selection.head.affinity,
            SourceAffinity::Before
        );
    }

    #[test]
    fn taking_resident_surface_moves_complete_view_state_and_installs_fresh_owner() {
        let mut surface = SourceSurface::resident();
        let expected = SourceViewState {
            selection: SourceSelection::from_range(7..19, true),
            top_byte_anchor: SourceAnchor::new(5, SourceAffinity::After),
            line_offset_y: 0.375,
        };
        match &surface.backend {
            SourceSurfaceBackend::Resident(state) => *state.borrow_mut() = expected.clone(),
            SourceSurfaceBackend::Disk(_) => panic!("new resident surface changed backend"),
        }

        let moved = surface.take();

        assert_eq!(moved.resident_state(), Some(expected));
        assert_eq!(surface.resident_state(), Some(SourceViewState::default()));
    }
}
