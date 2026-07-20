// @author kongweiguang

//! 无副作用的命令行解析；路径解析和进程退出留给应用启动 adapter。

use std::path::PathBuf;

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum CliCommand {
    Run {
        detach: bool,
        input_paths: Vec<PathBuf>,
    },
    Help,
    Version,
    UnknownOption(String),
}

pub(crate) fn parse(arguments: &[String]) -> CliCommand {
    let mut detach = false;
    let mut input_paths = Vec::new();

    for argument in arguments {
        match argument.as_str() {
            "--version" | "-v" => return CliCommand::Version,
            "--help" | "-h" => return CliCommand::Help,
            "--detach" | "-d" => detach = true,
            option if option.starts_with('-') => {
                return CliCommand::UnknownOption(option.to_owned());
            }
            path => input_paths.push(PathBuf::from(path)),
        }
    }

    CliCommand::Run {
        detach,
        input_paths,
    }
}

pub(crate) fn help_text(version: &str) -> String {
    format!(
        "gmark {version} - A block-based Markdown editor\n\n\
         USAGE:\n\
             gmark [OPTIONS] [FILES...]\n\n\
         OPTIONS:\n\
             -v, --version    Print version information\n\
             -h, --help       Print this help message\n\
             -d, --detach     Launch in background (non-blocking)\n\n\
         FILES:\n\
             One or more markdown files to open. If no files are specified,\n\
             opens an empty document."
    )
}

#[cfg(test)]
#[path = "../tests/unit/cli.rs"]
mod tests;
