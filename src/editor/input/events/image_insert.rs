// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn inserted_image_tree_for_block(
        block: &super::Block,
        markdown: &str,
    ) -> InlineTextTree {
        if block.uses_raw_text_editing() || block.kind().is_code_block() {
            InlineTextTree::plain(markdown.to_string())
        } else {
            InlineTextTree::from_markdown(markdown)
        }
    }

    pub(super) fn replace_current_block_selection_with_image_text(
        &mut self,
        block: &Entity<super::Block>,
        leading: &InlineTextTree,
        markdown: &str,
        trailing: &InlineTextTree,
        cx: &mut Context<Self>,
    ) {
        let (kind, title, cursor) = block.read_with(cx, |block, _cx| {
            let mut title = leading.clone();
            title.append_tree(Self::inserted_image_tree_for_block(block, markdown));
            let cursor = title.visible_len();
            title.append_tree(trailing.clone());
            (block.kind(), title, cursor)
        });
        Self::set_block_title_and_kind(block, kind, title, cursor, cx);
        if let Some(binding) = self.table_cell_binding(block.entity_id()) {
            self.sync_table_record_from_runtime(&binding.table_block, cx);
        }
        self.focus_block(block.entity_id());
        self.rebuild_image_runtimes(cx);
    }

    pub(super) fn insert_image_block_after_paragraph(
        &mut self,
        block: &Entity<super::Block>,
        leading: &InlineTextTree,
        markdown: &str,
        trailing: &InlineTextTree,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(location) = self.document.find_block_location(block.entity_id()) else {
            return false;
        };
        let leading_empty = leading.visible_len() == 0;
        let trailing_empty = trailing.visible_len() == 0;

        if leading_empty {
            Self::set_block_title_and_kind(
                block,
                BlockKind::Paragraph,
                InlineTextTree::plain(markdown.to_string()),
                markdown.len(),
                cx,
            );
            let image_block = block.clone();
            if !trailing_empty {
                let trailing_block =
                    Self::new_block(cx, BlockRecord::new(BlockKind::Paragraph, trailing.clone()));
                self.document.insert_blocks_at(
                    location.parent,
                    location.index + 1,
                    vec![trailing_block],
                    cx,
                );
            }
            self.focus_block(image_block.entity_id());
            self.rebuild_image_runtimes(cx);
            return true;
        }

        Self::set_block_title_and_kind(
            block,
            BlockKind::Paragraph,
            leading.clone(),
            leading.visible_len(),
            cx,
        );
        let image_block = Self::new_block(cx, BlockRecord::paragraph(markdown.to_string()));
        let mut inserted = vec![image_block.clone()];
        if !trailing_empty {
            inserted.push(Self::new_block(
                cx,
                BlockRecord::new(BlockKind::Paragraph, trailing.clone()),
            ));
        }
        self.document
            .insert_blocks_at(location.parent, location.index + 1, inserted, cx);
        self.focus_block(image_block.entity_id());
        self.rebuild_image_runtimes(cx);
        true
    }
}
