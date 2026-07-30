// @author kongweiguang

//! Incremental JSON structural navigation.

use super::*;

impl DocumentHost {
    pub(super) fn json_root_index(&self) -> Option<&JsonIndex> {
        match self.structured_index.as_ref() {
            Some(StructuredIndex::Json { index, .. }) => Some(index),
            _ => None,
        }
    }

    pub(super) fn json_container_index(&self, path: &[u64]) -> Option<&JsonIndex> {
        if path.is_empty() {
            self.json_root_index()
        } else {
            self.json_child_indexes.get(path)
        }
    }

    pub(super) fn json_visible_count(&self, container_path: &[u64], index: &JsonIndex) -> u64 {
        let mut count = index.item_count();
        for expanded in &self.json_expanded_nodes {
            if expanded.len() != container_path.len() + 1 || !expanded.starts_with(container_path) {
                continue;
            }
            if let Some(child) = self.json_child_indexes.get(expanded) {
                count = count.saturating_add(self.json_visible_count(expanded, child));
            }
        }
        count
    }

    pub(super) fn json_node_at(&self, display_index: u64) -> Option<JsonNode> {
        let root = self.json_root_index()?;
        self.json_node_at_in(&[], root, display_index, 0)
    }

    pub(super) fn json_node_at_in(
        &self,
        container_path: &[u64],
        index: &JsonIndex,
        display_index: u64,
        depth: usize,
    ) -> Option<JsonNode> {
        let mut inserted = 0u64;
        for expanded in &self.json_expanded_nodes {
            if expanded.len() != container_path.len() + 1 || !expanded.starts_with(container_path) {
                continue;
            }
            let item = *expanded.last()?;
            let root_position = item.saturating_add(inserted);
            if display_index < root_position {
                break;
            }
            if display_index == root_position {
                return Some(JsonNode {
                    container_path: container_path.to_vec(),
                    item,
                    depth,
                });
            }
            let child = self.json_child_indexes.get(expanded)?;
            let child_count = self.json_visible_count(expanded, child);
            if display_index <= root_position.saturating_add(child_count) {
                return self.json_node_at_in(
                    expanded,
                    child,
                    display_index - root_position - 1,
                    depth + 1,
                );
            }
            inserted = inserted.saturating_add(child_count);
        }
        let item = display_index.saturating_sub(inserted);
        (item < index.item_count()).then(|| JsonNode {
            container_path: container_path.to_vec(),
            item,
            depth,
        })
    }

    pub(super) fn request_json_rows(&mut self, visible: Range<usize>, cx: &mut Context<Self>) {
        let Some(StructuredIndex::Json { source, .. }) = self.structured_index.clone() else {
            return;
        };
        let Some(root) = self.json_root_index() else {
            return;
        };
        let row_count = self.json_visible_count(&[], root);
        let start = visible.start.saturating_sub(STRUCTURED_OVERSCAN_ROWS) as u64;
        let end = (visible.end.saturating_add(STRUCTURED_OVERSCAN_ROWS) as u64).min(row_count);
        let nodes = (start..end)
            .filter_map(|row| self.json_node_at(row))
            .filter(|node| !self.json_rows.contains_key(&node.path()))
            .filter_map(|node| {
                self.json_container_index(&node.container_path)
                    .cloned()
                    .map(|index| (node, index))
            })
            .collect::<Vec<_>>();
        if nodes.is_empty() {
            return;
        }
        self.structured_generation = self.structured_generation.wrapping_add(1);
        let generation = self.structured_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        self.structured_pending = Some(start..end);
        self.structured_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let mut rows = Vec::with_capacity(nodes.len());
                    for (node, index) in nodes {
                        let Some(range) = index.item_range(node.item)? else {
                            continue;
                        };
                        rows.push((
                            node.path(),
                            StructuredRow {
                                index: node.item,
                                byte_range: range,
                                column_start: 0,
                                cells: read_json_cells(&index, &source, node.item)?,
                                depth: node.depth,
                            },
                        ));
                    }
                    Ok::<_, gmark_paged_document::PagedDocumentError>(rows)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.structured_generation) {
                    return;
                }
                view.structured_pending = None;
                match result {
                    Ok(rows) => view.json_rows.extend(rows),
                    Err(error) => view.set_structure_error(error, cx),
                }
                cx.notify();
            });
        });
    }

    pub(super) fn activate_json_node(&mut self, display_row: u64, cx: &mut Context<Self>) {
        if let Some(cancellation) = self.json_expand_cancellation.take() {
            cancellation.cancel();
        }
        let Some(node) = self.json_node_at(display_row) else {
            return;
        };
        let path = node.path();
        if self.json_child_indexes.contains_key(&path) {
            if !self.json_expanded_nodes.remove(&path) {
                self.json_expanded_nodes.insert(path);
            }
            self.structured_pending = None;
            cx.notify();
            return;
        }
        let Some(parent) = self.json_container_index(&node.container_path).cloned() else {
            return;
        };
        self.json_expand_generation = self.json_expand_generation.wrapping_add(1);
        let generation = self.json_expand_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        let cancellation = SearchCancellation::default();
        self.json_expand_cancellation = Some(cancellation.clone());
        self.json_expand_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    parent.child_index_cancellable(
                        node.item,
                        JsonIndexOptions::default(),
                        &cancellation,
                    )
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.json_expand_generation) {
                    return;
                }
                view.json_expand_cancellation = None;
                match result {
                    Ok(Some(child)) => {
                        view.json_child_indexes.insert(path.clone(), child);
                        view.json_expanded_nodes.insert(path);
                        view.structured_pending = None;
                    }
                    Ok(None) => {
                        if let Some(byte_offset) =
                            view.json_rows.get(&path).map(|row| row.byte_range.start)
                        {
                            view.jump_byte_offset_to_source(byte_offset, cx);
                        }
                    }
                    Err(gmark_paged_document::PagedDocumentError::Cancelled) => {}
                    Err(error) => view.set_structure_error(error, cx),
                }
                cx.notify();
            });
        });
    }
}
