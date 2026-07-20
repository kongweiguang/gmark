// @author kongweiguang

use super::format_update_message;

#[test]
fn update_message_templates_replace_versions() {
    assert_eq!(
        format_update_message("Current {current}, latest {latest}.", "0.2.1", "0.2.2"),
        "Current 0.2.1, latest 0.2.2."
    );
}
