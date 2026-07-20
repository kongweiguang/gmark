// @author kongweiguang

use std::fs;

use gmark_document::{AtomicWriteStage, atomic_write};
use tempfile::tempdir;

#[test]
fn atomic_write_creates_and_replaces_target_without_leaking_temp_files() {
    let directory = tempdir().expect("应能创建测试目录");
    let target = directory.path().join("document.md");

    atomic_write(&target, b"first").expect("首次原子写入应成功");
    atomic_write(&target, b"second").expect("替换原文件应成功");

    assert_eq!(fs::read(&target).expect("应能读取结果"), b"second");
    let remaining = fs::read_dir(directory.path())
        .expect("应能枚举测试目录")
        .collect::<Result<Vec<_>, _>>()
        .expect("目录项应可读取");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].path(), target);
}

#[test]
fn failed_atomic_write_preserves_existing_file() {
    let directory = tempdir().expect("应能创建测试目录");
    let target = directory.path().join("document.md");
    fs::write(&target, b"stable").expect("应能创建原文件");

    let invalid_target = target.join("child.md");
    let error = atomic_write(&invalid_target, b"new").expect_err("非法父路径必须失败");

    assert_eq!(error.stage(), AtomicWriteStage::CreateTemporary);
    assert_eq!(fs::read(&target).expect("原文件应保持可读"), b"stable");
}
