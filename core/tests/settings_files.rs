//! The settings file on disk: how it is written, and what happens when it is damaged.

use std::path::{Path, PathBuf};

use roll_model_core::config::{Config, Paths};
use roll_model_core::testing::TempDir;

/// A folder that is removed when the test ends, even if it panics.
struct Dir(TempDir);

impl Dir {
    fn new(name: &str) -> Dir {
        Dir(TempDir::new(&format!("settings-{name}")))
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn paths(&self) -> Paths {
        Paths::in_dir(self.path().to_path_buf())
    }

    fn file(&self) -> PathBuf {
        self.path().join("config.txt")
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
    let names: Vec<String> =
        std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["config.txt"], "no temporary file is left");
    assert_eq!(Config::load_from(&dir.paths()).zoom, 2.0);
}

#[test]
fn a_save_that_fails_leaves_the_old_file_alone() {
    let dir = Dir::new("fail");
    zoomed(1.5).save(&dir.paths()).unwrap();
    // Something that is not a file where the temporary file would go.
    std::fs::create_dir(dir.path().join("config.txt.tmp")).unwrap();
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
    assert_eq!(std::fs::read(dir.path().join("config.txt.bad")).unwrap(), bytes, "the original is kept");
}

#[test]
fn a_file_that_cannot_be_read_gives_the_defaults_and_a_message() {
    let dir = Dir::new("unreadable");
    std::fs::create_dir(dir.file()).unwrap(); // reading a folder as a file fails
    let (config, notice) = Config::load_with_notice(&dir.paths());
    assert_eq!(config, Config::default());
    assert!(notice.is_some_and(|n| n.contains("could not be read")));
}

#[test]
fn every_number_of_the_file_is_kept_in_the_range_the_screen_and_the_controller_use() {
    use roll_model_core::config::limits;
    let c = Config::from_text(
        "perfect_ms=-5\ngood_ms=1e9\nwindow_ms=0\nlatency_ms=-1000\nclick_volume=1e30\nzoom=9\ncount_in_bars=4\nbars_per_row=12\nmetro_bpm=5\nramp_percent=50\nramp_passes=32\nmetro_ramp_bpm=60\nmetro_ramp_bars=64\n",
    );
    assert_eq!(c.tol.perfect_ms, limits::PERFECT_MS.lo);
    assert_eq!(c.tol.good_ms, limits::GOOD_MS.hi);
    assert_eq!(c.tol.window_ms, limits::WINDOW_MS.lo);
    assert_eq!(c.latency_ms, limits::LATENCY_MS.lo);
    assert_eq!(c.click_volume, limits::CLICK_VOLUME.hi);
    assert_eq!(c.zoom, limits::ZOOM.hi);
    assert_eq!(c.count_in_bars, limits::COUNT_IN_BARS.hi);
    assert_eq!(c.bars_per_row, limits::BARS_PER_ROW.hi);
    assert_eq!(c.metro_bpm, limits::METRO_BPM.lo);
    assert_eq!(c.ramp_percent, limits::RAMP_PERCENT.hi);
    assert_eq!(c.ramp_passes, limits::RAMP_PASSES.hi);
    assert_eq!(c.metro_ramp_bpm, limits::METRO_RAMP_BPM.hi);
    assert_eq!(c.metro_ramp_bars, limits::METRO_RAMP_BARS.hi);
}

#[test]
fn a_span_maps_to_the_unit_and_back_for_integers_and_floats() {
    use roll_model_core::config::limits;
    assert_eq!(limits::COUNT_IN_BARS.unit(1), 0.5);
    assert_eq!(limits::COUNT_IN_BARS.at(0.5), 1);
    assert_eq!(limits::BARS_PER_ROW.at(1.0), 8);
    assert_eq!(limits::BARS_PER_ROW.at(2.0), 8, "kept inside");
    assert_eq!(limits::METRO_BPM.at(0.0), 20.0);
    assert!((limits::METRO_BPM.unit(160.0) - 0.5).abs() < 1e-6);
    for v in 0..=8usize {
        assert_eq!(limits::BARS_PER_ROW.at(limits::BARS_PER_ROW.unit(v)), v);
    }
}
