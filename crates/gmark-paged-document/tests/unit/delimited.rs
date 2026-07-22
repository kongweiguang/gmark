// @author kongweiguang

use super::*;

#[test]
fn sidecar_cleanup_preserves_current_entry_and_enforces_budget() {
    let dir = tempfile::tempdir().unwrap();
    let keep = dir.path().join("keep.gmark-delimited-v1");
    let stale_a = dir.path().join("a.gmark-delimited-v1");
    let stale_b = dir.path().join("b.gmark-delimited-v1");
    std::fs::write(&keep, [0u8; 10]).unwrap();
    std::fs::write(&stale_a, [0u8; 10]).unwrap();
    std::fs::write(&stale_b, [0u8; 10]).unwrap();

    cleanup_delimited_sidecars(dir.path(), &keep, 15).unwrap();

    assert!(keep.exists());
    let total = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.metadata().unwrap().len())
        .sum::<u64>();
    assert!(total <= 15, "sidecar cache retained {total} bytes");
}

#[test]
fn resident_snapshot_index_reads_ranges_and_filters_without_a_file() {
    let bytes: Arc<[u8]> = Arc::from(&b"name,note\r\nalpha,\"one\r\ntwo\"\r\nbeta,three\r\n"[..]);
    let index = DelimitedIndex::build_snapshot_cancellable(
        Arc::clone(&bytes),
        DelimitedIndexOptions::default(),
        &SearchCancellation::default(),
    )
    .unwrap();

    assert_eq!(index.headers(), ["name", "note"]);
    assert_eq!(index.record_count(), 2);
    let records = index.read_records(0, 2).unwrap();
    assert_eq!(records[0].fields, ["alpha", "one\r\ntwo"]);
    assert_eq!(
        &bytes[records[0].byte_range.start as usize..records[0].byte_range.end as usize],
        b"alpha,\"one\r\ntwo\"\r\n"
    );
    assert_eq!(
        index
            .filter_record_indices(
                "three",
                DelimitedFilterOptions::default(),
                &SearchCancellation::default(),
            )
            .unwrap(),
        [1]
    );
}
