// @author kongweiguang

use std::fs;

use gmark_document_core::{
    DocumentFormat, DocumentProfile, DocumentRevision, LoadingPolicy, SourceEdit, TextEncoding,
    Transaction,
};
use gmark_document_runtime::{
    DocumentCommand, DocumentController, DocumentId, DocumentStore, DocumentViewInstanceId,
    FileIdentity, ResidentDocument, TransactionId,
};
use gmark_paged_document::{
    ExternalChange, FileSource, LineIndex, SearchCancellation, prepare_utf8_source,
};

fn resident_session(
    path: &std::path::Path,
    text: &str,
    encoding: TextEncoding,
) -> gmark_document_runtime::DocumentSession {
    let source = FileSource::open(path).expect("open resident fixture");
    let identity = source.identity().expect("resident identity");
    let profile = DocumentProfile {
        len: text.len() as u64,
        format: DocumentFormat::PlainText,
        encoding: encoding.clone(),
        estimated_lines: 1,
        estimated_structural_units: 0,
    };
    gmark_document_runtime::DocumentSession::new(
        profile.clone(),
        DocumentStore::Resident(Box::new(ResidentDocument::new(
            text,
            profile.encoding.clone(),
            identity.clone(),
        ))),
        LoadingPolicy::default().resolve(&profile),
        FileIdentity::from(&identity),
    )
    .expect("resident session")
}

fn paged_session(path: &std::path::Path, text: &str) -> gmark_document_runtime::DocumentSession {
    let source = FileSource::open(path).expect("open paged fixture");
    let identity = source.identity().expect("paged identity");
    let index = LineIndex::build(&source).expect("paged index");
    let document = gmark_paged_document::PagedDocument::new(
        gmark_paged_document::PieceDocument::open(source, index).expect("paged document"),
    );
    let profile = DocumentProfile {
        len: text.len() as u64,
        format: DocumentFormat::PlainText,
        encoding: TextEncoding::Utf8 { bom: false },
        estimated_lines: 1,
        estimated_structural_units: 0,
    };
    gmark_document_runtime::DocumentSession::new(
        profile.clone(),
        DocumentStore::Paged(Box::new(document)),
        LoadingPolicy {
            force_safe_source: true,
            ..LoadingPolicy::default()
        }
        .resolve(&profile),
        FileIdentity::from(&identity),
    )
    .expect("paged session")
}

#[test]
fn resident_snapshot_stream_preserves_mixed_endings_and_utf16_bom() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("mixed.txt");
    fs::write(&path, b"a\r\nb\nc").expect("write fixture");
    let session = resident_session(&path, "a\r\nb\nc", TextEncoding::Utf16Le);
    let snapshot = DocumentController::new(DocumentId::new(), session).save_snapshot();
    let cancellation = SearchCancellation::default();
    snapshot
        .save_atomic_cancellable(&path, &cancellation)
        .expect("stream resident UTF16");
    assert_eq!(
        fs::read(&path).expect("read UTF16 output"),
        [
            0xff, 0xfe, b'a', 0, b'\r', 0, b'\n', 0, b'b', 0, b'\n', 0, b'c', 0
        ]
    );
}

#[test]
fn resident_snapshot_stream_honors_utf8_bom_metadata() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("utf8-bom.txt");
    fs::write(&path, b"a\r\n").expect("write fixture");
    let session = resident_session(&path, "a\r\n", TextEncoding::Utf8 { bom: true });
    let snapshot = DocumentController::new(DocumentId::new(), session).save_snapshot();
    snapshot
        .save_atomic_cancellable(&path, &SearchCancellation::default())
        .expect("stream UTF8 BOM");
    assert_eq!(
        fs::read(&path).expect("read UTF8 output"),
        [0xef, 0xbb, 0xbf, b'a', b'\r', b'\n']
    );
}

#[test]
fn resident_snapshot_stream_supports_legacy_and_save_as() {
    let directory = tempfile::tempdir().expect("tempdir");
    let source_path = directory.path().join("legacy.txt");
    let target_path = directory.path().join("legacy-copy.txt");
    fs::write(&source_path, "café\r\n".as_bytes()).expect("write fixture");
    let session = resident_session(
        &source_path,
        "café\r\n",
        TextEncoding::Legacy("windows-1252".to_owned()),
    );
    let snapshot = DocumentController::new(DocumentId::new(), session).save_snapshot();
    snapshot
        .save_as_atomic_cancellable(&target_path, &SearchCancellation::default())
        .expect("stream legacy Save As");
    assert_eq!(
        fs::read(&target_path).expect("read legacy output"),
        [0x63, 0x61, 0x66, 0xe9, 0x0d, 0x0a]
    );

    fs::write(&target_path, b"keep").expect("reset Save As target");
    let cancellation = SearchCancellation::default();
    cancellation.cancel();
    assert!(
        snapshot
            .save_as_atomic_cancellable(&target_path, &cancellation)
            .is_err()
    );
    assert_eq!(
        fs::read(target_path).expect("read cancelled target"),
        b"keep"
    );
}

#[test]
fn paged_snapshot_stream_uses_shadow_plan_without_materializing_current_session() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("paged-utf16.txt");
    let text = "αbeta\n".repeat(1_500_000);
    let mut bytes = vec![0xff, 0xfe];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&path, &bytes).expect("write UTF16 fixture");
    let source = FileSource::open(&path).expect("open paged fixture");
    let prepared = prepare_utf8_source(source, TextEncoding::Utf16Le).expect("prepare shadow");
    let shadow = prepared.source().clone();
    let index = LineIndex::build(&shadow).expect("shadow index");
    let document = gmark_paged_document::PagedDocument::from_prepared(prepared, index)
        .expect("paged document");
    let profile = DocumentProfile {
        len: bytes.len() as u64,
        format: DocumentFormat::PlainText,
        encoding: TextEncoding::Utf16Le,
        estimated_lines: 1,
        estimated_structural_units: 0,
    };
    let identity = FileSource::open(&path)
        .expect("reopen fixture")
        .identity()
        .expect("fixture identity");
    let session = gmark_document_runtime::DocumentSession::new(
        profile,
        DocumentStore::Paged(Box::new(document)),
        LoadingPolicy {
            force_safe_source: true,
            ..LoadingPolicy::default()
        }
        .resolve(&DocumentProfile {
            len: bytes.len() as u64,
            format: DocumentFormat::PlainText,
            encoding: TextEncoding::Utf16Le,
            estimated_lines: 1,
            estimated_structural_units: 0,
        }),
        FileIdentity::from(&identity),
    )
    .expect("paged session");
    let mut controller = DocumentController::new(DocumentId::new(), session);
    let snapshot = controller
        .request_save_snapshot()
        .expect("request paged save")
        .expect("paged save request");
    assert!(snapshot.paged_save_plan.is_some());
    snapshot
        .save_atomic_cancellable(&path, &SearchCancellation::default())
        .expect("stream paged UTF16");
    assert_eq!(fs::read(&path).expect("read paged output"), bytes);

    let target = directory.path().join("paged-copy.txt");
    let promoted = controller
        .complete_save(
            snapshot.revision,
            snapshot
                .save_as_atomic_cancellable(&target, &SearchCancellation::default())
                .expect("stream paged Save As"),
        )
        .expect("complete paged Save As");
    assert!(promoted.is_none());
    let target_identity = FileSource::open(&target)
        .expect("open paged target")
        .identity()
        .expect("target identity");
    let retained_plan = controller
        .save_snapshot()
        .paged_save_plan
        .expect("retained plan");
    assert_eq!(retained_plan.original_identity(), &target_identity);
}

#[test]
fn old_revision_snapshot_writes_old_body_and_completion_keeps_newer_dirty() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("race.txt");
    fs::write(&path, b"one").expect("write fixture");
    let handle = gmark_document_runtime::DocumentHandle::new(DocumentController::new(
        DocumentId::new(),
        resident_session(&path, "one", TextEncoding::Utf8 { bom: false }),
    ));
    let view = DocumentViewInstanceId::new();
    handle
        .lock()
        .expect("lock edit")
        .dispatch(DocumentCommand::ApplyTransaction {
            view_id: view,
            transaction_id: gmark_document_runtime::TransactionId(1),
            transaction: Transaction::new(DocumentRevision(0), vec![SourceEdit::new(0..1, "t")]),
            selection_before: Default::default(),
            selection_after: Default::default(),
        })
        .expect("first edit");
    let first = handle
        .request_save_snapshot()
        .expect("request first")
        .expect("first in flight");
    handle
        .lock()
        .expect("lock second edit")
        .dispatch(DocumentCommand::ApplyTransaction {
            view_id: view,
            transaction_id: gmark_document_runtime::TransactionId(2),
            transaction: Transaction::new(DocumentRevision(1), vec![SourceEdit::new(1..2, "w")]),
            selection_before: Default::default(),
            selection_after: Default::default(),
        })
        .expect("second edit");
    assert_eq!(
        handle.save_in_flight_revision().expect("in flight status"),
        Some(DocumentRevision(1))
    );
    let identity = first
        .save_atomic_cancellable(&path, &SearchCancellation::default())
        .expect("write old snapshot");
    assert_eq!(fs::read(&path).expect("read old output"), b"tne");
    handle
        .complete_save(DocumentRevision(1), identity)
        .expect("complete old snapshot");
    assert!(handle.lock().expect("lock dirty").session().dirty);
}

#[test]
fn paged_save_completion_acknowledges_current_tree_but_not_stale_edits() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("paged-save.txt");
    let target = directory.path().join("paged-save-target.txt");
    let stale_target = directory.path().join("paged-save-stale.txt");
    let promoted_target = directory.path().join("paged-save-promoted.txt");
    fs::write(&path, b"one").expect("write fixture");
    let handle = gmark_document_runtime::DocumentHandle::new(DocumentController::new(
        DocumentId::new(),
        paged_session(&path, "one"),
    ));
    let view = DocumentViewInstanceId::new();

    handle
        .lock()
        .expect("lock first edit")
        .dispatch(DocumentCommand::ApplyTransaction {
            view_id: view,
            transaction_id: TransactionId(1),
            transaction: Transaction::new(DocumentRevision(0), vec![SourceEdit::new(0..3, "two")]),
            selection_before: Default::default(),
            selection_after: Default::default(),
        })
        .expect("first edit");
    let first = handle
        .request_save_snapshot()
        .expect("request first paged save")
        .expect("first paged save");
    let first_identity = first
        .save_as_atomic_cancellable(&target, &SearchCancellation::default())
        .expect("write first paged save");
    handle
        .complete_save(first.revision, first_identity)
        .expect("complete first paged save");
    {
        let controller = handle.lock().expect("lock clean paged session");
        assert!(!controller.session().dirty);
        assert!(controller.session().is_pristine());
    }

    handle
        .lock()
        .expect("lock second edit")
        .dispatch(DocumentCommand::ApplyTransaction {
            view_id: view,
            transaction_id: TransactionId(2),
            transaction: Transaction::new(first.revision, vec![SourceEdit::new(0..3, "three")]),
            selection_before: Default::default(),
            selection_after: Default::default(),
        })
        .expect("second edit");
    let stale = handle
        .request_save_snapshot()
        .expect("request stale paged save")
        .expect("stale paged save");
    handle
        .lock()
        .expect("lock newer edit")
        .dispatch(DocumentCommand::ApplyTransaction {
            view_id: view,
            transaction_id: TransactionId(3),
            transaction: Transaction::new(stale.revision, vec![SourceEdit::new(0..5, "four")]),
            selection_before: Default::default(),
            selection_after: Default::default(),
        })
        .expect("newer edit");
    let _ = handle
        .request_save_snapshot()
        .expect("queue newer paged save");

    let stale_identity = stale
        .save_as_atomic_cancellable(&stale_target, &SearchCancellation::default())
        .expect("write stale paged save");
    let promoted = handle
        .complete_save(stale.revision, stale_identity)
        .expect("complete stale paged save")
        .expect("promote newer paged save");
    assert!(handle.lock().expect("lock stale dirty").session().dirty);

    let promoted_identity = promoted
        .save_as_atomic_cancellable(&promoted_target, &SearchCancellation::default())
        .expect("write promoted paged save");
    handle
        .complete_save(promoted.revision, promoted_identity)
        .expect("complete promoted paged save");
    let controller = handle.lock().expect("lock final paged session");
    assert!(!controller.session().dirty);
    assert!(controller.session().is_pristine());
}

#[test]
fn paged_snapshot_replaces_open_source_path_and_clears_dirty() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("paged-same-path.txt");
    fs::write(&path, b"one").expect("write fixture");
    let handle = gmark_document_runtime::DocumentHandle::new(DocumentController::new(
        DocumentId::new(),
        paged_session(&path, "one"),
    ));
    let view = DocumentViewInstanceId::new();

    handle
        .lock()
        .expect("lock edit")
        .dispatch(DocumentCommand::ApplyTransaction {
            view_id: view,
            transaction_id: TransactionId(1),
            transaction: Transaction::new(DocumentRevision(0), vec![SourceEdit::new(0..3, "two")]),
            selection_before: Default::default(),
            selection_after: Default::default(),
        })
        .expect("edit source");
    let snapshot = handle
        .request_save_snapshot()
        .expect("request same-path save")
        .expect("same-path save snapshot");

    // The controller, PieceDocument and immutable save snapshot all retain a
    // live FileSource for the original path.  The replacement must therefore
    // use the source's delete-sharing handle rather than relying on the test
    // to close the document before saving.
    let identity = snapshot
        .save_atomic_cancellable(&path, &SearchCancellation::default())
        .expect("replace source while it remains open");
    handle
        .complete_save(snapshot.revision, identity)
        .expect("complete same-path save");

    assert_eq!(fs::read(&path).expect("read replaced source"), b"two");
    let controller = handle.lock().expect("lock clean session");
    assert!(!controller.session().dirty);
    assert!(controller.session().is_pristine());
    assert_eq!(
        controller
            .session()
            .external_change()
            .expect("check persisted source baseline"),
        ExternalChange::Unchanged
    );
}
