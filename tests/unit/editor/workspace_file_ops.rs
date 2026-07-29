// @author kongweiguang

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::{
    WorkspaceCreateKind, markdown_destination_spans, plan_workspace_create, plan_workspace_delete,
    plan_workspace_move,
};

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

#[test]
fn parser_ranges_cover_inline_images_and_reference_definitions_only() {
    let source = concat!(
        "[inline](docs/a.md) ![image](img/a.png) [ref][id]\n\n",
        "[id]: docs/b.md \"title\"\n",
        "`[code](ignored.md)`\n",
        "```md\n[fenced](ignored.md)\n```\n",
    );
    let spans = markdown_destination_spans(source);
    let values = spans
        .iter()
        .map(|span| &source[span.range.clone()])
        .collect::<HashSet<_>>();
    assert_eq!(
        values,
        HashSet::from(["docs/a.md", "img/a.png", "docs/b.md"])
    );
}

#[test]
fn creates_and_safely_undoes_regular_files_and_directories() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let file = plan_workspace_create(root, root, "main.java", WorkspaceCreateKind::File).unwrap();
    file.execute().unwrap();
    assert_eq!(fs::read(&file.path).unwrap(), b"");
    file.undo().unwrap();
    assert!(!file.path.exists());

    let directory =
        plan_workspace_create(root, root, "notes", WorkspaceCreateKind::Directory).unwrap();
    directory.execute().unwrap();
    assert!(directory.path.is_dir());
    directory.undo().unwrap();
    assert!(!directory.path.exists());
}

#[test]
fn create_accepts_arbitrary_extensions_and_rejects_traversal_and_collisions() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(&root.join("existing.md"), "existing");
    assert!(plan_workspace_create(root, root, "../escape.md", WorkspaceCreateKind::File).is_err());
    assert!(plan_workspace_create(root, root, "existing.md", WorkspaceCreateKind::File).is_err());
    let source = plan_workspace_create(root, root, "main.rs", WorkspaceCreateKind::File).unwrap();
    let canonical_root = dunce::canonicalize(root).unwrap();
    assert_eq!(source.path, canonical_root.join("main.rs"));
}

#[test]
fn create_undo_refuses_modified_files_and_nonempty_directories() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let file = plan_workspace_create(root, root, "note.md", WorkspaceCreateKind::File).unwrap();
    file.execute().unwrap();
    write(&file.path, "changed");
    assert!(file.undo().is_err());
    assert_eq!(fs::read_to_string(&file.path).unwrap(), "changed");

    let directory =
        plan_workspace_create(root, root, "notes", WorkspaceCreateKind::Directory).unwrap();
    directory.execute().unwrap();
    write(&directory.path.join("child.md"), "child");
    assert!(directory.undo().is_err());
    assert!(directory.path.join("child.md").exists());
}

#[test]
fn create_parent_symlink_cannot_escape_workspace() {
    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let root = temp.path();
    let link = root.join("outside");
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(outside.path(), &link).is_ok();
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_dir(outside.path(), &link).is_ok();
    if !linked {
        return;
    }
    assert!(plan_workspace_create(root, &link, "escape.md", WorkspaceCreateKind::File).is_err());
    assert!(!outside.path().join("escape.md").exists());
}

#[test]
fn rename_rewrites_relative_links_and_can_be_undone() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let source = root.join("docs/old.md");
    let destination = root.join("archive/new.md");
    write(&source, "# old\r\n");
    write(
        &root.join("index.md"),
        "[old](docs/old.md) [web](https://example.com) [anchor](#x)\r\n",
    );
    fs::create_dir_all(destination.parent().unwrap()).unwrap();

    let plan = plan_workspace_move(root, &source, &destination).unwrap();
    assert_eq!(plan.rewrites.len(), 1);
    plan.execute().unwrap();
    assert!(!source.exists());
    assert!(destination.exists());
    assert_eq!(
        fs::read_to_string(root.join("index.md")).unwrap(),
        "[old](archive/new.md) [web](https://example.com) [anchor](#x)\r\n"
    );

    plan.reversed().execute().unwrap();
    assert!(source.exists());
    assert!(!destination.exists());
    assert_eq!(
        fs::read_to_string(root.join("index.md")).unwrap(),
        "[old](docs/old.md) [web](https://example.com) [anchor](#x)\r\n"
    );
}

#[test]
fn workspace_move_preserves_strict_resource_links_verbatim() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let source = root.join("old.md");
    let destination = root.join("new.md");
    write(&source, "# old\n");
    let before = concat!(
        "[resource](old.md 'gmark:resource')\n",
        "[video](old.md \"gmark:resource ; type = video\")\n",
        "[normal](old.md)\n",
        "![image](old.md \"gmark:resource\")\n",
        "[unknown](old.md \"gmark:resource;mode=tip\")\n",
        "[uppercase](old.md \"GMARK:RESOURCE\")\n",
    );
    write(&root.join("index.md"), before);

    let plan = plan_workspace_move(root, &source, &destination).unwrap();
    plan.execute().unwrap();

    assert_eq!(
        fs::read_to_string(root.join("index.md")).unwrap(),
        concat!(
            "[resource](old.md 'gmark:resource')\n",
            "[video](old.md \"gmark:resource ; type = video\")\n",
            "[normal](new.md)\n",
            "![image](new.md \"gmark:resource\")\n",
            "[unknown](new.md \"gmark:resource;mode=tip\")\n",
            "[uppercase](new.md \"GMARK:RESOURCE\")\n",
        )
    );
}

#[test]
fn directory_move_rebases_links_inside_and_outside_the_directory() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let source = root.join("guide");
    let destination = root.join("archive/guide");
    write(
        &source.join("chapter/a.md"),
        "[sibling](../b.md) [root](../../index.md)\n",
    );
    write(&source.join("b.md"), "[a](chapter/a.md)\n");
    write(&root.join("index.md"), "[chapter](guide/chapter/a.md)\n");
    fs::create_dir_all(destination.parent().unwrap()).unwrap();

    let plan = plan_workspace_move(root, &source, &destination).unwrap();
    plan.execute().unwrap();
    assert_eq!(
        fs::read_to_string(destination.join("chapter/a.md")).unwrap(),
        "[sibling](../b.md) [root](../../../index.md)\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("index.md")).unwrap(),
        "[chapter](archive/guide/chapter/a.md)\n"
    );
}

#[test]
fn preserves_utf8_bom_crlf_and_utf16_encoding() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let source = root.join("old.md");
    let destination = root.join("new.md");
    write(&source, "# old\n");
    write(&root.join("bom.md"), b"\xef\xbb\xbf[x](old.md)\r\n");
    let mut utf16 = vec![0xff, 0xfe];
    for unit in "[x](old.md)\r\n".encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    write(&root.join("utf16.md"), &utf16);

    let plan = plan_workspace_move(root, &source, &destination).unwrap();
    plan.execute().unwrap();
    assert_eq!(
        fs::read(root.join("bom.md")).unwrap(),
        b"\xef\xbb\xbf[x](new.md)\r\n"
    );
    let rewritten_utf16 = fs::read(root.join("utf16.md")).unwrap();
    assert!(rewritten_utf16.starts_with(&[0xff, 0xfe]));
    assert_eq!(rewritten_utf16.len(), utf16.len());
}

#[test]
fn collision_and_changed_snapshot_leave_everything_untouched() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let source = root.join("old.md");
    let destination = root.join("new.md");
    write(&source, "# old\n");
    write(&destination, "# existing\n");
    assert!(plan_workspace_move(root, &source, &destination).is_err());
    fs::remove_file(&destination).unwrap();
    write(&root.join("index.md"), "[x](old.md)\n");
    let plan = plan_workspace_move(root, &source, &destination).unwrap();
    write(&root.join("index.md"), "changed\n");
    assert!(plan.execute().is_err());
    assert!(source.exists());
    assert!(!destination.exists());
    assert_eq!(
        fs::read_to_string(root.join("index.md")).unwrap(),
        "changed\n"
    );
}

#[test]
fn rewrites_reference_definitions_and_percent_encoded_paths() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let source = root.join("old folder/note.md");
    let destination = root.join("new folder/note.md");
    write(&source, "# note\n");
    write(
        &root.join("index.md"),
        "[note][target]\n\n[target]: <old%20folder/note.md#part> \"title\"\n",
    );
    fs::create_dir_all(destination.parent().unwrap()).unwrap();

    let plan = plan_workspace_move(root, &source, &destination).unwrap();
    plan.execute().unwrap();
    assert_eq!(
        fs::read_to_string(root.join("index.md")).unwrap(),
        "[note][target]\n\n[target]: <new%20folder/note.md#part> \"title\"\n"
    );
}

#[test]
fn resolves_markdown_escaped_inline_destinations_from_parser_values() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let source = root.join("old(1).md");
    let destination = root.join("new(1).md");
    write(&source, "# old\n");
    write(&root.join("index.md"), "[old](old\\(1\\).md)\n");

    let plan = plan_workspace_move(root, &source, &destination).unwrap();
    plan.execute().unwrap();
    assert_eq!(
        fs::read_to_string(root.join("index.md")).unwrap(),
        "[old](new(1).md)\n"
    );
}

#[test]
fn ignored_markdown_is_not_rewritten() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let source = root.join("old.md");
    let destination = root.join("new.md");
    write(&source, "# old\n");
    write(&root.join(".gitignore"), "ignored.md\n");
    write(&root.join("ignored.md"), "[old](old.md)\n");
    write(&root.join("tracked.md"), "[old](old.md)\n");

    let plan = plan_workspace_move(root, &source, &destination).unwrap();
    plan.execute().unwrap();
    assert_eq!(
        fs::read_to_string(root.join("ignored.md")).unwrap(),
        "[old](old.md)\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("tracked.md")).unwrap(),
        "[old](new.md)\n"
    );
}

#[test]
fn failed_rewrite_rolls_back_prior_writes_and_the_move() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let source = root.join("old.md");
    let destination = root.join("new.md");
    let first = root.join("a.md");
    let second = root.join("b.md");
    write(&source, "# old\n");
    write(&first, "[old](old.md)\n");
    write(&second, "[old](old.md)\n");

    let mut plan = plan_workspace_move(root, &source, &destination).unwrap();
    plan.rewrites
        .sort_by(|left, right| left.before_path.cmp(&right.before_path));
    plan.rewrites[1].after_path = root.join("missing/parent/b.md");
    assert!(plan.execute().is_err());
    assert!(source.exists());
    assert!(!destination.exists());
    assert_eq!(fs::read_to_string(&first).unwrap(), "[old](old.md)\n");
    assert_eq!(fs::read_to_string(&second).unwrap(), "[old](old.md)\n");
}

#[test]
fn destination_parent_symlink_cannot_escape_workspace() {
    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let root = temp.path();
    let source = root.join("old.md");
    let link = root.join("outside");
    write(&source, "# old\n");
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(outside.path(), &link).is_ok();
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_dir(outside.path(), &link).is_ok();
    if !linked {
        return;
    }

    assert!(plan_workspace_move(root, &source, &link.join("new.md")).is_err());
    assert!(source.exists());
    assert!(!outside.path().join("new.md").exists());
}

#[test]
fn workspace_delete_plan_rejects_root_and_outside_targets() {
    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let root = temp.path();
    let target = root.join("note.md");
    write(&target, "# note\n");
    let outside_target = outside.path().join("outside.md");
    write(&outside_target, "# outside\n");

    assert!(plan_workspace_delete(root, root).is_err());
    assert!(plan_workspace_delete(root, &outside_target).is_err());
    assert!(plan_workspace_delete(root, &target).is_ok());
}

#[test]
fn workspace_delete_plan_revalidates_and_delegates_the_exact_target() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let target = root.join("folder");
    fs::create_dir_all(&target).unwrap();
    write(&target.join("note.md"), "# note\n");
    let plan = plan_workspace_delete(root, &target).unwrap();
    let expected = dunce::canonicalize(&target).unwrap();
    let mut delegated = None;

    assert_eq!(plan.workspace_path, target);

    plan.execute_with(|path| {
        delegated = Some(path.to_path_buf());
        fs::remove_dir_all(path).map_err(Into::into)
    })
    .unwrap();

    assert_eq!(delegated, Some(expected));
    assert!(!target.exists());
}

use std::collections::HashSet;
