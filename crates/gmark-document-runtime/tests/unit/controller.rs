// @author kongweiguang

use std::fs;

use gmark_document_core::{
    DocumentFormat, DocumentProfile, LoadingPolicy, SourceEdit, TextEncoding,
};
use gmark_paged_document::FileSource;

use super::*;
use crate::{DocumentStore, ResidentDocument};

fn session() -> DocumentSession {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let path = directory.path().join("controller.txt");
    fs::write(&path, "one").unwrap_or_else(|error| panic!("fixture write: {error}"));
    let source_identity = FileSource::open(&path)
        .and_then(|source| source.identity())
        .unwrap_or_else(|error| panic!("source identity: {error}"));
    let identity = FileIdentity::from(&source_identity);
    let profile = DocumentProfile {
        len: 3,
        format: DocumentFormat::PlainText,
        encoding: TextEncoding::Utf8 { bom: false },
        estimated_lines: 1,
        estimated_structural_units: 0,
    };
    DocumentSession::new(
        profile.clone(),
        DocumentStore::Resident(Box::new(ResidentDocument::new(
            "one",
            profile.encoding.clone(),
            source_identity,
        ))),
        LoadingPolicy::default().resolve(&profile),
        identity,
    )
    .unwrap_or_else(|error| panic!("session: {error}"))
}

#[test]
fn saves_coalesce_and_old_save_does_not_clear_newer_edits() {
    let mut controller = DocumentController::new(DocumentId::from_raw(1), session());
    controller
        .dispatch(DocumentCommand::ApplyTransaction {
            transaction_id: TransactionId(1),
            transaction: Transaction::new(DocumentRevision(0), vec![SourceEdit::new(0..1, "t")]),
        })
        .unwrap_or_else(|error| panic!("first edit: {error}"));
    controller
        .dispatch(DocumentCommand::RequestSave)
        .unwrap_or_else(|error| panic!("first save request: {error}"));
    controller
        .dispatch(DocumentCommand::ApplyTransaction {
            transaction_id: TransactionId(2),
            transaction: Transaction::new(DocumentRevision(1), vec![SourceEdit::new(1..2, "w")]),
        })
        .unwrap_or_else(|error| panic!("second edit: {error}"));
    controller
        .dispatch(DocumentCommand::RequestSave)
        .unwrap_or_else(|error| panic!("second save request: {error}"));
    let identity = controller.session().file_identity.clone();
    controller
        .dispatch(DocumentCommand::SaveSucceeded {
            revision: DocumentRevision(1),
            identity,
        })
        .unwrap_or_else(|error| panic!("old save completion: {error}"));

    assert!(controller.session().is_dirty());
    assert!(controller.drain_events().any(|event| {
        matches!(
            event,
            DocumentEvent::SaveRequested {
                revision: DocumentRevision(2),
                ..
            }
        )
    }));
}

#[test]
fn registry_returns_the_same_handle_for_the_same_path() {
    let registry = DocumentRegistry::default();
    let identity = session().file_identity.clone();
    let key = DocumentRegistryKey::for_file(&identity);
    let (first, state) = registry
        .open_or_insert(key.clone(), || {
            Ok(DocumentController::new(DocumentId::from_raw(1), session()))
        })
        .unwrap_or_else(|error| panic!("first open: {error}"));
    assert_eq!(state, RegistryOpen::Inserted);
    let (second, state) = registry
        .open_or_insert(key, || {
            Ok(DocumentController::new(DocumentId::from_raw(2), session()))
        })
        .unwrap_or_else(|error| panic!("second open: {error}"));
    assert_eq!(state, RegistryOpen::Existing);
    assert!(Arc::ptr_eq(&first.0, &second.0));
}
