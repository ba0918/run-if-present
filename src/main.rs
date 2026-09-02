mod cli;
mod runtime;

use clap::{error::ErrorKind, Arg, ArgAction, CommandFactory, FromArgMatches};
use std::io::Write;

fn print_syntax(message: impl std::fmt::Display) {
    let _ = writeln!(
        std::io::stderr().lock(),
        "run-if-present: syntax: {message}"
    );
}

fn main() {
    let parser = cli::Arguments::command()
        .arg(Arg::new("help").long("help").action(ArgAction::Help))
        .arg(
            Arg::new("version")
                .long("version")
                .action(ArgAction::Version),
        );
    let arguments = match parser.try_get_matches() {
        Ok(matches) => cli::Arguments::from_arg_matches(&matches).expect("matches are valid"),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            error.exit()
        }
        Err(_) => {
            print_syntax("invalid command line");
            std::process::exit(2);
        }
    };
    if let Some(name) = arguments.subcommand_help_requested() {
        let mut command = cli::Arguments::command();
        let subcommand = command
            .find_subcommand_mut(name)
            .expect("subcommand exists");
        let _ = subcommand.print_help();
        let _ = writeln!(std::io::stdout());
        return;
    }
    if arguments.invalid_help_request() || arguments.missing_path_command() {
        print_syntax("invalid command line");
        std::process::exit(2);
    }
    if let Some(name) = arguments.empty_wrapper_value() {
        print_syntax(format_args!("{name} must not be empty"));
        std::process::exit(2);
    }
    if let Err(error) = runtime::run(arguments) {
        error.print();
        std::process::exit(error.code());
    }
}
