use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_run-if-present"))
}

fn command_with_sigpipe_ignored(program: &str, arguments: &[&str]) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args([
        "-c",
        "trap '' PIPE; exec \"$@\"",
        "sigpipe-ignoring-parent",
        program,
    ]);
    command.args(arguments);
    command
}

fn permission_test_binary(temp: &TempDir) -> Command {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }

    if unsafe { geteuid() } != 0 {
        return binary();
    }

    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let executable = temp.path().join("run-if-present-permission-test");
    let binary_bytes = fs::read(env!("CARGO_BIN_EXE_run-if-present")).unwrap();
    let mut fixture_binary = fs::File::create(&executable).unwrap();
    fixture_binary.write_all(&binary_bytes).unwrap();
    fixture_binary.sync_all().unwrap();
    drop(fixture_binary);
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = Command::new(executable);
    command.gid(65_534).uid(65_534);
    command
}

fn assert_permission_diagnostic(output: &Output, operation: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8(output.stderr.clone()).unwrap();
    assert_eq!(diagnostic.lines().count(), 1);
    assert!(diagnostic.starts_with(&format!("run-if-present: {operation}:")));
    assert!(diagnostic.contains(&io::Error::from_raw_os_error(13).to_string()));
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "run-if-present-{}-{nonce}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
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

fn compile_c_program(temp: &TempDir, output: &Path, source: &[u8]) {
    let source_path = temp.path().join("fixture.c");
    fs::write(&source_path, source).unwrap();
    let compiler = Command::new("cc")
        .arg(&source_path)
        .arg("-o")
        .arg(output)
        .output()
        .unwrap();
    assert!(compiler.status.success(), "{:?}", compiler.stderr);
}

fn process_boundary_interposer(temp: &TempDir) -> PathBuf {
    let source = temp.path().join("process-boundary.c");
    let library = if cfg!(target_os = "macos") {
        temp.path().join("libprocess-boundary.dylib")
    } else {
        temp.path().join("libprocess-boundary.so")
    };
    let body = if cfg!(target_os = "macos") {
        r#"#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
__attribute__((constructor))
static void open_close_on_exec_descriptor(void) {
    if (getenv("RUN_IF_PRESENT_CLOEXEC_FD") != NULL) return;
    const char *path = getenv("RUN_IF_PRESENT_OPEN_CLOEXEC");
    if (path == NULL) return;
    int fd = open(path, O_RDONLY);
    if (fd < 0 || fcntl(fd, F_SETFD, FD_CLOEXEC) < 0) _exit(125);
    char value[32];
    if (snprintf(value, sizeof(value), "%d", fd) < 0 ||
        setenv("RUN_IF_PRESENT_CLOEXEC_FD", value, 1) < 0) _exit(125);
}
static int disappearing_execve(const char *path, char *const argv[], char *const envp[]) {
    const char *target = getenv("RUN_IF_PRESENT_DISAPPEAR");
    if (target != NULL && strcmp(path, target) == 0) unlink(path);
    return execve(path, argv, envp);
}
#define INTERPOSE(replacement, replacee) \
    __attribute__((used)) static struct { const void *replacement_ptr; const void *replacee_ptr; } \
    interpose_##replacee __attribute__((section("__DATA,__interpose"))) = \
    { (const void *)(unsigned long)&replacement, (const void *)(unsigned long)&replacee };
INTERPOSE(disappearing_execve, execve)
"#
    } else {
        r#"#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
__attribute__((constructor))
static void open_close_on_exec_descriptor(void) {
    if (getenv("RUN_IF_PRESENT_CLOEXEC_FD") != NULL) return;
    const char *path = getenv("RUN_IF_PRESENT_OPEN_CLOEXEC");
    if (path == NULL) return;
    int fd = open(path, O_RDONLY);
    if (fd < 0 || fcntl(fd, F_SETFD, FD_CLOEXEC) < 0) _exit(125);
    char value[32];
    if (snprintf(value, sizeof(value), "%d", fd) < 0 ||
        setenv("RUN_IF_PRESENT_CLOEXEC_FD", value, 1) < 0) _exit(125);
}
typedef int (*execve_function)(const char *, char *const[], char *const[]);
int execve(const char *path, char *const argv[], char *const envp[]) {
    const char *target = getenv("RUN_IF_PRESENT_DISAPPEAR");
    if (target != NULL && strcmp(path, target) == 0) unlink(path);
    execve_function real_execve = (execve_function)dlsym(RTLD_NEXT, "execve");
    return real_execve(path, argv, envp);
}
"#
    };
    fs::write(&source, body).unwrap();
    let mut compiler = Command::new("cc");
    if cfg!(target_os = "macos") {
        compiler.args(["-dynamiclib"]);
    } else {
        compiler.args(["-shared", "-fPIC"]);
    }
    let status = compiler
        .arg(&source)
        .arg("-o")
        .arg(&library)
        .status()
        .unwrap();
    assert!(status.success());
    library
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
fn a_permission_denied_path_guard_is_an_inspection_failure() {
    let temp = TempDir::new();
    let locked = temp.path().join("locked");
    fs::create_dir(&locked).unwrap();
    let guard = locked.join("guard");
    fs::write(&guard, b"").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let output = permission_test_binary(&temp)
        .arg("path")
        .arg(&guard)
        .args(["--", "/bin/false"])
        .output()
        .unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_permission_diagnostic(&output, "inspect");
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
fn an_unset_path_is_silent_success() {
    let output = binary()
        .env_remove("PATH")
        .args(["command", "not-present"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn an_explicit_path_ignores_a_matching_path_candidate() {
    let temp = TempDir::new();
    temp.executable("tool", b"#!/bin/sh\nprintf wrong");
    let output = binary()
        .env("PATH", temp.path())
        .args(["command", "./tool"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn a_shell_only_name_is_not_resolved() {
    let temp = TempDir::new();
    let output = binary()
        .env("PATH", temp.path())
        .args(["command", "cd"])
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
fn a_permission_denied_path_candidate_is_an_inspection_failure() {
    let temp = TempDir::new();
    let locked = temp.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("tool"), b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let output = permission_test_binary(&temp)
        .env("PATH", &locked)
        .args(["command", "tool"])
        .output()
        .unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_permission_diagnostic(&output, "inspect executable");
}

#[test]
fn an_executable_disappearing_after_resolution_exits_127() {
    let temp = TempDir::new();
    let program = temp.executable("disappears", b"#!/bin/sh\nexit 0\n");
    let interposer = process_boundary_interposer(&temp);
    let mut command = binary();
    command
        .env("RUN_IF_PRESENT_DISAPPEAR", &program)
        .arg("command")
        .arg(&program);
    if cfg!(target_os = "macos") {
        command.env("DYLD_INSERT_LIBRARIES", interposer);
    } else {
        command.env("LD_PRELOAD", interposer);
    }

    let output = command.output().unwrap();

    assert_eq!(output.status.code(), Some(127));
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(diagnostic.starts_with("run-if-present: execute:"));
    assert_eq!(diagnostic.lines().count(), 1);
    assert!(diagnostic.ends_with('\n'));
    assert!(!program.exists());
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
fn search_finds_a_usable_candidate_after_an_inspection_failure() {
    let first = TempDir::new();
    let second = TempDir::new();
    let not_a_directory = first.path().join("not-a-directory");
    fs::write(&not_a_directory, b"").unwrap();
    second.executable("tool", b"#!/bin/sh\nprintf usable");
    let path = std::env::join_paths([&not_a_directory, second.path()]).unwrap();
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
fn a_permission_denied_chdir_is_visible() {
    let temp = TempDir::new();
    let locked = temp.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let output = permission_test_binary(&temp)
        .arg("--chdir")
        .arg(&locked)
        .args(["command", "/bin/true"])
        .output()
        .unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_permission_diagnostic(&output, "chdir");
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
fn child_sigpipe_disposition_matches_direct_execution() {
    let script = "kill -PIPE $$; printf survived";
    let direct = Command::new("/bin/sh")
        .args(["-c", script])
        .output()
        .unwrap();
    let wrapped = binary()
        .args(["path", "/bin", "--", "/bin/sh", "-c", script])
        .output()
        .unwrap();

    assert_eq!(direct.status.signal(), Some(13));
    assert!(direct.stdout.is_empty());
    assert_eq!(wrapped.status.signal(), Some(13));
    assert!(wrapped.stdout.is_empty());
}

#[test]
fn child_inherits_an_explicitly_ignored_sigpipe_like_direct_execution() {
    let script = "kill -PIPE $$; printf ignored";
    let direct = command_with_sigpipe_ignored("/bin/sh", &["-c", script])
        .output()
        .unwrap();
    let wrapped = command_with_sigpipe_ignored(
        env!("CARGO_BIN_EXE_run-if-present"),
        &["path", "/bin", "--", "/bin/sh", "-c", script],
    )
    .output()
    .unwrap();

    assert_eq!(direct.status.code(), Some(0));
    assert_eq!(direct.stdout, b"ignored");
    assert_eq!(wrapped.status.code(), Some(0));
    assert_eq!(wrapped.stdout, b"ignored");
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
fn repeated_slashes_in_a_tilde_guard_do_not_escape_home() {
    assert!(Path::new("/etc").exists());
    let home = TempDir::new();

    let output = binary()
        .env("HOME", home.path())
        .args(["path", "~//etc", "--", "/bin/printf", "escaped"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn repeated_slashes_in_a_tilde_chdir_stay_within_home() {
    let home = TempDir::new();
    let name = home.path().file_name().unwrap();
    let directory = home.path().join(name);
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("guard"), b"").unwrap();
    let mut chdir = OsString::from("~//");
    chdir.push(name);

    let output = binary()
        .env("HOME", home.path())
        .arg("--chdir")
        .arg(chdir)
        .args(["path", "guard", "--", "/bin/pwd"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        directory.to_string_lossy()
    );
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
fn explicit_launches_keep_the_caller_supplied_path_as_argv0() {
    let temp = TempDir::new();
    let token = "./explicit-argv0-reporter";
    let reporter = temp.path().join("explicit-argv0-reporter");
    compile_c_program(
        &temp,
        &reporter,
        br#"#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc < 1) return 2;
    size_t length = strlen(argv[0]);
    return write(STDOUT_FILENO, argv[0], length) == (ssize_t)length ? 0 : 3;
}
"#,
    );

    let direct = Command::new(token)
        .current_dir(temp.path())
        .output()
        .unwrap();
    let command_mode = binary()
        .current_dir(temp.path())
        .args(["command", token])
        .output()
        .unwrap();
    let path_mode = binary()
        .current_dir(temp.path())
        .args(["path", ".", "--", token])
        .output()
        .unwrap();

    assert_eq!(direct.status.code(), Some(0));
    assert_eq!(direct.stdout, token.as_bytes());
    assert_eq!(command_mode.status.code(), Some(0));
    assert_eq!(command_mode.stdout, direct.stdout);
    assert_eq!(path_mode.status.code(), Some(0));
    assert_eq!(path_mode.stdout, direct.stdout);
}

#[test]
fn bare_launches_keep_the_same_argv0_as_direct_path_invocation() {
    let temp = TempDir::new();
    let name = "argv0-reporter";
    let reporter = temp.path().join(name);
    compile_c_program(
        &temp,
        &reporter,
        br#"#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc < 1) return 2;
    size_t length = strlen(argv[0]);
    return write(STDOUT_FILENO, argv[0], length) == (ssize_t)length ? 0 : 3;
}
"#,
    );

    let direct = Command::new(name)
        .env("PATH", temp.path())
        .output()
        .unwrap();
    let command_mode = binary()
        .env("PATH", temp.path())
        .args(["command", name])
        .output()
        .unwrap();
    let path_mode = binary()
        .env("PATH", temp.path())
        .args(["path", "/bin", "--", name])
        .output()
        .unwrap();

    assert_eq!(direct.status.code(), Some(0));
    assert_eq!(direct.stdout, name.as_bytes());
    assert_eq!(command_mode.status.code(), Some(0));
    assert_eq!(command_mode.stdout, direct.stdout);
    assert_eq!(path_mode.status.code(), Some(0));
    assert_eq!(path_mode.stdout, direct.stdout);
}

#[test]
fn a_non_utf8_bare_command_is_preserved_as_argv0() {
    let temp = TempDir::new();
    let name = OsString::from_vec(b"argv0-\xff".to_vec());
    let reporter = temp.path().join(&name);
    compile_c_program(
        &temp,
        &reporter,
        br#"#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc < 1) return 2;
    size_t length = strlen(argv[0]);
    return write(STDOUT_FILENO, argv[0], length) == (ssize_t)length ? 0 : 3;
}
"#,
    );

    let direct = Command::new(&name)
        .env("PATH", temp.path())
        .output()
        .unwrap();
    let command_mode = binary()
        .env("PATH", temp.path())
        .arg("command")
        .arg(&name)
        .output()
        .unwrap();
    let path_mode = binary()
        .env("PATH", temp.path())
        .args(["path", "/bin", "--"])
        .arg(&name)
        .output()
        .unwrap();

    assert_eq!(direct.status.code(), Some(0));
    assert_eq!(direct.stdout, name.as_encoded_bytes());
    assert_eq!(command_mode.status.code(), Some(0));
    assert_eq!(command_mode.stdout, direct.stdout);
    assert_eq!(path_mode.status.code(), Some(0));
    assert_eq!(path_mode.stdout, direct.stdout);
}

#[test]
fn child_arguments_follow_argv0_in_original_order_and_raw_bytes() {
    let temp = TempDir::new();
    let name = "raw-argv-reporter";
    let reporter = temp.path().join(name);
    compile_c_program(
        &temp,
        &reporter,
        br#"#include <stdio.h>
#include <string.h>
#include <unistd.h>
static int emit(const char *bytes, size_t length) {
    while (length > 0) {
        ssize_t written = write(STDOUT_FILENO, bytes, length);
        if (written <= 0) return 1;
        bytes += written;
        length -= (size_t)written;
    }
    return 0;
}
int main(int argc, char **argv) {
    for (int index = 0; index < argc; index++) {
        size_t length = strlen(argv[index]);
        char prefix[64];
        int prefix_length = snprintf(prefix, sizeof(prefix), "%zu:", length);
        if (prefix_length < 0 || emit(prefix, (size_t)prefix_length) ||
            emit(argv[index], length) || emit("\n", 1)) return 3;
    }
    return 0;
}
"#,
    );
    let children = [
        OsString::from("ascii"),
        OsString::new(),
        OsString::from_vec(b"z\xffq".to_vec()),
    ];

    let direct = Command::new(name)
        .env("PATH", temp.path())
        .args(&children)
        .output()
        .unwrap();
    let command_mode = binary()
        .env("PATH", temp.path())
        .args(["command", name])
        .args(&children)
        .output()
        .unwrap();
    let path_mode = binary()
        .env("PATH", temp.path())
        .args(["path", "/bin", "--", name])
        .args(&children)
        .output()
        .unwrap();

    let expected = b"17:raw-argv-reporter\n5:ascii\n0:\n3:z\xffq\n";
    assert_eq!(direct.status.code(), Some(0));
    assert_eq!(direct.stdout, expected);
    assert_eq!(command_mode.status.code(), Some(0));
    assert_eq!(command_mode.stdout, direct.stdout);
    assert_eq!(path_mode.status.code(), Some(0));
    assert_eq!(path_mode.stdout, direct.stdout);
    assert!(direct.stderr.is_empty());
    assert!(command_mode.stderr.is_empty());
    assert!(path_mode.stderr.is_empty());
}

#[test]
fn an_accessible_candidate_discovered_by_which_runs() {
    let temp = TempDir::new();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let interpreter = temp.path().join("script-path-reporter");
    compile_c_program(
        &temp,
        &interpreter,
        br#"#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc < 2) return 2;
    size_t length = strlen(argv[1]);
    return write(STDOUT_FILENO, argv[1], length) == (ssize_t)length ? 0 : 3;
}
"#,
    );
    let tool = bin.join("provider-tool");
    fs::write(
        &tool,
        format!("#!{}\n", interpreter.to_string_lossy()).as_bytes(),
    )
    .unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();

    let output = binary()
        .env("PATH", "./bin")
        .arg("--chdir")
        .arg(temp.path())
        .args(["command", "provider-tool"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, tool.as_os_str().as_encoded_bytes());
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
fn a_tilde_path_entry_is_literal_and_relative_to_the_effective_directory() {
    let effective = TempDir::new();
    let home = TempDir::new();
    let effective_bin = effective.path().join("~/bin");
    let home_bin = home.path().join("bin");
    fs::create_dir_all(&effective_bin).unwrap();
    fs::create_dir(&home_bin).unwrap();
    let effective_tool = effective_bin.join("tool");
    let home_tool = home_bin.join("tool");
    fs::write(&effective_tool, b"#!/bin/sh\nprintf effective").unwrap();
    fs::write(&home_tool, b"#!/bin/sh\nprintf home").unwrap();
    fs::set_permissions(&effective_tool, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&home_tool, fs::Permissions::from_mode(0o755)).unwrap();

    let output = binary()
        .env("HOME", home.path())
        .env("PATH", "~/bin")
        .arg("--chdir")
        .arg(effective.path())
        .args(["command", "tool"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"effective");
    assert!(output.stderr.is_empty());
}

#[test]
fn the_first_regular_file_with_any_execute_bit_is_not_bypassed() {
    let first = TempDir::new();
    let second = TempDir::new();
    let first_tool = first.path().join("tool");
    fs::write(&first_tool, b"not an executable format").unwrap();
    fs::set_permissions(&first_tool, fs::Permissions::from_mode(0o010)).unwrap();
    second.executable("tool", b"#!/bin/sh\nprintf second");
    let path = std::env::join_paths([first.path(), second.path()]).unwrap();

    let output = binary()
        .env("PATH", path)
        .args(["command", "tool"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(126));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("run-if-present: execute:"));
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
fn raw_environment_entries_are_preserved_in_order_without_reconstruction() {
    let temp = TempDir::new();
    let observer = temp.path().join("environment-observer");
    compile_c_program(
        &temp,
        &observer,
        br#"#include <stdio.h>
#include <string.h>
#include <unistd.h>
extern char **environ;
static int emit(const char *bytes, size_t length) {
    while (length > 0) {
        ssize_t written = write(STDOUT_FILENO, bytes, length);
        if (written <= 0) return 1;
        bytes += written;
        length -= (size_t)written;
    }
    return 0;
}
int main(void) {
    for (char **entry = environ; *entry != NULL; entry++) {
        size_t length = strlen(*entry);
        char prefix[64];
        int prefix_length = snprintf(prefix, sizeof(prefix), "%zu:", length);
        if (prefix_length < 0 || emit(prefix, (size_t)prefix_length) ||
            emit(*entry, length) || emit("\n", 1)) return 3;
    }
    return 0;
}
"#,
    );
    let launcher = temp.path().join("environment-launcher");
    compile_c_program(
        &temp,
        &launcher,
        br#"#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc != 5) return 2;
    size_t home_size = strlen(argv[3]) + 6;
    char *home = malloc(home_size);
    if (home == NULL || snprintf(home, home_size, "HOME=%s", argv[3]) < 0) return 3;
    char raw_no_equals[] = "NO_EQUALS\xff";
    char *environment[] = {
        "FIRST=1",
        home,
        raw_no_equals,
        "HOME=/duplicate",
        "LAST=1",
        NULL
    };
    char *direct[] = { argv[1], NULL };
    char *wrapped[] = { argv[2], "path", "~", "--", argv[1], NULL };
    if (strcmp(argv[4], "direct") == 0) execve(argv[1], direct, environment);
    else execve(argv[2], wrapped, environment);
    return 4;
}
"#,
    );

    let launch = |mode: &str| {
        Command::new(&launcher)
            .arg(&observer)
            .arg(env!("CARGO_BIN_EXE_run-if-present"))
            .arg(temp.path())
            .arg(mode)
            .output()
            .unwrap()
    };
    let direct = launch("direct");
    let wrapped = launch("wrapped");

    let mut expected = Vec::new();
    let entries = [
        b"FIRST=1".to_vec(),
        [
            b"HOME=".as_slice(),
            temp.path().as_os_str().as_encoded_bytes(),
        ]
        .concat(),
        b"NO_EQUALS\xff".to_vec(),
        b"HOME=/duplicate".to_vec(),
        b"LAST=1".to_vec(),
    ];
    for entry in &entries {
        expected.extend_from_slice(entry.len().to_string().as_bytes());
        expected.push(b':');
        expected.extend_from_slice(entry);
        expected.push(b'\n');
    }
    assert_eq!(direct.status.code(), Some(0));
    assert_eq!(direct.stdout, expected);
    assert_eq!(wrapped.status.code(), Some(0));
    assert_eq!(wrapped.stdout, direct.stdout);
    assert!(direct.stderr.is_empty());
    assert!(wrapped.stderr.is_empty());
}

#[test]
fn child_stderr_is_preserved() {
    let output = binary()
        .args([
            "path",
            "/bin",
            "--",
            "/bin/sh",
            "-c",
            "printf child-error >&2",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"child-error");
}

#[test]
fn real_and_effective_credentials_are_preserved() {
    for arguments in [["-u"], ["-ru"]] {
        let expected = Command::new("id").args(arguments).output().unwrap();
        let actual = binary()
            .args(["path", "/bin", "--", "id"])
            .args(arguments)
            .output()
            .unwrap();
        assert_eq!(actual.status.code(), Some(0));
        assert_eq!(actual.stdout, expected.stdout);
        assert!(actual.stderr.is_empty());
    }
}

#[test]
fn resource_limits_are_preserved() {
    let expected = Command::new("/bin/sh")
        .args(["-c", "ulimit -n"])
        .output()
        .unwrap();
    let actual = binary()
        .args(["path", "/bin", "--", "/bin/sh", "-c", "ulimit -n"])
        .output()
        .unwrap();
    assert_eq!(actual.status.code(), Some(0));
    assert_eq!(actual.stdout, expected.stdout);
    assert!(actual.stderr.is_empty());
}

#[test]
fn a_close_on_exec_descriptor_is_closed_before_the_child() {
    let temp = TempDir::new();
    let interposer = process_boundary_interposer(&temp);
    let mut command = binary();
    command
        .env(
            "RUN_IF_PRESENT_OPEN_CLOEXEC",
            fs::canonicalize("Cargo.toml").unwrap(),
        )
        .args([
            "path",
            "/bin",
            "--",
            "/bin/sh",
            "-c",
            "test -n \"$RUN_IF_PRESENT_CLOEXEC_FD\" && test ! -e \"/dev/fd/$RUN_IF_PRESENT_CLOEXEC_FD\"",
        ]);
    if cfg!(target_os = "macos") {
        command.env("DYLD_INSERT_LIBRARIES", interposer);
    } else {
        command.env("LD_PRELOAD", interposer);
    }

    let output = command.output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn a_non_utf8_diagnostic_escapes_control_bytes_and_includes_the_os_error() {
    let operand = OsString::from_vec(vec![b'/', b'm', b'i', b's', b's', b'\t', 0xff]);
    let output = binary()
        .env("LC_ALL", "C")
        .args(["path", "/bin", "--"])
        .arg(operand)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(127));
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert_eq!(diagnostic.lines().count(), 1);
    assert!(diagnostic.contains("\\t"));
    assert!(diagnostic.contains("\\xff"));
    assert!(diagnostic.contains("No such file or directory"));
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
        .args(["path", "~", "--", "/bin/printf", "database-home"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"database-home");
    assert!(output.stderr.is_empty());
}

#[test]
fn an_empty_home_uses_the_operating_system_user_database() {
    let output = binary()
        .env("HOME", "")
        .args(["path", "~", "--", "/bin/printf", "database-home"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"database-home");
    assert!(output.stderr.is_empty());
}
