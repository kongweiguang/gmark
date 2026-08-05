// @author kongweiguang

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

const DOMAIN_CRATES: &[&str] = &[
    "gmark-config",
    "gmark-i18n",
    "gmark-markdown",
    "gmark-source-tools",
    "gmark-export",
    "gmark-update-core",
];

#[test]
fn source_size_enforces_all_manual_rust_limits() {
    let accepted = Fixture::new();
    accepted.write("src/bounded.rs", &with_author(lines(799)));
    accepted.write("src/warning.rs", &with_author(lines(500)));
    accepted.write("tests/bounded.rs", &with_author(lines(799)));
    accepted.write("tests/warning.rs", &with_author(lines(500)));
    xtask::run_at(accepted.path(), "source-size").unwrap();

    let rejected = Fixture::new();
    rejected.write("src/oversized.rs", &with_author(lines(800)));
    rejected.write("tests/oversized.rs", &with_author(lines(800)));
    rejected.write("benches/oversized.rs", &with_author(lines(800)));
    rejected.write("examples/oversized.rs", &with_author(lines(800)));
    rejected.write("fuzz/oversized.rs", &with_author(lines(800)));
    rejected.write("xtask/src/oversized.rs", &with_author(lines(800)));
    rejected.write("build.rs", &with_author(lines(800)));
    let error = xtask::run_at(rejected.path(), "source-size").unwrap_err();
    for path in [
        "src/oversized.rs",
        "tests/oversized.rs",
        "benches/oversized.rs",
        "examples/oversized.rs",
        "fuzz/oversized.rs",
        "xtask/src/oversized.rs",
        "build.rs",
    ] {
        assert!(error.contains(path), "missing {path} in {error}");
    }
    assert!(error.contains("801"));
}

#[test]
fn test_layout_allows_explicit_test_support_but_rejects_ad_hoc_test_paths() {
    let accepted = Fixture::new();
    accepted.write(
        "src/lib.rs",
        &with_author("#[cfg(test)]\nmod test_support;\n"),
    );
    accepted.write("src/domain.rs", &with_author("pub fn works() {}\n"));
    accepted.write("src/contest.rs", &with_author("pub fn contest() {}\n"));
    accepted.write("src/test_support/mod.rs", &with_author("mod helpers;\n"));
    accepted.write(
        "src/test_support/helpers.rs",
        &with_author("pub fn helper() {}\n"),
    );
    accepted.write("tests/domain.rs", "#[test]\nfn works() {}\n");
    accepted.write("tests/domain_state_tests.rs", "#[test]\nfn works() {}\n");
    xtask::run_at(accepted.path(), "test-layout").unwrap();

    let rejected = Fixture::new();
    rejected.write(
        "src/domain.rs",
        &with_author("#[cfg(test)]\nmod specs { #[test] fn works() {} }\n"),
    );
    rejected.write("src/fixtures/case.rs", &with_author("pub fn case() {}\n"));
    rejected.write(
        "src/read_fixture.rs",
        &with_author("const CASE: &str = include_str!(\"tests/support/case.txt\");\n"),
    );
    rejected.write(
        "src/domain/test_state.rs",
        &with_author("pub fn test_state() {}\n"),
    );
    rejected.write(
        "src/domain_state_tests.rs",
        &with_author("pub fn state_tests() {}\n"),
    );
    rejected.write("src/mocks.rs", &with_author("pub fn mock() {}\n"));
    let error = xtask::run_at(rejected.path(), "test-layout").unwrap_err();
    assert!(error.contains("#[cfg(test)] inline module"));
    assert!(error.contains("#[test] body"));
    assert!(error.contains("test fixture"));
    assert!(error.contains("production code references test support"));
    for path in [
        "src/domain/test_state.rs",
        "src/domain_state_tests.rs",
        "src/mocks.rs",
    ] {
        assert!(error.contains(path), "missing {path} in {error}");
    }
}

#[test]
fn lint_allow_requires_an_adjacent_reason_and_removal_condition() {
    let accepted = Fixture::new();
    accepted.write(
        "src/lib.rs",
        &with_author("mod connected;\nmod chinese_comment;\nmod inner_allow;\n"),
    );
    accepted.write(
        "src/connected.rs",
        &with_author(
            "// reason: compatibility callback remains public; remove when downstream support retires\n#[allow(dead_code)]\nfn connected() {}\n",
        ),
    );
    accepted.write(
        "src/chinese_comment.rs",
        &with_author(
            "// 原因：兼容回调仍对下游开放；下游迁移完成后移除\n#[allow(dead_code)]\nfn chinese_comment() {}\n",
        ),
    );
    accepted.write(
        "src/inner_allow.rs",
        &with_author(
            "// reason: legacy module stays compiled for compatibility; remove when v1 support retires\n#![allow(dead_code)]\npub fn legacy() {}\n",
        ),
    );
    xtask::run_at(accepted.path(), "architecture").unwrap();

    let rejected = Fixture::new();
    rejected.write(
        "src/lib.rs",
        &with_author("mod connected;\nmod inner_allow;\n"),
    );
    rejected.write(
        "src/connected.rs",
        &with_author(
            "// 原因：兼容回调仍对下游开放\n// 下游迁移完成后移除\n#[allow(dead_code)]\nfn connected() {}\n",
        ),
    );
    rejected.write(
        "src/inner_allow.rs",
        &with_author("#![allow(dead_code)]\npub fn legacy() {}\n"),
    );
    let error = xtask::run_at(rejected.path(), "architecture").unwrap_err();
    assert!(error.contains("immediately preceding reason"));
    assert!(error.contains("src/inner_allow.rs"));
}

#[test]
fn source_structure_rejects_implementation_includes_numbered_files_and_orphans() {
    let rejected = Fixture::new();
    rejected.write(
        "src/lib.rs",
        &with_author("mod numbered_02;\ninclude!(\"parts.rs\");\n"),
    );
    rejected.write("src/numbered_02.rs", &with_author("pub fn numbered() {}\n"));
    rejected.write("src/orphan.rs", &with_author("pub fn orphan() {}\n"));
    rejected.write("src/orphan/mod.rs", &with_author("pub fn nested() {}\n"));
    rejected.write(
        "src/nested_orphan/lib.rs",
        &with_author("pub fn nested_library() {}\n"),
    );
    let error = xtask::run_at(rejected.path(), "architecture").unwrap_err();
    assert!(error.contains("implementation include!"));
    assert!(error.contains("numbered production source filename"));
    assert!(error.contains("orphan Rust source"));
    assert!(error.contains("src/orphan/mod.rs"));
    assert!(error.contains("src/nested_orphan/lib.rs"));

    let accepted = Fixture::new();
    accepted.write(
        "src/lib.rs",
        &with_author("mod descriptive;\nmod nested;\n"),
    );
    accepted.write(
        "src/descriptive.rs",
        &with_author("const LABEL: &str = include_str!(\"README.txt\");\n"),
    );
    accepted.write("src/nested/mod.rs", &with_author("mod host;\n"));
    accepted.write(
        "src/nested/host.rs",
        &with_author("#[path = \"../parts/linked.rs\"]\nmod linked;\n"),
    );
    accepted.write("src/parts/linked.rs", &with_author("pub fn linked() {}\n"));
    accepted.write("build.rs", &with_author("fn main() {}\n"));
    accepted.write("src/main.rs", &with_author("fn main() {}\n"));
    accepted.write("src/bin/inspect.rs", &with_author("fn main() {}\n"));
    accepted.write("src/bin/worker/main.rs", &with_author("fn main() {}\n"));
    accepted.package("gmark-fixture", "");
    xtask::run_at(accepted.path(), "architecture").unwrap();
}

#[test]
fn implementation_include_is_rejected_even_for_generated_sources() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", &with_author("mod i18n;\n"));
    fixture.write("src/i18n/mod.rs", &with_author("mod parts;\n"));
    fixture.write("src/i18n/parts/mod.rs", &with_author("mod catalog;\n"));
    fixture.write(
        "src/i18n/parts/catalog.rs",
        &with_author("include!(\"i18n_strings_catalog.rs\");\n"),
    );
    fixture.write(
        "src/i18n/parts/i18n_strings_catalog.rs",
        "// @generated; do not edit\npub const CATALOG: &str = \"generated\";\n",
    );
    let error = xtask::run_at(fixture.path(), "architecture").unwrap_err();
    assert!(error.contains("implementation include! is forbidden"));
}

#[test]
fn authors_require_header_but_ignore_non_maintainable_data() {
    let accepted = Fixture::new();
    accepted.write("src/owned.rs", &with_author("pub fn owned() {}\n"));
    xtask::run_at(accepted.path(), "authors").unwrap();

    let fixture = Fixture::new();
    fixture.write("src/missing.rs", "fn missing() {}\n");
    fixture.write("src/machine.json", "{}\n");
    let error = xtask::run_at(fixture.path(), "authors").unwrap_err();
    assert!(error.contains("missing.rs"));
    assert!(!error.contains("machine.json"));
}

#[test]
fn ui_colors_allow_theme_and_transparent_geometry_but_reject_runtime_literals() {
    let accepted = Fixture::new();
    accepted.write(
        "src/ui/theme/palette.rs",
        &with_author("pub fn color() { let _ = rgba(0x007affff); }\n"),
    );
    accepted.write(
        "src/editor.rs",
        &with_author("pub fn hit_target() { let _ = hsla(0.0, 0.0, 0.0, 0.0); }\n"),
    );
    xtask::run_at(accepted.path(), "ui-colors").unwrap();

    let rejected = Fixture::new();
    rejected.write(
        "src/editor.rs",
        &with_author("pub fn panel() { let _ = rgba(0x007affff); }\n"),
    );
    let error = xtask::run_at(rejected.path(), "ui-colors").unwrap_err();
    assert!(error.contains("ThemeColors.workbench/material tokens"));
    assert!(error.contains("src/editor.rs"));
}

#[test]
fn domain_crates_accept_clean_dependencies() {
    let fixture = Fixture::new();
    fixture.add_domain_packages();
    xtask::run_at(fixture.path(), "architecture").unwrap();
}

#[test]
fn domain_crates_reject_ui_accessibility_window_and_main_application_edges() {
    let fixture = Fixture::new();
    fixture.package("gmark-config", "gpui = \"0.2\"");
    fixture.package("gmark-i18n", "accesskit = \"0.24\"");
    fixture.package("gmark-markdown", "windows = \"0.62\"");
    fixture.package("gmark-source-tools", "gmark = { path = \"../..\" }");
    fixture.package("gmark-export", "raw-window-handle = \"0.6\"");
    fixture.package("gmark-update-core", "");
    fixture.write(
        "crates/gmark-update-core/src/lib.rs",
        &with_author("use std::os::windows::process::CommandExt;\n"),
    );

    let error = xtask::run_at(fixture.path(), "architecture").unwrap_err();
    for expected in [
        "gmark-config",
        "gmark-i18n",
        "gmark-markdown",
        "gmark-source-tools",
        "gmark-export",
        "gmark-update-core",
        "gpui",
        "accesskit",
        "windows",
        "main application package",
        "raw-window-handle",
        "std::os::windows::process",
    ] {
        assert!(error.contains(expected), "missing {expected} in {error}");
    }
}

#[test]
fn domain_crates_accept_platform_filesystem_extensions() {
    let fixture = Fixture::new();
    fixture.add_domain_packages();
    fixture.write(
        "crates/gmark-update-core/src/lib.rs",
        &with_author("use std::os::windows::fs::{MetadataExt, OpenOptionsExt};\n"),
    );
    xtask::run_at(fixture.path(), "architecture").unwrap();
}

#[test]
fn domain_source_scan_ignores_comments_and_strings() {
    let fixture = Fixture::new();
    fixture.add_domain_packages();
    fixture.write(
        "crates/gmark-config/src/lib.rs",
        &with_author("// gpui::App is forbidden\nconst LABEL: &str = \"std::os::windows\";\n"),
    );
    xtask::run_at(fixture.path(), "architecture").unwrap();
}

#[test]
fn export_accepts_its_workspace_allowlist_and_rejects_other_workspace_crates() {
    let accepted = Fixture::new();
    accepted.add_domain_packages();
    accepted.package(
        "gmark-export",
        "gmark-markdown = { path = \"../gmark-markdown\" }\ngmark-source-tools = { path = \"../gmark-source-tools\" }",
    );
    xtask::run_at(accepted.path(), "architecture").unwrap();

    let rejected = Fixture::new();
    rejected.add_domain_packages();
    rejected.package(
        "gmark-export",
        "gmark-markdown = { path = \"../gmark-markdown\" }\ngmark-config = { path = \"../gmark-config\" }",
    );
    let error = xtask::run_at(rejected.path(), "architecture").unwrap_err();
    assert!(error.contains("gmark-export may depend"));
    assert!(error.contains("gmark-config"));
}

#[test]
fn module_boundaries_ignore_comments_and_strings_but_reject_real_reverse_edges() {
    let accepted = Fixture::new();
    accepted.write(
        "src/lib.rs",
        &with_author(
            "mod ui;\nmod platform;\nmod adapters;\nmod document_host;\nmod components;\n",
        ),
    );
    for path in [
        "src/ui.rs",
        "src/platform.rs",
        "src/adapters.rs",
        "src/document_host.rs",
        "src/components/mod.rs",
    ] {
        accepted.write(
            path,
            &with_author(
                "// crate::editor must not be parsed\nconst LABEL: &str = \"crate::app\";\n",
            ),
        );
    }
    xtask::run_at(accepted.path(), "architecture").unwrap();

    let rejected = Fixture::new();
    rejected.write(
        "src/lib.rs",
        &with_author(
            "mod ui;\nmod platform;\nmod adapters;\nmod document_host;\nmod components;\n",
        ),
    );
    for (path, layer) in [
        ("src/ui.rs", "ui"),
        ("src/platform.rs", "platform"),
        ("src/adapters.rs", "adapters"),
        ("src/document_host.rs", "document_host"),
        ("src/components/mod.rs", "components"),
    ] {
        rejected.write(
            path,
            &with_author("use crate::{editor::Editor, app::Application};\n"),
        );
        let error = xtask::run_at(rejected.path(), "architecture").unwrap_err();
        assert!(error.contains(&format!("layer '{layer}'")));
    }
}

#[test]
fn legacy_domain_dependency_guard_is_not_relaxed() {
    let accepted = Fixture::new();
    for name in [
        "gmark-document",
        "gmark-paged-document",
        "gmark-recovery-codec",
    ] {
        accepted.package(name, "");
    }
    xtask::run_at(accepted.path(), "architecture").unwrap();

    let fixture = Fixture::new();
    fixture.package("gmark-document", "gpui = \"0.2\"");
    let error = xtask::run_at(fixture.path(), "architecture").unwrap_err();
    assert!(error.contains("gmark-document"));
    assert!(error.contains("GPUI"));
}

fn lines(count: usize) -> String {
    (0..count).map(|_| "// line\n").collect()
}

fn with_author(source: impl AsRef<str>) -> String {
    format!("// @author kongweiguang\n{}", source.as_ref())
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("gmark-quality-{}-{id}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let fixture = Self { root };
        fixture.refresh_workspace_members();
        fixture.write("src/lib.rs", &with_author(""));
        fixture
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn add_domain_packages(&self) {
        for name in DOMAIN_CRATES {
            self.package(name, "");
        }
    }

    fn package(&self, name: &str, dependencies: &str) {
        self.write(
            &format!("crates/{name}/Cargo.toml"),
            &format!(
                "# @author kongweiguang\n[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\n{dependencies}\n"
            ),
        );
        self.write(
            &format!("crates/{name}/src/lib.rs"),
            &with_author("pub fn boundary() {}\n"),
        );
        self.refresh_workspace_members();
    }

    fn refresh_workspace_members(&self) {
        let crates_root = self.root.join("crates");
        let mut members = fs::read_dir(&crates_root)
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .filter_map(|entry| {
                entry
                    .file_type()
                    .ok()
                    .filter(|kind| kind.is_dir())
                    .map(|_| entry.file_name().to_string_lossy().into_owned())
            })
            .filter(|name| crates_root.join(name).join("Cargo.toml").is_file())
            .map(|name| format!("    \"crates/{name}\","))
            .collect::<Vec<_>>();
        members.sort();
        self.write(
            "Cargo.toml",
            &format!(
                "# @author kongweiguang\n[package]\nname = \"gmark\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[workspace]\nmembers = [\n{}\n]\nresolver = \"2\"\n",
                members.join("\n")
            ),
        );
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
