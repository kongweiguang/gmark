// @author kongweiguang

use super::*;

/// 以字节而非字符限制查找输入，避免多字节文本绕过查询预算。
#[::core::prelude::v1::test]
fn find_query_limit_rejects_only_bytes_above_four_kib() {
    let within = "x".repeat(MAX_FIND_QUERY_BYTES);
    let accepted = find_matches("source", &within, FindOptions::default(), Revision::INITIAL);
    assert!(accepted.error.is_none());

    let over = "x".repeat(MAX_FIND_QUERY_BYTES + 1);
    let rejected = find_matches("source", &over, FindOptions::default(), Revision::INITIAL);
    assert!(rejected.matches.is_empty());
    assert!(
        rejected
            .error
            .as_deref()
            .is_some_and(|error| error.contains("4 KiB"))
    );
}

/// 搜索结果维持既有 20,000 条上限，避免极端重复文本把 UI 结果列表扩成无界内存。
#[::core::prelude::v1::test]
fn find_matches_remain_bounded_at_twenty_thousand() {
    let source = "x".repeat(MAX_FIND_MATCHES + 1);
    let result = find_matches(&source, "x", FindOptions::default(), Revision::INITIAL);
    assert_eq!(result.matches.len(), MAX_FIND_MATCHES);
    assert!(result.truncated);
}

/// 高匹配数替换在后台仍只返回一个全源码 edit，避免 20,000 个编辑逐个进入 UI 提交路径。
#[::core::prelude::v1::test]
fn replace_plan_collapses_many_matches_to_one_source_edit() {
    let source = "x".repeat(MAX_FIND_MATCHES);
    let matches = (0..MAX_FIND_MATCHES)
        .map(|index| {
            let end = index.checked_add(1).expect("bounded match end");
            index..end
        })
        .collect::<Vec<_>>();
    let metadata = matches
        .iter()
        .cloned()
        .map(|range| FindMatchMetadata {
            visible: range.clone(),
            source: Some(range),
            replaceability: Replaceability::Direct,
        })
        .collect::<Vec<_>>();

    let plan = build_replace_all_plan(
        &source,
        "x",
        "y",
        FindOptions::default(),
        &matches,
        &metadata,
    )
    .expect("bounded replacement plan");
    assert_eq!(plan.edits.len(), 1);
    assert_eq!(plan.edits[0].range(), &(0..source.len()));
    assert_eq!(plan.edits[0].replacement().len(), source.len());
    assert_eq!(plan.selection, 0..1);
}

/// 验证恰好 64 MiB 可提交，而再增加一个字节必须在事务构造前失败。
#[::core::prelude::v1::test]
fn replace_output_boundary_is_checked_without_wrapping() {
    let range = 0..1;
    assert_eq!(
        checked_replace_output_len(1, &range, MAX_REPLACE_OUTPUT_BYTES).unwrap(),
        MAX_REPLACE_OUTPUT_BYTES
    );
    assert!(checked_replace_output_len(1, &range, MAX_REPLACE_OUTPUT_BYTES + 1).is_err());
}

/// 重叠编辑必须整体拒绝，防止事务构造阶段留下不可提交的部分计划。
#[::core::prelude::v1::test]
fn replace_plan_rejects_overlapping_ranges_before_returning_edits() {
    let metadata = [
        FindMatchMetadata {
            visible: 0..1,
            source: Some(0..1),
            replaceability: Replaceability::Direct,
        },
        FindMatchMetadata {
            visible: 0..1,
            source: Some(0..1),
            replaceability: Replaceability::Direct,
        },
    ];
    let result = build_replace_all_plan(
        "aa",
        "a",
        "b",
        FindOptions::default(),
        &[0..1, 0..1],
        &metadata,
    );
    assert!(result.is_err());
}

/// 单次替换的后台结果必须同时匹配 revision 与 generation，避免旧查询覆盖新输入。
#[::core::prelude::v1::test]
fn replace_result_gate_rejects_stale_revision_or_generation() {
    let revision = Revision::INITIAL;
    assert!(find_replace_result_is_current(revision, 7, revision, 7));
    assert!(!find_replace_result_is_current(revision, 7, revision, 8));
    assert!(!find_replace_result_is_current(
        revision,
        7,
        Revision::from_u64(1),
        7
    ));
}
