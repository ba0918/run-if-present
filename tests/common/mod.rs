#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
