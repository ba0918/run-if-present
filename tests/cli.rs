use std::process::Command;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_run-if-present"))
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
    assert!(!help.lines().any(|line| line.trim_start().starts_with("help")));
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
