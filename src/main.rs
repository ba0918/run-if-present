mod cli;
mod runtime;

use clap::{error::ErrorKind, CommandFactory, Parser};

fn main() {
    let arguments = match cli::Arguments::try_parse() {
        Ok(arguments) => arguments,
        Err(error) if matches!(error.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
            error.exit()
        }
        Err(error) => {
            let rendered = error.to_string();
            let message = rendered
                .lines()
                .next()
                .unwrap_or("invalid command line")
                .strip_prefix("error: ")
                .unwrap_or(rendered.lines().next().unwrap_or("invalid command line"));
            eprintln!("run-if-present: syntax: {message}");
            std::process::exit(2);
        }
    };
    if arguments.command_help_requested() {
        let mut command = cli::Arguments::command();
        command
            .find_subcommand_mut("command")
            .expect("command subcommand exists")
            .print_help()
            .expect("stdout is writable");
        println!();
        return;
    }
    if arguments.invalid_empty_chdir() {
        eprintln!("run-if-present: syntax: chdir must not be empty");
        std::process::exit(2);
    }
    if arguments.invalid_empty_launch_command() {
        eprintln!("run-if-present: syntax: command must not be empty");
        std::process::exit(2);
    }
    if arguments.invalid_empty_path() {
        eprintln!("run-if-present: syntax: path must not be empty");
        std::process::exit(2);
    }
    if let Err(error) = runtime::run(arguments) {
        error.print();
        std::process::exit(error.code());
    }
}
