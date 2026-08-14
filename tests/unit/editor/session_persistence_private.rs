// @author kongweiguang

use super::{SessionWriteGenerationRegistry, run_current_session_write};
use std::sync::Mutex;
use std::sync::{Arc, Barrier};
use std::thread;

// This regression test keeps a writer that waited on the mutex from publishing
// an older snapshot after a newer generation was scheduled.
#[test]
fn waiting_older_session_writer_is_rejected_after_newer_generation() {
    let write_lock = Arc::new(Mutex::new(()));
    let generations = Arc::new(SessionWriteGenerationRegistry::default());
    let session_id = uuid::Uuid::new_v4();
    generations
        .set(session_id, 1)
        .expect("test generation registry must accept the initial value");
    let write_guard = write_lock
        .lock()
        .expect("test write lock must be available");
    let started = Arc::new(Barrier::new(2));
    let writes = Arc::new(Mutex::new(Vec::new()));
    let writer_lock = Arc::clone(&write_lock);
    let writer_generations = Arc::clone(&generations);
    let writer_started = Arc::clone(&started);
    let writer_writes = Arc::clone(&writes);
    let writer = thread::spawn(move || {
        writer_started.wait();
        let result =
            run_current_session_write(&writer_lock, &writer_generations, session_id, 1, || {
                writer_writes
                    .lock()
                    .expect("test writes lock must be available")
                    .push("old");
                Ok::<(), anyhow::Error>(())
            })
            .expect("generation gate must not fail");
        assert!(result.is_none(), "stale writer must be skipped");
    });

    started.wait();
    generations
        .set(session_id, 2)
        .expect("test generation registry must accept the newer value");
    drop(write_guard);
    writer.join().expect("test writer must exit cleanly");
    assert!(
        writes
            .lock()
            .expect("test writes lock must be available")
            .is_empty(),
        "an older writer waiting for the lock must not write after a newer snapshot"
    );
}
