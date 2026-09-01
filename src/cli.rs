use std::ffi::OsString;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "run-if-present", version, color = clap::ColorChoice::Never)]
pub struct Arguments {
    #[arg(long, value_name = "DIR")]
    pub chdir: Option<OsString>,

    #[command(subcommand)]
    pub condition: Condition,
}

#[derive(Debug, Subcommand)]
pub enum Condition {
    #[command(disable_help_flag = true)]
    Command {
        #[arg(allow_hyphen_values = true)]
        command: OsString,

        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
    Path {
        path: OsString,

        #[arg(last = true, required = true, num_args = 1.., allow_hyphen_values = true)]
        command: Vec<OsString>,
    },
}

impl Arguments {
    pub fn invalid_empty_command(&self) -> bool {
        matches!(
            &self.condition,
            Condition::Command { command, .. } if command.is_empty()
        )
    }

    pub fn invalid_empty_path(&self) -> bool {
        matches!(&self.condition, Condition::Path { path, .. } if path.is_empty())
    }

    pub fn invalid_empty_chdir(&self) -> bool {
        self.chdir.as_ref().is_some_and(|path| path.is_empty())
    }

    pub fn command_help_requested(&self) -> bool {
        matches!(
            &self.condition,
            Condition::Command { command, arguments }
                if arguments.is_empty() && (command == "--help" || command == "-h")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_a_child_option_after_command() {
        let parsed = Arguments::try_parse_from(["run-if-present", "command", "printf", "--help"])
            .unwrap();

        match parsed.condition {
            Condition::Command {
                command,
                arguments,
            } => {
                assert_eq!(command, "printf");
                assert_eq!(arguments, ["--help"]);
            }
            Condition::Path { .. } => panic!("parsed the wrong condition"),
        }
    }

    #[test]
    fn preserves_an_empty_child_argument() {
        let parsed =
            Arguments::try_parse_from(["run-if-present", "command", "printf", ""]).unwrap();

        match parsed.condition {
            Condition::Command { arguments, .. } => assert_eq!(arguments, [""]),
            Condition::Path { .. } => panic!("parsed the wrong condition"),
        }
    }
}
