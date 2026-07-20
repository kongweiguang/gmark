// @author kongweiguang

//! Editor-facing export flow and file writing.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

#[cfg(test)]
use anyhow::Context as _;
use futures::channel::oneshot;
use gpui::*;

use super::Editor;
use crate::export::{self as document_export, ExportFormat};
use crate::i18n::I18nManager;
use crate::theme::{Theme, ThemeManager};

enum ExportTaskResult {
    Complete,
    Cancelled,
    Failed(String),
}

impl Editor {
    fn export_dialog_defaults(&self, format: ExportFormat) -> (PathBuf, String) {
        let extension = format.extension();
        if let Some(path) = self.file_path.as_ref() {
            let directory = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.is_empty())
                .unwrap_or("untitled");
            return (directory, format!("{stem}.{extension}"));
        }

        (
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            format!("untitled.{extension}"),
        )
    }

    fn export_title(&self) -> String {
        self.file_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .map(|stem| stem.to_string_lossy().to_string())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    #[cfg(test)]
    fn render_export_bytes(
        format: ExportFormat,
        markdown: &str,
        theme: &Theme,
        title: &str,
        source_base_dir: Option<&Path>,
    ) -> anyhow::Result<Vec<u8>> {
        match format {
            ExportFormat::Html => Ok(document_export::render_html_with_base_dir(
                markdown,
                theme,
                title,
                source_base_dir,
            )
            .into_bytes()),
            ExportFormat::Png => {
                document_export::render_png(markdown, theme, title, source_base_dir)
            }
            ExportFormat::Pdf => {
                document_export::render_pdf(markdown, theme, title, source_base_dir)
            }
        }
    }

    #[cfg(test)]
    fn write_export_bytes(
        format: ExportFormat,
        markdown: &str,
        theme: &Theme,
        title: &str,
        path: &Path,
        source_base_dir: Option<&Path>,
    ) -> anyhow::Result<()> {
        let bytes = Self::render_export_bytes(format, markdown, theme, title, source_base_dir)?;
        std::fs::write(path, bytes).with_context(|| format!("failed to write '{}'", path.display()))
    }

    fn write_export_bytes_cancellable(
        format: ExportFormat,
        markdown: &str,
        theme: &Theme,
        title: &str,
        path: &Path,
        source_base_dir: Option<&Path>,
        cancelled: &AtomicBool,
    ) -> ExportTaskResult {
        if cancelled.load(Ordering::Acquire) {
            return ExportTaskResult::Cancelled;
        }
        let rendered = match format {
            ExportFormat::Html => Ok(document_export::render_html_with_base_dir(
                markdown,
                theme,
                title,
                source_base_dir,
            )
            .into_bytes()),
            ExportFormat::Png => document_export::render_png_cancellable(
                markdown,
                theme,
                title,
                source_base_dir,
                cancelled,
            ),
            ExportFormat::Pdf => document_export::render_pdf_cancellable(
                markdown,
                theme,
                title,
                source_base_dir,
                cancelled,
            ),
        };
        let bytes = match rendered {
            Ok(bytes) => bytes,
            Err(_) if cancelled.load(Ordering::Acquire) => return ExportTaskResult::Cancelled,
            Err(error) => return ExportTaskResult::Failed(error.to_string()),
        };
        if cancelled.load(Ordering::Acquire) {
            return ExportTaskResult::Cancelled;
        }
        match gmark_document::atomic_write(path, &bytes) {
            Ok(()) => ExportTaskResult::Complete,
            Err(error) => ExportTaskResult::Failed(error.to_string()),
        }
    }

    #[cfg(test)]
    pub(crate) fn export_document_to_path(
        &self,
        format: ExportFormat,
        path: &Path,
        cx: &App,
    ) -> anyhow::Result<()> {
        let markdown = self.serialized_document_text(cx);
        let theme = cx.global::<ThemeManager>().current().clone();
        let title = self.export_title();
        let source_base_dir = self.file_path.as_ref().and_then(|path| path.parent());
        Self::write_export_bytes(format, &markdown, &theme, &title, path, source_base_dir)
    }

    pub(crate) fn export_document_via_prompt(
        &mut self,
        format: ExportFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.export_task.is_some() {
            return;
        }
        let markdown = self.serialized_document_text(cx);
        let theme = cx.global::<ThemeManager>().current().clone();
        let title = self.export_title();
        let source_base_dir = self
            .file_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(Path::to_path_buf);
        let (default_dir, suggested_name) = self.export_dialog_defaults(format);
        let prompt = cx.prompt_for_new_path(&default_dir, Some(&suggested_name));
        let window_handle = window.window_handle();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.export_cancel = Some(Arc::clone(&cancelled));
        self.export_cancel_requested = false;

        self.export_task = Some(cx.spawn(
            async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let mut path = match prompt.await {
                    Ok(Ok(Some(path))) => path,
                    Ok(Ok(None)) | Err(_) => {
                        let _ = this.update(cx, |editor, _cx| {
                            editor.export_task = None;
                            editor.export_cancel = None;
                            editor.export_in_progress = false;
                            editor.export_cancel_requested = false;
                        });
                        return;
                    }
                    Ok(Err(err)) => {
                        let _ = this.update(cx, |editor, _cx| {
                            editor.export_task = None;
                            editor.export_cancel = None;
                        });
                        let detail = err.to_string();
                        let _ = cx.update_window(
                            window_handle,
                            move |_view: AnyView, window: &mut Window, cx: &mut App| {
                                show_export_error(window, cx, &detail);
                            },
                        );
                        return;
                    }
                };

                if path.extension().is_none() {
                    path.set_extension(format.extension());
                }

                let _ = this.update(cx, |editor, cx| {
                    editor.export_in_progress = true;
                    cx.notify();
                });

                let (sender, receiver) = oneshot::channel();
                let worker_cancelled = Arc::clone(&cancelled);
                let spawn_result = thread::Builder::new()
                    .name("gmark-export".to_string())
                    .spawn(move || {
                        let result = Self::write_export_bytes_cancellable(
                            format,
                            &markdown,
                            &theme,
                            &title,
                            &path,
                            source_base_dir.as_deref(),
                            &worker_cancelled,
                        );
                        let _ = sender.send(result);
                    });

                if let Err(err) = spawn_result {
                    let _ = this.update(cx, |editor, cx| {
                        editor.export_task = None;
                        editor.export_cancel = None;
                        editor.export_in_progress = false;
                        editor.export_cancel_requested = false;
                        cx.notify();
                    });
                    let detail = format!("failed to start export task: {err}");
                    let _ = cx.update_window(
                        window_handle,
                        move |_view: AnyView, window: &mut Window, cx: &mut App| {
                            show_export_error(window, cx, &detail);
                        },
                    );
                    return;
                }

                let result = receiver.await.unwrap_or_else(|_| {
                    ExportTaskResult::Failed(
                        "export task stopped before reporting a result".to_owned(),
                    )
                });
                let _ = this.update(cx, |editor, cx| {
                    editor.export_task = None;
                    editor.export_cancel = None;
                    editor.export_in_progress = false;
                    editor.export_cancel_requested = false;
                    cx.notify();
                });
                if let ExportTaskResult::Failed(detail) = result {
                    let _ = cx.update_window(
                        window_handle,
                        move |_view: AnyView, window: &mut Window, cx: &mut App| {
                            show_export_error(window, cx, &detail);
                        },
                    );
                }
            },
        ));
    }

    pub(crate) fn on_cancel_export(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(cancelled) = self.export_cancel.as_ref() {
            cancelled.store(true, Ordering::Release);
            self.export_cancel_requested = true;
            cx.notify();
        }
    }
}

fn show_export_error(window: &mut Window, cx: &mut App, detail: &str) {
    let strings = cx.global::<I18nManager>().strings().clone();
    let buttons = [strings.info_dialog_ok.as_str()];
    let _ = window.prompt(
        PromptLevel::Critical,
        &strings.export_failed_title,
        Some(detail),
        &buttons,
        cx,
    );
}

#[cfg(test)]
#[path = "../../tests/unit/editor/export.rs"]
mod tests;
