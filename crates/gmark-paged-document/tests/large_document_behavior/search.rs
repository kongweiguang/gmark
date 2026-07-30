// @author kongweiguang

use super::*;

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
        Err(gmark_paged_document::PagedDocumentError::InvalidRange { len: 3, .. })
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
