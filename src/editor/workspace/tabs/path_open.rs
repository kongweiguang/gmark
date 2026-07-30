// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn tab_index_for_path(&self, path: &Path) -> Option<usize> {
        let active_path = self.file_path.as_deref();
        self.tabs
            .records
            .iter()
            .enumerate()
            .find_map(|(index, record)| {
                let candidate = if index == self.tabs.active {
                    active_path
                } else {
                    record
                        .snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.file_path.as_deref())
                };
                (candidate == Some(path)).then_some(index)
            })
    }

    pub(in crate::editor) fn workspace_tabs_affected_by_path(
        &self,
        target: &Path,
    ) -> (Vec<usize>, bool) {
        let active_path = self.file_path.as_deref();
        let mut indices = Vec::new();
        let mut has_dirty = false;
        for (index, record) in self.tabs.records.iter().enumerate() {
            let (candidate, dirty) = if index == self.tabs.active {
                (active_path, self.is_document_dirty())
            } else {
                let snapshot = record.snapshot.as_ref();
                (
                    snapshot.and_then(|snapshot| snapshot.file_path.as_deref()),
                    snapshot.is_some_and(|snapshot| snapshot.document_dirty),
                )
            };
            if candidate.is_some_and(|path| path == target || path.starts_with(target)) {
                indices.push(index);
                has_dirty |= dirty;
            }
        }
        (indices, has_dirty)
    }

    pub(in crate::editor) fn open_path_in_tab(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(index) = self.tab_index_for_path(&path) {
            self.switch_to_tab_index(index, cx);
            return;
        }
        if !self.can_switch_tabs() {
            return;
        }
        self.tabs.open_generation = self.tabs.open_generation.wrapping_add(1);
        let generation = self.tabs.open_generation;
        self.tabs.open_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let read_path = path.clone();
            let opened = cx
                .background_spawn(async move { crate::document_io::open_document(&read_path) })
                .await;
            let _ = this.update(cx, |editor, cx| {
                if editor.tabs.open_generation != generation {
                    return;
                }
                editor.tabs.open_task = None;
                match opened {
                    Ok(crate::document_io::OpenedDocument::Resident(opened)) => {
                        editor.install_new_tab(opened, path, cx)
                    }
                    Ok(
                        crate::document_io::OpenedDocument::ResidentFormat(probe)
                        | crate::document_io::OpenedDocument::Paged(probe),
                    ) => match gmark_paged_document::FileSource::open(&path) {
                        Ok(source) => editor.install_new_source_backed_tab(path, probe, source, cx),
                        Err(error) => {
                            editor.install_file_open_failure_tab(path, error.to_string(), cx)
                        }
                    },
                    Ok(crate::document_io::OpenedDocument::Image) => {
                        editor.install_image_preview_tab(path, cx)
                    }
                    Err(error) => editor.install_file_open_failure_tab(path, error.to_string(), cx),
                }
            });
        }));
    }
}
