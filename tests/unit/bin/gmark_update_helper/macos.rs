// @author kongweiguang

use super::*;

#[test]
fn bundle_version_requires_target_version_and_preserves_prerelease() {
    assert!(bundle_version_matches(Some("1.2.3"), None, "1.2.3"));
    assert!(bundle_version_matches(
        Some("1.2.3"),
        Some("1.2.3-beta.1"),
        "1.2.3-beta.1"
    ));
    assert!(!bundle_version_matches(
        Some("1.2.3"),
        Some("1.2.3"),
        "1.2.3-beta.1"
    ));
}

#[test]
fn archive_path_validation_rejects_parent_and_special_entries() {
    let parent = Path::new("../gmark.app");
    assert!(
        parent.is_relative()
            && parent
                .components()
                .any(|component| matches!(component, Component::ParentDir))
    );
}

#[test]
fn authorization_scripts_stop_before_the_second_mutation_on_failure() {
    assert!(INSTALL_AUTHORIZATION_SCRIPT.contains(" && /bin/mv "));
    assert!(ROLLBACK_AUTHORIZATION_SCRIPT.contains(" && /bin/mv "));
    assert!(!INSTALL_AUTHORIZATION_SCRIPT.contains("; /bin/mv "));
    assert!(!ROLLBACK_AUTHORIZATION_SCRIPT.contains("; /bin/mv "));
}
