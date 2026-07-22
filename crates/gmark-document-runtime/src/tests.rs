// @author kongweiguang

use std::fs;

use gmark_document_core::{
    DocumentFormat, LoadingPolicy, SourceAffinity, SourceAnchor, SourceEdit, TextEncoding,
};
use gmark_paged_document::{FileSource, LineIndex, PieceDocument};

use super::*;

fn profile(format: DocumentFormat, len: u64) -> DocumentProfile {
    DocumentProfile {
        len,
        format,
        encoding: TextEncoding::Utf8 { bom: false },
        estimated_lines: 1,
        estimated_structural_units: 0,
    }
}

fn identity(path: PathBuf, len: u64) -> FileIdentity {
    FileIdentity {
        canonical_path: path,
        len,
        modified_nanos: None,
        platform_id: None,
    }
}

#[test]
fn resident_session_owns_revision_dirty_selection_and_allowed_views() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resident.json");
    fs::write(&path, "{\"a\":1}").unwrap();
    let source_identity = FileSource::open(&path).unwrap().identity().unwrap();
    let profile = profile(DocumentFormat::Json, 7);
    let plan = LoadingPolicy::default().resolve(&profile);
    let mut session = DocumentSession::new(
        profile,
        DocumentStore::Resident(Box::new(ResidentDocument::new(
            "{\"a\":1}",
            TextEncoding::Utf8 { bom: false },
            source_identity,
        ))),
        plan,
        identity(path, 7),
    )
    .unwrap();
    let transaction = Transaction::new(DocumentRevision(0), vec![SourceEdit::new(5..6, "2")]);
    let selection = SourceSelection {
        anchor: SourceAnchor::new(6, SourceAffinity::After),
        head: SourceAnchor::new(6, SourceAffinity::After),
    };

    assert_eq!(
        session.apply_transaction(&transaction, selection).unwrap(),
        DocumentRevision(1)
    );
    assert!(session.dirty);
    assert_eq!(session.view_state.source.selection, selection);
    assert_eq!(session.snapshot().read_range(0..7).unwrap(), b"{\"a\":2}");
    assert!(
        session
            .allowed_views()
            .contains(&DocumentViewId::json_graph())
    );
}

#[test]
fn resident_growth_over_frozen_limit_warns_without_hot_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("growth.txt");
    fs::write(&path, "1234567").unwrap();
    let source_identity = FileSource::open(&path).unwrap().identity().unwrap();
    let profile = profile(DocumentFormat::PlainText, 7);
    let policy = LoadingPolicy {
        max_resident_bytes: Some(7),
        max_resident_lines: Some(u64::MAX),
        max_structural_units: Some(u64::MAX),
        ..LoadingPolicy::default()
    };
    let plan = policy.resolve(&profile);
    let mut session = DocumentSession::new(
        profile,
        DocumentStore::Resident(Box::new(ResidentDocument::new(
            "1234567",
            TextEncoding::Utf8 { bom: false },
            source_identity,
        ))),
        plan,
        identity(path, 7),
    )
    .unwrap();

    assert_eq!(session.resident_growth_reason(), None);
    session.replace_text(7..7, "8").unwrap();
    assert_eq!(
        session.resident_growth_reason(),
        Some(OpenReason::ByteLimitExceeded)
    );
    assert_eq!(session.store.kind(), DocumentBackendKind::Resident);

    // 越界提示属于本次 Resident 会话的稳定事实；撤销不会偷偷迁移或抹掉提示。
    assert!(session.undo());
    assert_eq!(session.len(), 7);
    assert_eq!(
        session.resident_growth_reason(),
        Some(OpenReason::ByteLimitExceeded)
    );
    assert_eq!(session.loading_limits, policy.effective_limits());
}

#[test]
fn paged_plan_rejects_derived_views_and_stale_transactions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paged.csv");
    fs::write(&path, "a,b\n1,2\n").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let document = PagedDocument::new(PieceDocument::open(source, index).unwrap());
    let profile = profile(DocumentFormat::Delimited { delimiter: b',' }, 10);
    let plan = LoadingPolicy {
        force_safe_source: true,
        ..LoadingPolicy::default()
    }
    .resolve(&profile);
    let mut session = DocumentSession::new(
        profile,
        DocumentStore::Paged(Box::new(document)),
        plan,
        identity(path, 10),
    )
    .unwrap();

    assert_eq!(session.allowed_views(), &[DocumentViewId::source()]);
    assert!(
        session
            .set_active_view(DocumentViewId::delimited_table())
            .is_err()
    );
    let stale = Transaction::new(DocumentRevision(9), vec![SourceEdit::new(0..1, "x")]);
    assert!(matches!(
        session.apply_transaction(&stale, SourceSelection::default()),
        Err(SessionEditError::Edit(EditError::StaleRevision { .. }))
    ));
}
