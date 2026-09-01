mod cli;

use clap::{CommandFactory, Parser};

fn main() {
    let arguments = cli::Arguments::parse();
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
        eprintln!("error: chdir must not be empty");
        std::process::exit(2);
    }
    if arguments.invalid_empty_command() {
        eprintln!("error: command must not be empty");
        std::process::exit(2);
    }
    if arguments.invalid_empty_path() {
        eprintln!("error: path must not be empty");
        std::process::exit(2);
    }
}
