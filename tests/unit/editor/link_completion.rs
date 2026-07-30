// @author kongweiguang

use std::path::Path;

use super::{
    detect_workspace_link_trigger, rank_workspace_link_candidates, relative_markdown_path,
};

#[test]
fn trigger_rejects_escaped_and_inline_code_markers() {
    assert_eq!(
        detect_workspace_link_trigger("See [[guide", 11),
        Some((4..11, "guide".to_owned()))
    );
    assert!(detect_workspace_link_trigger(r"See \[[guide", 12).is_none());
    assert!(detect_workspace_link_trigger("`[[guide`", 8).is_none());
    assert!(detect_workspace_link_trigger("[[guide\n", 8).is_none());
}

#[test]
fn relative_paths_use_forward_slashes_and_parent_segments() {
    let from = Path::new(r"C:\workspace\notes\daily");
    let target = Path::new(r"C:\workspace\guides\Start Here.md");
    assert_eq!(
        relative_markdown_path(from, target).as_deref(),
        Some("../../guides/Start Here.md")
    );
}

#[test]
fn ranking_prefers_stem_prefix_then_stem_fuzzy_then_path_fuzzy() {
    let root = Path::new(r"C:\workspace");
    let current = root.join("current.md");
    let candidates = rank_workspace_link_candidates(
        root,
        vec![
            root.join("other").join("query-path.md"),
            root.join("Quick Guide.md"),
            root.join("q-u-e-r-y.md"),
            current.clone(),
        ],
        &current,
        "qu",
    );
    assert_eq!(candidates[0].title, "Quick Guide");
    assert!(candidates.iter().all(|candidate| candidate.path != current));
}

#[test]
fn duplicate_titles_request_relative_path_disambiguation() {
    let root = Path::new(r"C:\workspace");
    let candidates = rank_workspace_link_candidates(
        root,
        vec![
            root.join("a").join("Guide.md"),
            root.join("b").join("Guide.md"),
        ],
        &root.join("current.md"),
        "guide",
    );
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().all(|candidate| candidate.disambiguate));
}
