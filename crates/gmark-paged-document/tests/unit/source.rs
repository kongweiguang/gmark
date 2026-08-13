// @author kongweiguang

use super::PageCache;
use std::sync::Arc;

fn page(value: u8) -> Arc<[u8]> {
    Arc::from([value])
}

#[test]
fn mature_lru_keeps_recent_hits_and_evicts_the_oldest_page() {
    let mut cache = PageCache::with_capacity(2);
    cache.insert(1, page(1));
    cache.insert(2, page(2));

    assert_eq!(cache.get(1).as_deref(), Some([1].as_slice()));
    cache.insert(3, page(3));

    assert!(cache.get(2).is_none());
    assert_eq!(cache.get(1).as_deref(), Some([1].as_slice()));
    assert_eq!(cache.get(3).as_deref(), Some([3].as_slice()));
    assert_eq!(cache.len(), 2);
}

#[cfg(windows)]
#[test]
fn shared_source_allows_atomic_replacement_while_open() {
    use std::io::Write;

    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("source.txt");
    std::fs::write(&path, b"old").expect("write source");
    let source = super::super::FileSource::open(&path).expect("open source");
    let mut temporary = tempfile::NamedTempFile::new_in(directory.path()).expect("temp file");
    temporary.write_all(b"new").expect("write replacement");
    temporary.as_file().sync_all().expect("sync replacement");
    let temporary_path = temporary.into_temp_path();

    super::replace_existing_windows(&temporary_path, &path).expect("replace open source");
    assert_eq!(std::fs::read(&path).expect("read replacement"), b"new");
    drop(source);
}
