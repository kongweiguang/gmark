// @author kongweiguang
// @quality-exempt optional board evidence harness: restore the platform capture backend before wiring into bootstrap.

//! Board evidence output ownership, PNG verification, and manifest publication.

use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{Read as _, Write as _},
    path::Path,
};

use image::{ImageFormat, RgbaImage};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

const AUTOMATED_REVIEWER: &str = "automated technical capture";
const MANUAL_VISUAL_NOT_VERIFIED: &str = "NOT VERIFIED";

#[derive(Clone, Serialize)]
pub(super) struct BoardEvidenceArtifact {
    pub(super) fixture: String,
    pub(super) file: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) bytes: u64,
    pub(super) sha256: String,
    pub(super) capture_method: &'static str,
    pub(super) unique_colors: u32,
    /// 不属于量化后的主导 RGBA 背景桶的像素数；不是“首像素之外”的像素数。
    pub(super) non_background_pixels: u64,
}

#[derive(Serialize)]
pub(super) struct BoardEvidenceManifest {
    pub(super) schema_version: u32,
    pub(super) status: &'static str,
    pub(super) platform: &'static str,
    pub(super) capture_method: String,
    pub(super) capture_backends: Vec<&'static str>,
    pub(super) output_directory: String,
    pub(super) build_metadata: BoardEvidenceBuildMetadata,
    pub(super) review: BoardEvidenceReview,
    pub(super) fixtures: Vec<BoardEvidenceArtifact>,
}

#[derive(Clone, Serialize)]
pub(super) struct BoardEvidenceBuildMetadata {
    pub(super) git_sha: &'static str,
    pub(super) workspace_dirty: bool,
}

#[derive(Clone, Serialize)]
pub(super) struct BoardEvidenceReview {
    pub(super) reviewer: &'static str,
    pub(super) date: &'static str,
    pub(super) manual_visual_status: &'static str,
}

pub(super) const BOARD_EVIDENCE_SCHEMA_VERSION: u32 = 3;

/// Build metadata is supplied by `build.rs`, never inferred from a caller-provided path or an
/// arbitrary runtime environment. Evidence capture must fail closed when Cargo did not establish
/// a full commit SHA, dirty-worktree bit, or build date.
pub(super) fn board_evidence_manifest(
    platform: &'static str,
    capture_method: String,
    capture_backends: Vec<&'static str>,
    output_directory: String,
    fixtures: Vec<BoardEvidenceArtifact>,
) -> Result<BoardEvidenceManifest, String> {
    let build_metadata = build_metadata_from_environment()?;
    let date = option_env!("GMARK_BUILD_DATE_UTC")
        .ok_or_else(|| "missing GMARK_BUILD_DATE_UTC build metadata".to_owned())?;
    validate_iso_date(date)?;
    Ok(BoardEvidenceManifest {
        schema_version: BOARD_EVIDENCE_SCHEMA_VERSION,
        status: "VERIFIED",
        platform,
        capture_method,
        capture_backends,
        output_directory,
        build_metadata,
        review: BoardEvidenceReview {
            reviewer: AUTOMATED_REVIEWER,
            date,
            // This process proves technical capture and PNG integrity only. It cannot perform or
            // attest to a human 100% visual review, so the generated default is never PASS.
            manual_visual_status: MANUAL_VISUAL_NOT_VERIFIED,
        },
        fixtures,
    })
}

fn build_metadata_from_environment() -> Result<BoardEvidenceBuildMetadata, String> {
    let git_sha = option_env!("GMARK_BUILD_GIT_SHA")
        .ok_or_else(|| "missing GMARK_BUILD_GIT_SHA build metadata".to_owned())?;
    validate_git_sha(git_sha)?;
    let workspace_dirty = match option_env!("GMARK_BUILD_WORKSPACE_DIRTY") {
        Some("true") => true,
        Some("false") => false,
        Some(value) => {
            return Err(format!(
                "GMARK_BUILD_WORKSPACE_DIRTY must be 'true' or 'false', got {value:?}"
            ));
        }
        None => return Err("missing GMARK_BUILD_WORKSPACE_DIRTY build metadata".to_owned()),
    };
    Ok(BoardEvidenceBuildMetadata {
        git_sha,
        workspace_dirty,
    })
}

fn validate_git_sha(value: &str) -> Result<(), String> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("build metadata git SHA must be exactly 40 hexadecimal characters".to_owned());
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err("build metadata git SHA must use lowercase hexadecimal characters".to_owned());
    }
    Ok(())
}

fn validate_iso_date(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return Err(format!(
            "build metadata date must use YYYY-MM-DD format, got {value:?}"
        ));
    }
    let year = value[0..4]
        .parse::<u16>()
        .map_err(|_| "build metadata date has an invalid year".to_owned())?;
    let month = value[5..7]
        .parse::<u8>()
        .map_err(|_| "build metadata date has an invalid month".to_owned())?;
    let day = value[8..10]
        .parse::<u8>()
        .map_err(|_| "build metadata date has an invalid day".to_owned())?;
    if !(1..=12).contains(&month) {
        return Err(format!(
            "build metadata date has an invalid calendar value: {value:?}"
        ));
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=days_in_month[usize::from(month - 1)]).contains(&day) {
        return Err(format!(
            "build metadata date has an invalid calendar value: {value:?}"
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RgbaImageMetrics {
    pub(super) unique_colors: u32,
    pub(super) non_background_pixels: u64,
}

pub(super) fn validate_board_evidence_output_target(path: &Path) -> Result<(), String> {
    crate::cli::validate_board_evidence_output_path(path)?;
    let _ = validate_board_evidence_parent_directory(path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err("output target must not be a symbolic link".to_owned())
        }
        Ok(metadata) if metadata.file_type().is_file() => {
            Err("output target already exists; refusing to overwrite it".to_owned())
        }
        Ok(_) => Err("output target must be a regular file when it exists".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect output target: {error}")),
    }
}

pub(super) fn validate_board_evidence_output_directory_target(path: &Path) -> Result<(), String> {
    crate::cli::validate_board_evidence_output_directory_path(path)?;
    let _ = validate_board_evidence_parent_directory(path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err("output directory must not be a symbolic link".to_owned())
        }
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err("output path must be a directory when it exists".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect output directory: {error}")),
    }
}

pub(super) fn validate_board_evidence_parent_directory(path: &Path) -> Result<&Path, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "output path has no parent directory".to_owned())?;
    let metadata = fs::symlink_metadata(parent)
        .map_err(|error| format!("cannot inspect output parent: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err("output parent must be a real directory, not a symlink".to_owned());
    }
    Ok(parent)
}

pub(super) fn validate_board_evidence_new_file_target(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "output target '{}' must not be a symbolic link",
            path.display()
        )),
        Ok(_) => Err(format!(
            "output target '{}' already exists; refusing to overwrite it",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "cannot inspect output target '{}': {error}",
            path.display()
        )),
    }
}

pub(super) fn ensure_board_evidence_output_directory(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err("output directory must not be a symbolic link".to_owned())
        }
        Ok(metadata) if metadata.file_type().is_dir() => Ok(false),
        Ok(_) => Err("output path must be a directory when it exists".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path)
                .map_err(|error| format!("failed to create output directory: {error}"))?;
            match fs::symlink_metadata(path) {
                Ok(metadata)
                    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() =>
                {
                    Ok(true)
                }
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    Err("created output directory became a symbolic link".to_owned())
                }
                Ok(_) => Err("created output path is not a directory".to_owned()),
                Err(error) => Err(format!("cannot inspect created output directory: {error}")),
            }
        }
        Err(error) => Err(format!("cannot inspect output directory: {error}")),
    }
}

pub(super) fn remove_board_evidence_output_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "refused to remove symlink '{}'; output ownership changed",
            path.display()
        )),
        Ok(metadata) if metadata.file_type().is_dir() => fs::remove_dir(path)
            .map_err(|error| format!("remove output directory '{}': {error}", path.display())),
        Ok(_) => Err(format!(
            "refused to remove non-directory '{}'; output ownership changed",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "inspect output directory '{}': {error}",
            path.display()
        )),
    }
}

pub(super) fn write_png_atomically(
    output: &Path,
    image: &RgbaImage,
    expected_width: u32,
    expected_height: u32,
) -> Result<(), String> {
    validate_board_evidence_output_target(output)?;
    if image.width() != expected_width || image.height() != expected_height {
        return Err(format!(
            "captured image dimensions are {}x{}, expected {}x{}",
            image.width(),
            image.height(),
            expected_width,
            expected_height
        ));
    }
    validate_rgba_image(image)?;
    let parent = output
        .parent()
        .ok_or_else(|| "output path has no parent directory".to_owned())?;
    let temporary = NamedTempFile::new_in(parent)
        .map_err(|error| format!("failed to create temporary PNG: {error}"))?;
    image
        .save_with_format(temporary.path(), ImageFormat::Png)
        .map_err(|error| format!("failed to encode PNG: {error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("failed to persist temporary PNG: {error}"))?;
    validate_png_file(temporary.path(), expected_width, expected_height)?;
    temporary.persist_noclobber(output).map_err(|error| {
        format!(
            "failed to atomically install PNG '{}': {}",
            output.display(),
            error.error
        )
    })?;
    if let Err(error) = validate_png_file(output, expected_width, expected_height) {
        // 安装后的回读仍是证据契约的一部分；失败时删除目标，不能留下不可验证的 PNG。
        let cleanup = fs::remove_file(output);
        return match cleanup {
            Ok(()) => Err(format!("installed PNG failed readback: {error}")),
            Err(cleanup_error) => Err(format!(
                "installed PNG failed readback: {error}; cleanup failed: {cleanup_error}"
            )),
        };
    }
    Ok(())
}

pub(super) fn validate_png_file(
    path: &Path,
    expected_width: u32,
    expected_height: u32,
) -> Result<(), String> {
    read_png_file(path, expected_width, expected_height).map(|_| ())
}

pub(super) fn read_png_file(
    path: &Path,
    expected_width: u32,
    expected_height: u32,
) -> Result<RgbaImage, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("failed to stat PNG: {error}"))?;
    if metadata.len() == 0 {
        return Err("PNG file is empty".to_owned());
    }
    let image = image::ImageReader::open(path)
        .map_err(|error| format!("failed to open PNG for readback: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("failed to identify PNG for readback: {error}"))?
        .decode()
        .map_err(|error| format!("failed to decode PNG for readback: {error}"))?
        .to_rgba8();
    if image.width() != expected_width || image.height() != expected_height {
        return Err(format!(
            "PNG readback dimensions are {}x{}, expected {}x{}",
            image.width(),
            image.height(),
            expected_width,
            expected_height
        ));
    }
    validate_rgba_image(&image)?;
    Ok(image)
}

pub(super) fn rgba_image_metrics(image: &RgbaImage) -> RgbaImageMetrics {
    if image.as_raw().is_empty() {
        return RgbaImageMetrics {
            unique_colors: 0,
            non_background_pixels: 0,
        };
    }
    let mut colors = HashSet::new();
    let mut background_buckets = HashMap::<u32, u64>::new();
    for pixel in image.pixels() {
        colors.insert(pixel.0);
        let bucket = quantized_rgba_bucket(pixel.0);
        let count = background_buckets.entry(bucket).or_default();
        *count = count.saturating_add(1);
    }
    // WGC 的圆角/合成边界可能把左上角变成透明或抗锯齿色；用主导量化桶定义背景，
    // 不依赖任何特定角点，也让轻微的 GPU 色差不会伪造整张图的“内容像素”。
    let dominant_background_pixels = background_buckets.values().copied().max().unwrap_or(0);
    let pixel_count = u64::try_from(image.pixels().count()).unwrap_or(u64::MAX);
    RgbaImageMetrics {
        unique_colors: u32::try_from(colors.len()).unwrap_or(u32::MAX),
        non_background_pixels: pixel_count.saturating_sub(dominant_background_pixels),
    }
}

fn quantized_rgba_bucket(pixel: [u8; 4]) -> u32 {
    // 每个颜色通道保留高 4 bit，合成 alpha 另外占 5 bit。完全透明像素无论 RGB
    // 噪声为何都归入桶 0；非透明 alpha 从桶 1 开始，避免和透明背景混淆。
    let alpha = if pixel[3] == 0 {
        0
    } else {
        u32::from((pixel[3] >> 4) + 1)
    };
    (u32::from(pixel[0] >> 4) << 13)
        | (u32::from(pixel[1] >> 4) << 9)
        | (u32::from(pixel[2] >> 4) << 5)
        | alpha
}

pub(super) fn validate_rgba_image(image: &RgbaImage) -> Result<(), String> {
    if image.as_raw().is_empty() {
        return Err("captured PNG has no pixel data".to_owned());
    }
    if !image.pixels().any(|pixel| pixel.0[3] != 0) {
        return Err("captured PNG has no non-transparent pixels".to_owned());
    }
    if !image.as_raw().iter().any(|byte| *byte != 0) {
        return Err(
            "captured PNG is all zero bytes (possible DirectComposition empty frame)".to_owned(),
        );
    }
    let metrics = rgba_image_metrics(image);
    let pixel_count = u64::from(image.width()).saturating_mul(u64::from(image.height()));
    let minimum_unique_colors = pixel_count.min(8) as u32;
    if metrics.unique_colors < minimum_unique_colors {
        return Err(format!(
            "captured PNG has insufficient color diversity: {} unique colors, expected at least {}",
            metrics.unique_colors, minimum_unique_colors
        ));
    }
    let minimum_non_background_pixels = pixel_count
        .saturating_sub(1)
        .min(16)
        .max(pixel_count / 1_000)
        .max(1);
    if metrics.non_background_pixels < minimum_non_background_pixels {
        return Err(format!(
            "captured PNG has insufficient content complexity: {} non-background pixels, expected at least {}",
            metrics.non_background_pixels, minimum_non_background_pixels
        ));
    }
    Ok(())
}

fn find_matrix_image(
    images: &[(crate::board_host::ui::BoardEvidenceFixture, RgbaImage)],
    fixture: crate::board_host::ui::BoardEvidenceFixture,
) -> Result<&RgbaImage, String> {
    images
        .iter()
        .find_map(|(candidate, image)| (*candidate == fixture).then_some(image))
        .ok_or_else(|| {
            format!(
                "Board evidence matrix is missing fixture '{}'",
                fixture.id()
            )
        })
}

fn images_are_exactly_equal(left: &RgbaImage, right: &RgbaImage) -> bool {
    left.width() == right.width()
        && left.height() == right.height()
        && left.as_raw() == right.as_raw()
}

fn images_are_reasonably_distinct(left: &RgbaImage, right: &RgbaImage) -> bool {
    if left.dimensions() != right.dimensions() {
        return true;
    }
    let total = u64::from(left.width()).saturating_mul(u64::from(left.height()));
    if total == 0 {
        return false;
    }
    let differing = left
        .pixels()
        .zip(right.pixels())
        .filter(|(left, right)| left != right)
        .count() as u64;
    // 同尺寸 fixture 至少要有 1% 的像素变化，避免仅靠一个噪声像素关闭矩阵门禁。
    differing.saturating_mul(100) >= total
}

fn images_are_reasonably_distinct_in_canvas_roi(left: &RgbaImage, right: &RgbaImage) -> bool {
    if left.dimensions() != right.dimensions() {
        return true;
    }
    let width = left.width();
    let height = left.height();
    // 画布核心区避开左右停靠面板、顶部工具条和底部 activity rail；这里只做
    // 确定性门禁，不声称这是 UI layout 的权威几何来源。
    let left_edge = width.saturating_mul(20) / 100;
    let right_edge = width.saturating_mul(75) / 100;
    let top_edge = height.saturating_mul(15) / 100;
    let bottom_edge = height.saturating_mul(88) / 100;
    if left_edge >= right_edge || top_edge >= bottom_edge {
        return false;
    }
    let roi_pixels =
        u64::from(right_edge - left_edge).saturating_mul(u64::from(bottom_edge - top_edge));
    let mut differing = 0_u64;
    for y in top_edge..bottom_edge {
        for x in left_edge..right_edge {
            if left.get_pixel(x, y) != right.get_pixel(x, y) {
                differing = differing.saturating_add(1);
            }
        }
    }
    differing.saturating_mul(100) >= roi_pixels
}

#[cfg(test)]
pub(super) fn validate_board_evidence_matrix(
    images: &[(crate::board_host::ui::BoardEvidenceFixture, RgbaImage)],
) -> Result<(), String> {
    for (fixture, image) in images {
        validate_rgba_image(image)
            .map_err(|error| format!("fixture '{}': {error}", fixture.id()))?;
    }
    validate_prevalidated_board_evidence_matrix(images)
}

/// 批量捕获路径在 native readback、原子 PNG 回读和 manifest artifact 三个边界都已完成
/// 像素验证。最终矩阵只需校验集合与场景差异，避免 debug 构建再次对十二张全分辨率图
/// 执行昂贵的颜色统计，导致证据已落盘却迟迟无法写出 manifest 或退出进程。
pub(super) fn validate_prevalidated_board_evidence_matrix(
    images: &[(crate::board_host::ui::BoardEvidenceFixture, RgbaImage)],
) -> Result<(), String> {
    let fixtures = crate::board_host::ui::BoardEvidenceFixture::ALL;
    if images.len() != fixtures.len() {
        return Err(format!(
            "Board evidence matrix has {} images, expected {}",
            images.len(),
            fixtures.len()
        ));
    }
    let mut seen = HashSet::new();
    for (fixture, _image) in images {
        if !seen.insert(*fixture) {
            return Err(format!(
                "Board evidence matrix contains duplicate fixture '{}'",
                fixture.id()
            ));
        }
    }
    for fixture in fixtures {
        let _ = find_matrix_image(images, fixture)?;
    }

    let light = find_matrix_image(images, crate::board_host::ui::BoardEvidenceFixture::Light)?;
    let dark = find_matrix_image(images, crate::board_host::ui::BoardEvidenceFixture::Dark)?;
    if images_are_exactly_equal(light, dark) {
        return Err("Board evidence matrix light.png and dark.png are identical".to_owned());
    }

    let unique_required = [
        crate::board_host::ui::BoardEvidenceFixture::Wizard,
        crate::board_host::ui::BoardEvidenceFixture::Export,
        crate::board_host::ui::BoardEvidenceFixture::Error,
        crate::board_host::ui::BoardEvidenceFixture::MissingAsset,
        crate::board_host::ui::BoardEvidenceFixture::Conflict,
        crate::board_host::ui::BoardEvidenceFixture::Recovery,
    ];
    for (index, fixture) in unique_required.iter().enumerate() {
        for other in unique_required.iter().skip(index + 1) {
            let image = find_matrix_image(images, *fixture)?;
            let other_image = find_matrix_image(images, *other)?;
            if images_are_exactly_equal(image, other_image) {
                return Err(format!(
                    "Board evidence matrix fixtures '{}' and '{}' are identical",
                    fixture.id(),
                    other.id()
                ));
            }
        }
    }

    for fixture in [
        crate::board_host::ui::BoardEvidenceFixture::Selected,
        crate::board_host::ui::BoardEvidenceFixture::TextEdit,
        crate::board_host::ui::BoardEvidenceFixture::Dense,
        crate::board_host::ui::BoardEvidenceFixture::Narrow,
    ] {
        let image = find_matrix_image(images, fixture)?;
        if !images_are_reasonably_distinct(light, image) {
            return Err(format!(
                "Board evidence fixture '{}' is not reasonably distinct from light.png",
                fixture.id()
            ));
        }
    }
    let selected = find_matrix_image(
        images,
        crate::board_host::ui::BoardEvidenceFixture::Selected,
    )?;
    let text_edit = find_matrix_image(
        images,
        crate::board_host::ui::BoardEvidenceFixture::TextEdit,
    )?;
    if !images_are_reasonably_distinct_in_canvas_roi(selected, text_edit) {
        return Err(
            "Board evidence fixtures 'selected' and 'text-edit' differ only outside the canvas ROI"
                .to_owned(),
        );
    }
    Ok(())
}

pub(super) fn board_evidence_artifact(
    fixture: crate::board_host::ui::BoardEvidenceFixture,
    output: &Path,
    image: &RgbaImage,
    capture_method: &'static str,
) -> Result<BoardEvidenceArtifact, String> {
    let metadata = fs::symlink_metadata(output).map_err(|error| {
        format!(
            "stage=manifest-artifact; failed to stat '{}': {error}",
            output.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(format!(
            "stage=manifest-artifact; output '{}' is not a regular file",
            output.display()
        ));
    }
    let file = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "stage=manifest-artifact; output filename is not valid UTF-8".to_owned())?;
    let metrics = rgba_image_metrics(image);
    Ok(BoardEvidenceArtifact {
        fixture: fixture.id().to_owned(),
        file: file.to_owned(),
        width: image.width(),
        height: image.height(),
        bytes: metadata.len(),
        sha256: sha256_file(output)?,
        capture_method,
        unique_colors: metrics.unique_colors,
        non_background_pixels: metrics.non_background_pixels,
    })
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| {
        format!(
            "stage=manifest-sha256; failed to open '{}': {error}",
            path.display()
        )
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            format!(
                "stage=manifest-sha256; failed to read '{}': {error}",
                path.display()
            )
        })?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn write_manifest_atomically(
    output: &Path,
    manifest: &BoardEvidenceManifest,
) -> Result<(), String> {
    validate_board_evidence_new_file_target(output)?;
    let parent = validate_board_evidence_parent_directory(output)?;
    let mut encoded = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("stage=manifest-encode; failed to encode JSON: {error}"))?;
    encoded.push(b'\n');
    let temporary = NamedTempFile::new_in(parent).map_err(|error| {
        format!("stage=manifest-temp; failed to create temporary JSON: {error}")
    })?;
    temporary.as_file().set_len(0).map_err(|error| {
        format!("stage=manifest-temp; failed to truncate temporary JSON: {error}")
    })?;
    let mut temporary_file = temporary.as_file();
    temporary_file
        .write_all(&encoded)
        .map_err(|error| format!("stage=manifest-write; failed to write JSON: {error}"))?;
    temporary_file
        .sync_all()
        .map_err(|error| format!("stage=manifest-sync; failed to persist JSON: {error}"))?;
    temporary.persist_noclobber(output).map_err(|error| {
        format!(
            "stage=manifest-install; failed to atomically install '{}': {}",
            output.display(),
            error.error
        )
    })?;
    let readback = fs::read(output)
        .map_err(|error| format!("stage=manifest-readback; failed to read JSON: {error}"));
    let readback = match readback {
        Ok(readback) => readback,
        Err(error) => {
            let cleanup = fs::remove_file(output);
            return match cleanup {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!("{error}; cleanup failed: {cleanup_error}")),
            };
        }
    };
    if let Err(error) = serde_json::from_slice::<serde_json::Value>(&readback) {
        let cleanup = fs::remove_file(output);
        return match cleanup {
            Ok(()) => Err(format!("stage=manifest-readback; invalid JSON: {error}")),
            Err(cleanup_error) => Err(format!(
                "stage=manifest-readback; invalid JSON: {error}; cleanup failed: {cleanup_error}"
            )),
        };
    }
    Ok(())
}
