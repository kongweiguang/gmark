// @author kongweiguang

use super::{Editor, ExportTaskResult};
use crate::export::ExportFormat;
use crate::theme::Theme;
use std::sync::atomic::AtomicBool;

#[test]
fn png_export_uses_png_extension() {
    assert_eq!(ExportFormat::Png.extension(), "png");
}

#[test]
fn cancelled_export_preserves_existing_target() {
    let path =
        std::env::temp_dir().join(format!("gmark-cancel-export-{}.html", uuid::Uuid::new_v4()));
    std::fs::write(&path, b"existing").unwrap();
    let cancelled = AtomicBool::new(true);

    let result = Editor::write_export_bytes_cancellable(
        ExportFormat::Html,
        "# replacement",
        &Theme::default_theme(),
        "Doc",
        &path,
        None,
        &cancelled,
    );
    assert!(matches!(result, ExportTaskResult::Cancelled));
    assert_eq!(std::fs::read(&path).unwrap(), b"existing");
    let _ = std::fs::remove_file(path);
}
