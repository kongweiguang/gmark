// @author kongweiguang

//! Read-only image preview loading, oversized-image tiling, and zoom interaction.

use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::BufReader;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context as _, Result, bail};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use image::{Frame, ImageFormat, ImageReader};
use smallvec::smallvec;

use super::Editor;
use crate::i18n::I18nStrings;
use crate::theme::Theme;

pub(super) const IMAGE_PREVIEW_MIN_ZOOM: f32 = 0.25;
pub(super) const IMAGE_PREVIEW_MAX_ZOOM: f32 = 8.0;
pub(super) const IMAGE_PREVIEW_TILE_EDGE: u32 = 4_096;
pub(super) const IMAGE_PREVIEW_MAX_PIXELS: u64 = 32 * 1024 * 1024;
pub(super) const IMAGE_PREVIEW_MAX_TILED_PIXELS: u64 = 256 * 1024 * 1024;
const IMAGE_PREVIEW_MAX_DECODE_BYTES: u64 = IMAGE_PREVIEW_MAX_PIXELS * 4;
const IMAGE_PREVIEW_ZOOM_STEP: f32 = 0.1;
const IMAGE_PREVIEW_PADDING: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ImagePreviewZoomAction {
    ZoomOut,
    ActualSize,
    ZoomIn,
    FitWidth,
}

#[derive(Clone)]
pub(super) struct ImagePreviewAsset {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) content: ImagePreviewContent,
}

#[derive(Clone)]
pub(super) enum ImagePreviewContent {
    Native,
    Tiled(Arc<[ImagePreviewTileRow]>),
}

#[derive(Clone)]
pub(super) struct ImagePreviewTileRow {
    pub(super) height: u32,
    pub(super) tiles: Arc<[ImagePreviewTile]>,
}

#[derive(Clone)]
pub(super) struct ImagePreviewTile {
    pub(super) width: u32,
    source: ImagePreviewTileSource,
}

#[derive(Clone)]
struct ImagePreviewTileSource {
    id: u64,
    png: Arc<[u8]>,
}

impl Hash for ImagePreviewTileSource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

pub(super) enum ImagePreviewAssetLoader {}

enum ImagePreviewTileLoader {}

impl Asset for ImagePreviewAssetLoader {
    type Source = PathBuf;
    type Output = std::result::Result<Arc<ImagePreviewAsset>, Arc<anyhow::Error>>;

    #[allow(clippy::manual_async_fn)]
    fn load(
        source: Self::Source,
        _cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        async move {
            load_image_preview_asset(&source)
                .map(Arc::new)
                .map_err(Arc::new)
        }
    }
}

impl Asset for ImagePreviewTileLoader {
    type Source = ImagePreviewTileSource;
    type Output = std::result::Result<Arc<RenderImage>, Arc<anyhow::Error>>;

    #[allow(clippy::manual_async_fn)]
    fn load(
        source: Self::Source,
        _cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        async move {
            let result = (|| -> Result<Arc<RenderImage>> {
                let mut image = image::load_from_memory_with_format(&source.png, ImageFormat::Png)
                    .context("failed to decode image preview tile")?
                    .into_rgba8();
                for pixel in image.as_flat_samples_mut().samples.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
                Ok(Arc::new(RenderImage::new(smallvec![Frame::new(image)])))
            })();
            result.map_err(Arc::new)
        }
    }
}

pub(super) fn image_preview_zoom_after_wheel(current: f32, delta_y: Pixels) -> f32 {
    (current - f32::from(delta_y) / 700.0).clamp(IMAGE_PREVIEW_MIN_ZOOM, IMAGE_PREVIEW_MAX_ZOOM)
}

fn image_preview_offset_after_anchored_zoom(
    current_offset: Point<Pixels>,
    pointer: Point<Pixels>,
    viewport: Bounds<Pixels>,
    image_size: Size<f32>,
    old_canvas_width: f32,
    old_scale: f32,
    new_canvas_width: f32,
    new_scale: f32,
) -> Point<Pixels> {
    let viewport_left = f32::from(viewport.left());
    let viewport_top = f32::from(viewport.top());
    let viewport_width = f32::from(viewport.size.width);
    let inner_width = (viewport_width - IMAGE_PREVIEW_PADDING * 2.0).max(1.0);
    let canvas_left =
        |width: f32| viewport_left + IMAGE_PREVIEW_PADDING + ((inner_width - width).max(0.0) / 2.0);

    let old_origin_x = canvas_left(old_canvas_width) + f32::from(current_offset.x);
    let old_origin_y = viewport_top + IMAGE_PREVIEW_PADDING + f32::from(current_offset.y);
    // 锚点先映射回原图坐标，再用新比例反算滚动量；这样跨越居中阈值时也不会跳。
    let image_x = ((f32::from(pointer.x) - old_origin_x) / old_scale.max(f32::EPSILON))
        .clamp(0.0, image_size.width);
    let image_y = ((f32::from(pointer.y) - old_origin_y) / old_scale.max(f32::EPSILON))
        .clamp(0.0, image_size.height);
    let next_x =
        (f32::from(pointer.x) - canvas_left(new_canvas_width) - image_x * new_scale).min(0.0);
    let next_y =
        (f32::from(pointer.y) - (viewport_top + IMAGE_PREVIEW_PADDING) - image_y * new_scale)
            .min(0.0);
    point(px(next_x), px(next_y))
}

pub(super) fn image_preview_zoom_for_action(
    current: f32,
    fit_scale: f32,
    action: ImagePreviewZoomAction,
) -> f32 {
    let fit_scale = fit_scale.max(f32::EPSILON);
    let zoom = match action {
        ImagePreviewZoomAction::ZoomOut => {
            (current * fit_scale - IMAGE_PREVIEW_ZOOM_STEP) / fit_scale
        }
        ImagePreviewZoomAction::ActualSize => 1.0 / fit_scale,
        ImagePreviewZoomAction::ZoomIn => {
            (current * fit_scale + IMAGE_PREVIEW_ZOOM_STEP) / fit_scale
        }
        ImagePreviewZoomAction::FitWidth => 1.0,
    };
    zoom.clamp(IMAGE_PREVIEW_MIN_ZOOM, IMAGE_PREVIEW_MAX_ZOOM)
}

pub(super) fn load_image_preview_asset(path: &Path) -> Result<ImagePreviewAsset> {
    let reader = ImageReader::open(path)
        .with_context(|| format!("failed to open image '{}'", path.display()))?
        .with_guessed_format()
        .with_context(|| format!("failed to detect image format for '{}'", path.display()))?;
    let format = reader.format();
    let (width, height) = reader
        .into_dimensions()
        .with_context(|| format!("failed to read image dimensions for '{}'", path.display()))?;
    if width == 0 || height == 0 {
        bail!("image dimensions must be non-zero");
    }
    if width <= IMAGE_PREVIEW_TILE_EDGE && height <= IMAGE_PREVIEW_TILE_EDGE {
        return Ok(ImagePreviewAsset {
            width,
            height,
            content: ImagePreviewContent::Native,
        });
    }

    let rows = if format == Some(ImageFormat::Png) {
        validate_image_preview_tiled_dimensions(width, height)?;
        load_oversized_png_tiles(path, width, height)?
    } else {
        validate_image_preview_dimensions(width, height)?;
        load_oversized_generic_tiles(path, width, height)?
    };
    Ok(ImagePreviewAsset {
        width,
        height,
        content: ImagePreviewContent::Tiled(rows.into()),
    })
}

fn validate_image_preview_dimensions(width: u32, height: u32) -> Result<()> {
    validate_image_pixel_count(width, height, IMAGE_PREVIEW_MAX_PIXELS)
}

fn validate_image_preview_tiled_dimensions(width: u32, height: u32) -> Result<()> {
    validate_image_pixel_count(width, height, IMAGE_PREVIEW_MAX_TILED_PIXELS)
}

fn validate_image_pixel_count(width: u32, height: u32, limit: u64) -> Result<()> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .context("image dimensions overflowed the preview limit")?;
    if pixels > limit {
        bail!("image contains {pixels} pixels, exceeding the preview limit of {limit}");
    }
    Ok(())
}

fn load_oversized_png_tiles(
    path: &Path,
    expected_width: u32,
    expected_height: u32,
) -> Result<Vec<ImagePreviewTileRow>> {
    let file = File::open(path)
        .with_context(|| format!("failed to open oversized PNG '{}'", path.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .with_context(|| format!("failed to read oversized PNG '{}'", path.display()))?;
    if reader.info().interlaced {
        return load_oversized_generic_tiles(path, expected_width, expected_height);
    }
    let width = reader.info().width;
    let height = reader.info().height;
    if (width, height) != (expected_width, expected_height) {
        bail!(
            "image dimensions changed while opening '{}'",
            path.display()
        );
    }
    let (color_type, bit_depth) = reader.output_color_type();
    if bit_depth != png::BitDepth::Eight {
        bail!("PNG normalization did not produce 8-bit pixels");
    }

    let column_widths = tile_column_widths(width);
    let mut rows = Vec::new();
    let mut band_height = 0u32;
    let band_capacity_height = height.min(IMAGE_PREVIEW_TILE_EDGE);
    let mut buffers = tile_band_buffers(&column_widths, band_capacity_height)?;
    while let Some(row) = reader
        .next_row()
        .with_context(|| format!("failed to decode oversized PNG '{}'", path.display()))?
    {
        append_png_row(row.data(), color_type, &column_widths, &mut buffers)?;
        band_height += 1;
        if band_height == IMAGE_PREVIEW_TILE_EDGE {
            rows.push(finish_tile_row(
                &column_widths,
                band_height,
                band_capacity_height,
                &mut buffers,
            )?);
            band_height = 0;
        }
    }
    if band_height > 0 {
        rows.push(finish_tile_row(
            &column_widths,
            band_height,
            band_capacity_height,
            &mut buffers,
        )?);
    }
    let decoded_height: u32 = rows.iter().map(|row| row.height).sum();
    if decoded_height != height {
        bail!(
            "decoded PNG height changed while opening '{}'",
            path.display()
        );
    }
    Ok(rows)
}

fn load_oversized_generic_tiles(
    path: &Path,
    expected_width: u32,
    expected_height: u32,
) -> Result<Vec<ImagePreviewTileRow>> {
    let mut reader = ImageReader::open(path)
        .with_context(|| format!("failed to open image '{}'", path.display()))?
        .with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(IMAGE_PREVIEW_MAX_DECODE_BYTES);
    reader.limits(limits);
    let image = reader
        .decode()
        .with_context(|| format!("failed to decode image '{}'", path.display()))?
        .into_rgba8();
    if image.dimensions() != (expected_width, expected_height) {
        bail!(
            "image dimensions changed while opening '{}'",
            path.display()
        );
    }
    let mut rows = Vec::new();
    for top in (0..expected_height).step_by(IMAGE_PREVIEW_TILE_EDGE as usize) {
        let tile_height = (expected_height - top).min(IMAGE_PREVIEW_TILE_EDGE);
        let mut tiles = Vec::new();
        for left in (0..expected_width).step_by(IMAGE_PREVIEW_TILE_EDGE as usize) {
            let tile_width = (expected_width - left).min(IMAGE_PREVIEW_TILE_EDGE);
            let capacity = u64::from(tile_width)
                .checked_mul(u64::from(tile_height))
                .and_then(|bytes| bytes.checked_mul(4))
                .and_then(|bytes| usize::try_from(bytes).ok())
                .context("image tile buffer size overflowed")?;
            let mut pixels = Vec::new();
            pixels
                .try_reserve_exact(capacity)
                .context("image tile buffer allocation failed")?;
            for y in top..top + tile_height {
                let start = ((y * expected_width + left) * 4) as usize;
                let end = start + (tile_width * 4) as usize;
                pixels.extend_from_slice(&image.as_raw()[start..end]);
            }
            tiles.push(render_tile(tile_width, tile_height, pixels)?);
        }
        rows.push(ImagePreviewTileRow {
            height: tile_height,
            tiles: tiles.into(),
        });
    }
    Ok(rows)
}

fn tile_column_widths(width: u32) -> Vec<u32> {
    (0..width)
        .step_by(IMAGE_PREVIEW_TILE_EDGE as usize)
        .map(|left| (width - left).min(IMAGE_PREVIEW_TILE_EDGE))
        .collect()
}

fn tile_band_buffers(column_widths: &[u32], height: u32) -> Result<Vec<Vec<u8>>> {
    let mut buffers = Vec::new();
    buffers
        .try_reserve_exact(column_widths.len())
        .context("image tile column allocation failed")?;
    for width in column_widths {
        let capacity = u64::from(*width)
            .checked_mul(u64::from(height))
            .and_then(|bytes| bytes.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .context("image tile buffer size overflowed")?;
        let mut buffer = Vec::new();
        buffer
            .try_reserve_exact(capacity)
            .context("image tile buffer allocation failed")?;
        buffers.push(buffer);
    }
    Ok(buffers)
}

fn append_png_row(
    row: &[u8],
    color_type: png::ColorType,
    column_widths: &[u32],
    buffers: &mut [Vec<u8>],
) -> Result<()> {
    let channels = match color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => bail!("normalized PNG retained indexed pixels"),
    };
    let expected = column_widths.iter().sum::<u32>() as usize * channels;
    if row.len() != expected {
        bail!("decoded PNG row length did not match its width");
    }

    let mut source_x = 0usize;
    for (width, output) in column_widths.iter().zip(buffers) {
        let end_x = source_x + *width as usize;
        for pixel in row[source_x * channels..end_x * channels].chunks_exact(channels) {
            let (red, green, blue, alpha) = match color_type {
                png::ColorType::Grayscale => (pixel[0], pixel[0], pixel[0], 255),
                png::ColorType::Rgb => (pixel[0], pixel[1], pixel[2], 255),
                png::ColorType::GrayscaleAlpha => (pixel[0], pixel[0], pixel[0], pixel[1]),
                png::ColorType::Rgba => (pixel[0], pixel[1], pixel[2], pixel[3]),
                png::ColorType::Indexed => bail!("normalized PNG retained indexed pixels"),
            };
            output.extend_from_slice(&[red, green, blue, alpha]);
        }
        source_x = end_x;
    }
    Ok(())
}

fn finish_tile_row(
    column_widths: &[u32],
    height: u32,
    band_capacity_height: u32,
    buffers: &mut Vec<Vec<u8>>,
) -> Result<ImagePreviewTileRow> {
    let replacement = tile_band_buffers(column_widths, band_capacity_height)?;
    let finished = std::mem::replace(buffers, replacement);
    let tiles = column_widths
        .iter()
        .copied()
        .zip(finished)
        .map(|(width, pixels)| render_tile(width, height, pixels))
        .collect::<Result<Vec<_>>>()?;
    Ok(ImagePreviewTileRow {
        height,
        tiles: tiles.into(),
    })
}

fn render_tile(width: u32, height: u32, pixels: Vec<u8>) -> Result<ImagePreviewTile> {
    static NEXT_TILE_ID: AtomicU64 = AtomicU64::new(1);

    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder
            .write_header()
            .context("failed to create image preview tile encoder")?;
        writer
            .write_image_data(&pixels)
            .context("failed to encode image preview tile")?;
    }
    Ok(ImagePreviewTile {
        width,
        source: ImagePreviewTileSource {
            id: NEXT_TILE_ID.fetch_add(1, Ordering::Relaxed),
            png: encoded.into(),
        },
    })
}

#[path = "render/image_preview/view.rs"]
mod view;

fn image_preview_canvas(
    path: &Path,
    asset: &ImagePreviewAsset,
    canvas_width: f32,
    scale: f32,
    scroll_y: f32,
    viewport_height: f32,
    cx: &mut App,
) -> AnyElement {
    let dimensions_id = (u64::from(asset.width) << 32) | u64::from(asset.height);
    let canvas = div()
        .id(("image-preview-canvas", dimensions_id))
        .debug_selector(|| "image-preview-canvas".to_owned())
        .w(px(canvas_width))
        .min_w(px(1.0))
        .flex_shrink_0()
        .flex()
        .flex_col();
    match &asset.content {
        ImagePreviewContent::Native => canvas
            .child(
                img(path.to_path_buf())
                    .id("image-preview-content")
                    .debug_selector(|| "image-preview-content".to_owned())
                    .w_full()
                    .h_auto()
                    .object_fit(ObjectFit::Contain),
            )
            .into_any_element(),
        ImagePreviewContent::Tiled(rows) => {
            let (visible, top_spacer, bottom_spacer) =
                visible_tile_row_window(rows, scale, scroll_y, viewport_height);
            for (row_index, row) in rows.iter().enumerate() {
                if visible.contains(&row_index) {
                    continue;
                }
                for tile in row.tiles.iter() {
                    cx.remove_asset::<ImagePreviewTileLoader>(&tile.source);
                }
            }
            canvas
                .children((top_spacer > 0.5).then(|| {
                    div()
                        .w_full()
                        .h(px(top_spacer))
                        .flex_shrink_0()
                        .into_any_element()
                }))
                .children(
                    rows[visible.clone()]
                        .iter()
                        .enumerate()
                        .map(|(visible_index, row)| {
                            let row_index = visible.start + visible_index;
                            div()
                                .id(("image-preview-tile-row", row_index))
                                .debug_selector(move || {
                                    format!("image-preview-tile-row-{row_index}")
                                })
                                .w_full()
                                .h(px(row.height as f32 * scale))
                                .flex_shrink_0()
                                .flex()
                                .children(row.tiles.iter().enumerate().map(
                                    |(column_index, tile)| {
                                        let source = tile.source.clone();
                                        img(move |window: &mut Window, cx: &mut App| {
                                            window
                                                .use_asset::<ImagePreviewTileLoader>(&source, cx)
                                                .map(|result| {
                                                    result.map_err(ImageCacheError::Other)
                                                })
                                        })
                                        .id((
                                            "image-preview-tile",
                                            row_index * 65_536 + column_index,
                                        ))
                                        .debug_selector(move || {
                                            format!("image-preview-tile-{row_index}-{column_index}")
                                        })
                                        .w(px(tile.width as f32 * scale))
                                        .h_full()
                                        .flex_shrink_0()
                                        .object_fit(ObjectFit::Fill)
                                    },
                                ))
                        }),
                )
                .children((bottom_spacer > 0.5).then(|| {
                    div()
                        .w_full()
                        .h(px(bottom_spacer))
                        .flex_shrink_0()
                        .into_any_element()
                }))
                .into_any_element()
        }
    }
}

fn visible_tile_row_window(
    rows: &[ImagePreviewTileRow],
    scale: f32,
    scroll_y: f32,
    viewport_height: f32,
) -> (Range<usize>, f32, f32) {
    let overdraw = viewport_height.max(1.0);
    let visible_start = (scroll_y - overdraw).max(0.0);
    let visible_end = scroll_y + viewport_height.max(1.0) + overdraw;
    let mut cursor = 0.0;
    let mut start = 0usize;
    while start < rows.len() {
        let next = cursor + rows[start].height as f32 * scale;
        if next >= visible_start {
            break;
        }
        cursor = next;
        start += 1;
    }
    let top_spacer = cursor;
    let mut end = start;
    while end < rows.len() && cursor <= visible_end {
        cursor += rows[end].height as f32 * scale;
        end += 1;
    }
    let total_height = rows
        .iter()
        .map(|row| row.height as f32 * scale)
        .sum::<f32>();
    (start..end, top_spacer, (total_height - cursor).max(0.0))
}

fn image_preview_message(
    selector: &'static str,
    message: impl Into<SharedString>,
    theme: &Theme,
) -> AnyElement {
    div()
        .id(selector)
        .debug_selector(move || selector.to_owned())
        .size_full()
        .p(px(24.0))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.0))
        .bg(theme.colors.editor_background)
        .text_color(theme.colors.dialog_muted)
        .child(
            svg()
                .path("icon/ui/image.svg")
                .size(px(28.0))
                .text_color(theme.colors.dialog_muted),
        )
        .child(message.into())
        .into_any_element()
}

#[cfg(test)]
#[path = "../../tests/unit/editor/image_preview.rs"]
mod tests;
