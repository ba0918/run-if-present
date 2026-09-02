mod cli;
mod runtime;

use clap::{error::ErrorKind, Arg, ArgAction, CommandFactory, FromArgMatches};

fn main() {
    let mut parser = cli::Arguments::command()
        .arg(Arg::new("help").long("help").action(ArgAction::Help))
        .arg(
            Arg::new("version")
                .long("version")
                .action(ArgAction::Version),
        );
    parser = parser.mut_subcommand("path", |command| {
        command.arg(Arg::new("help").long("help").action(ArgAction::Help))
    });
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
            eprintln!("run-if-present: syntax: invalid command line");
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
    if let Some(name) = arguments.empty_wrapper_value() {
        eprintln!("run-if-present: syntax: {name} must not be empty");
        std::process::exit(2);
    }
    if let Err(error) = runtime::run(arguments) {
        error.print();
        std::process::exit(error.code());
    }
}
