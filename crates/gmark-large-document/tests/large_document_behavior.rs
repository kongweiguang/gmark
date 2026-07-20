// @author kongweiguang

use std::fs;
use std::io::Write;

use gmark_large_document::{
    DelimitedIndex, DelimitedIndexOptions, DocumentFormat, ExternalChange, FileSource, JsonIndex,
    JsonIndexOptions, LargeDocumentAdapter, LargeDocumentError, LineIndex, MarkdownTableIndex,
    OpenStrategy, PieceDocument, ProbeOptions, SearchCancellation, SearchOptions, SourceAffinity,
    SourceAnchor, TextEncoding, ViewportRequest, prepare_utf8_source, probe_file,
    search_file_source,
};

#[test]
fn probe_routes_large_json_without_loading_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("records.jsonl");
    let line = b"{\"id\":1,\"name\":\"person\"}\n";
    let mut contents = Vec::new();
    for _ in 0..2_000 {
        contents.extend_from_slice(line);
    }
    fs::write(&path, &contents).unwrap();

    let probe = probe_file(
        &path,
        ProbeOptions {
            large_file_threshold: 1_024,
            ..ProbeOptions::default()
        },
    )
    .unwrap();
    assert_eq!(probe.len, contents.len() as u64);
    assert_eq!(probe.format, DocumentFormat::JsonLines);
    assert_eq!(probe.encoding, TextEncoding::Utf8 { bom: false });
    assert_eq!(probe.strategy, OpenStrategy::Large);
}

#[test]
fn probe_rejects_sparse_nul_and_control_heavy_binary_samples() {
    let dir = tempfile::tempdir().unwrap();
    let sparse_nul = dir.path().join("sparse-nul.txt");
    let mut sparse_bytes = vec![b'A'; 4_096];
    sparse_bytes[2_048] = 0;
    fs::write(&sparse_nul, sparse_bytes).unwrap();
    assert!(matches!(
        probe_file(&sparse_nul, ProbeOptions::default()),
        Err(LargeDocumentError::Binary)
    ));

    let controls = dir.path().join("controls.txt");
    fs::write(&controls, vec![0x01; 4_096]).unwrap();
    assert!(matches!(
        probe_file(&controls, ProbeOptions::default()),
        Err(LargeDocumentError::Binary)
    ));
}

#[test]
fn probe_keeps_control_free_legacy_text_source_backed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.txt");
    let bytes = b"caf\xe9 r\xe9sum\xe9\r\n".repeat(128);
    fs::write(&path, bytes).unwrap();

    let probe = probe_file(&path, ProbeOptions::default()).unwrap();
    assert!(matches!(probe.encoding, TextEncoding::Legacy(_)));
    assert_eq!(probe.format, DocumentFormat::PlainText);
}

#[test]
fn full_index_scan_rejects_binary_bytes_hidden_between_probe_samples() {
    let dir = tempfile::tempdir().unwrap();
    for (name, hidden) in [("middle-nul.txt", 0), ("middle-invalid-utf8.txt", 0xff)] {
        let path = dir.path().join(name);
        let mut bytes = vec![b'a'; 256 * 1024];
        bytes[128 * 1024] = hidden;
        fs::write(&path, bytes).unwrap();
        let probe = probe_file(&path, ProbeOptions::default()).unwrap();
        assert_eq!(probe.encoding, TextEncoding::Utf8 { bom: false });
        let source = FileSource::open(&path).unwrap();
        assert!(matches!(
            LineIndex::build(&source),
            Err(LargeDocumentError::Binary)
        ));
    }
}

#[test]
fn full_index_utf8_validation_streams_across_scan_boundaries_and_appends() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("boundary.txt");
    let mut bytes = vec![b'a'; 8 * 1024 * 1024 - 1];
    bytes.extend_from_slice("🙂\n".as_bytes());
    fs::write(&path, bytes).unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    assert_eq!(index.line_count(), 2);

    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(&[0]).unwrap();
    file.sync_all().unwrap();
    let appended = FileSource::open(&path).unwrap();
    assert!(matches!(
        index.extend_for_append(&appended),
        Err(LargeDocumentError::Binary)
    ));
}

#[test]
fn line_index_resolves_mixed_line_endings_and_final_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mixed.txt");
    fs::write(&path, b"alpha\r\nbeta\ngamma").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();

    assert_eq!(index.line_count(), 3);
    assert_eq!(index.line_range(0), Some(0..7));
    assert_eq!(index.line_range(1), Some(7..12));
    assert_eq!(index.line_range(2), Some(12..17));
    assert_eq!(index.line_range(3), None);
}

#[test]
fn source_selection_exports_atomically_without_changing_the_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.txt");
    let exported = dir.path().join("selection.txt");
    fs::write(&path, "alpha\n世界\nomega\n").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = LargeDocumentAdapter::new(PieceDocument::open(source, index).unwrap());
    document.set_selection(6.."alpha\n世界\n".len() as u64, false);

    document
        .save_range_atomic_cancellable(
            6.."alpha\n世界\n".len() as u64,
            &exported,
            &SearchCancellation::default(),
        )
        .unwrap();

    assert_eq!(fs::read_to_string(exported).unwrap(), "世界\n");
    assert!(document.is_pristine());
    assert_eq!(document.len(), "alpha\n世界\nomega\n".len() as u64);
    assert_eq!(
        document.selection(),
        (6.."alpha\n世界\n".len() as u64, false)
    );

    let missing_parent = dir.path().join("missing").join("selection.txt");
    assert!(
        document
            .save_range_atomic_cancellable(
                6.."alpha\n世界\n".len() as u64,
                &missing_parent,
                &SearchCancellation::default(),
            )
            .is_err()
    );
    assert!(document.is_pristine());
    assert_eq!(
        document.selection(),
        (6.."alpha\n世界\n".len() as u64, false)
    );
}

#[test]
fn immutable_selection_read_honors_pre_cancelled_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cancelled-copy.txt");
    fs::write(&path, "alpha\n世界\n").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let document = LargeDocumentAdapter::new(PieceDocument::open(source, index).unwrap());
    let cancellation = SearchCancellation::default();
    cancellation.cancel();

    assert!(matches!(
        document.read_range_cancellable(0..document.len(), &cancellation),
        Err(gmark_large_document::LargeDocumentError::Cancelled)
    ));
}

#[test]
fn immutable_selection_snapshot_is_stable_while_the_live_document_keeps_editing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copy-snapshot.txt");
    fs::write(&path, "alpha\n世界\nomega\n").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut live = LargeDocumentAdapter::new(PieceDocument::open(source, index).unwrap());
    let command_range = 6..12;

    // Copy 在命令触发时克隆持久 PieceTree 根；后续编辑只能生成新根，不能改变本次结果。
    let command_snapshot = live.clone();
    live.replace_text(command_range.clone(), "updated").unwrap();

    assert_eq!(
        command_snapshot
            .read_range_cancellable(command_range, &SearchCancellation::default())
            .unwrap(),
        "世界".as_bytes()
    );
    assert_eq!(
        live.read_range(0..live.len()).unwrap(),
        "alpha\nupdated\nomega\n".as_bytes()
    );
}

#[test]
fn opened_file_source_keeps_the_command_generation_after_external_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("external-copy-source.txt");
    let replacement = dir.path().join("external-copy-replacement.txt");
    fs::write(&path, "original generation").unwrap();
    let command_source = FileSource::open(&path).unwrap();
    let opened_identity = command_source.identity().unwrap();

    // 模拟编辑器/同步工具的原子替换。稳定打开句柄必须继续指向命令触发时的文件对象，
    // 而 identity 查询仍应看到路径现在代表另一代文件。
    fs::write(&replacement, "replacement content").unwrap();
    fs::remove_file(&path).unwrap();
    fs::rename(&replacement, &path).unwrap();

    assert_eq!(
        command_source
            .read_range(0, "original generation".len() as u64)
            .unwrap(),
        b"original generation"
    );
    assert_ne!(opened_identity.os_file_id, file_id::get_file_id(&path).ok());
}

#[test]
fn line_index_pages_split_on_source_span_and_keep_variable_page_roots_seekable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wide-lines.txt");
    let mut file = fs::File::create(&path).unwrap();
    let payload = vec![b'x'; 4 * 1024 * 1024];
    for _ in 0..3 {
        file.write_all(&payload).unwrap();
        file.write_all(b"\n").unwrap();
    }
    file.sync_all().unwrap();
    drop(file);

    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build_cached(&source, dir.path().join("cache")).unwrap();
    let stats = index.storage_stats();
    assert!(stats.disk_backed);
    assert!(stats.page_count >= 3, "page count was {}", stats.page_count);
    let width = payload.len() as u64 + 1;
    assert_eq!(index.line_range(0), Some(0..width));
    assert_eq!(index.line_range(1), Some(width..width * 2));
    assert_eq!(index.line_range(2), Some(width * 2..width * 3));
    assert_eq!(index.line_for_offset(width * 2 + 123), Some(2));
}

#[test]
fn disk_page_cache_has_a_file_size_independent_hard_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bounded-cache.bin");
    let file = fs::File::create(&path).unwrap();
    file.set_len(320 * 256 * 1024).unwrap();
    let source = FileSource::open(&path).unwrap();
    for page in 0..320u64 {
        let offset = page * 256 * 1024;
        source.read_range(offset, offset + 1).unwrap();
    }
    let stats = source.cache_stats();
    assert_eq!(stats.page_bytes, 256 * 1024);
    assert_eq!(stats.max_pages, 256);
    assert!(stats.resident_pages <= stats.max_pages);
}

#[test]
fn line_index_honors_cancellation_before_scanning_or_loading_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cancelled-index.txt");
    fs::write(&path, b"one\ntwo\nthree\n").unwrap();
    let source = FileSource::open(&path).unwrap();
    let cancellation = SearchCancellation::default();
    cancellation.cancel();

    assert!(matches!(
        LineIndex::build_cancellable(&source, &cancellation),
        Err(gmark_large_document::LargeDocumentError::Cancelled)
    ));
    assert!(matches!(
        LineIndex::build_cached_cancellable(&source, dir.path().join("cache"), &cancellation),
        Err(gmark_large_document::LargeDocumentError::Cancelled)
    ));
}

#[test]
fn structured_indexers_honor_shared_lifetime_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    let cancellation = SearchCancellation::default();
    cancellation.cancel();

    let csv_path = dir.path().join("cancel.csv");
    fs::write(&csv_path, b"id,name\n1,Ada\n").unwrap();
    let csv = FileSource::open(&csv_path).unwrap();
    assert!(matches!(
        DelimitedIndex::build_cancellable(&csv, DelimitedIndexOptions::default(), &cancellation,),
        Err(gmark_large_document::LargeDocumentError::Cancelled)
    ));

    let json_path = dir.path().join("cancel.json");
    fs::write(&json_path, br#"[{"id":1}]"#).unwrap();
    let json = FileSource::open(&json_path).unwrap();
    assert!(matches!(
        JsonIndex::build_cancellable(&json, JsonIndexOptions::default(), &cancellation),
        Err(gmark_large_document::LargeDocumentError::Cancelled)
    ));

    let markdown_path = dir.path().join("cancel.md");
    fs::write(&markdown_path, b"| a | b |\n| --- | --- |\n| 1 | 2 |\n").unwrap();
    let markdown = FileSource::open(&markdown_path).unwrap();
    let lines = LineIndex::build(&markdown).unwrap();
    assert!(matches!(
        MarkdownTableIndex::detect_all_cancellable(&markdown, lines, &cancellation),
        Err(gmark_large_document::LargeDocumentError::Cancelled)
    ));
}

#[test]
fn editor_adapter_returns_generation_bound_utf8_safe_bounded_viewports() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("adapter.txt");
    let long_line = format!("{}终点", "😀".repeat(40_000));
    fs::write(&path, format!("zero\n{long_line}\ntwo\n")).unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let document = PieceDocument::open(source, index).unwrap();
    let mut adapter = LargeDocumentAdapter::new(document);

    let request = ViewportRequest::bounded(1, 1, 1, 65_537, 42);
    let snapshot = adapter.read_viewport(&request).unwrap();
    assert_eq!(snapshot.generation, 42);
    assert_eq!(snapshot.requested_lines, 0..3);
    assert_eq!(snapshot.lines.len(), 3);
    let long = &snapshot.lines[1];
    assert!(long.leading_truncated);
    assert!(long.trailing_truncated);
    assert!(long.text.len() <= 64 * 1024);
    assert!(std::str::from_utf8(long.text.as_bytes()).is_ok());
    let cancellation = SearchCancellation::default();
    cancellation.cancel();
    assert!(matches!(
        adapter.read_viewport_cancellable(&request, &cancellation),
        Err(gmark_large_document::LargeDocumentError::Cancelled)
    ));

    adapter.set_selection(0..4, false);
    adapter.replace_text(0..4, "ZERO").unwrap();
    assert_eq!(adapter.selection(), (4..4, false));
    assert_eq!(adapter.backend().generation(), 1);
    assert!(adapter.undo());
    assert_eq!(adapter.backend().generation(), 2);
    assert!(adapter.redo());
    assert_eq!(adapter.backend().generation(), 3);
}

#[test]
fn line_index_sidecar_is_content_free_and_rebuilds_when_stale_or_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cached.txt");
    let cache = dir.path().join("cache");
    fs::write(&path, b"one\ntwo\n").unwrap();
    let source = FileSource::open(&path).unwrap();
    let first = LineIndex::build_cached(&source, &cache).unwrap();
    let sidecar = LineIndex::sidecar_path(&source, &cache).unwrap();
    assert_eq!(first.line_count(), 3);
    let cached = fs::read(&sidecar).unwrap();
    assert!(!cached.windows(3).any(|window| window == b"one"));

    fs::write(&sidecar, b"broken cache").unwrap();
    let rebuilt = LineIndex::build_cached(&source, &cache).unwrap();
    assert_eq!(rebuilt.line_count(), 3);

    fs::write(&path, b"one\ntwo\nthree\n").unwrap();
    let changed_source = FileSource::open(&path).unwrap();
    let changed = LineIndex::build_cached(&changed_source, &cache).unwrap();
    assert_eq!(changed.line_count(), 4);
}

#[test]
fn cached_line_index_pages_are_disk_backed_and_resident_cache_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("many-lines.txt");
    let cache = dir.path().join("cache");
    let text = (0..100_000)
        .map(|line| format!("line-{line:06}\n"))
        .collect::<String>();
    fs::write(&path, text).unwrap();
    let source = FileSource::open(&path).unwrap();

    let uncached = LineIndex::build(&source).unwrap();
    let uncached_stats = uncached.storage_stats();
    assert!(uncached_stats.disk_backed);
    assert!(uncached_stats.resident_pages <= uncached_stats.max_resident_pages);

    let index = LineIndex::build_cached(&source, &cache).unwrap();
    let initial = index.storage_stats();
    assert!(initial.disk_backed);
    assert!(initial.page_count > initial.resident_pages);
    assert!(initial.resident_pages <= initial.max_resident_pages);

    for line in (0..100_000u64).step_by(4_097) {
        let range = index.line_range(line).expect("random disk-backed line");
        let expected = format!("line-{line:06}\n");
        assert_eq!(
            source.read_range(range.start, range.end).unwrap(),
            expected.as_bytes()
        );
    }
    let settled = index.storage_stats();
    assert!(settled.disk_backed);
    assert!(settled.resident_pages <= settled.max_resident_pages);
}

#[test]
fn repeated_large_appends_keep_line_index_storage_layers_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("append.log");
    let initial = (0..70_000)
        .map(|line| format!("base-{line:06}\n"))
        .collect::<String>();
    fs::write(&path, initial).unwrap();
    let mut source = FileSource::open(&path).unwrap();
    let mut index = LineIndex::build(&source).unwrap();

    for round in 0..20u64 {
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        for line in 0..5_000u64 {
            writeln!(file, "append-{round:02}-{line:04}").unwrap();
        }
        file.flush().unwrap();
        source = FileSource::open(&path).unwrap();
        index = index.extend_for_append(&source).unwrap();
        let stats = index.storage_stats();
        assert!(stats.disk_backed);
        assert!(stats.resident_pages <= stats.max_resident_pages);
        assert!(stats.max_resident_pages <= 128);
    }

    assert_eq!(index.line_count(), 170_001);
    let last_content = index
        .line_range(169_999)
        .expect("last appended content line");
    assert_eq!(
        source
            .read_range(last_content.start, last_content.end)
            .unwrap(),
        b"append-19-4999\n"
    );
}

#[test]
fn utf16_shadow_edits_and_streams_back_to_the_original_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("utf16.txt");
    let mut encoded = vec![0xff, 0xfe];
    for unit in "alpha\n世界".encode_utf16() {
        encoded.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&path, encoded).unwrap();
    let original = FileSource::open(&path).unwrap();
    let mut prepared = prepare_utf8_source(original, TextEncoding::Utf16Le).unwrap();
    let source = prepared.source().clone();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    document.replace_text(0..5, "bravo").unwrap();
    let plan = prepared.save_plan().unwrap();
    let copy_path = dir.path().join("utf16-copy.txt");
    plan.save_atomic_as(&document, &copy_path).unwrap();
    assert!(fs::read(&copy_path).unwrap().starts_with(&[0xff, 0xfe]));
    let identity = plan.save_atomic(&document, &path).unwrap();
    prepared.mark_original_saved(identity);

    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(&[0xff, 0xfe]));
    let units = bytes[2..]
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    assert_eq!(String::from_utf16(&units).unwrap(), "bravo\n世界");
}

#[test]
fn selection_export_reuses_original_encoding_and_is_atomic_on_unrepresentable_text() {
    let dir = tempfile::tempdir().unwrap();
    let utf16_path = dir.path().join("utf16-source.txt");
    let mut encoded = vec![0xff, 0xfe];
    for unit in "alpha\n世界\nomega".encode_utf16() {
        encoded.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&utf16_path, encoded).unwrap();
    let prepared = prepare_utf8_source(
        FileSource::open(&utf16_path).unwrap(),
        TextEncoding::Utf16Le,
    )
    .unwrap();
    let source = prepared.source().clone();
    let index = LineIndex::build(&source).unwrap();
    let document = PieceDocument::open(source, index).unwrap();
    let exported = dir.path().join("utf16-selection.txt");
    prepared
        .save_plan()
        .unwrap()
        .save_range_atomic_as_cancellable(
            &document,
            6..12,
            &exported,
            &SearchCancellation::default(),
        )
        .unwrap();
    let bytes = fs::read(&exported).unwrap();
    assert!(bytes.starts_with(&[0xff, 0xfe]));
    let units = bytes[2..]
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    assert_eq!(String::from_utf16(&units).unwrap(), "世界");

    let legacy_path = dir.path().join("legacy-source.txt");
    fs::write(&legacy_path, b"cafe\xe9").unwrap();
    let legacy = prepare_utf8_source(
        FileSource::open(&legacy_path).unwrap(),
        TextEncoding::Legacy("windows-1252".to_owned()),
    )
    .unwrap();
    let source = legacy.source().clone();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    let start = document.len();
    document.replace_text(start..start, " 文").unwrap();
    let failed_target = dir.path().join("legacy-selection.txt");
    fs::write(&failed_target, b"keep-me").unwrap();
    assert!(matches!(
        legacy
            .save_plan()
            .unwrap()
            .save_range_atomic_as_cancellable(
                &document,
                start..document.len(),
                &failed_target,
                &SearchCancellation::default(),
            ),
        Err(gmark_large_document::LargeDocumentError::UnrepresentableEncoding { .. })
    ));
    assert_eq!(fs::read(&failed_target).unwrap(), b"keep-me");
}

#[test]
fn legacy_save_refuses_characters_the_original_encoding_cannot_represent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.txt");
    fs::write(&path, b"cafe\xe9").unwrap();
    let original = FileSource::open(&path).unwrap();
    let prepared =
        prepare_utf8_source(original, TextEncoding::Legacy("windows-1252".to_owned())).unwrap();
    let source = prepared.source().clone();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    document
        .replace_text(document.len()..document.len(), " 文")
        .unwrap();

    assert!(
        prepared
            .save_plan()
            .unwrap()
            .save_atomic(&document, &path)
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), b"cafe\xe9");
}

#[test]
fn piece_document_edits_disk_backed_source_and_searches_across_pieces() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.txt");
    fs::write(&path, b"alpha beta gamma").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    assert!(document.is_pristine());

    document.replace_text(6..10, "bravo").unwrap();
    assert!(!document.is_pristine());
    assert_eq!(document.len(), 17);
    assert_eq!(document.read_range(0..17).unwrap(), b"alpha bravo gamma");
    let found = &document.search_literal(b"o g", 10).unwrap()[0];
    assert_eq!(found.range, 10..13);
    assert_eq!(found.anchor, SourceAnchor::new(10, SourceAffinity::Before));
    assert_eq!(found.head, SourceAnchor::new(13, SourceAffinity::After));
    assert!(document.undo());
    assert!(document.is_pristine());
    assert_eq!(document.read_range(0..16).unwrap(), b"alpha beta gamma");
    assert!(document.redo());
    assert!(!document.is_pristine());
    document.save_atomic(&path).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"alpha bravo gamma");
}

#[test]
fn disk_source_search_returns_results_before_line_index_exists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pre-index-search.txt");
    let mut bytes = vec![b'x'; 256 * 1024 - 3];
    bytes.extend_from_slice("NEEDLE 世界".as_bytes());
    fs::write(&path, bytes).unwrap();
    let source = FileSource::open(&path).unwrap();
    let cancellation = SearchCancellation::default();
    let matches = search_file_source(
        &source,
        r"NEEDLE\s+世界",
        SearchOptions {
            case_sensitive: true,
            regex: true,
            ..SearchOptions::default()
        },
        &cancellation,
    )
    .unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].range.start, 256 * 1024 - 3);
    let folded = search_file_source(
        &source,
        "needle 世界",
        SearchOptions::default(),
        &cancellation,
    )
    .unwrap();
    assert_eq!(folded.len(), 1);
    assert_eq!(folded[0].range.start, 256 * 1024 - 3);
}

#[test]
fn file_source_reads_are_bound_to_the_opened_generation_length() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("generation.log");
    fs::write(&path, b"old").unwrap();
    let source = FileSource::open(&path).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"-new")
        .unwrap();

    assert_eq!(source.read_range(0, 3).unwrap(), b"old");
    assert!(matches!(
        source.read_range(0, 7),
        Err(gmark_large_document::LargeDocumentError::InvalidRange { len: 3, .. })
    ));
    let refreshed = FileSource::open(&path).unwrap();
    assert_eq!(refreshed.read_range(0, 7).unwrap(), b"old-new");
}

#[test]
fn search_supports_case_whole_word_regex_and_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("search.txt");
    fs::write(&path, "Alpha alpha alphabet\n编号42 编号7").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let document = PieceDocument::open(source, index).unwrap();
    let cancellation = SearchCancellation::default();

    let words = document
        .search(
            "alpha",
            SearchOptions {
                whole_word: true,
                ..SearchOptions::default()
            },
            &cancellation,
        )
        .unwrap();
    assert_eq!(words.len(), 2);
    let numbers = document
        .search(
            r"编号\d+",
            SearchOptions {
                regex: true,
                case_sensitive: true,
                ..SearchOptions::default()
            },
            &cancellation,
        )
        .unwrap();
    assert_eq!(numbers.len(), 2);

    let cancelled = SearchCancellation::default();
    cancelled.cancel();
    assert!(
        document
            .search("alpha", SearchOptions::default(), &cancelled)
            .is_err()
    );
    assert!(
        document
            .search(
                "Alpha",
                SearchOptions {
                    case_sensitive: true,
                    ..SearchOptions::default()
                },
                &cancelled,
            )
            .is_err(),
        "case-sensitive literal fast path must remain cancellable"
    );
}

#[test]
fn regex_search_keeps_automaton_state_across_windows_larger_than_eight_mib() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long-regex.txt");
    let prefix = b"prefix ";
    let body_len = 9 * 1024 * 1024;
    let mut contents = Vec::with_capacity(prefix.len() + body_len + 16);
    contents.extend_from_slice(prefix);
    contents.extend_from_slice(b"BEGIN");
    contents.resize(contents.len() + body_len, b'x');
    contents.extend_from_slice(b"END suffix");
    fs::write(&path, &contents).unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let document = PieceDocument::open(source, index).unwrap();

    let matches = document
        .search(
            r"BEGINx+END",
            SearchOptions {
                regex: true,
                case_sensitive: true,
                ..SearchOptions::default()
            },
            &SearchCancellation::default(),
        )
        .unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0].range,
        prefix.len() as u64..(prefix.len() + 5 + body_len + 3) as u64
    );
}

#[test]
fn streaming_regex_matches_standard_non_overlapping_utf8_and_anchor_semantics() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("regex-semantics.txt");
    let contents = "éaa\nrow one\nROW two\n尾";
    fs::write(&path, contents).unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let document = PieceDocument::open(source, index).unwrap();
    let patterns = [r"a*", r"(?m)^row.*$", r"^|$", r"\p{L}+", r"(?:)"];

    for pattern in patterns {
        let expected = regex::Regex::new(pattern)
            .unwrap()
            .find_iter(contents)
            .map(|found| found.start() as u64..found.end() as u64)
            .collect::<Vec<_>>();
        let actual = document
            .search(
                pattern,
                SearchOptions {
                    regex: true,
                    case_sensitive: true,
                    ..SearchOptions::default()
                },
                &SearchCancellation::default(),
            )
            .unwrap()
            .into_iter()
            .map(|found| found.range)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "pattern {pattern:?}");
    }
}

#[test]
fn atomic_save_refuses_to_overwrite_an_external_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.txt");
    fs::write(&path, b"base").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    document.replace_text(0..4, "local").unwrap();
    fs::write(&path, b"external change").unwrap();

    assert!(document.save_atomic(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"external change");
}

#[test]
fn cancelled_streaming_saves_leave_utf8_and_encoded_targets_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let utf8_path = dir.path().join("cancelled-save.txt");
    fs::write(&utf8_path, b"base").unwrap();
    let source = FileSource::open(&utf8_path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    document.replace_text(0..4, "local").unwrap();
    let cancellation = SearchCancellation::default();
    cancellation.cancel();
    assert!(matches!(
        document.save_atomic_cancellable(&utf8_path, &cancellation),
        Err(gmark_large_document::LargeDocumentError::Cancelled)
    ));
    assert_eq!(fs::read(&utf8_path).unwrap(), b"base");

    let utf16_path = dir.path().join("cancelled-save-utf16.txt");
    let mut encoded = vec![0xff, 0xfe];
    for unit in "base".encode_utf16() {
        encoded.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&utf16_path, &encoded).unwrap();
    let prepared = prepare_utf8_source(
        FileSource::open(&utf16_path).unwrap(),
        TextEncoding::Utf16Le,
    )
    .unwrap();
    let shadow = prepared.source().clone();
    let shadow_index = LineIndex::build(&shadow).unwrap();
    let encoded_document = PieceDocument::open(shadow, shadow_index).unwrap();
    assert!(matches!(
        prepared.save_plan().unwrap().save_atomic_cancellable(
            &encoded_document,
            &utf16_path,
            &cancellation
        ),
        Err(gmark_large_document::LargeDocumentError::Cancelled)
    ));
    assert_eq!(fs::read(&utf16_path).unwrap(), encoded);
}

#[test]
fn clean_document_accepts_a_pure_append_with_incremental_line_indexing() {
    use std::io::Write as _;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tail.log");
    fs::write(&path, b"alpha").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index.clone()).unwrap();

    let mut writer = fs::OpenOptions::new().append(true).open(&path).unwrap();
    writer.write_all(b"\nbeta\n").unwrap();
    writer.sync_all().unwrap();
    assert!(matches!(
        document.external_change().unwrap(),
        ExternalChange::Appended { .. }
    ));

    let appended_source = FileSource::open(&path).unwrap();
    let appended_index = index.extend_for_append(&appended_source).unwrap();
    assert_eq!(appended_index.line_range(0), Some(0..6));
    assert_eq!(appended_index.line_range(1), Some(6..11));
    assert_eq!(appended_index.line_range(2), Some(11..11));
    document
        .accept_external_append(appended_source, appended_index)
        .unwrap();
    assert_eq!(document.line_count(), 3);
    assert_eq!(
        document.read_range(0..document.len()).unwrap(),
        b"alpha\nbeta\n"
    );
    assert_eq!(
        document.external_change().unwrap(),
        ExternalChange::Unchanged
    );
}

#[test]
fn larger_same_file_with_rewritten_prefix_is_not_misclassified_as_append() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rewritten.log");
    fs::write(&path, b"alpha\n").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let document = PieceDocument::open(source, index).unwrap();

    fs::write(&path, b"omega\nlonger\n").unwrap();
    assert_eq!(
        document.external_change().unwrap(),
        ExternalChange::Modified
    );
}

#[test]
fn line_ranges_follow_newlines_inserted_and_removed_by_piece_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.txt");
    fs::write(&path, b"alpha\nbeta\ngamma").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    assert_eq!(document.line_for_offset(0), Some(0));
    assert_eq!(document.line_for_offset(5), Some(0));
    assert_eq!(document.line_for_offset(6), Some(1));

    document.replace_text(6..10, "one\ntwo").unwrap();
    assert_eq!(document.line_count(), 4);
    assert_eq!(document.line_for_offset(10), Some(2));
    assert_eq!(
        document
            .read_range(document.line_range(1).unwrap())
            .unwrap(),
        b"one\n"
    );
    assert_eq!(
        document
            .read_range(document.line_range(2).unwrap())
            .unwrap(),
        b"two\n"
    );
    document.replace_text(5..14, " ").unwrap();
    assert_eq!(document.line_count(), 1);
    assert_eq!(
        document.read_range(0..document.len()).unwrap(),
        b"alpha gamma"
    );
}

#[test]
fn fragmented_piece_document_preserves_public_behavior_across_history_and_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fragmented.txt");
    let mut expected = (0..2_048)
        .map(|line| format!("row-{line:04}\n"))
        .collect::<String>();
    fs::write(&path, expected.as_bytes()).unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    let mut snapshots = Vec::new();

    for edit in 0..512usize {
        snapshots.push(expected.clone());
        let start = (edit * 37) % (expected.len() + 1);
        let remove = (edit % 3).min(expected.len() - start);
        let replacement = match edit % 4 {
            0 => "X\n",
            1 => "yz",
            2 => "",
            _ => "Q",
        };
        document
            .replace_text(start as u64..(start + remove) as u64, replacement)
            .unwrap();
        expected.replace_range(start..start + remove, replacement);
    }

    assert_eq!(
        document.read_range(0..document.len()).unwrap(),
        expected.as_bytes()
    );
    for offset in [0, expected.len() / 3, expected.len() / 2, expected.len()] {
        let expected_line = expected.as_bytes()[..offset]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u64;
        assert_eq!(document.line_for_offset(offset as u64), Some(expected_line));
    }
    let expected_match = memchr::memmem::find(expected.as_bytes(), b"row-1500")
        .expect("untouched search marker") as u64;
    assert_eq!(
        document.search_literal(b"row-1500", 1).unwrap()[0].range,
        expected_match..expected_match + 8
    );

    for snapshot in snapshots.iter().rev() {
        assert!(document.undo());
        assert_eq!(
            document.read_range(0..document.len()).unwrap(),
            snapshot.as_bytes()
        );
    }
    for _ in 0..snapshots.len() {
        assert!(document.redo());
    }
    assert_eq!(
        document.read_range(0..document.len()).unwrap(),
        expected.as_bytes()
    );
    document.save_atomic(&path).unwrap();
    assert_eq!(fs::read(&path).unwrap(), expected.as_bytes());
}
