use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;
use std::process::Stdio;

mod common;

use common::{closed_pipe_writer, TempDir};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_run-if-present"))
}

fn output_with_closed_stdout(arguments: &[&str]) -> std::process::Output {
    binary()
        .args(arguments)
        .stdout(Stdio::from(closed_pipe_writer()))
        .output()
        .unwrap()
}

#[test]
fn rejects_an_empty_wrapper_command() {
    let output = binary().args(["command", ""]).output().unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "run-if-present: syntax: command must not be empty\n"
    );
}

#[test]
fn rejects_an_empty_path_guard() {
    let output = binary().args(["path", "", "--", "true"]).output().unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "run-if-present: syntax: path must not be empty\n"
    );
}

#[test]
fn accepts_a_hyphen_leading_path_guard_as_a_filesystem_value() {
    let directory = TempDir::new();
    let output = binary()
        .current_dir(directory.path())
        .args([
            "path",
            "-definitely-absent",
            "--",
            "/bin/printf",
            "must-not-run",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn path_evaluates_hyphen_leading_guard_names_after_the_subcommand() {
    let directory = TempDir::new();

    for guard in ["--chdir", "--version", "-h", "-V"] {
        fs::write(directory.path().join(guard), b"present").unwrap();
        let output = binary()
            .current_dir(directory.path())
            .args([
                "path",
                guard,
                "--",
                "/bin/sh",
                "-c",
                "printf '%s' \"$1\"",
                "guard-child",
                guard,
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{guard}");
        assert_eq!(output.stdout, guard.as_bytes(), "{guard}");
        assert!(output.stderr.is_empty(), "{guard}");
    }
}

#[test]
fn rejects_an_empty_chdir_value() {
    let output = binary()
        .args(["--chdir", "", "command", "true"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "run-if-present: syntax: chdir must not be empty\n"
    );
}

#[test]
fn requires_the_path_separator() {
    let output = binary().args(["path", "guard", "true"]).output().unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostic.starts_with("run-if-present: syntax:"));
    assert_eq!(diagnostic.lines().count(), 1);
    assert!(!diagnostic.contains('\u{1b}'));
}

#[test]
fn rejects_an_empty_path_launch_command_as_invalid_syntax() {
    let output = binary().args(["path", "/bin", "--", ""]).output().unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "run-if-present: syntax: command must not be empty\n"
    );
}

#[test]
fn writes_help_to_stdout() {
    let output = binary().arg("--help").output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(!output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("command"));
    assert!(help.contains("path"));
    assert!(!help
        .lines()
        .any(|line| line.trim_start().starts_with("help")));
}

#[test]
fn rejects_the_unapproved_help_subcommand() {
    let output = binary().arg("help").output().unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostic.starts_with("run-if-present: syntax:"));
    assert_eq!(diagnostic.lines().count(), 1);
}

#[test]
fn writes_version_to_stdout() {
    let output = binary().arg("--version").output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "run-if-present 0.1.0\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn writes_command_help_to_stdout_before_a_command_begins() {
    let output = binary().args(["command", "--help"]).output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
    assert!(output.stderr.is_empty());
}

#[test]
fn writes_path_help_to_stdout_before_a_guard_begins() {
    let output = binary().args(["path", "--help"]).output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
    assert!(output.stderr.is_empty());
}

#[test]
fn rejects_tokens_after_command_help() {
    let output = binary()
        .args(["command", "--help", "extra"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"run-if-present: syntax: invalid command line\n"
    );
}

#[test]
fn rejects_tokens_after_path_help() {
    let output = binary()
        .args(["path", "--help", "extra", "--", "true"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"run-if-present: syntax: invalid command line\n"
    );
}

#[test]
fn help_and_version_ignore_a_closed_stdout() {
    for arguments in [
        &["--help"][..],
        &["--version"][..],
        &["command", "--help"][..],
        &["path", "--help"][..],
    ] {
        let output = output_with_closed_stdout(arguments);
        assert_eq!(output.status.code(), Some(0), "{arguments:?}");
        assert!(output.stderr.is_empty(), "{arguments:?}");
    }
}

#[test]
fn help_precedes_empty_chdir_validation() {
    for arguments in [
        &["--chdir", "", "command", "--help"][..],
        &["--chdir", "", "path", "--help"][..],
    ] {
        let output = binary().args(arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{arguments:?}");
        assert!(!output.stdout.is_empty(), "{arguments:?}");
        assert!(output.stderr.is_empty(), "{arguments:?}");
    }
}

#[test]
fn long_wrapper_option_names_are_operands_after_a_subcommand() {
    let directory = TempDir::new();
    for name in ["--version", "--chdir"] {
        directory.executable(name, b"#!/bin/sh\nexit 0\n");
        let output = binary()
            .env("PATH", directory.path())
            .args(["command", name])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{name}");
        assert!(output.stderr.is_empty(), "{name}");
    }
}

#[test]
fn rejects_top_level_short_help_and_version_spellings() {
    for arguments in [vec!["-h"], vec!["-V"]] {
        let output = binary().args(&arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(
            diagnostic.starts_with("run-if-present: syntax:"),
            "{arguments:?}"
        );
        assert_eq!(diagnostic.lines().count(), 1, "{arguments:?}");
    }
}

#[test]
fn command_resolves_short_hyphen_executable_names_after_the_subcommand() {
    let directory = TempDir::new();
    for name in ["-h", "-V"] {
        directory.executable(
            name,
            format!("#!/bin/sh\nprintf '%s' '{name}'\n").as_bytes(),
        );

        let output = binary()
            .env("PATH", directory.path())
            .args(["command", name])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{name}");
        assert_eq!(output.stdout, name.as_bytes(), "{name}");
        assert!(output.stderr.is_empty(), "{name}");
    }
}

#[test]
fn parser_syntax_errors_do_not_render_untrusted_operand_bytes() {
    let dangerous = OsString::from_vec(b"operand\t\xff\rforged".to_vec());
    let cases = [
        vec![dangerous.clone()],
        vec![OsString::from("path"), dangerous.clone()],
        vec![
            OsString::from("--chdir"),
            dangerous,
            OsString::from("unknown"),
        ],
    ];

    for arguments in cases {
        let output = binary().args(&arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        assert_eq!(
            output.stderr, b"run-if-present: syntax: invalid command line\n",
            "{arguments:?}"
        );
        assert!(!output.stderr.contains(&b'\r'), "{arguments:?}");
        assert!(!output.stderr.contains(&b'\t'), "{arguments:?}");
        assert!(
            !output
                .stderr
                .windows(3)
                .any(|bytes| bytes == [0xef, 0xbf, 0xbd]),
            "{arguments:?}"
        );
    }
}
