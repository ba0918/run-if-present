#![allow(dead_code)]

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// The built wrapper, for tests that also set arguments piecewise, the environment, or the
/// working directory.
pub fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_run-if-present"))
}

/// Runs the built wrapper with `arguments` in the test's own environment and waits for it.
pub fn run<I, S>(arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    binary().args(arguments).output().unwrap()
}

/// Everything the wrapper left behind, for the message of a failed assertion.
pub fn output_report(output: &Output) -> String {
    format!(
        "exit code {:?}, signal {:?}, stdout {:?}, stderr {:?}",
        output.status.code(),
        output.status.signal(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )
}

/// Exit 0 with nothing on either stream: the wrapper skipped, or the child was silent.
pub fn assert_silent_success(output: &Output) {
    assert_success_output(output, b"");
}

/// Exit 0, exactly `stdout` on standard output, and nothing on standard error.
pub fn assert_success_output(output: &Output, stdout: &[u8]) {
    let report = output_report(output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, stdout, "{report}");
    assert!(output.stderr.is_empty(), "{report}");
}

/// Exit `code`, nothing on standard output, and one colorless diagnostic line on standard error
/// naming `operation`. Returns the line so a test can check its operand or reason.
pub fn assert_diagnostic(output: &Output, code: i32, operation: &str) -> String {
    let report = output_report(output);
    assert_eq!(output.status.code(), Some(code), "{report}");
    assert!(output.stdout.is_empty(), "{report}");
    let diagnostic = String::from_utf8(output.stderr.clone()).unwrap();
    assert_eq!(diagnostic.matches('\n').count(), 1, "{report}");
    assert!(diagnostic.ends_with('\n'), "{report}");
    assert!(
        diagnostic.starts_with(&format!("run-if-present: {operation}:")),
        "{report}"
    );
    assert!(!diagnostic.contains('\u{1b}'), "{report}");
    diagnostic
}

/// An operand diagnostic, byte for byte: `run-if-present: <operation>: "<operand>": <reason>`.
pub fn assert_operand_diagnostic(
    output: &Output,
    code: i32,
    operation: &str,
    operand: impl AsRef<Path>,
    reason: &str,
) {
    let diagnostic = assert_diagnostic(output, code, operation);
    assert_eq!(
        diagnostic,
        format!(
            "run-if-present: {operation}: \"{}\": {reason}\n",
            operand.as_ref().display()
        )
    );
}

/// A syntax diagnostic, byte for byte: exit 2 and `run-if-present: syntax: <message>`.
pub fn assert_syntax_diagnostic(output: &Output, message: &str) {
    let diagnostic = assert_diagnostic(output, 2, "syntax");
    assert_eq!(diagnostic, format!("run-if-present: syntax: {message}\n"));
}

pub fn closed_pipe_writer() -> OwnedFd {
    let (reader, writer) = UnixStream::pair().unwrap();
    drop(reader);
    OwnedFd::from(writer)
}

/// A directory under the system temporary directory, removed when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "run-if-present-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn executable(&self, name: &str, body: &[u8]) -> PathBuf {
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

// EILSEQ as macOS reports it when APFS refuses a file name that is not valid UTF-8; Linux
// filesystems accept arbitrary bytes, so the probe never takes the unsupported branch there.
const EILSEQ: i32 = 92;

/// Interprets an attempt to create an entry whose name is not valid UTF-8.
///
/// The specification covers non-UTF-8 input "where supported"; this is the filesystem's own
/// answer. A refusal (`EILSEQ`) prints a notice and yields `None` so the test returns without
/// asserting; every other failure panics like an ordinary fixture error.
pub fn non_utf8_entry<T>(created: io::Result<T>, path: &Path) -> Option<T> {
    match created {
        Ok(value) => Some(value),
        Err(error) if error.raw_os_error() == Some(EILSEQ) => {
            // Written to the raw handle so the notice reaches the log of a passing test,
            // which the test harness would otherwise capture and discard.
            let _ = writeln!(
                io::stderr(),
                "not supported on this filesystem: creating {} failed: {error}",
                path.display()
            );
            None
        }
        Err(error) => panic!("creating {}: {error}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::non_utf8_entry;
    use std::io;
    use std::path::Path;

    #[test]
    #[should_panic(expected = "os error 13")]
    fn any_other_creation_error_fails_the_test() {
        let denied: io::Result<()> = Err(io::Error::from_raw_os_error(13));
        let _ = non_utf8_entry(denied, Path::new("/probe/h\u{fffd}"));
    }

    #[test]
    fn a_created_non_utf8_entry_is_handed_back() {
        assert_eq!(
            non_utf8_entry(Ok(7), Path::new("/probe/h\u{fffd}")),
            Some(7)
        );
    }
}
