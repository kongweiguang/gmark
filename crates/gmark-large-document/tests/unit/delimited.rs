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
