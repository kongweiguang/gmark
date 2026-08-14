// @author kongweiguang

use super::*;

/// 每个测试使用独立目录，避免冲突命名测试把其他运行的资源文件当作既有输入。
fn temporary_resource_directory(label: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "gmark-resource-{label}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&directory).expect("temporary resource directory");
    directory
}

/// 测试结束后删除本测试创建的目录，避免边界文件污染用户工作区。
fn remove_temporary_resource_directory(directory: &std::path::Path) {
    let _ = std::fs::remove_dir_all(directory);
}

/// 资源输入恰好 64 MiB 可继续进入后台流程，超过一字节必须在复制前拒绝。
#[::core::prelude::v1::test]
fn resource_input_limit_is_checked_before_copying() {
    let directory = temporary_resource_directory("input-limit");
    let exact = directory.join("exact.bin");
    let over = directory.join("over.bin");
    std::fs::File::create(&exact)
        .and_then(|file| file.set_len(MAX_RESOURCE_BYTES))
        .expect("exact resource size");
    std::fs::File::create(&over)
        .and_then(|file| file.set_len(MAX_RESOURCE_BYTES + 1))
        .expect("over resource size");

    assert_eq!(
        checked_resource_input_size(&exact).expect("exact boundary"),
        MAX_RESOURCE_BYTES
    );
    assert!(checked_resource_input_size(&over).is_err());
    remove_temporary_resource_directory(&directory);
}

/// 生成结果的精确 64 MiB 边界可接受，超过一字节必须在提交前拒绝。
#[::core::prelude::v1::test]
fn resource_output_limit_is_checked_without_wrapping() {
    assert!(checked_resource_output_size(MAX_RESOURCE_BYTES).is_ok());
    assert!(checked_resource_output_size(MAX_RESOURCE_BYTES + 1).is_err());
}

/// 1,000 个冲突候选必须返回错误而不是继续无界重命名，且不产生第 1,001 个副本。
#[::core::prelude::v1::test]
fn resource_conflict_names_stop_at_one_thousand_attempts() {
    let source_directory = temporary_resource_directory("source");
    let target_directory = temporary_resource_directory("target");
    let source = source_directory.join("clip.bin");
    std::fs::write(&source, b"resource").expect("source resource");
    for index in 0..MAX_RESOURCE_NAME_ATTEMPTS {
        let candidate = bounded_resource_candidate_path(&target_directory, "clip.bin", index);
        std::fs::write(candidate, b"occupied").expect("occupied candidate");
    }

    let result = copy_resource_without_overwrite(&source, &target_directory, "clip.bin");
    assert!(result.is_err());
    assert!(
        !bounded_resource_candidate_path(&target_directory, "clip.bin", MAX_RESOURCE_NAME_ATTEMPTS)
            .exists()
    );
    remove_temporary_resource_directory(&source_directory);
    remove_temporary_resource_directory(&target_directory);
}

/// 后台结果只接受原文档 epoch 与 revision，任一变化都必须丢弃迟到提交。
#[::core::prelude::v1::test]
fn resource_materialization_gate_rejects_stale_epoch_or_revision() {
    let revision = gmark_document::Revision::INITIAL;
    assert!(resource_materialization_is_current(
        3, revision, 3, revision
    ));
    assert!(!resource_materialization_is_current(
        3, revision, 4, revision
    ));
    assert!(!resource_materialization_is_current(
        3,
        revision,
        3,
        gmark_document::Revision::from_u64(1)
    ));
}

/// 取消或目标消失时只删除当前任务标记为 created 的副本，既有文件必须保留。
#[::core::prelude::v1::test]
fn resource_cleanup_guard_only_removes_created_copy() {
    let directory = temporary_resource_directory("cleanup");
    let created = directory.join("created.bin");
    let existing = directory.join("existing.bin");
    std::fs::write(&created, b"new").expect("created copy");
    std::fs::write(&existing, b"old").expect("existing file");

    {
        let _guard = ResourceCleanupGuard::new(crate::resource_io::MaterializedResource {
            path: created.clone(),
            created: true,
        });
    }
    {
        let _guard = ResourceCleanupGuard::new(crate::resource_io::MaterializedResource {
            path: existing.clone(),
            created: false,
        });
    }
    assert!(!created.exists());
    assert_eq!(
        std::fs::read(&existing).expect("existing file remains"),
        b"old"
    );
    remove_temporary_resource_directory(&directory);
}

/// 模拟实体销毁后 WeakEntity 不调用回调，验证 closure 丢弃时仍能回收新副本。
#[::core::prelude::v1::test]
fn resource_cleanup_guard_runs_when_callback_is_not_invoked() {
    let directory = temporary_resource_directory("callback-drop");
    let created = directory.join("created.bin");
    std::fs::write(&created, b"new copy").expect("create copied resource");

    let callback = {
        let guard = ResourceCleanupGuard::new(crate::resource_io::MaterializedResource {
            path: created.clone(),
            created: true,
        });
        move || {
            let _guard = guard;
        }
    };
    drop(callback);

    assert!(!created.exists());
    remove_temporary_resource_directory(&directory);
}

/// materialize 失败必须发生在任何 UI 文档事务前，原始源文件保持不变。
#[::core::prelude::v1::test]
fn oversized_resource_is_rejected_atomically() {
    let directory = temporary_resource_directory("atomic");
    let source = directory.join("oversized.bin");
    std::fs::File::create(&source)
        .and_then(|file| file.set_len(MAX_RESOURCE_BYTES + 1))
        .expect("oversized resource");
    let result = Editor::materialize_resource_with_limits(
        "",
        &source,
        Some(&directory.join("document.md")),
        ResourceInsertBehavior::CopyToAssetsFolder,
        None,
    );
    assert!(result.is_err());
    assert!(source.exists());
    remove_temporary_resource_directory(&directory);
}

/// 仅源不存在或不是普通文件时允许路径粘贴回退原文本，超限输入绝不能绕过上限。
#[::core::prelude::v1::test]
fn missing_resource_error_is_fallback_only_for_not_found_or_not_file() {
    let directory = temporary_resource_directory("missing-classification");
    let missing = directory.join("missing.png");
    let missing_error = Editor::materialize_resource_with_limits(
        "",
        &missing,
        Some(&directory.join("note.md")),
        ResourceInsertBehavior::None,
        None,
    )
    .expect_err("missing source must be rejected in the backend");
    assert!(resource_materialization_is_missing(&missing_error));

    let source_directory = directory.join("source-directory");
    std::fs::create_dir_all(&source_directory).expect("source directory");
    let directory_error = Editor::materialize_resource_with_limits(
        "",
        &source_directory,
        Some(&directory.join("note.md")),
        ResourceInsertBehavior::None,
        None,
    )
    .expect_err("directory source must be rejected in the backend");
    assert!(resource_materialization_is_missing(&directory_error));

    let oversized = directory.join("oversized.png");
    std::fs::File::create(&oversized)
        .and_then(|file| file.set_len(MAX_RESOURCE_BYTES + 1))
        .expect("oversized source");
    let oversized_error = Editor::materialize_resource_with_limits(
        "",
        &oversized,
        Some(&directory.join("note.md")),
        ResourceInsertBehavior::None,
        None,
    )
    .expect_err("oversized source must remain a visible failure");
    assert!(!resource_materialization_is_missing(&oversized_error));
    remove_temporary_resource_directory(&directory);
}

/// 路径粘贴沿用同一 materialize 边界；超限必须在创建 assets 副本前失败。
#[::core::prelude::v1::test]
fn pasted_path_over_limit_does_not_create_asset_copy() {
    let directory = temporary_resource_directory("paste-over-limit");
    let source = directory.join("pasted.bin");
    let document = directory.join("note.md");
    let assets = directory.join("assets");
    std::fs::File::create(&source)
        .and_then(|file| file.set_len(MAX_RESOURCE_BYTES + 1))
        .expect("oversized pasted path");

    let result = Editor::materialize_resource_with_limits(
        "",
        &source,
        Some(&document),
        ResourceInsertBehavior::CopyToAssetsFolder,
        None,
    );

    assert!(result.is_err());
    assert!(source.exists());
    assert!(!assets.exists());
    remove_temporary_resource_directory(&directory);
}

/// 外部拖放目标变更 block、选区、tab 或 generation 时，后台结果必须整体丢弃。
#[::core::prelude::v1::test]
fn pasted_path_late_result_rejects_changed_selection_or_tab() {
    let expected = ResourceDropTarget {
        document_epoch: 3,
        generation: 9,
        revision: gmark_document::Revision::INITIAL,
        tab_id: uuid::Uuid::from_u128(1),
        block_id: EntityId::from(1),
        selection: 4..4,
        selection_reversed: false,
    };
    assert!(resource_drop_target_is_current(&expected, &expected));

    let mut changed = expected.clone();
    changed.selection = 1..1;
    assert!(!resource_drop_target_is_current(&expected, &changed));
    changed = expected.clone();
    changed.tab_id = uuid::Uuid::from_u128(2);
    assert!(!resource_drop_target_is_current(&expected, &changed));
    changed = expected.clone();
    changed.generation = 10;
    assert!(!resource_drop_target_is_current(&expected, &changed));
    changed = expected.clone();
    changed.document_epoch = 4;
    assert!(!resource_drop_target_is_current(&expected, &changed));
    changed = expected.clone();
    changed.revision = gmark_document::Revision::from_u64(1);
    assert!(!resource_drop_target_is_current(&expected, &changed));
}

/// 缺失路径的文本回退也必须在原 tab 上完成，切 tab 后迟到错误不能回写正文。
#[::core::prelude::v1::test]
fn missing_path_text_fallback_rejects_changed_tab() {
    let expected = ResourceDropTarget {
        document_epoch: 7,
        generation: 7,
        revision: gmark_document::Revision::INITIAL,
        tab_id: uuid::Uuid::from_u128(10),
        block_id: EntityId::from(2),
        selection: 0..0,
        selection_reversed: false,
    };
    let mut changed_tab = expected.clone();
    changed_tab.tab_id = uuid::Uuid::from_u128(11);

    assert!(!resource_drop_target_is_current(&expected, &changed_tab));
}

/// 资源替换失败提示也必须丢弃迟到的 revision 或 tab 结果，避免污染新文档。
#[::core::prelude::v1::test]
fn resource_error_gate_rejects_stale_revision_or_tab() {
    let revision = gmark_document::Revision::INITIAL;
    let tab = uuid::Uuid::from_u128(1);
    assert!(resource_materialization_is_current_for_tab(
        3, revision, tab, 3, revision, tab
    ));
    assert!(!resource_materialization_is_current_for_tab(
        3,
        revision,
        tab,
        3,
        revision,
        uuid::Uuid::from_u128(2)
    ));
    assert!(!resource_materialization_is_current_for_tab(
        3,
        revision,
        tab,
        3,
        gmark_document::Revision::from_u64(1),
        tab
    ));
}
