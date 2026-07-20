// @author kongweiguang

use super::*;

fn injected_failure(stage: AtomicWriteStage) -> io::Error {
    io::Error::new(
        io::ErrorKind::StorageFull,
        format!("injected failure at {stage}"),
    )
}

#[test]
fn every_pre_commit_failure_preserves_the_existing_target() {
    let stages = [
        AtomicWriteStage::ValidateTarget,
        AtomicWriteStage::InspectTarget,
        AtomicWriteStage::CreateTemporary,
        AtomicWriteStage::ApplyPermissions,
        AtomicWriteStage::WriteContents,
        AtomicWriteStage::FlushContents,
        AtomicWriteStage::SyncTemporary,
        AtomicWriteStage::PersistTemporary,
    ];

    for failed_stage in stages {
        let directory = tempfile::tempdir().expect("create test directory");
        let target = directory.path().join("document.md");
        fs::write(&target, b"old source").expect("seed target");

        let error = atomic_write_with_stage_hook(&target, b"new source", |stage| {
            if stage == failed_stage {
                Err(injected_failure(stage))
            } else {
                Ok(())
            }
        })
        .expect_err("injected stage must fail");

        assert_eq!(error.stage(), failed_stage);
        assert!(!error.target_may_have_changed());
        assert_eq!(fs::read(&target).expect("read target"), b"old source");
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("scan test directory")
                .count(),
            1,
            "temporary file leaked after {failed_stage}"
        );
    }
}

#[test]
fn post_commit_failure_reports_that_the_target_was_replaced() {
    for failed_stage in [
        AtomicWriteStage::SyncPersisted,
        AtomicWriteStage::SyncDirectory,
    ] {
        let directory = tempfile::tempdir().expect("create test directory");
        let target = directory.path().join("document.md");
        fs::write(&target, b"old source").expect("seed target");

        let error = atomic_write_with_stage_hook(&target, b"new source", |stage| {
            if stage == failed_stage {
                Err(injected_failure(stage))
            } else {
                Ok(())
            }
        })
        .expect_err("injected stage must fail");

        assert_eq!(error.stage(), failed_stage);
        assert!(error.target_may_have_changed());
        assert_eq!(fs::read(&target).expect("read target"), b"new source");
    }
}

#[test]
fn permission_denied_before_commit_keeps_the_original_bytes() {
    let directory = tempfile::tempdir().expect("create test directory");
    let target = directory.path().join("document.md");
    fs::write(&target, b"old source").expect("seed target");

    let error = atomic_write_with_stage_hook(&target, b"new source", |stage| {
        if stage == AtomicWriteStage::CreateTemporary {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected read-only directory",
            ))
        } else {
            Ok(())
        }
    })
    .expect_err("permission failure must abort the save");

    assert_eq!(error.stage(), AtomicWriteStage::CreateTemporary);
    assert!(!error.target_may_have_changed());
    assert_eq!(fs::read(&target).expect("read target"), b"old source");
}
