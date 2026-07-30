// @author kongweiguang

use std::fs;
use std::sync::atomic::AtomicBool;

use gmark_export::{
    ExportCancellation, ExportCancellationHandle, ExportTheme, prepare_html_resources, render_html,
    render_pdf_cancellable, render_png_cancellable,
};
use uuid::Uuid;

#[test]
fn html_export_projects_toc_math_mermaid_and_safe_html() {
    let html = render_html(
        "[TOC]\n\n# Title\n\n$$\nx^2\n$$\n\n```mermaid\nflowchart LR\nA --> B\n```\n\n<span style=\"color:blue; background-image:url(javascript:bad)\">safe</span>",
        &ExportTheme::default(),
        "Doc",
    );

    assert!(html.contains("<nav class=\"gmark-toc\""));
    assert!(html.contains("<h1 id=\"title\">Title</h1>"));
    assert!(html.contains("class=\"vlt-math\""));
    assert!(html.contains("class=\"vlt-mermaid\""));
    assert!(html.contains("color: rgba(0,0,255,1.000);"));
    assert!(!html.contains("background-image"));
}

#[test]
fn html_export_projects_unicode_toc_entries_without_frontmatter_or_fence_headings() {
    let html = render_html(
        "[TOC]\n\n---\ntitle: ignored\n---\n\n# 你好 **gmark**\n## 你好 gmark\nTitle\n-----\n~~~md\n# ignored\n~~~",
        &ExportTheme::default(),
        "Doc",
    );

    assert!(html.contains("href=\"#你好-gmark\""));
    assert!(html.contains("href=\"#你好-gmark-1\""));
    assert!(html.contains("href=\"#title\""));
    assert!(!html.contains("href=\"#ignored\""));
}

#[test]
fn html_export_replaces_invalid_mermaid_with_a_safe_error_projection() {
    let html = render_html(
        "~~~mermaid\nnot a real mermaid diagram ::::\n~~~",
        &ExportTheme::default(),
        "Doc",
    );

    assert!(html.contains("vlt-mermaid-error"));
    assert!(!html.contains("<script"));
}

#[test]
fn html_export_inlines_local_images() {
    let root = std::env::temp_dir().join(format!("gmark-export-image-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("diagram.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\"></svg>",
    )
    .unwrap();

    let html = gmark_export::render_html_with_base_dir(
        "![diagram](diagram.svg)",
        &ExportTheme::default(),
        "Doc",
        Some(&root),
    );
    let _ = fs::remove_dir_all(&root);

    assert!(html.contains("data:image/svg+xml;base64,"));
    assert!(!html.contains("src=\"diagram.svg\""));
}

#[test]
fn resource_cleanup_removes_only_owned_files() {
    let root = std::env::temp_dir().join(format!("gmark-export-resource-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("demo.pdf"), b"fixture").unwrap();
    let output = root.join("note.html");
    let cancelled = AtomicBool::new(false);

    let prepared = prepare_html_resources(
        "[Demo](./demo.pdf \"gmark:resource\")",
        Some(&root),
        &output,
        &cancelled,
    )
    .unwrap();
    assert!(root.join("note.assets/demo.pdf").is_file());
    assert!(prepared.markdown.contains("note.assets/demo.pdf"));

    prepared.cleanup_created();
    assert!(!root.join("note.assets/demo.pdf").exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cancellation_short_circuits_chromium_work_without_launching_a_browser() {
    let cancelled = AtomicBool::new(true);
    let theme = ExportTheme::default();

    assert_eq!(
        render_png_cancellable("# Title", &theme, "Doc", None, &cancelled)
            .unwrap_err()
            .to_string(),
        "export cancelled"
    );
    assert_eq!(
        render_pdf_cancellable("# Title", &theme, "Doc", None, &cancelled)
            .unwrap_err()
            .to_string(),
        "export cancelled"
    );

    let handle = ExportCancellationHandle::default();
    assert!(!handle.is_cancelled());
    handle.cancel();
    assert!(handle.is_cancelled());
}
