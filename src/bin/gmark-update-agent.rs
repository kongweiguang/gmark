// @author kongweiguang

#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

//! Standalone progress feedback window for a Gmark update transaction.

#[path = "../update_agent/mod.rs"]
mod update_agent;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    match update_agent::parse_args(&args) {
        Ok(None) => {
            println!("usage: gmark-update-agent --progress <transaction-progress.json>");
            ExitCode::SUCCESS
        }
        Ok(Some(args)) => ExitCode::from(update_agent::run(args) as u8),
        Err(error) => {
            eprintln!("gmark-update-agent: {error}");
            ExitCode::from(2)
        }
    }
}
