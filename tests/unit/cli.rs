// @author kongweiguang

use super::{CliCommand, help_text, parse};
use std::path::PathBuf;

#[test]
fn parses_files_and_detach_without_touching_process_state() {
    let arguments = vec![
        "--detach".to_owned(),
        "one.md".to_owned(),
        "two.md".to_owned(),
    ];

    assert_eq!(
        parse(&arguments),
        CliCommand::Run {
            detach: true,
            input_paths: vec![PathBuf::from("one.md"), PathBuf::from("two.md")],
        }
    );
}

#[test]
fn terminal_commands_keep_precedence_over_following_arguments() {
    assert_eq!(
        parse(&["document.md".to_owned(), "--version".to_owned()]),
        CliCommand::Version
    );
    assert_eq!(parse(&["-h".to_owned()]), CliCommand::Help);
    assert_eq!(
        parse(&["--unknown".to_owned()]),
        CliCommand::UnknownOption("--unknown".to_owned())
    );
}

#[test]
fn help_preserves_the_public_cli_contract() {
    let help = help_text("1.2.3");
    assert!(help.contains("gmark 1.2.3"));
    assert!(help.contains("gmark [OPTIONS] [FILES...]"));
    assert!(help.contains("-d, --detach"));
}
