// @author kongweiguang

use super::*;
use crate::preferences::ResourceInsertBehavior;
use std::fs;

fn temp_root() -> tempfile::TempDir {
    tempfile::tempdir().expect("temporary resource root should exist")
}

fn unique_file_path(dir: &Path, preferred_name: &str) -> PathBuf {
    for index in 0.. {
        let candidate = resource_candidate_path(dir, preferred_name, index);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("deterministic resource name search is unbounded")
}

#[test]
fn copies_without_overwriting_and_returns_relative_target() {
    let root = temp_root();
    let document = root.path().join("note.md");
    let source = root.path().join("outside.txt");
    fs::write(&source, b"one").expect("source should be writable");
    fs::write(root.path().join("assets.txt"), b"existing").expect("fixture should be writable");

    let first = resource_markdown_for_path(
        "Attachment",
        &source,
        Some(&document),
        ResourceInsertBehavior::CopyToAssetsFolder,
        None,
    )
    .expect("resource should copy");
    assert!(first.1.created);
    assert!(first.0.contains("./assets/outside.txt"));
    assert!(first.1.path.is_file());
}

#[test]
fn copy_requires_a_saved_document_but_none_keeps_source() {
    let root = temp_root();
    let source = root.path().join("file.pdf");
    fs::write(&source, b"pdf").expect("source should be writable");

    assert!(
        materialize_local_resource(&source, None, ResourceInsertBehavior::CopyToAssetsFolder)
            .is_err()
    );
    let result = materialize_local_resource(&source, None, ResourceInsertBehavior::None)
        .expect("none behavior should preserve source");
    assert_eq!(result.path, source);
    assert!(!result.created);
}

#[test]
fn unique_file_path_uses_deterministic_suffixes() {
    let root = temp_root();
    fs::write(root.path().join("clip.mp4"), b"one").expect("fixture should exist");
    fs::write(root.path().join("clip-1.mp4"), b"two").expect("fixture should exist");
    assert_eq!(
        unique_file_path(root.path(), "clip.mp4"),
        root.path().join("clip-2.mp4")
    );
}

#[test]
fn all_copy_behaviors_use_their_stable_destination() {
    let root = temp_root();
    let document_dir = root.path().join("document");
    let incoming_dir = root.path().join("incoming");
    fs::create_dir_all(&document_dir).expect("document directory should exist");
    fs::create_dir_all(&incoming_dir).expect("incoming directory should exist");
    let document = document_dir.join("note.md");

    let direct = incoming_dir.join("direct.pdf");
    fs::write(&direct, b"direct").expect("fixture should exist");
    let untouched =
        materialize_local_resource(&direct, Some(&document), ResourceInsertBehavior::None)
            .expect("none behavior should retain the source");
    assert_eq!(untouched.path, direct);
    assert!(!untouched.created);

    let already_local = document_dir.join("already-local.pdf");
    fs::write(&already_local, b"local").expect("local fixture should exist");
    let untouched = materialize_local_resource(
        &already_local,
        Some(&document),
        ResourceInsertBehavior::CopyToDocumentFolder,
    )
    .expect("a resource already in the destination should be reused");
    assert_eq!(untouched.path, already_local);
    assert!(!untouched.created);

    let document_copy = incoming_dir.join("document.pdf");
    fs::write(&document_copy, b"document").expect("fixture should exist");
    let copied = materialize_local_resource(
        &document_copy,
        Some(&document),
        ResourceInsertBehavior::CopyToDocumentFolder,
    )
    .expect("document-folder copy should succeed");
    assert_eq!(copied.path, document_dir.join("document.pdf"));

    let assets_copy = incoming_dir.join("assets.pdf");
    fs::write(&assets_copy, b"assets").expect("fixture should exist");
    let copied = materialize_local_resource(
        &assets_copy,
        Some(&document),
        ResourceInsertBehavior::CopyToAssetsFolder,
    )
    .expect("assets copy should succeed");
    assert_eq!(copied.path, document_dir.join("assets/assets.pdf"));

    let named_copy = incoming_dir.join("named.pdf");
    fs::write(&named_copy, b"named").expect("fixture should exist");
    let copied = materialize_local_resource(
        &named_copy,
        Some(&document),
        ResourceInsertBehavior::CopyToNamedAssetsFolder,
    )
    .expect("named-assets copy should succeed");
    assert_eq!(copied.path, document_dir.join("note.assets/named.pdf"));
}

#[test]
fn named_assets_directory_is_reused_and_only_the_file_is_numbered() {
    let root = temp_root();
    let document = root.path().join("note.md");
    let source = root.path().join("incoming.pdf");
    fs::write(&source, b"pdf").expect("source should exist");

    let first = materialize_local_resource(
        &source,
        Some(&document),
        ResourceInsertBehavior::CopyToNamedAssetsFolder,
    )
    .expect("first copy should succeed");
    let second = materialize_local_resource(
        &source,
        Some(&document),
        ResourceInsertBehavior::CopyToNamedAssetsFolder,
    )
    .expect("second copy should succeed");

    assert_eq!(first.path, root.path().join("note.assets/incoming.pdf"));
    assert_eq!(second.path, root.path().join("note.assets/incoming-1.pdf"));
    assert!(!root.path().join("note1.assets").exists());
}

#[test]
fn existing_target_is_never_overwritten_and_cleanup_removes_only_new_copy() {
    let root = temp_root();
    let document = root.path().join("note.md");
    let source_dir = root.path().join("incoming");
    let assets_dir = root.path().join("assets");
    fs::create_dir_all(&source_dir).expect("source directory should exist");
    fs::create_dir_all(&assets_dir).expect("assets directory should exist");
    let source = source_dir.join("spec.pdf");
    let existing = assets_dir.join("spec.pdf");
    fs::write(&source, b"new").expect("source should exist");
    fs::write(&existing, b"existing").expect("existing target should exist");

    let copied = materialize_local_resource(
        &source,
        Some(&document),
        ResourceInsertBehavior::CopyToAssetsFolder,
    )
    .expect("copy should choose a deterministic suffix");

    assert_eq!(
        fs::read(&existing).expect("existing target should remain"),
        b"existing"
    );
    assert_eq!(copied.path, assets_dir.join("spec-1.pdf"));
    assert_eq!(fs::read(&copied.path).expect("copy should exist"), b"new");
    copied.cleanup_if_created();
    assert!(!copied.path.exists());
    assert_eq!(
        fs::read(existing).expect("existing target should remain"),
        b"existing"
    );
}

#[test]
fn images_stay_images_and_video_detection_is_case_insensitive() {
    let root = temp_root();
    let document = root.path().join("note.md");
    let image = root.path().join("示例 image.PNG");
    let tiff = root.path().join("scan.TIFF");
    let video = root.path().join("Demo.MP4");
    fs::write(&image, b"image").expect("image fixture should exist");
    fs::write(&tiff, b"tiff").expect("tiff fixture should exist");
    fs::write(&video, b"video").expect("video fixture should exist");

    let (image_markdown, _) = resource_markdown_for_path(
        "",
        &image,
        Some(&document),
        ResourceInsertBehavior::None,
        Some(ResourceKind::Video),
    )
    .expect("image insertion should succeed");
    assert_eq!(image_markdown, "![示例 image.PNG](<./示例 image.PNG>)");

    let (tiff_markdown, _) = resource_markdown_for_path(
        "",
        &tiff,
        Some(&document),
        ResourceInsertBehavior::None,
        None,
    )
    .expect("tiff insertion should succeed");
    assert_eq!(tiff_markdown, "![scan.TIFF](./scan.TIFF)");

    let (video_markdown, _) = resource_markdown_for_path(
        "",
        &video,
        Some(&document),
        ResourceInsertBehavior::None,
        None,
    )
    .expect("video insertion should succeed");
    assert!(video_markdown.starts_with("[Demo.MP4]("));
    assert!(video_markdown.contains("\"gmark:resource;type=video\""));
}

#[test]
fn copy_failure_does_not_modify_the_source_or_replace_blocking_path() {
    let root = temp_root();
    let document = root.path().join("note.md");
    let source = root.path().join("source.pdf");
    let blocking_assets = root.path().join("assets");
    fs::write(&source, b"source").expect("source should exist");
    fs::write(&blocking_assets, b"blocking file").expect("blocking fixture should exist");

    assert!(
        materialize_local_resource(
            &source,
            Some(&document),
            ResourceInsertBehavior::CopyToAssetsFolder,
        )
        .is_err()
    );
    assert_eq!(fs::read(source).expect("source should remain"), b"source");
    assert_eq!(
        fs::read(blocking_assets).expect("blocking file should remain"),
        b"blocking file"
    );
}

#[test]
fn markdown_target_preserves_unicode_percent_and_descendant_boundaries() {
    let root = temp_root();
    let document_dir = root.path().join("document");
    let document = document_dir.join("note.md");
    let descendant = document_dir.join("assets/100% 完成.pdf");
    let sibling = root.path().join("outside/100% 完成.pdf");

    assert_eq!(
        markdown_target(Some(&document), &descendant).expect("descendant should serialize"),
        "./assets/100% 完成.pdf"
    );
    let sibling_target =
        markdown_target(Some(&document), &sibling).expect("sibling should serialize");
    assert!(!sibling_target.starts_with("./"));
    assert!(sibling_target.ends_with("/outside/100% 完成.pdf"));
    assert!(!sibling_target.contains('\\'));
}
