// @author kongweiguang

use std::path::PathBuf;

use super::take_update_acknowledgement;

#[test]
fn internal_update_ack_is_removed_before_user_cli_parsing() {
    let mut args = vec![
        "gmark".to_owned(),
        "--update-ack".to_owned(),
        "C:/temp/update-ack".to_owned(),
        "note.md".to_owned(),
    ];
    assert_eq!(
        take_update_acknowledgement(&mut args),
        Some(PathBuf::from("C:/temp/update-ack"))
    );
    assert_eq!(args, ["gmark", "note.md"]);
}
