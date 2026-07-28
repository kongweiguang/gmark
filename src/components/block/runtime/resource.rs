// @author kongweiguang

//! Asynchronous resource-card probing and runtime state.

use super::*;

#[derive(Clone, Debug)]
pub(crate) struct ResourceRuntime {
    pub(crate) record: ResourceRecord,
    pub(crate) status: ResourceStatus,
}

impl Block {
    pub(crate) fn resource_runtime(&self) -> Option<&ResourceRuntime> {
        self.resource_runtime.as_ref()
    }

    /// Drops the cached probe so a focus/activation refresh starts a new
    /// background metadata check without touching the Markdown source.
    pub(crate) fn invalidate_resource_probe(&mut self) {
        self.resource_probe_key = None;
        self.resource_probe_task = None;
    }

    /// Explicit retry is a runtime-only operation. It deliberately preserves
    /// the source record so probing can never create an undo entry or rewrite
    /// a relative Markdown destination.
    pub(crate) fn retry_resource_probe(&mut self, cx: &mut Context<Self>) {
        let base_dir = self.image_base_dir().map(Path::to_path_buf);
        self.invalidate_resource_probe();
        self.sync_resource_runtime(base_dir, cx);
    }

    /// Platform launch failures are distinct from metadata readiness. Keep the
    /// card and its source intact while exposing a retryable runtime status.
    pub(crate) fn mark_resource_open_failed(&mut self, cx: &mut Context<Self>) {
        if let Some(runtime) = self.resource_runtime.as_mut() {
            runtime.status = ResourceStatus::OpenFailed;
        }
        cx.notify();
    }

    pub(crate) fn sync_resource_runtime(
        &mut self,
        base_dir: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let Some(resource) = self.record.resource.as_ref() else {
            self.resource_runtime = None;
            self.resource_probe_key = None;
            self.resource_probe_task = None;
            return;
        };

        let record = resource.with_base_dir(base_dir.as_deref());
        let key = format!("{}\n{}", record.destination, record.location_key());
        if self.resource_probe_key.as_deref() == Some(key.as_str()) {
            return;
        }
        self.resource_probe_key = Some(key.clone());
        self.resource_runtime = Some(ResourceRuntime {
            status: if record.is_unsafe_url() {
                ResourceStatus::UnsafeScheme
            } else {
                ResourceStatus::Loading
            },
            record: record.clone(),
        });
        if record.is_unsafe_url() {
            cx.notify();
            return;
        }

        let local_path = record.local_path().map(Path::to_path_buf);
        let is_remote = matches!(record.location, ResourceLocation::Url(_));
        let probe_key = key.clone();
        self.resource_probe_task = Some(cx.spawn(async move |this, cx| {
            let status = cx
                .background_spawn(async move {
                    if is_remote {
                        return ResourceStatus::Ready { size: None };
                    }
                    match local_path {
                        Some(path) => match std::fs::metadata(&path) {
                            Ok(metadata) if metadata.is_file() => ResourceStatus::Ready {
                                size: Some(metadata.len()),
                            },
                            Ok(_) => ResourceStatus::OpenFailed,
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                ResourceStatus::Missing
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                                ResourceStatus::PermissionDenied
                            }
                            Err(_) => ResourceStatus::OpenFailed,
                        },
                        None => ResourceStatus::OpenFailed,
                    }
                })
                .await;
            let _ = this.update(cx, |block, cx| {
                if block.resource_probe_key.as_deref() != Some(probe_key.as_str()) {
                    return;
                }
                if let Some(runtime) = block.resource_runtime.as_mut() {
                    runtime.status = status;
                }
                block.resource_probe_task = None;
                cx.notify();
            });
        }));
    }

    /// Compatibility path for callers without a mounted GPUI entity (notably
    /// block runtime tests). It updates the visible record but deliberately
    /// does not touch the filesystem; mounted editor blocks use
    /// `sync_resource_runtime` above for the background probe.
    pub(super) fn sync_resource_runtime_without_probe(&mut self, base_dir: Option<PathBuf>) {
        let Some(resource) = self.record.resource.as_ref() else {
            self.resource_runtime = None;
            self.resource_probe_key = None;
            self.resource_probe_task = None;
            return;
        };

        let record = resource.with_base_dir(base_dir.as_deref());
        self.resource_probe_key = None;
        self.resource_probe_task = None;
        self.resource_runtime = Some(ResourceRuntime {
            status: if record.is_unsafe_url() {
                ResourceStatus::UnsafeScheme
            } else {
                ResourceStatus::Loading
            },
            record,
        });
    }
}

impl ResourceRecord {
    fn location_key(&self) -> String {
        match &self.location {
            ResourceLocation::Local(path) => format!("local:{}", path.display()),
            ResourceLocation::Url(url) => format!("url:{url}"),
        }
    }
}
