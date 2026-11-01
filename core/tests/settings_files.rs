//! The settings file on disk: how it is written, and what happens when it is damaged.

use std::path::PathBuf;

use roll_model_core::config::{Config, Paths};

/// A folder that is removed when the test ends, even if it panics.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let dir = std::env::temp_dir().join(format!("roll-model-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Dir(dir)
    }

    fn paths(&self) -> Paths {
        Paths::in_dir(self.0.clone())
    }

    fn file(&self) -> PathBuf {
        self.0.join("config.txt")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn zoomed(zoom: f32) -> Config {
    Config { zoom, ..Config::default() }
}

#[test]
fn saving_replaces_the_file_whole_and_leaves_nothing_beside_it() {
    let dir = Dir::new("save");
    zoomed(1.5).save(&dir.paths()).unwrap();
    zoomed(2.0).save(&dir.paths()).unwrap();
    assert_eq!(std::fs::read_to_string(dir.file()).unwrap(), zoomed(2.0).to_text());
    let names: Vec<String> = std::fs::read_dir(&dir.0).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["config.txt"], "no temporary file is left");
    assert_eq!(Config::load_from(&dir.paths()).zoom, 2.0);
}

#[test]
fn a_save_that_fails_leaves_the_old_file_alone() {
    let dir = Dir::new("fail");
    zoomed(1.5).save(&dir.paths()).unwrap();
    // Something that is not a file where the temporary file would go.
    std::fs::create_dir(dir.0.join("config.txt.tmp")).unwrap();
    assert!(zoomed(2.5).save(&dir.paths()).is_err());
    assert_eq!(Config::load_from(&dir.paths()).zoom, 1.5, "the file before is still there, whole");
}

#[test]
fn no_file_is_not_a_problem() {
    let dir = Dir::new("none");
    let (config, notice) = Config::load_with_notice(&dir.paths());
    assert_eq!(config, Config::default());
    assert_eq!(notice, None);
}

#[test]
fn a_file_that_is_not_text_is_read_as_far_as_possible_and_kept_aside() {
    let dir = Dir::new("bytes");
    let mut bytes = b"zoom = 1.75\n".to_vec();
    bytes.extend([0xFF, 0xFE, 0x00, b'\n']);
    std::fs::write(dir.file(), &bytes).unwrap();
    let (config, notice) = Config::load_with_notice(&dir.paths());
    assert_eq!(config.zoom, 1.75, "the readable lines count");
    let notice = notice.expect("the user is told");
    assert!(notice.contains("config.txt.bad"), "{notice}");
    assert_eq!(std::fs::read(dir.0.join("config.txt.bad")).unwrap(), bytes, "the original is kept");
}

#[test]
fn a_file_that_cannot_be_read_gives_the_defaults_and_a_message() {
    let dir = Dir::new("unreadable");
    std::fs::create_dir(dir.file()).unwrap(); // reading a folder as a file fails
    let (config, notice) = Config::load_with_notice(&dir.paths());
    assert_eq!(config, Config::default());
    assert!(notice.is_some_and(|n| n.contains("could not be read")));
}
