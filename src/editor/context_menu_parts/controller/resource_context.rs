// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn resource_context_block(
        &self,
        _cx: &App,
    ) -> Option<gpui::Entity<crate::components::Block>> {
        let entity_id = match self.context_menu.as_ref()? {
            ContextMenuState::Resource { entity_id, .. } => *entity_id,
            _ => return None,
        };
        self.focusable_entity_by_id(entity_id)
    }

    /// Context-menu actions must use the runtime's base-directory-resolved
    /// record. The source record intentionally keeps relative Markdown targets
    /// lexical so save-as and round trips do not rewrite user text.
    pub(in crate::editor) fn resource_context_record(&self, cx: &App) -> Option<ResourceRecord> {
        let block = self.resource_context_block(cx)?;
        let base_dir = self.image_base_dir();
        let block = block.read(cx);
        block
            .resource_runtime()
            .map(|runtime| runtime.record.clone())
            .or_else(|| {
                block
                    .record
                    .resource
                    .as_ref()
                    .map(|record| record.with_base_dir(base_dir.as_deref()))
            })
    }

    /// The menu must not probe the filesystem synchronously. Mounted blocks
    /// publish the cached adapter status; an unmounted/test block remains in
    /// Loading until the next runtime-context synchronization.
    pub(in crate::editor) fn resource_context_status(&self, cx: &App) -> Option<ResourceStatus> {
        let block = self.resource_context_block(cx)?;
        let block = block.read(cx);
        block
            .resource_runtime()
            .map(|runtime| runtime.status.clone())
            .or_else(|| {
                block.record.resource.as_ref().map(|record| {
                    if record.is_unsafe_url() {
                        ResourceStatus::UnsafeScheme
                    } else {
                        ResourceStatus::Loading
                    }
                })
            })
    }
}
