// @author kongweiguang

use super::*;

fn profile(format: DocumentFormat) -> DocumentProfile {
    DocumentProfile {
        len: DEFAULT_LOADING_LIMITS.max_resident_bytes,
        format,
        encoding: TextEncoding::Utf8 { bom: false },
        estimated_lines: u64::MAX,
        estimated_structural_units: u64::MAX,
    }
}

#[test]
fn only_byte_overflow_uses_paged_source() {
    let policy = LoadingPolicy::default();
    let exact = profile(DocumentFormat::Json);
    assert_eq!(
        policy.resolve(&exact).backend,
        DocumentBackendKind::Resident
    );

    let plan = policy.resolve(&DocumentProfile {
        len: exact.len + 1,
        ..exact
    });
    assert_eq!(plan.backend, DocumentBackendKind::Paged);
    assert_eq!(plan.initial_view, DocumentViewId::source());
    assert_eq!(plan.allowed_views, vec![ViewDescriptor::source()]);
}

#[test]
fn regular_formats_select_their_own_default_views() {
    for (format, expected) in [
        (DocumentFormat::Markdown, DocumentViewId::markdown_live()),
        (DocumentFormat::Json, DocumentViewId::json_graph()),
        (DocumentFormat::JsonLines, DocumentViewId::json_structure()),
        (
            DocumentFormat::Delimited { delimiter: b',' },
            DocumentViewId::delimited_table(),
        ),
        (DocumentFormat::PlainText, DocumentViewId::source()),
    ] {
        let plan = LoadingPolicy::default().resolve(&profile(format));
        assert_eq!(plan.backend, DocumentBackendKind::Resident);
        assert_eq!(plan.initial_view, expected);
    }
}

#[test]
fn safe_source_overrides_regular_profile_without_changing_limits() {
    let policy = LoadingPolicy {
        force_safe_source: true,
        ..LoadingPolicy::default()
    };
    let plan = OpenPolicyResolver.resolve(
        policy,
        &DocumentProfile {
            len: 1,
            ..profile(DocumentFormat::Markdown)
        },
    );
    assert_eq!(plan.backend, DocumentBackendKind::Paged);
    assert_eq!(plan.reason, OpenReason::ForcedSafeSource);
}
