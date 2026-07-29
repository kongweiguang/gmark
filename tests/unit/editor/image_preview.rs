// @author kongweiguang

use gpui::px;
use image::{ImageBuffer, Rgba};

use super::{
    IMAGE_PREVIEW_MAX_PIXELS, IMAGE_PREVIEW_MAX_TILED_PIXELS, IMAGE_PREVIEW_MAX_ZOOM,
    IMAGE_PREVIEW_MIN_ZOOM, IMAGE_PREVIEW_TILE_EDGE, ImagePreviewContent, ImagePreviewZoomAction,
    image_preview_zoom_after_wheel, image_preview_zoom_for_action, load_image_preview_asset,
    validate_image_preview_dimensions, validate_image_preview_tiled_dimensions,
    visible_tile_row_window,
};

#[test]
fn ctrl_wheel_zoom_has_stable_direction_and_limits() {
    assert!(image_preview_zoom_after_wheel(1.0, px(-120.0)) > 1.0);
    assert!(image_preview_zoom_after_wheel(1.0, px(120.0)) < 1.0);
    assert_eq!(
        image_preview_zoom_after_wheel(IMAGE_PREVIEW_MAX_ZOOM, px(-10_000.0)),
        IMAGE_PREVIEW_MAX_ZOOM
    );
    assert_eq!(
        image_preview_zoom_after_wheel(IMAGE_PREVIEW_MIN_ZOOM, px(10_000.0)),
        IMAGE_PREVIEW_MIN_ZOOM
    );
}

#[test]
fn toolbar_actions_distinguish_actual_size_from_fit_width() {
    let fit_scale = 0.5;
    assert_eq!(
        image_preview_zoom_for_action(1.0, fit_scale, ImagePreviewZoomAction::FitWidth),
        1.0
    );
    assert_eq!(
        image_preview_zoom_for_action(1.0, fit_scale, ImagePreviewZoomAction::ActualSize),
        2.0
    );
    assert_eq!(
        image_preview_zoom_for_action(1.0, fit_scale, ImagePreviewZoomAction::ZoomIn),
        1.2
    );
    assert_eq!(
        image_preview_zoom_for_action(1.0, fit_scale, ImagePreviewZoomAction::ZoomOut),
        0.8
    );
}

#[test]
fn oversized_png_is_tiled_without_losing_dimensions() {
    let dir = tempfile::tempdir().expect("image preview tempdir");
    let path = dir.path().join("tall.png");
    let width = 8;
    let height = IMAGE_PREVIEW_TILE_EDGE + 3;
    ImageBuffer::from_pixel(width, height, Rgba([12u8, 34, 56, 255]))
        .save(&path)
        .expect("write oversized PNG fixture");

    let asset = load_image_preview_asset(&path).expect("load oversized PNG preview");
    assert_eq!((asset.width, asset.height), (width, height));
    let ImagePreviewContent::Tiled(rows) = asset.content else {
        panic!("oversized PNG must use tiled rendering");
    };
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].height, IMAGE_PREVIEW_TILE_EDGE);
    assert_eq!(rows[1].height, 3);
    assert_eq!(rows[0].tiles.len(), 1);
    assert!(!rows[0].tiles[0].source.png.is_empty());

    let (visible, top, bottom) = visible_tile_row_window(&rows, 1.0, 0.0, 600.0);
    assert_eq!(visible, 0..1);
    assert_eq!(top, 0.0);
    assert_eq!(bottom, 3.0);
}

#[test]
fn hostile_image_dimensions_are_rejected_before_allocating_pixels() {
    assert!(validate_image_preview_dimensions(8_192, 4_096).is_ok());
    let error = validate_image_preview_dimensions(u32::MAX, u32::MAX)
        .expect_err("hostile dimensions must exceed the bounded preview budget");
    assert!(error.to_string().contains("exceeding the preview limit"));
    assert_eq!(IMAGE_PREVIEW_MAX_PIXELS, 32 * 1024 * 1024);
    assert!(validate_image_preview_tiled_dimensions(1_537, 63_671).is_ok());
    assert_eq!(IMAGE_PREVIEW_MAX_TILED_PIXELS, 256 * 1024 * 1024);
}
