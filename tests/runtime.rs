use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_run-if-present"))
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("run-if-present-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn executable(&self, name: &str, body: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_present_path_runs_the_command_and_preserves_its_output() {
    let output = binary()
        .args(["path", "/bin", "--", "/bin/printf", "present"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"present");
    assert!(output.stderr.is_empty());
}

#[test]
fn an_absent_path_is_silent_success() {
    let temp = TempDir::new();
    let output = binary()
        .arg("path")
        .arg(temp.path().join("absent"))
        .args(["--", "/bin/false"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn a_dangling_link_is_silent_success() {
    let temp = TempDir::new();
    let guard = temp.path().join("guard");
    symlink(temp.path().join("missing"), &guard).unwrap();

    let output = binary()
        .arg("path")
        .arg(guard)
        .args(["--", "/bin/false"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn a_link_loop_is_an_inspection_failure() {
    let temp = TempDir::new();
    let guard = temp.path().join("guard");
    symlink(&guard, &guard).unwrap();

    let output = binary()
        .arg("path")
        .arg(&guard)
        .args(["--", "/bin/false"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("run-if-present: inspect:"));
}

#[test]
fn a_present_guard_does_not_hide_a_missing_launch_target() {
    let output = binary()
        .args(["path", "/bin", "--", "/definitely/not/present"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(127));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("run-if-present: execute:"));
}

#[test]
fn an_absent_command_is_silent_success() {
    let output = binary()
        .env("PATH", "")
        .args(["command", "not-present"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn an_unusable_command_exits_126() {
    let temp = TempDir::new();
    fs::write(temp.path().join("tool"), b"not executable").unwrap();

    let output = binary()
        .env("PATH", temp.path())
        .args(["command", "tool"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(126));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn an_uninspectable_search_location_exits_1() {
    let temp = TempDir::new();
    let not_a_directory = temp.path().join("not-a-directory");
    fs::write(&not_a_directory, b"").unwrap();

    let output = binary()
        .env("PATH", not_a_directory)
        .args(["command", "tool"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).starts_with("run-if-present: inspect executable:")
    );
}

#[test]
fn search_continues_past_an_unusable_candidate() {
    let first = TempDir::new();
    let second = TempDir::new();
    fs::write(first.path().join("tool"), b"not executable").unwrap();
    second.executable("tool", b"#!/bin/sh\nprintf usable");
    let path = std::env::join_paths([first.path(), second.path()]).unwrap();

    let output = binary()
        .env("PATH", path)
        .args(["command", "tool"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"usable");
    assert!(output.stderr.is_empty());
}

#[test]
fn relative_guards_are_evaluated_after_chdir() {
    let temp = TempDir::new();
    fs::write(temp.path().join("guard"), b"").unwrap();

    let output = binary()
        .arg("--chdir")
        .arg(temp.path())
        .args(["path", "guard", "--", "/bin/pwd"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        temp.path().to_string_lossy()
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn a_missing_chdir_is_silent_success() {
    let temp = TempDir::new();
    let output = binary()
        .arg("--chdir")
        .arg(temp.path().join("absent"))
        .args(["command", "/bin/false"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn a_non_directory_chdir_is_visible() {
    let temp = TempDir::new();
    let path = temp.path().join("file");
    fs::write(&path, b"").unwrap();

    let output = binary()
        .arg("--chdir")
        .arg(&path)
        .args(["command", "/bin/true"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("run-if-present: chdir:"));
}

#[test]
fn a_missing_script_interpreter_exits_127() {
    let temp = TempDir::new();
    let script = temp.executable("script", b"#!/definitely/missing/interpreter\n");

    let output = binary().arg("command").arg(script).output().unwrap();

    assert_eq!(output.status.code(), Some(127));
    assert!(!output.stderr.is_empty());
}

#[test]
fn an_executable_format_failure_exits_126() {
    let temp = TempDir::new();
    let program = temp.executable("program", b"not an executable format");

    let output = binary().arg("command").arg(program).output().unwrap();

    assert_eq!(output.status.code(), Some(126));
    assert!(!output.stderr.is_empty());
}

#[test]
fn child_exit_status_is_preserved() {
    let output = binary()
        .args(["path", "/bin", "--", "/bin/sh", "-c", "exit 42"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn child_signal_is_preserved() {
    let status = binary()
        .args(["path", "/bin", "--", "/bin/sh", "-c", "kill -TERM $$"])
        .status()
        .unwrap();

    assert_eq!(status.signal(), Some(15));
}

#[test]
fn stdin_is_preserved() {
    let mut child = binary()
        .args(["path", "/bin", "--", "/bin/cat"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"stream").unwrap();
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.stdout, b"stream");
}

#[test]
fn non_utf8_child_arguments_are_preserved() {
    let argument = OsString::from_vec(vec![b'a', 0xff, b'z']);
    let output = binary()
        .args(["path", "/bin", "--", "/bin/printf", "%s"])
        .arg(&argument)
        .output()
        .unwrap();

    assert_eq!(output.stdout, argument.as_encoded_bytes());
}

#[test]
fn tilde_guards_use_non_empty_home() {
    let temp = TempDir::new();
    fs::write(temp.path().join("guard"), b"").unwrap();

    let output = binary()
        .env("HOME", temp.path())
        .args(["path", "~/guard", "--", "/bin/printf", "expanded"])
        .output()
        .unwrap();

    assert_eq!(output.stdout, b"expanded");
    assert!(output.stderr.is_empty());
}

#[test]
fn diagnostic_operands_cannot_forge_a_new_line() {
    let program = OsString::from("/missing\nforged");
    let output = binary()
        .args(["path", "/bin", "--"])
        .arg(program)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(127));
    assert_eq!(
        output.stderr.iter().filter(|byte| **byte == b'\n').count(),
        1
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("\\nforged"));
    assert!(!output.stderr.contains(&0x1b));
}

#[test]
fn a_present_guard_resolves_a_bare_launch_target_from_path() {
    let output = binary()
        .env("PATH", "/bin:/usr/bin")
        .args(["path", "/bin", "--", "printf", "resolved"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"resolved");
    assert!(output.stderr.is_empty());
}

#[test]
fn relative_path_entries_use_the_effective_directory() {
    let temp = TempDir::new();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let tool = bin.join("tool");
    fs::write(&tool, b"#!/bin/sh\nprintf relative").unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();

    let output = binary()
        .env("PATH", "bin")
        .arg("--chdir")
        .arg(temp.path())
        .args(["command", "tool"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"relative");
    assert!(output.stderr.is_empty());
}

#[test]
fn environment_is_preserved() {
    let output = binary()
        .env("RUN_IF_PRESENT_TEST_VALUE", "preserved")
        .args([
            "path",
            "/bin",
            "--",
            "/bin/sh",
            "-c",
            "printf %s \"$RUN_IF_PRESENT_TEST_VALUE\"",
        ])
        .output()
        .unwrap();

    assert_eq!(output.stdout, b"preserved");
    assert!(output.stderr.is_empty());
}

#[test]
fn an_empty_child_argument_reaches_the_child() {
    let output = binary()
        .args(["path", "/bin", "--", "/bin/printf", "[%s]", ""])
        .output()
        .unwrap();

    assert_eq!(output.stdout, b"[]");
    assert!(output.stderr.is_empty());
}

#[test]
fn non_utf8_home_is_used_without_lossy_conversion() {
    let temp = TempDir::new();
    let home = temp.path().join(OsString::from_vec(vec![b'h', 0xff]));
    fs::create_dir(&home).unwrap();
    fs::write(home.join("guard"), b"").unwrap();

    let output = binary()
        .env("HOME", &home)
        .args(["path", "~/guard", "--", "/bin/printf", "bytes"])
        .output()
        .unwrap();

    assert_eq!(output.stdout, b"bytes");
    assert!(output.stderr.is_empty());
}

#[test]
fn an_unset_home_uses_the_operating_system_user_database() {
    let output = binary()
        .env_remove("HOME")
        .args(["path", "~", "--", "/bin/true"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn an_empty_home_uses_the_operating_system_user_database() {
    let output = binary()
        .env("HOME", "")
        .args(["path", "~", "--", "/bin/true"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
