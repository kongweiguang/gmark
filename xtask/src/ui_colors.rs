// @author kongweiguang

//! Static gate preventing new raw runtime UI colors outside semantic themes.

use std::path::Path;

use crate::source;

const LEGACY_UI_COLOR_LINES: &[(&str, &str)] = &[
    ("src/platform/window.rs", "Hsla::from(rgba(0xf4f4f5ff))"),
    ("src/platform/window.rs", "Hsla::from(rgba(0x18181bff))"),
    (
        "src/editor/diagram_overlay.rs",
        ".bg(gpui::black().opacity(0.58))",
    ),
];

const CONTENT_COLOR_PATHS: &[&str] = &[
    "src/editor/block/element.rs",
    "src/editor/block/render_parts/html.rs",
    "src/editor/document/markdown/html.rs",
    "src/editor/render/latex/mod.rs",
];

pub(crate) fn check(root: &Path) -> Result<(), String> {
    let mut violations = Vec::new();
    for path in source::manual_production_rust_files(root)? {
        let relative = source::relative(root, &path).replace('\\', "/");
        if !relative.starts_with("src/")
            || relative.starts_with("src/ui/theme/")
            || relative.ends_with("/evidence.rs")
            || CONTENT_COLOR_PATHS.contains(&relative.as_str())
        {
            continue;
        }
        for (index, line) in source::read_text(&path)?.lines().enumerate() {
            let trimmed = line.trim();
            if !contains_raw_ui_color(trimmed)
                || is_transparent_geometry(trimmed)
                || LEGACY_UI_COLOR_LINES.contains(&(relative.as_str(), trimmed))
            {
                continue;
            }
            violations.push(format!(
                "{relative}:{}: raw runtime UI color must use ThemeColors.workbench/material tokens: {trimmed}",
                index + 1
            ));
        }
    }
    source::finish("ui-colors", violations)
}

fn contains_raw_ui_color(line: &str) -> bool {
    contains_call(line, "rgba(0x")
        || contains_call(line, "hsla(")
        || line.contains("gpui::black()")
        || line.contains("gpui::white()")
}

fn contains_call(line: &str, needle: &str) -> bool {
    line.match_indices(needle).any(|(index, _)| {
        index == 0
            || line[..index]
                .chars()
                .next_back()
                .is_some_and(|character| !character.is_ascii_alphanumeric() && character != '_')
    })
}

fn is_transparent_geometry(line: &str) -> bool {
    let compact = line
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    compact.contains("hsla(0.0,0.0,0.0,0.0)")
        || compact.contains("hsla(0.,0.,0.,0.)")
        || compact.contains("rgba(0x00000000)")
}
