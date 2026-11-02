//! Helpers for tests of this crate and of the application. Not part of the program.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// A new empty folder in the system's temporary one, removed when this is dropped, a failing test
/// included. Its name is unique to the process and to the call.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let n = COUNT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("roll-model-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary folder");
        TempDir(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::TempDir;

    #[test]
    fn a_temporary_folder_is_removed_with_its_guard_and_each_one_is_its_own() {
        let (a, b) = (TempDir::new("guard"), TempDir::new("guard"));
        assert_ne!(a.path(), b.path());
        let path = a.path().to_path_buf();
        std::fs::write(path.join("file"), "x").unwrap();
        drop(a);
        assert!(!path.exists());
        assert!(b.path().is_dir());
    }
}
