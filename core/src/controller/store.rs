//! The user's controller presets as files in a folder: making a new one, a copy of one, renaming
//! and deleting. Nothing here knows about a window; the Manage window asks and shows the answers.

use std::io;
use std::path::{Path, PathBuf};

use super::preset::builtin_text;

/// Why a preset cannot get this name, if it cannot.
pub fn check_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        Err("Type a name.")
    } else if name.chars().any(|c| "\\/:*?\"<>|".contains(c) || c.is_control()) || name.starts_with('.') || name.ends_with('.') {
        Err("A name cannot contain \\ / : * ? \" < > | or start or end with a dot.")
    } else if builtin_text(name).is_some() {
        Err("That is the name of a built-in preset.")
    } else {
        Ok(())
    }
}

/// What can go wrong renaming a preset.
#[derive(Debug)]
pub enum RenameError {
    /// The name is not allowed (see [`check_name`]).
    Name(&'static str),
    /// A preset of that name is there already.
    Exists,
    Io(io::Error),
}

impl std::fmt::Display for RenameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenameError::Name(why) => write!(f, "{why}"),
            RenameError::Exists => write!(f, "A preset with that name exists already."),
            RenameError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// The folder of the user's presets, which may not exist yet.
#[derive(Clone, Debug)]
pub struct PresetStore {
    dir: PathBuf,
}

impl PresetStore {
    pub fn new(dir: PathBuf) -> PresetStore {
        PresetStore { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes `text` as "NAME.txt", or "NAME 2.txt", … if that exists; never overwrites.
    pub fn create(&self, name: &str, text: &str) -> io::Result<PathBuf> {
        self.write_unused(|n| if n == 1 { format!("{name}.txt") } else { format!("{name} {n}.txt") }, text)
    }

    /// Writes `text` as "NAME (copy).txt", or "(copy 2)", … ; never overwrites.
    pub fn copy(&self, name: &str, text: &str) -> io::Result<PathBuf> {
        self.write_unused(|n| if n == 1 { format!("{name} (copy).txt") } else { format!("{name} (copy {n}).txt") }, text)
    }

    fn write_unused(&self, file: impl Fn(u32) -> String, text: &str) -> io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let mut n = 1;
        let mut path = self.dir.join(file(n));
        while path.exists() {
            n += 1;
            path = self.dir.join(file(n));
        }
        std::fs::write(&path, text)?;
        Ok(path)
    }

    /// Moves the file of a preset to the file of the name `new` (the `name` line inside is the
    /// caller's to change).
    pub fn rename(&self, file: &Path, new: &str) -> Result<PathBuf, RenameError> {
        check_name(new).map_err(RenameError::Name)?;
        let target = self.dir.join(format!("{new}.txt"));
        if target.exists() {
            return Err(RenameError::Exists);
        }
        std::fs::rename(file, &target).map_err(RenameError::Io)?;
        Ok(target)
    }

    pub fn delete(&self, file: &Path) -> io::Result<()> {
        std::fs::remove_file(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn new_presets_and_copies_never_overwrite() {
        let tmp = TempDir::new("store");
        let store = PresetStore::new(tmp.path().join("presets"));
        let a = store.create("My controller", "x").unwrap();
        let b = store.create("My controller", "y").unwrap();
        assert_ne!(a, b);
        assert!(b.to_string_lossy().ends_with("My controller 2.txt"));
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "x", "the first is untouched");
        let c = store.copy("Pads", "one").unwrap();
        let d = store.copy("Pads", "two").unwrap();
        assert!(c.to_string_lossy().ends_with("Pads (copy).txt") && d.to_string_lossy().ends_with("Pads (copy 2).txt"));
    }

    #[test]
    fn a_preset_is_renamed_to_a_good_unused_name_and_deleted() {
        let tmp = TempDir::new("store-rename");
        let store = PresetStore::new(tmp.path().to_path_buf());
        let a = store.create("Pads", "x").unwrap();
        store.create("Other", "y").unwrap();
        assert!(matches!(store.rename(&a, "Other"), Err(RenameError::Exists)));
        assert!(matches!(store.rename(&a, "a/b"), Err(RenameError::Name(_))));
        assert!(matches!(store.rename(&a, ""), Err(RenameError::Name(_))));
        let moved = store.rename(&a, "Stage pads").unwrap();
        assert!(moved.ends_with("Stage pads.txt") && moved.exists() && !a.exists());
        store.delete(&moved).unwrap();
        assert!(!moved.exists());
        assert!(store.delete(&moved).is_err(), "nothing left to delete");
    }

    #[test]
    fn the_names_a_file_cannot_have_are_refused() {
        assert!(check_name("My pads 2").is_ok());
        for bad in ["", "a/b", "a:b", ".hidden", "ends.", "tab\there"] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
        assert!(check_name("Korg nanoKONTROL2").is_err(), "the name of a built-in preset is taken");
    }
}
