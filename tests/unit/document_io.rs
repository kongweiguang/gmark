// @author kongweiguang

use super::{
    DocumentEncoding, DocumentOpenPolicy, OpenedDocument, decode_markdown_bytes,
    document_open_policy, open_document, open_document_with_policy, read_resident_text_from_probe,
};

#[test]
fn decodes_utf8_bom_utf16_and_detected_legacy_text() {
    let utf8 = decode_markdown_bytes(b"\xef\xbb\xbf# title\r\n").unwrap();
    assert_eq!(utf8.encoding, DocumentEncoding::Utf8);
    assert!(utf8.text.starts_with('\u{feff}'));

    let utf16le =
        decode_markdown_bytes(&[0xff, 0xfe, b'#', 0, b' ', 0, 0x2d, 0x4e, 0x87, 0x65]).unwrap();
    assert_eq!(utf16le.text, "# 中文");
    assert_eq!(
        utf16le.encoding,
        DocumentEncoding::Legacy("UTF-16LE".to_owned())
    );

    let windows_1252 = decode_markdown_bytes(&[b'c', b'a', b'f', 0xe9]).unwrap();
    assert_eq!(windows_1252.text, "caf\u{e9}");
    assert!(!windows_1252.encoding.is_utf8());
}

#[test]
fn rejects_malformed_utf16_and_binary_controls() {
    assert!(decode_markdown_bytes(&[0xff, 0xfe, 0x00]).is_err());
    assert!(decode_markdown_bytes(&[0x00, 0x01, 0x02, 0x03, 0xff]).is_err());

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("binary-without-nul.txt");
    std::fs::write(&path, vec![0x01; 4_096]).unwrap();
    assert!(open_document(&path).is_err());
}

#[test]
fn regular_non_markdown_formats_keep_resident_strategy_and_format_capabilities() {
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in [
        ("small.json", "{\"ok\":true}"),
        ("small.jsonl", "{\"id\":1}\n"),
        ("small.csv", "name,score\nAda,10\n"),
        ("small.tsv", "name\tscore\nAda\t10\n"),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        let OpenedDocument::ResidentFormat(probe) = open_document(&path).unwrap() else {
            panic!("structured text uses the format host");
        };
        assert_eq!(probe.strategy, gmark_paged_document::OpenStrategy::Resident);
    }

    for name in ["small.txt", "small.log", "small.rs", "README"] {
        let plain = dir.path().join(name);
        std::fs::write(&plain, "plain text").unwrap();
        let opened = open_document(&plain).unwrap();
        let OpenedDocument::ResidentFormat(probe) = opened else {
            panic!("plain text must use the Source host: {name}");
        };
        assert_eq!(probe.strategy, gmark_paged_document::OpenStrategy::Resident);
        assert_eq!(
            document_open_policy(&plain, &probe),
            DocumentOpenPolicy::ResidentFormat
        );
    }

    let utf16 = dir.path().join("small-utf16.md");
    std::fs::write(&utf16, [0xff, 0xfe, b'#', 0, b' ', 0, b'x', 0]).unwrap();
    let OpenedDocument::Resident(opened) = open_document(&utf16).unwrap() else {
        panic!("resident Markdown keeps regular Markdown modes regardless of text encoding");
    };
    assert_eq!(opened.text, "# x");
    assert_eq!(
        opened.encoding,
        DocumentEncoding::Legacy("UTF-16LE".to_owned())
    );
    assert_eq!(
        opened.text_encoding,
        gmark_document_core::TextEncoding::Utf16Le
    );
    assert!(opened.file_identity.is_some());
}

#[test]
fn resident_markdown_keeps_the_regular_editor_policy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("small.md");
    std::fs::write(&path, "# title\n").unwrap();

    assert!(matches!(
        open_document(&path).unwrap(),
        OpenedDocument::Resident(_)
    ));
}

#[test]
fn resident_markdown_freezes_limits_and_rejects_a_replaced_probe_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.md");
    std::fs::write(&path, "# title\n").unwrap();
    let policy = gmark_document_core::LoadingPolicy {
        max_resident_bytes: Some(8),
        max_resident_lines: Some(1_234),
        max_structural_units: Some(56_789),
        ..gmark_document_core::LoadingPolicy::default()
    };
    let OpenedDocument::Resident(opened) = open_document_with_policy(&path, policy).unwrap() else {
        panic!("exact byte threshold remains Resident");
    };
    assert_eq!(opened.loading_limits, policy.effective_limits());

    let options = gmark_paged_document::ProbeOptions {
        max_resident_bytes: 8,
        max_resident_lines: 1_234,
        max_structural_units: 56_789,
        ..gmark_paged_document::ProbeOptions::default()
    };
    let stale_probe = gmark_paged_document::probe_file(&path, options).unwrap();
    let replacement = dir.path().join("replacement.md");
    std::fs::write(&replacement, "# other\n").unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::rename(replacement, &path).unwrap();
    assert!(read_resident_text_from_probe(&path, &stale_probe, policy.effective_limits()).is_err());
}

#[test]
fn loading_policy_can_force_safe_source_without_changing_global_preferences() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("small.md");
    std::fs::write(&path, "# title\n").unwrap();
    let opened = open_document_with_policy(
        &path,
        gmark_document_core::LoadingPolicy {
            force_safe_source: true,
            ..gmark_document_core::LoadingPolicy::default()
        },
    )
    .unwrap();
    let OpenedDocument::Paged(probe) = opened else {
        panic!("safe mode must force Paged Source");
    };
    assert!(probe.force_safe_source);
}
