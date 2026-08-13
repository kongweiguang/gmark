// @author kongweiguang

use super::*;

fn file_watcher_is_shared_and_reclaimed_after_last_lease() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = write_fixture(root.path())?;
    let service = DocumentService::new();
    let first = service.open_resident_file(&path, LoadingPolicy::default(), |normalized, _| {
        Ok::<_, io::Error>(source(normalized, "# watched\n"))
    })?;
    assert_eq!(service.watcher_count(), 1);

    let second = service.open_resident_file(&path, LoadingPolicy::default(), |normalized, _| {
        Ok::<_, io::Error>(source(normalized, "unexpected second body\n"))
    })?;
    assert_eq!(second.open, RegistryOpen::Existing);
    assert_eq!(service.watcher_count(), 1);

    drop(first);
    assert_eq!(second.lease_count(), 1);
    drop(second);
    for _ in 0..40 {
        if service.watcher_count() == 0 {
            return Ok(());
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    Err(test_error(
        "file watcher was not reclaimed after the last lease",
    ))
}

#[test]
fn save_as_commit_rekeys_the_process_watcher() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = write_fixture(root.path())?;
    let target = root.path().join("renamed.md");
    std::fs::write(&target, b"# target\n")?;
    let service = DocumentService::new();
    let shared = service.open_resident_file(&path, LoadingPolicy::default(), |normalized, _| {
        let probe =
            crate::document_io::probe_document_with_policy(normalized, LoadingPolicy::default())?;
        let opened = crate::document_io::read_resident_text_from_probe(
            normalized,
            &probe,
            LoadingPolicy::default().effective_limits(),
        )?;
        Ok::<_, anyhow::Error>(ResidentMarkdownSource::from_opened(normalized, opened))
    })?;
    assert_eq!(service.watcher_count(), 1);

    let reservation = service.reserve_save_as_target(&shared.handle(), &target)?;
    let SaveAsTargetReservation::Reserved(reservation) = reservation else {
        return Err(test_error("Save As target unexpectedly occupied"));
    };
    reservation.commit()?;
    assert_eq!(service.watcher_count(), 1);
    drop(shared);
    for _ in 0..40 {
        if service.watcher_count() == 0 {
            return Ok(());
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    Err(test_error("rekeyed watcher was not reclaimed"))
}

#[test]
fn save_as_reservation_drop_keeps_old_watcher_and_releases_target() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = write_fixture(root.path())?;
    let target = root.path().join("reserved.md");
    let service = DocumentService::new();
    let shared = service.open_resident_file(&path, LoadingPolicy::default(), |normalized, _| {
        Ok::<_, io::Error>(source(normalized, "# shared\n"))
    })?;
    let SaveAsTargetReservation::Reserved(first) =
        service.reserve_save_as_target(&shared.handle(), &target)?
    else {
        return Err(test_error("Save As target unexpectedly occupied"));
    };
    assert_eq!(service.watcher_count(), 1);
    drop(first);
    assert_eq!(service.watcher_count(), 1);
    let SaveAsTargetReservation::Reserved(second) =
        service.reserve_save_as_target(&shared.handle(), &target)?
    else {
        return Err(test_error("Save As target unexpectedly occupied"));
    };
    drop(second);
    drop(shared);
    Ok(())
}

#[test]
fn save_as_occupied_target_returns_shared_handle_and_lease_without_reservation() -> TestResult {
    let root = tempfile::tempdir()?;
    let source_path = root.path().join("source.md");
    let target_path = root.path().join("target.md");
    std::fs::write(&source_path, b"# source\n")?;
    std::fs::write(&target_path, b"# target\n")?;
    let service = DocumentService::new();
    let source_open =
        service.open_resident_file(&source_path, LoadingPolicy::default(), |normalized, _| {
            Ok::<_, io::Error>(source(normalized, "# source\n"))
        })?;
    let target_open =
        service.open_resident_file(&target_path, LoadingPolicy::default(), |normalized, _| {
            Ok::<_, io::Error>(source(normalized, "# target\n"))
        })?;

    let outcome = service.reserve_save_as_target(&source_open.handle(), &target_path)?;
    let SaveAsTargetReservation::Occupied(occupied) = outcome else {
        return Err(test_error("Save As target was incorrectly reserved"));
    };
    assert_eq!(occupied.document_id, target_open.document_id);
    assert_eq!(occupied.handle().lease_count(), 2);
    assert_eq!(occupied.lease_count(), 2);
    let occupied_handle = occupied.handle();
    let existing = occupied.into_existing_open()?;
    let SharedExistingOpen::Resident(existing) = existing else {
        return Err(test_error("Markdown target did not preserve Resident kind"));
    };
    assert_eq!(existing.open, RegistryOpen::Existing);
    assert_eq!(existing.handle().lease_count(), 2);
    drop(existing);
    assert_eq!(occupied_handle.lease_count(), 1);
    drop(source_open);
    drop(target_open);
    Ok(())
}

#[test]
fn save_as_occupied_paged_target_switches_without_second_body_open() -> TestResult {
    let root = tempfile::tempdir()?;
    let source_path = root.path().join("source.md");
    let target_path = root.path().join("target.txt");
    std::fs::write(&source_path, b"# source\n")?;
    std::fs::write(&target_path, b"row 1\nrow 2\n")?;
    let policy = LoadingPolicy {
        max_resident_bytes: Some(1),
        force_safe_source: false,
    };
    let target_probe = crate::document_io::probe_document_with_policy(&target_path, policy)?;
    assert_eq!(target_probe.strategy, OpenStrategy::Paged);
    let service = DocumentService::new();
    let source_open = service.open_resident_file(&source_path, policy, |normalized, _| {
        Ok::<_, io::Error>(source(normalized, "# source\n"))
    })?;
    let prepare_count = Arc::new(AtomicUsize::new(0));
    let prepare_count_for_open = prepare_count.clone();
    let target_open = service.open_paged(
        &target_path,
        target_probe.clone(),
        policy,
        move |normalized, probe, _| {
            prepare_count_for_open.fetch_add(1, Ordering::SeqCst);
            let source = FileSource::open(normalized)?;
            prepare_utf8_source(source, probe.encoding.clone())
        },
    )?;
    assert_eq!(prepare_count.load(Ordering::SeqCst), 1);
    let outcome = service.reserve_save_as_target(&source_open.handle(), &target_path)?;
    let SaveAsTargetReservation::Occupied(occupied) = outcome else {
        return Err(test_error("Paged Save As target was incorrectly reserved"));
    };
    let existing = occupied.into_existing_open()?;
    let SharedExistingOpen::Host(existing) = existing else {
        return Err(test_error("Paged target did not preserve Host kind"));
    };
    assert_eq!(existing.open, RegistryOpen::Existing);
    assert_eq!(existing.probe.strategy, OpenStrategy::Paged);
    assert_eq!(existing.handle().lease_count(), 2);
    assert_eq!(prepare_count.load(Ordering::SeqCst), 1);
    drop(existing);
    drop(source_open);
    drop(target_open);
    Ok(())
}

#[test]
fn process_watcher_accepts_clean_append_without_conflict() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = root.path().join("append.md");
    std::fs::write(&path, b"# old\n")?;
    let service = DocumentService::new();
    let shared = open_disk_as_untitled(&service, &path)?;
    let handle = shared.handle();

    std::fs::write(&path, b"# old\nnew\n")?;
    process_external_change(&handle, &path, None).map_err(test_error)?;

    let controller = handle
        .lock()
        .map_err(|error| test_error(error.to_string()))?;
    assert_eq!(controller.session().len(), 10);
    assert!(!controller.session().dirty);
    drop(controller);
    drop(shared);
    Ok(())
}

#[test]
fn process_watcher_reloads_clean_replacement() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = root.path().join("replace.md");
    std::fs::write(&path, b"# old\n")?;
    let service = DocumentService::new();
    let shared = open_disk_as_untitled(&service, &path)?;
    let handle = shared.handle();

    std::fs::remove_file(&path)?;
    std::fs::write(&path, b"# new\n")?;
    process_external_change(&handle, &path, None).map_err(test_error)?;

    let controller = handle
        .lock()
        .map_err(|error| test_error(error.to_string()))?;
    let body = controller
        .session()
        .read_range(0..controller.session().len())?;
    assert_eq!(body, b"# new\n");
    assert!(!controller.session().dirty);
    drop(controller);
    drop(shared);
    Ok(())
}

#[test]
fn process_watcher_dirty_change_broadcasts_conflict() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = root.path().join("dirty.md");
    std::fs::write(&path, b"# old\n")?;
    let service = DocumentService::new();
    let shared = open_disk_as_untitled(&service, &path)?;
    let handle = shared.handle();
    let (_snapshot, mut subscription) = handle.subscribe_with_snapshot()?;
    handle
        .lock()
        .map_err(|error| test_error(error.to_string()))?
        .dispatch(DocumentCommand::ApplyTransaction {
            view_id: DocumentViewInstanceId::new(),
            transaction_id: gmark_document_runtime::TransactionId(1),
            transaction: Transaction::new(
                DocumentRevision(0),
                vec![SourceEdit::new(0..0, "local ")],
            ),
            selection_before: SourceSelection::default(),
            selection_after: SourceSelection::default(),
        })?;

    std::fs::remove_file(&path)?;
    std::fs::write(&path, b"# disk\n")?;
    process_external_change(&handle, &path, None).map_err(test_error)?;

    let events = subscription.poll()?;
    assert!(
        events
            .iter()
            .any(|event| { matches!(event, DocumentEvent::ExternalConflict { .. }) })
    );
    drop(shared);
    Ok(())
}

#[test]
fn completed_own_save_identity_is_not_reported_as_external_conflict() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = root.path().join("own-save.md");
    std::fs::write(&path, b"# old\n")?;
    let service = DocumentService::new();
    let shared = open_disk_as_untitled(&service, &path)?;
    let handle = shared.handle();
    let (_snapshot, mut subscription) = handle.subscribe_with_snapshot()?;
    handle
        .lock()
        .map_err(|error| test_error(error.to_string()))?
        .dispatch(DocumentCommand::ApplyTransaction {
            view_id: DocumentViewInstanceId::new(),
            transaction_id: gmark_document_runtime::TransactionId(1),
            transaction: Transaction::new(
                DocumentRevision(0),
                vec![SourceEdit::new(0..0, "local ")],
            ),
            selection_before: SourceSelection::default(),
            selection_after: SourceSelection::default(),
        })?;
    let save = handle
        .request_save_snapshot()?
        .ok_or_else(|| test_error("save request was not started"))?;
    let bytes = save.read_all()?;
    std::fs::write(&path, bytes)?;
    // The watcher can observe the atomic-save replacement while the save is
    // still in flight.  The service must defer classification until the
    // controller publishes the completed identity rather than reporting an
    // external conflict from this own write.
    process_external_change(&handle, &path, None).map_err(test_error)?;
    let identity =
        gmark_document_runtime::FileIdentity::from(&FileSource::open(&path)?.identity()?);
    handle.complete_save(save.revision, identity)?;

    process_external_change(&handle, &path, None).map_err(test_error)?;
    assert!(
        !subscription
            .poll()?
            .iter()
            .any(|event| matches!(event, DocumentEvent::ExternalConflict { .. }))
    );
    drop(shared);
    Ok(())
}

#[test]
fn pending_watcher_signal_is_reclassified_after_own_save_without_second_notification() -> TestResult
{
    let root = tempfile::tempdir()?;
    let path = root.path().join("pending-own-save.md");
    std::fs::write(&path, b"# old\n")?;
    let service = DocumentService::new();
    let shared = service.open_resident_file(&path, LoadingPolicy::default(), |normalized, _| {
        Ok::<_, io::Error>(source(normalized, "# old\n"))
    })?;
    let handle = shared.handle();
    let (_snapshot, mut subscription) = handle.subscribe_with_snapshot()?;
    handle
        .lock()
        .map_err(|error| test_error(error.to_string()))?
        .dispatch(DocumentCommand::ApplyTransaction {
            view_id: DocumentViewInstanceId::new(),
            transaction_id: gmark_document_runtime::TransactionId(2),
            transaction: Transaction::new(
                DocumentRevision(0),
                vec![SourceEdit::new(0..0, "local ")],
            ),
            selection_before: SourceSelection::default(),
            selection_after: SourceSelection::default(),
        })?;
    let save = handle
        .request_save_snapshot()?
        .ok_or_else(|| test_error("save request was not started"))?;
    let bytes = save.read_all()?;
    std::fs::write(&path, bytes)?;
    for _ in 0..80 {
        if service.pending_watcher_count() != 0 {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    if service.pending_watcher_count() == 0 {
        return Err(test_error(
            "watcher did not retain the in-flight file signal",
        ));
    }
    let identity =
        gmark_document_runtime::FileIdentity::from(&FileSource::open(&path)?.identity()?);
    handle.complete_save(save.revision, identity)?;
    for _ in 0..80 {
        if service.pending_watcher_count() == 0 {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    assert_eq!(service.pending_watcher_count(), 0);
    assert!(
        !subscription
            .poll()?
            .iter()
            .any(|event| matches!(event, DocumentEvent::ExternalConflict { .. }))
    );
    drop(shared);
    Ok(())
}

#[test]
fn pending_external_replace_reloads_after_save_completion_without_second_notification() -> TestResult
{
    let root = tempfile::tempdir()?;
    let path = root.path().join("pending-replace.md");
    std::fs::write(&path, b"# old\n")?;
    let service = DocumentService::new();
    let shared = service.open_resident_file(&path, LoadingPolicy::default(), |normalized, _| {
        Ok::<_, io::Error>(source(normalized, "# old\n"))
    })?;
    let handle = shared.handle();
    handle
        .lock()
        .map_err(|error| test_error(error.to_string()))?
        .dispatch(DocumentCommand::ApplyTransaction {
            view_id: DocumentViewInstanceId::new(),
            transaction_id: gmark_document_runtime::TransactionId(3),
            transaction: Transaction::new(
                DocumentRevision(0),
                vec![SourceEdit::new(0..0, "local ")],
            ),
            selection_before: SourceSelection::default(),
            selection_after: SourceSelection::default(),
        })?;
    let save = handle
        .request_save_snapshot()?
        .ok_or_else(|| test_error("save request was not started"))?;
    std::fs::write(&path, b"# external\n")?;
    for _ in 0..80 {
        if service.pending_watcher_count() != 0 {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    if service.pending_watcher_count() == 0 {
        return Err(test_error(
            "watcher did not retain the external file signal",
        ));
    }
    let synthetic_save_identity = gmark_document_runtime::FileIdentity {
        canonical_path: path.clone(),
        len: save.identity.len,
        modified_nanos: None,
        platform_id: None,
    };
    handle.complete_save(save.revision, synthetic_save_identity)?;
    for _ in 0..80 {
        if service.pending_watcher_count() == 0 {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    let controller = handle
        .lock()
        .map_err(|error| test_error(error.to_string()))?;
    let body = controller
        .session()
        .read_range(0..controller.session().len())?;
    assert_eq!(body, b"# external\n");
    assert!(!controller.session().dirty);
    drop(controller);
    drop(shared);
    Ok(())
}

#[test]
fn pending_external_replace_reports_conflict_after_save_failure_without_second_notification()
-> TestResult {
    let root = tempfile::tempdir()?;
    let path = root.path().join("pending-failure.md");
    std::fs::write(&path, b"# old\n")?;
    let service = DocumentService::new();
    let shared = service.open_resident_file(&path, LoadingPolicy::default(), |normalized, _| {
        Ok::<_, io::Error>(source(normalized, "# old\n"))
    })?;
    let handle = shared.handle();
    let (_snapshot, mut subscription) = handle.subscribe_with_snapshot()?;
    handle
        .lock()
        .map_err(|error| test_error(error.to_string()))?
        .dispatch(DocumentCommand::ApplyTransaction {
            view_id: DocumentViewInstanceId::new(),
            transaction_id: gmark_document_runtime::TransactionId(4),
            transaction: Transaction::new(
                DocumentRevision(0),
                vec![SourceEdit::new(0..0, "local ")],
            ),
            selection_before: SourceSelection::default(),
            selection_after: SourceSelection::default(),
        })?;
    let save = handle
        .request_save_snapshot()?
        .ok_or_else(|| test_error("save request was not started"))?;
    std::fs::write(&path, b"# external\n")?;
    for _ in 0..80 {
        if service.pending_watcher_count() != 0 {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    if service.pending_watcher_count() == 0 {
        return Err(test_error("watcher did not retain the failed-save signal"));
    }
    handle.fail_save(save.revision, SaveFailureCode::Other)?;
    for _ in 0..80 {
        if service.pending_watcher_count() == 0 {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    assert_eq!(service.pending_watcher_count(), 0);
    assert!(
        subscription
            .poll()?
            .iter()
            .any(|event| matches!(event, DocumentEvent::ExternalConflict { .. }))
    );
    drop(shared);
    Ok(())
}
