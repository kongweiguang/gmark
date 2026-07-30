// @author kongweiguang

use super::*;

fn menu_item_kinds(format: DocumentMenuFormat) -> Vec<FormatMenuItemKind> {
    format_menu_item_states(
        format,
        DocumentMenuCapabilities {
            has_selection: true,
            has_structure: true,
            has_json_selection: true,
            has_filter: true,
            has_columns: true,
            ..DocumentMenuCapabilities::default()
        },
    )
    .into_iter()
    .map(|item| item.kind)
    .collect()
}

#[test]
fn labels_cover_all_supported_document_formats() {
    assert_eq!(DocumentMenuFormat::Markdown.label(true), "Markdown");
    assert_eq!(DocumentMenuFormat::Json.label(false), "JSON");
    assert_eq!(DocumentMenuFormat::JsonLines.label(false), "JSONL");
    assert_eq!(DocumentMenuFormat::Csv.label(false), "CSV");
    assert_eq!(DocumentMenuFormat::Tsv.label(false), "TSV");
    assert_eq!(DocumentMenuFormat::Text.label(true), "文本");
}

#[test]
fn probe_formats_keep_jsonl_and_tab_delimited_documents_distinct() {
    use gmark_document_core::DocumentFormat;

    assert_eq!(
        DocumentMenuFormat::from_document_format(&DocumentFormat::Json),
        DocumentMenuFormat::Json
    );
    assert_eq!(
        DocumentMenuFormat::from_document_format(&DocumentFormat::JsonLines),
        DocumentMenuFormat::JsonLines
    );
    assert_eq!(
        DocumentMenuFormat::from_document_format(&DocumentFormat::Delimited { delimiter: b',' }),
        DocumentMenuFormat::Csv
    );
    assert_eq!(
        DocumentMenuFormat::from_document_format(&DocumentFormat::Delimited { delimiter: b'\t' }),
        DocumentMenuFormat::Tsv
    );
}

#[test]
fn menu_matrix_keeps_json_lines_and_delimited_tools_distinct() {
    assert_eq!(
        menu_item_kinds(DocumentMenuFormat::JsonLines),
        vec![
            FormatMenuItemKind::Records,
            FormatMenuItemKind::Filter,
            FormatMenuItemKind::DocumentInfo,
            FormatMenuItemKind::ExportSelection,
        ]
    );
    assert_eq!(
        menu_item_kinds(DocumentMenuFormat::Csv),
        vec![
            FormatMenuItemKind::Table,
            FormatMenuItemKind::Filter,
            FormatMenuItemKind::Columns,
            FormatMenuItemKind::DocumentInfo,
            FormatMenuItemKind::ExportSelection,
        ]
    );
    assert_eq!(
        menu_item_kinds(DocumentMenuFormat::Tsv),
        menu_item_kinds(DocumentMenuFormat::Csv)
    );
}

#[test]
fn menu_matrix_covers_all_formats_and_disabled_reasons() {
    let empty = DocumentMenuCapabilities::default();
    assert_eq!(
        menu_item_kinds(DocumentMenuFormat::Markdown),
        vec![
            FormatMenuItemKind::InsertResource,
            FormatMenuItemKind::Outline,
            FormatMenuItemKind::DocumentInfo,
            FormatMenuItemKind::ExportHtml,
            FormatMenuItemKind::ExportImage,
            FormatMenuItemKind::ExportPdf,
            FormatMenuItemKind::ExportSelection,
        ]
    );
    assert_eq!(
        menu_item_kinds(DocumentMenuFormat::Json),
        vec![
            FormatMenuItemKind::Structure,
            FormatMenuItemKind::Inspector,
            FormatMenuItemKind::DocumentInfo,
            FormatMenuItemKind::ExportSelection,
        ]
    );
    assert_eq!(
        menu_item_kinds(DocumentMenuFormat::Text),
        vec![
            FormatMenuItemKind::DocumentInfo,
            FormatMenuItemKind::ExportSelection
        ]
    );
    let json_states = format_menu_item_states(DocumentMenuFormat::Json, empty);
    assert_eq!(
        json_states[0].disabled_reason,
        Some(FormatMenuDisabledReason::ProjectionUnavailable)
    );
    assert_eq!(
        json_states[1].disabled_reason,
        Some(FormatMenuDisabledReason::ProjectionUnavailable)
    );
    assert_eq!(
        json_states[3].disabled_reason,
        Some(FormatMenuDisabledReason::NoSelection)
    );
    let paged_csv = format_menu_item_states(
        DocumentMenuFormat::Csv,
        DocumentMenuCapabilities {
            paged: true,
            ..empty
        },
    );
    assert_eq!(
        paged_csv[0].disabled_reason,
        Some(FormatMenuDisabledReason::PagedSource)
    );
    assert_eq!(
        paged_csv[1].disabled_reason,
        Some(FormatMenuDisabledReason::PagedSource)
    );
    assert_eq!(
        paged_csv[2].disabled_reason,
        Some(FormatMenuDisabledReason::PagedSource)
    );
}

#[test]
fn menu_matrix_distinguishes_filter_labels() {
    assert_eq!(
        FormatMenuItemKind::Filter.label(DocumentMenuFormat::JsonLines, false),
        "Filter Records"
    );
    assert_eq!(
        FormatMenuItemKind::Filter.label(DocumentMenuFormat::Csv, false),
        "Filter Rows"
    );
    assert_eq!(
        FormatMenuItemKind::Table.label(DocumentMenuFormat::Tsv, true),
        "表格视图"
    );
}
