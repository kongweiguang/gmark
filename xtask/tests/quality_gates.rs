// @author kongweiguang

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

#[test]
fn source_size_rejects_more_than_eight_hundred_lines() {
    let fixture = Fixture::new();
    fixture.write("src/oversized.rs", &with_author(lines(800)));

    let error = xtask::run_at(fixture.path(), "source-size").unwrap_err();

    assert!(error.contains("oversized.rs"));
    assert!(error.contains("801"));
}

#[test]
fn source_size_accepts_exact_hard_limit() {
    let fixture = Fixture::new();
    fixture.write("src/bounded.rs", &with_author(lines(799)));

    xtask::run_at(fixture.path(), "source-size").unwrap();
}

#[test]
fn test_layout_rejects_inline_tests_and_src_test_files() {
    let fixture = Fixture::new();
    fixture.write(
        "src/domain.rs",
        &with_author("#[cfg(test)]\nmod tests { #[test] fn works() {} }\n".to_owned()),
    );
    fixture.write("src/tests.rs", &with_author("// test body\n".to_owned()));

    let error = xtask::run_at(fixture.path(), "test-layout").unwrap_err();

    assert!(error.contains("inline test module"));
    assert!(error.contains("test implementation must live under tests"));
}

#[test]
fn architecture_rejects_ui_dependency_in_domain_crate() {
    let fixture = Fixture::new();
    fixture.write(
        "crates/gmark-document/Cargo.toml",
        "# @author kongweiguang\n[dependencies]\ngpui = \"1\"\n",
    );

    let error = xtask::run_at(fixture.path(), "architecture").unwrap_err();

    assert!(error.contains("domain crate depends on UI/platform crate"));
}

#[test]
fn architecture_rejects_implementation_includes_and_mechanical_source_names() {
    let fixture = Fixture::new();
    fixture.write(
        "src/lib.rs",
        &with_author("include!(\"parts/fn_render_editor_01.rs\");\nmod numbered_02;\n".to_owned()),
    );
    fixture.write(
        "src/parts/fn_render_editor_01.rs",
        &with_author("fn render() {}\n".to_owned()),
    );
    fixture.write(
        "src/numbered_02.rs",
        &with_author("fn numbered() {}\n".to_owned()),
    );

    let error = xtask::run_at(fixture.path(), "architecture").unwrap_err();

    assert!(error.contains("implementation include! is forbidden"));
    assert!(error.contains("mechanical source filename is forbidden"));
}

#[test]
fn architecture_allows_the_generated_i18n_catalog_include() {
    let fixture = Fixture::new();
    fixture.write(
        "src/i18n/parts/mod.rs",
        &with_author("mod catalog;\n".to_owned()),
    );
    fixture.write(
        "src/i18n/parts/catalog.rs",
        &with_author("include!(\"i18n_strings_catalog.rs\");\n".to_owned()),
    );
    fixture.write(
        "src/i18n/parts/i18n_strings_catalog.rs",
        &with_author("fn generated_data() {}\n".to_owned()),
    );

    xtask::run_at(fixture.path(), "architecture").unwrap();
}

#[test]
fn architecture_rejects_orphan_sources_and_unexplained_lint_allows() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", &with_author("mod connected;\n".to_owned()));
    fixture.write(
        "src/connected.rs",
        &with_author("#[allow(dead_code)]\nfn connected() {}\n".to_owned()),
    );
    fixture.write(
        "src/orphan.rs",
        &with_author("fn unreachable() {}\n".to_owned()),
    );

    let error = xtask::run_at(fixture.path(), "architecture").unwrap_err();

    assert!(error.contains("orphan Rust source"));
    assert!(error.contains("lint allow requires a reason"));
}

#[test]
fn architecture_accepts_a_reasoned_lint_allow() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", &with_author("mod connected;\n".to_owned()));
    fixture.write(
        "src/connected.rs",
        &with_author(
            "// reason: public compatibility hook is exercised downstream; remove when retired\n#[allow(dead_code)]\nfn connected() {}\n"
                .to_owned(),
        ),
    );

    xtask::run_at(fixture.path(), "architecture").unwrap();
}

#[test]
fn authors_require_header_but_ignore_json() {
    let fixture = Fixture::new();
    fixture.write("src/missing.rs", "fn missing() {}\n");
    fixture.write("src/machine.json", "{}\n");

    let error = xtask::run_at(fixture.path(), "authors").unwrap_err();

    assert!(error.contains("missing.rs"));
    assert!(!error.contains("machine.json"));
}

fn lines(count: usize) -> String {
    (0..count).map(|_| "// line\n").collect()
}

fn with_author(mut source: String) -> String {
    source.insert_str(0, "// @author kongweiguang\n");
    source
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("gmark-quality-{}-{id}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, source: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
