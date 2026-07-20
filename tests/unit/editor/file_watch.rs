// @author kongweiguang

use super::{FileWatchSignal, external_conflict_for_snapshot, same_watch_path, start_file_watch};
use futures::channel::mpsc::TryRecvError;
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
fn watch_path_filter_rejects_unrelated_siblings() {
    assert!(same_watch_path(
        Path::new("notes.md"),
        Path::new("notes.md")
    ));
    assert!(!same_watch_path(
        Path::new("notes.md"),
        Path::new("other.md")
    ));
}

#[test]
fn stale_fingerprint_result_does_not_replace_new_save_baseline() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("notes.md");
    std::fs::write(&path, "first").unwrap();
    let first = crate::recovery::fingerprint_contents(&path, b"first").unwrap();
    std::fs::write(&path, "second").unwrap();
    let second = crate::recovery::fingerprint_contents(&path, b"second").unwrap();
    assert_eq!(
        external_conflict_for_snapshot(Some(&path), Some(&first), &path, &first, Ok(&first),),
        Some(false)
    );
    assert_eq!(
        external_conflict_for_snapshot(Some(&path), Some(&first), &path, &first, Ok(&second),),
        Some(true)
    );
    assert_eq!(
        external_conflict_for_snapshot(Some(&path), Some(&first), &path, &first, Err(())),
        Some(true)
    );
    assert_eq!(
        external_conflict_for_snapshot(Some(&path), Some(&second), &path, &first, Ok(&second),),
        None
    );
}

#[test]
fn watcher_survives_atomic_replacement_and_filters_siblings() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("notes.md");
    let sibling = temp.path().join("other.md");
    std::fs::write(&path, "base").unwrap();
    std::fs::write(&sibling, "base").unwrap();
    let (_guard, mut receiver) = start_file_watch(path.clone()).unwrap();

    std::fs::write(&sibling, "sibling edit").unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));

    gmark_document::atomic_write(&path, b"atomic replacement").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match receiver.try_recv() {
            Ok(FileWatchSignal::Changed) => break,
            Ok(FileWatchSignal::Error(error)) => {
                panic!("watcher reported error: {error}")
            }
            Err(TryRecvError::Empty) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(TryRecvError::Empty) => {
                panic!("timed out waiting for atomic replacement event")
            }
            Err(TryRecvError::Closed) => panic!("watcher channel closed"),
        }
    }
}
