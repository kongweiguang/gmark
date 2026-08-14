// @author kongweiguang

use super::*;

pub(super) struct DroppedResourceTarget {
    pub(super) block: Entity<super::super::Block>,
    pub(super) leading: crate::components::InlineTextTree,
    pub(super) trailing: crate::components::InlineTextTree,
    pub(super) document_path: Option<PathBuf>,
    pub(super) behavior: ResourceInsertBehavior,
    pub(super) fingerprint: ResourceDropTarget,
}

impl Editor {
    /// 在拖放事件开始时冻结插入意图；后台等待期间切换 tab 或选区不会改写新的目标。
    pub(super) fn capture_dropped_resource_target(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<DroppedResourceTarget> {
        let block = self
            .focused_edit_target(window, cx)
            .or_else(|| self.current_edit_target_from_state(cx))?;
        let (leading, trailing) = block.read(cx).paste_resource_split();
        let selection = block.read(cx).selected_range.clone();
        let selection_reversed = block.read(cx).selection_reversed;
        let behavior = crate::preferences::read_app_preferences()
            .map(|preferences| preferences.resource_insert_behavior())
            .unwrap_or(ResourceInsertBehavior::None);
        let fingerprint = ResourceDropTarget {
            document_epoch: self.document_epoch,
            generation: self.document_epoch,
            revision: self.source_document.revision(),
            tab_id: self.tabs.active_id(),
            block_id: block.entity_id(),
            selection,
            selection_reversed,
        };
        Some(DroppedResourceTarget {
            block,
            leading,
            trailing,
            document_path: self.file_path.clone(),
            behavior,
            fingerprint,
        })
    }

    /// 生成提交前的当前目标指纹，确保仅原始 block/selection/tab 仍存在时才能写入。
    pub(in crate::editor) fn current_dropped_resource_target(
        &self,
        block: &Entity<super::super::Block>,
        cx: &App,
    ) -> Option<ResourceDropTarget> {
        let block = self.focusable_entity_by_id(block.entity_id())?;
        let block_ref = block.read(cx);
        Some(ResourceDropTarget {
            document_epoch: self.document_epoch,
            generation: self.document_epoch,
            revision: self.source_document.revision(),
            tab_id: self.tabs.active_id(),
            block_id: block.entity_id(),
            selection: block_ref.selected_range.clone(),
            selection_reversed: block_ref.selection_reversed,
        })
    }
}
