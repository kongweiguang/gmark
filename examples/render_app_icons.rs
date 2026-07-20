// @author kongweiguang

//! Renders the canonical gmark SVG into platform raster source sizes.

use std::path::{Path, PathBuf};

use resvg::{tiny_skia, usvg};

const ICON_SIZES: [u32; 7] = [16, 32, 48, 64, 128, 256, 512];

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("assets/icon/gmark-icon.svg");
    let output = root.join("assets/icon");
    let svg = std::fs::read(&source)?;
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default())?;

    for size in ICON_SIZES {
        render_png(&tree, size, &output.join(format!("gmark-icon-{size}.png")))?;
    }
    render_png(&tree, 1024, &output.join("gmark-icon.png"))?;
    Ok(())
}

fn render_png(tree: &usvg::Tree, size: u32, output: &Path) -> anyhow::Result<()> {
    let mut pixmap = tiny_skia::Pixmap::new(size, size)
        .ok_or_else(|| anyhow::anyhow!("invalid icon size {size}"))?;
    let scale = size as f32 / tree.size().width();
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.save_png(output)?;
    Ok(())
}
