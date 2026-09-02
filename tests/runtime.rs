use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod common;

use common::{
    assert_diagnostic, assert_operand_diagnostic, assert_silent_success, assert_success_output,
    binary, closed_pipe_writer, non_utf8_entry, output_report, run, TempDir,
};

fn status_with_closed_stderr(mut command: Command) -> std::process::ExitStatus {
    command
        .stderr(Stdio::from(closed_pipe_writer()))
        .status()
        .unwrap()
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
    let diagnostic = assert_diagnostic(output, 1, operation);
    assert!(diagnostic.contains(&io::Error::from_raw_os_error(13).to_string()));
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

// Writes its own argv[0] to stdout so the caller can compare it with a direct invocation.
fn compile_argv0_reporter(temp: &TempDir, output: &Path) {
    compile_c_program(
        temp,
        output,
        br#"#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc < 1) return 2;
    size_t length = strlen(argv[0]);
    return write(STDOUT_FILENO, argv[0], length) == (ssize_t)length ? 0 : 3;
}
"#,
    );
}

fn compile_closed_descriptor_observer(temp: &TempDir, output: &Path) {
    compile_c_program(
        temp,
        output,
        br#"#include <errno.h>
#include <fcntl.h>
#include <stdlib.h>
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    int descriptor = atoi(argv[1]);
    errno = 0;
    return fcntl(descriptor, F_GETFD) == -1 && errno == EBADF ? 0 : 3;
}
"#,
    );
}

fn close_descriptor_before_exec(command: &mut Command, descriptor: i32) {
    unsafe {
        command.pre_exec(move || {
            unsafe extern "C" {
                fn close(descriptor: i32) -> i32;
            }
            if close(descriptor) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

// The loader variable that injects a fixture library into the wrapper would otherwise pass
// through the wrapper's unchanged environment into the child it execs; on macOS dyld aborts a
// system binary whose inserted dylib it cannot load. Each fixture removes the variable once it
// is loaded, so the wrapper's pass-through of that one variable is not observed by any test.
const UNSET_INJECTION_VARIABLE: &str = r#"
__attribute__((constructor))
static void unset_injection_variable(void) {
#ifdef __APPLE__
    unsetenv("DYLD_INSERT_LIBRARIES");
#else
    unsetenv("LD_PRELOAD");
#endif
}
"#;

fn process_boundary_interposer(temp: &TempDir) -> PathBuf {
    let source = temp.path().join("process-boundary.c");
    let library = if cfg!(target_os = "macos") {
        temp.path().join("libprocess-boundary.dylib")
    } else {
        temp.path().join("libprocess-boundary.so")
    };
    const OPEN_CLOEXEC_DESCRIPTOR: &str = r#"
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
static void unlink_disappearing_target(const char *path) {
    const char *target = getenv("RUN_IF_PRESENT_DISAPPEAR");
    if (target != NULL && strcmp(path, target) == 0) unlink(path);
}
"#;
    let body = if cfg!(target_os = "macos") {
        format!(
            r#"#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
{UNSET_INJECTION_VARIABLE}{OPEN_CLOEXEC_DESCRIPTOR}
static int disappearing_execv(const char *path, char *const argv[]) {{
    unlink_disappearing_target(path);
    return execv(path, argv);
}}
#define INTERPOSE(replacement, replacee) \
    __attribute__((used)) static struct {{ const void *replacement_ptr; const void *replacee_ptr; }} \
    interpose_##replacee __attribute__((section("__DATA,__interpose"))) = \
    {{ (const void *)(unsigned long)&replacement, (const void *)(unsigned long)&replacee }};
INTERPOSE(disappearing_execv, execv)
"#
        )
    } else {
        format!(
            r#"#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
{UNSET_INJECTION_VARIABLE}{OPEN_CLOEXEC_DESCRIPTOR}
typedef int (*execv_function)(const char *, char *const[]);
int execv(const char *path, char *const argv[]) {{
    unlink_disappearing_target(path);
    execv_function real_execv = (execv_function)dlsym(RTLD_NEXT, "execv");
    return real_execv(path, argv);
}}
"#
        )
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

fn empty_user_database_home_interposer(temp: &TempDir) -> PathBuf {
    let source = temp.path().join("empty-user-database-home.c");
    let library = if cfg!(target_os = "macos") {
        temp.path().join("libempty-user-database-home.dylib")
    } else {
        temp.path().join("libempty-user-database-home.so")
    };
    let getpwuid_name = if cfg!(target_os = "macos") {
        "interposed_getpwuid"
    } else {
        "getpwuid"
    };
    let interpose = if cfg!(target_os = "macos") {
        r#"
#define INTERPOSE(replacement, replacee) \
    __attribute__((used)) static struct { const void *replacement_ptr; const void *replacee_ptr; } \
    interpose_##replacee __attribute__((section("__DATA,__interpose"))) = \
    { (const void *)(unsigned long)&replacement, (const void *)(unsigned long)&replacee };
INTERPOSE(interposed_getpwuid, getpwuid)
"#
    } else {
        ""
    };
    let body = format!(
        r#"#include <pwd.h>
#include <stdlib.h>
#include <sys/types.h>
{UNSET_INJECTION_VARIABLE}
static struct passwd record = {{ .pw_dir = "" }};
struct passwd *{getpwuid_name}(uid_t uid) {{
    (void)uid;
    return &record;
}}
{interpose}
"#,
        getpwuid_name = getpwuid_name,
    );
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

fn preload(command: &mut Command, library: &Path) {
    if cfg!(target_os = "macos") {
        command.env("DYLD_INSERT_LIBRARIES", library);
    } else {
        command.env("LD_PRELOAD", library);
    }
}

#[test]
fn a_present_path_runs_the_command_and_preserves_its_output() {
    let output = run(["path", "/bin", "--", "/usr/bin/printf", "present"]);

    assert_success_output(&output, b"present");
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

    assert_silent_success(&output);
}

#[test]
fn a_path_guard_through_a_regular_file_is_confirmed_absent() {
    let temp = TempDir::new();
    let file = temp.path().join("file");
    fs::write(&file, b"").unwrap();

    let output = binary()
        .arg("path")
        .arg(file.join("guard"))
        .args(["--", "/bin/false"])
        .output()
        .unwrap();

    assert_silent_success(&output);
}

#[test]
fn a_path_guard_with_a_trailing_slash_on_a_regular_file_is_confirmed_absent() {
    let temp = TempDir::new();
    let file = temp.path().join("file");
    fs::write(&file, b"").unwrap();
    let mut guard = file.into_os_string();
    guard.push("/");

    let output = binary()
        .arg("path")
        .arg(guard)
        .args(["--", "/bin/false"])
        .output()
        .unwrap();

    assert_silent_success(&output);
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

    assert_silent_success(&output);
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

    assert_diagnostic(&output, 1, "inspect");
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
    let program = "/definitely/not/present";
    let output = run(["path", "/bin", "--", program]);

    assert_operand_diagnostic(&output, 127, "execute", program, "command not found");
}

#[test]
fn an_uninvokable_explicit_launch_target_has_the_command_mode_diagnostic() {
    let temp = TempDir::new();
    let program = temp.path().join("not-executable");
    fs::write(&program, b"not executable").unwrap();

    let command_mode = binary().arg("command").arg(&program).output().unwrap();
    let path_mode = binary()
        .arg("path")
        .arg(temp.path())
        .args(["--"])
        .arg(&program)
        .output()
        .unwrap();

    let diagnostic = assert_diagnostic(&command_mode, 126, "resolve executable");
    assert!(diagnostic.contains("not an executable regular file"));
    assert_eq!(path_mode.status.code(), Some(126));
    assert_eq!(path_mode.stderr, command_mode.stderr);
}

#[test]
fn an_explicit_launch_target_through_a_regular_file_is_command_not_found() {
    let temp = TempDir::new();
    let file = temp.path().join("file");
    fs::write(&file, b"").unwrap();
    let program = file.join("tool");

    let output = binary()
        .arg("path")
        .arg(temp.path())
        .args(["--"])
        .arg(&program)
        .output()
        .unwrap();

    assert_operand_diagnostic(&output, 127, "execute", &program, "command not found");
}

#[test]
fn an_absent_command_is_silent_success() {
    let output = binary()
        .env("PATH", "")
        .args(["command", "not-present"])
        .output()
        .unwrap();

    assert_silent_success(&output);
}

#[test]
fn a_symbolic_link_to_an_executable_is_selected_from_path() {
    let temp = TempDir::new();
    let executable = temp.executable("executable", b"#!/bin/sh\nprintf linked");
    symlink(executable, temp.path().join("tool")).unwrap();

    let output = binary()
        .env("PATH", temp.path())
        .args(["command", "tool"])
        .output()
        .unwrap();

    assert_success_output(&output, b"linked");
}

#[test]
fn a_dangling_symbolic_link_is_no_path_candidate() {
    let temp = TempDir::new();
    symlink(temp.path().join("absent"), temp.path().join("tool")).unwrap();

    let output = binary()
        .env("PATH", temp.path())
        .args(["command", "tool"])
        .output()
        .unwrap();

    assert_silent_success(&output);
}

#[test]
fn an_empty_path_entry_names_the_effective_working_directory() {
    let temp = TempDir::new();
    temp.executable("tool", b"#!/bin/sh\nprintf local");

    let output = binary()
        .current_dir(temp.path())
        .env("PATH", ":")
        .args(["command", "tool"])
        .output()
        .unwrap();

    assert_success_output(&output, b"local");
}

#[test]
fn a_bare_launch_target_without_a_candidate_is_command_not_found() {
    let output = binary()
        .env("PATH", "")
        .args(["path", "/bin", "--", "not-present"])
        .output()
        .unwrap();

    assert_operand_diagnostic(&output, 127, "execute", "not-present", "command not found");
}

#[test]
fn an_unset_path_is_silent_success() {
    let output = binary()
        .env_remove("PATH")
        .args(["command", "not-present"])
        .output()
        .unwrap();
    assert_silent_success(&output);
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
    assert_silent_success(&output);
}

#[test]
fn a_shell_only_name_is_not_resolved() {
    let temp = TempDir::new();
    let output = binary()
        .env("PATH", temp.path())
        .args(["command", "cd"])
        .output()
        .unwrap();
    assert_silent_success(&output);
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

    assert_diagnostic(&output, 126, "resolve executable");
}

#[test]
fn a_path_entry_through_a_regular_file_contributes_no_candidate() {
    let temp = TempDir::new();
    let file = temp.path().join("file");
    fs::write(&file, b"").unwrap();

    let output = binary()
        .env("PATH", file.join("directory"))
        .args(["command", "tool"])
        .output()
        .unwrap();

    assert_silent_success(&output);
}

#[test]
fn an_explicit_command_through_a_regular_file_is_confirmed_absent() {
    let temp = TempDir::new();
    let file = temp.path().join("file");
    fs::write(&file, b"").unwrap();

    let output = binary()
        .arg("command")
        .arg(file.join("tool"))
        .output()
        .unwrap();

    assert_silent_success(&output);
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
fn an_inspection_failure_does_not_hide_a_later_usable_candidate() {
    let temp = TempDir::new();
    let locked = temp.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("tool"), b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    temp.executable("tool", b"#!/bin/sh\nprintf usable");
    let path = std::env::join_paths([locked.as_path(), temp.path()]).unwrap();

    let output = permission_test_binary(&temp)
        .env("PATH", path)
        .args(["command", "tool"])
        .output()
        .unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_success_output(&output, b"usable");
}

#[test]
fn an_inspection_failure_takes_priority_over_an_unusable_candidate() {
    let temp = TempDir::new();
    let locked = temp.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("tool"), b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    fs::write(temp.path().join("tool"), b"not executable").unwrap();
    let path = std::env::join_paths([locked.as_path(), temp.path()]).unwrap();

    let output = permission_test_binary(&temp)
        .env("PATH", path)
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
    preload(&mut command, &interposer);

    let output = command.output().unwrap();

    assert_diagnostic(&output, 127, "execute");
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

    assert_success_output(&output, b"usable");
}

#[test]
fn search_continues_past_a_path_entry_that_is_a_regular_file() {
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
    assert_success_output(&output, b"usable");
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
        fs::canonicalize(temp.path()).unwrap().to_string_lossy()
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

    assert_silent_success(&output);
}

#[test]
fn a_chdir_target_through_a_regular_file_is_confirmed_absent() {
    let temp = TempDir::new();
    let file = temp.path().join("file");
    fs::write(&file, b"").unwrap();

    let output = binary()
        .arg("--chdir")
        .arg(file.join("directory"))
        .args(["command", "/bin/false"])
        .output()
        .unwrap();

    assert_silent_success(&output);
}

#[test]
fn a_chdir_target_with_a_trailing_slash_on_a_regular_file_is_confirmed_absent() {
    let temp = TempDir::new();
    let file = temp.path().join("file");
    fs::write(&file, b"").unwrap();
    let mut directory = file.into_os_string();
    directory.push("/");

    let output = binary()
        .arg("--chdir")
        .arg(directory)
        .args(["command", "/bin/false"])
        .output()
        .unwrap();

    assert_silent_success(&output);
}

#[test]
fn an_existing_regular_file_chdir_is_visible() {
    let temp = TempDir::new();
    let path = temp.path().join("file");
    fs::write(&path, b"").unwrap();

    let output = binary()
        .arg("--chdir")
        .arg(&path)
        .args(["command", "/bin/true"])
        .output()
        .unwrap();

    assert_diagnostic(&output, 1, "chdir");
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

    assert_diagnostic(&output, 127, "execute");
}

#[test]
fn an_executable_format_failure_exits_126() {
    let temp = TempDir::new();
    let program = temp.executable("program", b"not an executable format");

    let output = binary().arg("command").arg(program).output().unwrap();

    assert_diagnostic(&output, 126, "execute");
}

#[test]
fn child_exit_status_is_preserved() {
    let output = run(["path", "/bin", "--", "/bin/sh", "-c", "exit 42"]);

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
    let wrapped = run(["path", "/bin", "--", "/bin/sh", "-c", script]);

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
        .args(["path", "/bin", "--", "/usr/bin/printf", "%s"])
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
        .args(["path", "~/guard", "--", "/usr/bin/printf", "expanded"])
        .output()
        .unwrap();

    assert_success_output(&output, b"expanded");
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

    assert_silent_success(&output);
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
        fs::canonicalize(&directory).unwrap().to_string_lossy()
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

    let diagnostic = assert_diagnostic(&output, 127, "execute");
    assert!(diagnostic.contains("\\nforged"));
}

#[test]
fn a_present_guard_resolves_a_bare_launch_target_from_path() {
    let output = binary()
        .env("PATH", "/bin:/usr/bin")
        .args(["path", "/bin", "--", "printf", "resolved"])
        .output()
        .unwrap();

    assert_success_output(&output, b"resolved");
}

#[test]
fn explicit_launches_keep_the_caller_supplied_path_as_argv0() {
    let temp = TempDir::new();
    let token = "./explicit-argv0-reporter";
    let reporter = temp.path().join("explicit-argv0-reporter");
    compile_argv0_reporter(&temp, &reporter);

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
    compile_argv0_reporter(&temp, &reporter);

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
    if non_utf8_entry(fs::File::create(&reporter), &reporter).is_none() {
        return;
    }
    fs::remove_file(&reporter).unwrap();
    compile_argv0_reporter(&temp, &reporter);

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
fn a_relative_path_entry_is_joined_literally_for_the_launch() {
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

    assert_success_output(&output, b"./bin/provider-tool");
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

    assert_success_output(&output, b"relative");
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

    assert_success_output(&output, b"effective");
}

#[test]
fn an_executable_whose_execute_bit_is_unavailable_to_the_caller_exits_126() {
    let first = TempDir::new();
    let second = TempDir::new();
    let first_tool = first.path().join("tool");
    fs::write(&first_tool, b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&first_tool, fs::Permissions::from_mode(0o010)).unwrap();
    second.executable("tool", b"#!/bin/sh\nprintf second");
    let path = std::env::join_paths([first.path(), second.path()]).unwrap();

    let output = permission_test_binary(&first)
        .env("PATH", path)
        .args(["command", "tool"])
        .output()
        .unwrap();

    assert_operand_diagnostic(
        &output,
        126,
        "execute",
        &first_tool,
        &io::Error::from_raw_os_error(13).to_string(),
    );
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

    assert_success_output(&output, b"preserved");
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
    let output = run([
        "path",
        "/bin",
        "--",
        "/bin/sh",
        "-c",
        "printf child-error >&2",
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"child-error");
}

#[test]
fn standard_descriptors_closed_at_start_remain_closed_in_the_child() {
    let temp = TempDir::new();
    let observer = temp.path().join("closed-descriptor-observer");
    compile_closed_descriptor_observer(&temp, &observer);

    for descriptor in 0..=2 {
        let mut command = binary();
        command
            .arg("command")
            .arg(&observer)
            .arg(descriptor.to_string());
        close_descriptor_before_exec(&mut command, descriptor);

        let status = command.status().unwrap();
        assert_eq!(status.code(), Some(0), "descriptor {descriptor}");
    }
}

#[test]
fn a_replacement_failure_keeps_its_exit_code_when_stderr_is_a_closed_pipe() {
    let temp = TempDir::new();
    let program = temp.executable("invalid-format", b"not executable format");
    let mut command = binary();
    command.arg("command").arg(program);

    let status = status_with_closed_stderr(command);

    assert_eq!(status.code(), Some(126));
    assert_eq!(status.signal(), None);
}

#[test]
fn an_empty_wrapper_value_keeps_exit_two_when_stderr_is_a_closed_pipe() {
    let mut command = binary();
    command.args(["command", ""]);

    let status = status_with_closed_stderr(command);

    assert_eq!(status.code(), Some(2));
}

#[test]
fn an_unknown_top_level_argument_keeps_exit_two_when_stderr_is_a_closed_pipe() {
    let mut command = binary();
    command.arg("unknown");

    let status = status_with_closed_stderr(command);

    assert_eq!(status.code(), Some(2));
}

#[test]
fn a_replacement_failure_keeps_its_exit_code_when_stderr_started_closed() {
    let temp = TempDir::new();
    let program = temp.executable("invalid-format", b"not executable format");
    let mut command = binary();
    command.arg("command").arg(program);
    close_descriptor_before_exec(&mut command, 2);

    let output = command.output().unwrap();
    let report = output_report(&output);

    assert_eq!(output.status.code(), Some(126), "{report}");
    assert_eq!(output.status.signal(), None, "{report}");
}

#[test]
fn chdir_does_not_update_the_callers_pwd_environment_entry() {
    let temp = TempDir::new();
    let output = binary()
        .env("PWD", "caller-pwd-marker")
        .arg("--chdir")
        .arg(temp.path())
        .args(["command", "/usr/bin/env"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output
        .stdout
        .split(|byte| *byte == b'\n')
        .any(|line| line == b"PWD=caller-pwd-marker"));
    assert!(output.stderr.is_empty());
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
    let actual = run(["path", "/bin", "--", "/bin/sh", "-c", "ulimit -n"]);
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
    preload(&mut command, &interposer);

    let output = command.output().unwrap();

    assert_silent_success(&output);
}

#[test]
fn a_non_utf8_missing_launch_target_is_escaped_with_the_fixed_reason() {
    let operand = OsString::from_vec(vec![b'/', b'm', b'i', b's', b's', b'\t', 0xff]);
    let output = binary()
        .env("LC_ALL", "C")
        .args(["path", "/bin", "--"])
        .arg(operand)
        .output()
        .unwrap();
    let diagnostic = assert_diagnostic(&output, 127, "execute");
    assert!(diagnostic.contains("\\t"));
    assert!(diagnostic.contains("\\xff"));
    assert!(diagnostic.contains("command not found"));
}

#[test]
fn an_empty_child_argument_reaches_the_child() {
    let output = run(["path", "/bin", "--", "/usr/bin/printf", "[%s]", ""]);

    assert_success_output(&output, b"[]");
}

#[test]
fn non_utf8_home_is_used_without_lossy_conversion() {
    let temp = TempDir::new();
    let home = temp.path().join(OsString::from_vec(vec![b'h', 0xff]));
    if non_utf8_entry(fs::create_dir(&home), &home).is_none() {
        return;
    }
    fs::write(home.join("guard"), b"").unwrap();

    let output = binary()
        .env("HOME", &home)
        .args(["path", "~/guard", "--", "/usr/bin/printf", "bytes"])
        .output()
        .unwrap();

    assert_success_output(&output, b"bytes");
}

#[test]
fn an_unset_home_uses_the_operating_system_user_database() {
    let output = binary()
        .env_remove("HOME")
        .args(["path", "~", "--", "/usr/bin/printf", "database-home"])
        .output()
        .unwrap();

    assert_success_output(&output, b"database-home");
}

#[test]
fn an_empty_home_uses_the_operating_system_user_database() {
    let output = binary()
        .env("HOME", "")
        .args(["path", "~", "--", "/usr/bin/printf", "database-home"])
        .output()
        .unwrap();

    assert_success_output(&output, b"database-home");
}

#[test]
fn an_empty_user_database_home_is_reported_by_the_cli() {
    let temp = TempDir::new();
    let interposer = empty_user_database_home_interposer(&temp);
    let mut command = binary();
    command
        .env_remove("HOME")
        .args(["path", "~", "--", "/bin/true"]);
    preload(&mut command, &interposer);

    let output = command.output().unwrap();

    let diagnostic = assert_diagnostic(&output, 1, "expand");
    assert!(diagnostic.contains("home directory is unavailable"));
}
