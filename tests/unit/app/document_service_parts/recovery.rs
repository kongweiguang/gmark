// @author kongweiguang

use super::*;

#[test]
fn recovered_source_format_is_applied_before_controller_publication() -> TestResult {
    let format = SourceDocument::new("first\r\nsecond\r\n").source_format();
    let source = ResidentMarkdownSource::from_recovered(
        "first\r\nsecond\r\n",
        Some(PathBuf::from("recovery.md")),
        format.clone(),
    )?;
    let service = DocumentService::new();
    let opened = service.open_recovery(DocumentId::new(), source)?;
    let opened_handle = opened.handle();
    let controller = opened_handle
        .lock()
        .map_err(|error| test_error(error.to_string()))?;
    assert_eq!(controller.source_format_snapshot(), Some(format));
    assert!(controller.session().dirty);
    Ok(())
}

#[test]
fn recovery_resume_keeps_source_for_shared_controller_and_journal() -> TestResult {
    let root = tempfile::tempdir()?;
    let recovered_text = "recovered\r\nbody\r\n";
    let mut journal =
        crate::recovery::RecoveryJournal::create(root.path(), None, "base\r\nbody\r\n".to_owned())?;
    let source_format = SourceDocument::new(recovered_text).source_format();
    journal.record_formatted(
        recovered_text,
        source_format,
        crate::recovery::RecoverySelection::from_source_selection(
            gmark_document_core::SourceSelection::default(),
        ),
        "source",
    )?;

    let mut recovered_documents = crate::recovery::load_recovery_documents(root.path())?;
    let recovered = recovered_documents
        .pop()
        .ok_or_else(|| test_error("recovery journal did not produce a document"))?;
    let source = ResidentMarkdownSource::from_recovered(
        recovered.source.as_str(),
        recovered.file_path.clone(),
        recovered.source_format.clone(),
    )?;
    let service = DocumentService::new();
    let opened = service.open_recovery(DocumentId::new(), source)?;
    let opened_handle = opened.handle();
    let controller = opened_handle
        .lock()
        .map_err(|error| test_error(error.to_string()))?;
    let (shared_text, serialized_bytes) = controller
        .session()
        .resident_source_document()
        .map(|source| (source.text(), source.serialized_bytes()))
        .ok_or_else(|| test_error("recovery did not create a resident source"))?;
    let normalized = SourceDocument::new(&recovered.source).text();
    assert_eq!(shared_text, normalized);
    assert_eq!(serialized_bytes, recovered.source.as_bytes());
    drop(controller);

    let mut resumed = crate::recovery::RecoveryJournal::resume(&recovered);
    resumed.record_formatted(
        "recovered\r\nupdated\r\n",
        recovered.source_format.clone(),
        recovered.selection.clone(),
        &recovered.view_mode,
    )?;
    Ok(())
}

#[test]
fn external_conflict_reaches_shared_controller_event_log() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = write_fixture(root.path())?;
    let service = DocumentService::new();
    let opened = service.open_resident_file(&path, LoadingPolicy::default(), |normalized, _| {
        Ok::<_, io::Error>(source(normalized, "# watched\n"))
    })?;
    let handle = opened.handle();
    let (_, mut events) = handle
        .subscribe_with_snapshot()
        .map_err(|error| test_error(error.to_string()))?;
    std::fs::write(&path, b"# changed\n")?;
    super::dispatch_external_conflict(&handle, &path);
    let events = events
        .poll()
        .map_err(|error| test_error(error.to_string()))?;
    assert!(
        events
            .iter()
            .any(|event| matches!(event, DocumentEvent::ExternalConflict { .. }))
    );
    Ok(())
}

#[gpui::test]
fn global_init_exposes_one_registry(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        DocumentService::init(cx);
        let first = DocumentService::registry(cx);
        let second = DocumentService::registry(cx);
        assert!(Arc::ptr_eq(&first, &second));
    });
}
