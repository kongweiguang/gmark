// @author kongweiguang

use super::{ResourceKind, ResourceLocation, ResourceRecord, parse_resource_parts};
use std::path::Path;

#[test]
fn parses_resource_marker_and_auto_video_kind() {
    let record = ResourceRecord::parse(
        r#"[演示](./assets/demo.MP4 "gmark:resource")"#,
        Some(Path::new("C:/notes")),
    )
    .expect("resource link should parse");

    assert_eq!(record.label, "演示");
    let domain: gmark_markdown::ResourceRecord = record.clone().into();
    assert_eq!(ResourceRecord::from(domain), record);
    assert_eq!(record.kind, ResourceKind::Video);
    assert_eq!(record.explicit_kind, None);
    assert_eq!(
        record.location,
        ResourceLocation::Local(Path::new("C:/notes/assets/demo.MP4").to_path_buf())
    );
}

#[test]
fn explicit_file_override_disables_video_inference() {
    let record = ResourceRecord::parse(
        r#"[archive](archive.mp4 "gmark:resource; type=file")"#,
        None,
    )
    .expect("resource link should parse");

    assert_eq!(record.kind, ResourceKind::File);
    assert_eq!(record.explicit_kind, Some(ResourceKind::File));
}

#[test]
fn accepts_auto_type_and_rejects_duplicate_or_unknown_parameters() {
    let auto = ResourceRecord::parse(r#"[clip](clip.mp4 "gmark:resource; type=auto")"#, None)
        .expect("auto type should be accepted");
    assert_eq!(auto.kind, ResourceKind::Video);
    assert_eq!(auto.explicit_kind, None);

    assert!(
        parse_resource_parts(r#"[clip](clip.mp4 "gmark:resource;type=auto;type=file")"#).is_none()
    );
    assert!(
        parse_resource_parts(r#"[clip](clip.mp4 "gmark:resource; type=auto; extra=yes")"#)
            .is_none()
    );
}

#[test]
fn resource_round_trip_uses_canonical_title_and_escaped_destination() {
    let record = ResourceRecord::from_parts(
        "A [spec]".to_string(),
        "./folder/my file (final).pdf".to_string(),
        Some(ResourceKind::File),
        Some(Path::new("/notes")),
    );

    assert_eq!(
        record.to_markdown(),
        r#"[A \[spec\]](<./folder/my file (final).pdf> "gmark:resource;type=file")"#
    );
}

#[test]
fn rejects_reference_links_unknown_parameters_and_mixed_paragraphs() {
    for markdown in [
        r#"[doc][ref]"#,
        r#"[doc](doc.pdf "gmark:resource;unknown=yes")"#,
        r#"before [doc](doc.pdf "gmark:resource")"#,
        r#"[doc](doc.pdf "gmark:resource;type=wat")"#,
    ] {
        assert!(parse_resource_parts(markdown).is_none(), "{markdown}");
    }
}

#[test]
fn parses_inline_label_formatting_and_url_without_fetching() {
    let record = ResourceRecord::parse(
        r#"[**online**](https://example.com/spec.pdf "gmark:resource")"#,
        None,
    )
    .expect("formatted label should parse");

    assert_eq!(record.label, "online");
    assert!(matches!(record.location, ResourceLocation::Url(_)));
    assert!(!record.is_unsafe_url());
}

#[test]
fn identifies_unsafe_url_without_executing_it() {
    let record =
        ResourceRecord::parse(r#"[payload](data:text/plain,hello "gmark:resource")"#, None)
            .expect("marked link should still parse");
    assert!(record.is_unsafe_url());
}

#[test]
fn empty_label_falls_back_to_local_filename_or_url_host() {
    let local = ResourceRecord::parse(
        r#"[](./assets/spec.pdf "gmark:resource")"#,
        Some(Path::new("C:/notes")),
    )
    .expect("empty local label should use the filename");
    assert_eq!(local.label, "spec.pdf");

    let remote =
        ResourceRecord::parse(r#"[](https://example.com/download "gmark:resource")"#, None)
            .expect("empty URL label should use the host");
    assert_eq!(remote.label, "example.com");
}

#[test]
fn windows_drive_relative_target_is_local_before_scheme_detection() {
    let record = ResourceRecord::parse(r#"[clip](C:clips/demo.mp4 "gmark:resource")"#, None)
        .expect("drive-relative Windows path should parse");

    assert_eq!(record.kind, ResourceKind::Video);
    assert_eq!(
        record.location,
        ResourceLocation::Local(Path::new("C:clips/demo.mp4").to_path_buf())
    );
}

#[test]
fn rejects_inline_wrappers_outside_the_standalone_link() {
    for markdown in [
        r#"*[doc](doc.pdf "gmark:resource")*"#,
        r#"`prefix` [doc](doc.pdf "gmark:resource")"#,
        r#"![preview](preview.png) [doc](doc.pdf "gmark:resource")"#,
    ] {
        assert!(parse_resource_parts(markdown).is_none(), "{markdown}");
    }
}

#[test]
fn preserves_unedited_source_with_standard_title_delimiters() {
    for markdown in [
        r#" [doc](doc.pdf 'gmark:resource; type=file') "#,
        r#"[doc](doc.pdf (gmark:resource; type=file))"#,
    ] {
        let record = ResourceRecord::parse(markdown, None)
            .unwrap_or_else(|| panic!("standard title delimiter should parse: {markdown}"));
        assert_eq!(record.source_or_canonical_markdown(), markdown);
        assert_eq!(record.explicit_kind, Some(ResourceKind::File));
    }
}

#[test]
fn canonical_destination_with_angle_brackets_reparses_losslessly() {
    let destination = "./folder/a <draft>.pdf";
    let record = ResourceRecord::from_parts("draft".to_owned(), destination.to_owned(), None, None);

    let reparsed = ResourceRecord::parse(&record.to_markdown(), None)
        .expect("canonical destination should remain valid Markdown");
    assert_eq!(reparsed.destination, destination);
}

#[test]
fn explicit_video_supports_extensionless_url_and_only_known_unsafe_schemes_are_blocked() {
    let video = ResourceRecord::parse(
        r#"[stream](https://media.example/watch "gmark:resource;type=video")"#,
        None,
    )
    .expect("explicit video URL should parse");
    assert_eq!(video.kind, ResourceKind::Video);

    for destination in [
        "javascript:alert(1)",
        "data:text/plain,hello",
        "blob:https://example.com/id",
    ] {
        let markdown = format!(r#"[payload]({destination} "gmark:resource")"#);
        let record = ResourceRecord::parse(&markdown, None).unwrap_or_else(|| {
            panic!("marked unsafe URL should remain representable: {destination}")
        });
        assert!(record.is_unsafe_url(), "{destination}");
    }

    let custom = ResourceRecord::parse(
        r#"[custom](gmark-help:resource/cards "gmark:resource")"#,
        None,
    )
    .expect("legal custom URL scheme should parse");
    assert!(matches!(custom.location, ResourceLocation::Url(_)));
    assert!(!custom.is_unsafe_url());
}
