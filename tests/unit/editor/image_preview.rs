// @author kongweiguang

use gpui::{Bounds, point, px, size};
use image::{ImageBuffer, Rgba};

use super::{
    IMAGE_PREVIEW_MAX_PIXELS, IMAGE_PREVIEW_MAX_TILED_PIXELS, IMAGE_PREVIEW_MAX_ZOOM,
    IMAGE_PREVIEW_MIN_ZOOM, IMAGE_PREVIEW_TILE_EDGE, ImagePreviewContent, ImagePreviewZoomAction,
    bounded_tile_band_height, image_preview_offset_after_anchored_zoom,
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
fn ctrl_wheel_zoom_keeps_the_point_under_the_pointer_stable() {
    let viewport = Bounds::new(point(px(100.0), px(50.0)), size(px(1_000.0), px(800.0)));
    let next = image_preview_offset_after_anchored_zoom(
        point(px(0.0), px(-100.0)),
        point(px(600.0), px(350.0)),
        viewport,
        size(1_000.0, 1_000.0),
        900.0,
        0.9,
        1_200.0,
        1.2,
    );

    assert!((f32::from(next.x) - -124.0).abs() < 0.01);
    assert!((f32::from(next.y) - -225.333_34).abs() < 0.01);
}

#[test]
fn ctrl_wheel_zoom_compensates_when_canvas_stops_being_centered() {
    let viewport = Bounds::new(point(px(0.0), px(0.0)), size(px(1_000.0), px(800.0)));
    let next = image_preview_offset_after_anchored_zoom(
        point(px(0.0), px(0.0)),
        point(px(500.0), px(300.0)),
        viewport,
        size(1_000.0, 1_000.0),
        940.0,
        0.94,
        970.0,
        0.97,
    );

    assert!((f32::from(next.x) - -9.0).abs() < 0.01);
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
    let ImagePreviewContent::Tiled(rows) = asset.content;
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
fn small_png_uses_the_bounded_tile_decoder_too() {
    let dir = tempfile::tempdir().expect("image preview tempdir");
    let path = dir.path().join("small.png");
    ImageBuffer::from_pixel(8, 4, Rgba([12u8, 34, 56, 255]))
        .save(&path)
        .expect("write png fixture");

    let asset = load_image_preview_asset(&path).expect("load small PNG preview");
    let ImagePreviewContent::Tiled(rows) = asset.content;
    assert_eq!((asset.width, asset.height), (8, 4));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tiles.len(), 1);
    assert!(!rows[0].tiles[0].source.png.is_empty());
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

#[test]
fn wide_png_tile_bands_keep_temporary_buffers_bounded() {
    assert_eq!(
        bounded_tile_band_height(1_537, 63_671),
        IMAGE_PREVIEW_TILE_EDGE
    );
    assert_eq!(bounded_tile_band_height(4_194_304, 4_096), 4);
    assert_eq!(bounded_tile_band_height(u32::MAX, 4_096), 1);
}
