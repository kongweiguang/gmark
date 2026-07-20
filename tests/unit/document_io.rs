// @author kongweiguang

use super::{
    DocumentEncoding, DocumentOpenPolicy, OpenedDocument, decode_markdown_bytes,
    document_open_policy, open_document,
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
fn every_non_markdown_text_uses_the_source_backed_tab_at_any_size() {
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in [
        ("small.json", "{\"ok\":true}"),
        ("small.jsonl", "{\"id\":1}\n"),
        ("small.csv", "name,score\nAda,10\n"),
        ("small.tsv", "name\tscore\nAda\t10\n"),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        assert!(matches!(
            open_document(&path).unwrap(),
            OpenedDocument::Large(_)
        ));
    }

    for name in ["small.txt", "small.log", "small.rs", "README"] {
        let plain = dir.path().join(name);
        std::fs::write(&plain, "plain text").unwrap();
        let opened = open_document(&plain).unwrap();
        let OpenedDocument::Large(probe) = opened else {
            panic!("non-Markdown text must use Source-backed storage: {name}");
        };
        assert_eq!(
            document_open_policy(&plain, &probe),
            DocumentOpenPolicy::SourceBacked
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
