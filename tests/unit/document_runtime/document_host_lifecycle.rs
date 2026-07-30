// @author kongweiguang

//! DocumentHost opening, backend selection, and recovery lifecycle tests.

use super::*;

#[test]
fn source_font_uses_a_real_direct_write_family_on_windows() {
    #[cfg(target_os = "windows")]
    assert_eq!(source_monospace_font_family(), "Consolas");
    #[cfg(target_os = "macos")]
    assert_eq!(source_monospace_font_family(), "Menlo");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    assert_eq!(source_monospace_font_family(), "monospace");
}

#[gpui::test]
async fn untitled_json_and_csv_install_resident_format_capabilities(cx: &mut gpui::TestAppContext) {
    let json = cx.new(|cx| {
        DocumentHost::new_untitled(
            PathBuf::from("Untitled.json"),
            DocumentFormat::Json,
            "{\n  \"name\": \"x\"\n}\n",
            cx,
        )
    });
    json.update(cx, |host, _cx| {
        assert!(host.is_json_document());
        assert!(host.has_registered_structure_view());
        assert!(host.source_view_for_test());
        assert_eq!(host.source_text_for_test(), "{\n  \"name\": \"x\"\n}\n");
        assert!(host.provisional_source.is_none());
        let snapshot = host.document_sidebar_snapshot();
        assert_eq!(snapshot.format, DocumentMenuFormat::Json);
        assert_eq!(snapshot.metadata.encoding, "UTF-8");
        assert!(snapshot.metadata.lines >= 1);
        assert!(snapshot.document_epoch > 0);
    });

    let csv = cx.new(|cx| {
        DocumentHost::new_untitled(
            PathBuf::from("Untitled.csv"),
            DocumentFormat::Delimited { delimiter: b',' },
            "Column 1,Column 2\n",
            cx,
        )
    });
    csv.update(cx, |host, _cx| {
        assert!(host.is_delimited_document());
        assert!(host.has_registered_structure_view());
        assert!(host.source_view_for_test());
        assert_eq!(host.source_text_for_test(), "Column 1,Column 2\n");
        assert!(matches!(
            host.structured_index,
            Some(StructuredIndex::Delimited(_))
        ));
        assert!(host.provisional_source.is_none());
        let snapshot = host.document_sidebar_snapshot();
        assert_eq!(snapshot.format, DocumentMenuFormat::Csv);
        assert_eq!(snapshot.metadata.length, "Column 1,Column 2\n".len() as u64);
        assert_eq!(snapshot.nodes.len(), 2);
        assert!(matches!(
            snapshot.nodes[0].target,
            DocumentSidebarTarget::Column { column: 0 }
        ));
    });
}

#[test]
fn stale_resident_probe_is_rejected_and_fresh_probe_replans_to_paged() {
    let temp = tempfile::tempdir().expect("probe race tempdir");
    let path = temp.path().join("probe-race.csv");
    fs::write(&path, "a,b\n").expect("small probe fixture");
    let options = gmark_paged_document::ProbeOptions {
        max_resident_bytes: 8,
        ..gmark_paged_document::ProbeOptions::default()
    };
    let stale_probe = gmark_paged_document::probe_file(&path, options).expect("resident probe");
    assert_eq!(stale_probe.strategy, OpenStrategy::Resident);

    fs::write(&path, "a,b\n1,2\n3,4\n").expect("grow fixture after probe");
    let changed_source = FileSource::open(&path).expect("changed source");
    let changed_index = LineIndex::build(&changed_source).expect("changed index");
    assert!(matches!(
        build_document_session(
            &stale_probe,
            &changed_source,
            changed_source.clone(),
            changed_index,
            false,
        ),
        Err(PagedDocumentError::SourceChanged)
    ));

    let fresh_probe = gmark_paged_document::probe_file(&path, options).expect("fresh probe");
    assert_eq!(fresh_probe.strategy, OpenStrategy::Paged);
    let fresh_source = FileSource::open(&path).expect("fresh source");
    let fresh_index = LineIndex::build(&fresh_source).expect("fresh index");
    let session = build_document_session(
        &fresh_probe,
        &fresh_source,
        fresh_source.clone(),
        fresh_index,
        false,
    )
    .expect("fresh paged session");
    assert_eq!(
        session.store.kind(),
        gmark_document_core::DocumentBackendKind::Paged
    );
}

#[test]
fn same_length_replacement_invalidates_a_stale_probe() {
    let temp = tempfile::tempdir().expect("same-length probe race tempdir");
    let path = temp.path().join("same-length.json");
    fs::write(&path, br#"{"a":1}"#).expect("initial same-length fixture");
    let options = gmark_paged_document::ProbeOptions::default();
    let stale_probe = gmark_paged_document::probe_file(&path, options).expect("initial probe");
    let stale_source = FileSource::open(&path).expect("initial stable source");

    let replacement = temp.path().join("same-length.replacement");
    fs::write(&replacement, br#"{"b":2}"#).expect("replacement fixture");
    fs::remove_file(&path).expect("remove original fixture");
    fs::rename(&replacement, &path).expect("install same-length replacement");
    assert_eq!(
        fs::metadata(&path).expect("replacement metadata").len(),
        stale_probe.len
    );

    let replacement_source = FileSource::open(&path).expect("replacement source");
    let replacement_index = LineIndex::build(&replacement_source).expect("replacement index");
    assert_ne!(
        replacement_source.identity().expect("replacement identity"),
        stale_probe.identity,
        "identity must detect a replacement even when byte length is unchanged"
    );
    assert!(matches!(
        build_document_session(
            &stale_probe,
            &stale_source,
            replacement_source,
            replacement_index,
            false,
        ),
        Err(PagedDocumentError::SourceChanged)
    ));
}

#[test]
fn resident_host_session_keeps_probe_limits_and_only_marks_runtime_growth() {
    let temp = tempfile::tempdir().expect("resident growth tempdir");
    let path = temp.path().join("growth.csv");
    fs::write(&path, "a,b\n").expect("resident growth fixture");
    let options = gmark_paged_document::ProbeOptions {
        max_resident_bytes: 5,
        ..gmark_paged_document::ProbeOptions::default()
    };
    let probe = gmark_paged_document::probe_file(&path, options).expect("resident growth probe");
    assert_eq!(probe.strategy, OpenStrategy::Resident);
    let source = FileSource::open(&path).expect("resident growth source");
    let index = LineIndex::build(&source).expect("resident growth index");
    let mut session = build_document_session(&probe, &source, source.clone(), index, false)
        .expect("resident growth session");

    assert_eq!(session.loading_limits.max_resident_bytes, 5);
    session
        .replace_text(4..4, "12")
        .expect("grow resident source");
    assert_eq!(
        session.resident_growth_reason(),
        Some(gmark_document_core::OpenReason::ByteLimitExceeded)
    );
    assert_eq!(
        session.store.kind(),
        gmark_document_core::DocumentBackendKind::Resident
    );
}

#[test]
fn resident_recovery_contract_records_the_resulting_undo_snapshot() {
    let temp = tempfile::tempdir().expect("resident recovery tempdir");
    let path = temp.path().join("recovery.csv");
    fs::write(&path, "one").expect("resident recovery fixture");
    let source = FileSource::open(&path).expect("resident recovery source");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("resident recovery probe");
    assert_eq!(probe.strategy, OpenStrategy::Resident);
    let index = LineIndex::build(&source).expect("resident recovery index");
    let mut document = build_document_session(&probe, &source, source.clone(), index, false)
        .expect("resident recovery session");
    let mut journal =
        DocumentRecoveryJournal::create(temp.path(), &source, probe.encoding.clone(), &document)
            .expect("resident recovery journal");
    assert!(matches!(&journal, DocumentRecoveryJournal::Resident(_)));

    document
        .replace_text(0..3, "two")
        .expect("replace resident source");
    journal
        .record_after_change(
            &document,
            &RecoveryRecord {
                action: RecoveryAction::Transaction(Transaction::new(
                    gmark_document_core::DocumentRevision(0),
                    vec![SourceEdit::new(0..3, "two")],
                )),
                selection: Some(SourceSelection::collapsed(3, SourceAffinity::After)),
                view_id: DocumentViewId::source(),
            },
        )
        .expect("record resident replacement");
    assert!(document.undo(), "resident edit must be undoable");
    journal
        .record_after_change(
            &document,
            &RecoveryRecord {
                action: RecoveryAction::Undo,
                selection: Some(SourceSelection::collapsed(0, SourceAffinity::Before)),
                view_id: DocumentViewId::source(),
            },
        )
        .expect("record resident undo snapshot");

    let recovered = gmark_document_runtime::replay_resident_recovery_journal(journal.path())
        .expect("replay resident runtime journal");
    assert_eq!(recovered.source, "one");
    assert_eq!(
        recovered.selection,
        SourceSelection::collapsed(0, SourceAffinity::Before)
    );
}

#[test]
fn paged_recovery_contract_keeps_the_large_journal_path() {
    let temp = tempfile::tempdir().expect("paged recovery tempdir");
    let path = temp.path().join("paged.txt");
    fs::write(&path, "paged source").expect("paged recovery fixture");
    let source = FileSource::open(&path).expect("paged recovery source");
    let mut probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("paged recovery probe");
    probe.strategy = OpenStrategy::Paged;
    let identity = source.identity().expect("paged recovery identity");
    let index = LineIndex::build(&source).expect("paged recovery index");
    let piece = PieceDocument::open(source.clone(), index).expect("paged recovery document");
    let document = build_paged_session(&probe, piece, identity).expect("paged recovery session");
    let journal =
        DocumentRecoveryJournal::create(temp.path(), &source, probe.encoding.clone(), &document)
            .expect("paged recovery journal");

    assert!(matches!(&journal, DocumentRecoveryJournal::Paged(_)));
    assert_eq!(
        journal
            .path()
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("large-journal")
    );
}
