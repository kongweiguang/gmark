// @author kongweiguang

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use xtask::{
    UnsavedDecision, UpdaterE2eOptions, decision_plan, parse_ack_version, parse_timeout,
    parse_updater_e2e_args, resolve_paths, run_at_args, version_is_newer,
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "gmark-xtask-updater-e2e-test-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("fixture root");
        Self { root }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn roots(&self) -> (PathBuf, PathBuf) {
        (self.path("ui-check"), self.path("updates"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Locks the parser contract to the paths needed for V2 handoff and diagnostics.
#[test]
fn parser_accepts_isolated_artifact_and_key_contract() {
    let arguments = [
        "--ui-check-root",
        "ui-check",
        "--updates-root",
        "updates",
        "--current-binary",
        "dist/gmark-n",
        "--next-binary",
        "dist/gmark-n1",
        "--current-installer",
        "dist/n.msi",
        "--next-installer",
        "dist/n1.msi",
        "--signing-private-key",
        "keys/test.pem",
        "--public-key-base64",
        "dGVzdA==",
        "--current-version",
        "1.0.0",
        "--target-version",
        "1.1.0",
        "--manifest-url",
        "http://127.0.0.1:48123/update-manifest-v2.json",
        "--driver",
        "driver.ps1",
        "--lifetime-lock",
        "updates/lifetime.lock",
        "--helper-log",
        "updates/helper.log",
        "--decision",
        "discard",
        "--timeout",
        "2s",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    let parsed = parse_updater_e2e_args(&arguments).expect("arguments parse");
    assert!(!parsed.help);
    assert_eq!(parsed.options.decision, UnsavedDecision::Discard);
    assert_eq!(
        parsed.options.ui_check_root.as_deref(),
        Some(Path::new("ui-check"))
    );
    assert_eq!(parsed.options.timeout, std::time::Duration::from_secs(2));
    assert_eq!(parsed.options.target_version.as_deref(), Some("1.1.0"));
    assert_eq!(
        parsed.options.manifest_url.as_deref(),
        Some("http://127.0.0.1:48123/update-manifest-v2.json")
    );
    assert_eq!(
        parsed.options.signing_private_key.as_deref(),
        Some(Path::new("keys/test.pem"))
    );
    assert_eq!(
        parsed.options.lifetime_lock.as_deref(),
        Some(Path::new("updates/lifetime.lock"))
    );
    assert_eq!(
        parsed.options.helper_log.as_deref(),
        Some(Path::new("updates/helper.log"))
    );
}

#[test]
fn parser_accepts_legacy_config_root_aliases_for_ui_check_sandbox() {
    for option in ["--config-root", "--config-dir"] {
        let arguments = [option, "legacy-sandbox"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let parsed = parse_updater_e2e_args(&arguments).expect("legacy root alias");
        assert_eq!(
            parsed.options.ui_check_root.as_deref(),
            Some(Path::new("legacy-sandbox"))
        );
    }
}

#[test]
fn parser_rejects_invalid_timeout_and_unknown_option() {
    let timeout = ["--timeout", "0s"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert!(parse_updater_e2e_args(&timeout).is_err());
    let unknown = ["--not-an-option"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert!(parse_updater_e2e_args(&unknown).is_err());
    assert_eq!(parse_timeout("500ms").unwrap().as_millis(), 500);
    assert!(parse_timeout("2h").is_err());
}

#[test]
fn unsaved_decisions_have_distinct_process_contracts() {
    let cancel = decision_plan(UnsavedDecision::Cancel);
    assert!(!cancel.continue_install);
    assert!(cancel.helper_must_not_start);
    assert!(!cancel.old_process_must_exit);

    for decision in [UnsavedDecision::Save, UnsavedDecision::Discard] {
        let plan = decision_plan(decision);
        assert!(plan.continue_install);
        assert!(!plan.helper_must_not_start);
        assert!(plan.old_process_must_exit);
    }
}

/// Ensures all derived markers stay in the isolated update root that the driver receives.
#[test]
fn path_resolution_keeps_markers_inside_update_root() {
    let fixture = Fixture::new();
    let (ui_check, updates) = fixture.roots();
    let mut options = UpdaterE2eOptions {
        ui_check_root: Some(ui_check.clone()),
        updates_root: Some(updates.clone()),
        ..UpdaterE2eOptions::default()
    };
    let paths = resolve_paths(&options, &fixture.root).expect("isolated paths");
    assert!(paths.acknowledgement.starts_with(&updates));
    assert!(paths.lifetime_lock.starts_with(&updates));
    assert!(paths.helper_log.starts_with(&updates));
    assert!(paths.helper_pid.starts_with(&updates));
    assert_eq!(paths.ui_check_root, ui_check);
    assert_ne!(paths.ui_check_root, paths.updates_root);

    options.acknowledgement = Some(fixture.path("outside/startup-ack"));
    let error = resolve_paths(&options, &fixture.root).expect_err("outside marker rejected");
    assert!(error.contains("inside update root"));
}

#[test]
fn acknowledgement_requires_exact_newline_terminated_semver() {
    assert_eq!(parse_ack_version(b"1.2.3\n").unwrap(), "1.2.3");
    assert_eq!(parse_ack_version(b"1.2.3-rc.1\n").unwrap(), "1.2.3-rc.1");
    assert!(parse_ack_version(b"1.2.3").is_err());
    assert!(parse_ack_version(b"1.2.3\n\n").is_err());
    assert!(parse_ack_version(b"not-a-version\n").is_err());
}

#[test]
fn stable_release_is_newer_than_its_prerelease() {
    assert!(version_is_newer("0.1.8-rc.1", "0.1.8"));
    assert!(!version_is_newer("0.1.8", "0.1.8-rc.1"));
    assert!(!version_is_newer("0.1.8", "0.1.8"));
    assert!(!version_is_newer("not-semver", "0.1.8"));
}

#[test]
fn help_and_dry_run_are_explicit_non_mutating_modes() {
    let fixture = Fixture::new();
    run_at_args(&fixture.root, "updater-e2e", &["--help".to_owned()]).expect("help");
    let (ui_check, updates) = fixture.roots();
    let arguments = [
        "--ui-check-root",
        ui_check.to_str().unwrap(),
        "--updates-root",
        updates.to_str().unwrap(),
        "--dry-run",
        "--decision",
        "cancel",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    run_at_args(&fixture.root, "updater-e2e", &arguments).expect("dry run");
    assert!(ui_check.is_dir());
    assert!(updates.join(".gmark-updater-e2e/logs").is_dir());
}

/// Keeps fixture failures actionable by requiring result and log locations in the diagnostic.
#[test]
fn fixture_mode_never_reports_an_unexecuted_production_pass() {
    let fixture = Fixture::new();
    let arguments = ["--fixture"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let error = run_at_args(&fixture.root, "updater-e2e", &arguments)
        .expect_err("fixture must not be reported as a production pass");
    assert!(error.contains("contract-only"));
    assert!(error.contains("logs="));
    assert!(error.contains("result="));
    assert!(error.contains("helper-log="));
    assert!(error.contains("installer-log="));
    assert!(error.contains("manual-download="));
}
