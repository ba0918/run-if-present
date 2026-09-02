#![allow(dead_code)]

use std::fs;
use std::io::{self, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
