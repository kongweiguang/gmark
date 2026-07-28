// @author kongweiguang

use super::{Editor, ExportTaskResult, mermaid_svg_export_defaults, write_mermaid_svg};
use crate::export::ExportFormat;
use crate::theme::Theme;
use std::path::{Path, PathBuf};
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

#[test]
fn mermaid_svg_export_uses_document_directory_and_stable_suggested_name() {
    let document = Path::new("C:/work/notes/architecture.md");
    let (directory, name) = mermaid_svg_export_defaults(Some(document));
    assert_eq!(directory, PathBuf::from("C:/work/notes"));
    assert_eq!(name, "architecture-mermaid.svg");

    let (_, untitled_name) = mermaid_svg_export_defaults(None);
    assert_eq!(untitled_name, "untitled-mermaid.svg");
}

#[test]
fn mermaid_svg_export_writes_the_current_vector_bytes_atomically() {
    let path =
        std::env::temp_dir().join(format!("gmark-mermaid-export-{}.svg", uuid::Uuid::new_v4()));
    write_mermaid_svg(&path, "<svg viewBox=\"0 0 1 1\"></svg>").unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "<svg viewBox=\"0 0 1 1\"></svg>"
    );
    let _ = std::fs::remove_file(path);
}
