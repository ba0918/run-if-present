use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_run-if-present"))
}

fn temporary_directory() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("run-if-present-cli-{nonce}"));
    fs::create_dir(&path).unwrap();
    path
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
    let directory = temporary_directory();
    let output = binary()
        .current_dir(&directory)
        .args([
            "path",
            "-definitely-absent",
            "--",
            "/bin/printf",
            "must-not-run",
        ])
        .output()
        .unwrap();
    fs::remove_dir_all(directory).unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn path_keeps_known_wrapper_options_out_of_the_guard_value() {
    let help = binary().args(["path", "--help"]).output().unwrap();
    assert_eq!(help.status.code(), Some(0));
    assert!(!help.stdout.is_empty());
    assert!(help.stderr.is_empty());

    for guard in ["--chdir", "--version", "-h", "-V"] {
        let output = binary()
            .args(["path", guard, "--", "/bin/true"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{guard}");
        assert!(output.stdout.is_empty(), "{guard}");
        assert_eq!(
            output.stderr, b"run-if-present: syntax: invalid command line\n",
            "{guard}"
        );
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
fn rejects_all_short_help_and_version_spellings() {
    for arguments in [
        vec!["-h"],
        vec!["-V"],
        vec!["command", "-h"],
        vec!["command", "-V"],
        vec!["path", "-h"],
        vec!["path", "-V"],
    ] {
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
