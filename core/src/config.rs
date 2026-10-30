//! Persistent settings, stored as a small `key = value` text file:
//! Windows `%APPDATA%\RollModel\config.txt`, macOS
//! `~/Library/Application Support/RollModel/config.txt`, otherwise
//! `$XDG_CONFIG_HOME/roll-model/config.txt` (or `~/.config/...`).
//! Settings of the application's former name (Drum Tutor) are copied across once.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::analysis::Tolerances;
use crate::drums::{format_note_list, parse_note_list, Lane, NoteMap};
use crate::rudiments::{self, DEFAULT_REPEATS, MAX_REPEATS};

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Substring of the MIDI input port name to auto-connect to.
    pub midi_in_port: String,
    pub midi_out_port: String,
    /// Subtracted from every captured hit (input + monitoring latency).
    pub latency_ms: f64,
    pub tol: Tolerances,
    pub metronome_during_take: bool,
    pub count_in_bars: u32,
    /// Listen starts with the count-in too (a take always does).
    pub listen_count_in: bool,
    /// A bar that ends with a pause is the last of its row on the Score tab.
    pub break_at_pause: bool,
    /// Listen with the loop on raises the tempo by `ramp_percent` (points of the written tempo,
    /// 0 = off) every `ramp_passes` passes.
    pub ramp_percent: u32,
    pub ramp_passes: usize,
    pub click_audio: bool,
    pub click_volume: f32,
    pub click_midi: bool,
    pub click_note: u8,
    pub click_channel: u8,
    /// "Listen": play the score through the sound card (software drum kit).
    pub playback_audio: bool,
    pub playback_volume: f32,
    /// "Listen": also send the notes to the MIDI output (the module's sounds).
    pub playback_midi: bool,
    pub playback_channel: u8,
    pub note_map: NoteMap,
    /// Value the hi-hat pedal (CC#4) reports when fully closed; the closed/open
    /// decision scales with it (127 for a pedal that uses the full range).
    pub hihat_cc_max: u8,
    pub keyboard_input: bool,
    pub zoom: f32,
    pub expand_repeats: bool,
    /// Scores opened before, most recent first (absolute paths).
    pub recent_files: Vec<String>,
    /// How the rows of a score were laid out the last time it was open (bars per row, the bar that
    /// starts a row), most recently changed first. Only scores that differ from "auto, off".
    pub layouts: Vec<(String, RowLayout)>,
    /// Sound output device by name; empty = the system default.
    pub audio_output: String,
    /// Bars per row of the score; 0 = as many as fit.
    pub bars_per_row: usize,
    /// Position of a bar that must start a row; 0 = none.
    pub layout_align_bar: usize,
    /// Volume of single instruments in Listen (1.0 = as rendered; absent = 1.0).
    pub lane_gain: BTreeMap<Lane, f32>,
    pub lane_mute: BTreeSet<Lane>,
    /// The free metronome.
    pub metro_bpm: f32,
    pub metro_beats: u32,
    /// Index into `playback::Subdivision::ALL`.
    pub metro_sub: usize,
    /// Show the loaded score rolling by on the Metronome tab.
    pub metro_preview: bool,
    /// The free metronome gets faster by `metro_ramp_bpm` (0 = off) every `metro_ramp_bars` bars.
    pub metro_ramp_bpm: u32,
    pub metro_ramp_bars: usize,
    /// Hardware controller: a built-in preset's name or the name of a file in the
    /// controllers folder; empty = none.
    pub controller_preset: String,
    /// Part of the controller's MIDI port name; empty = the preset's own.
    pub controller_port: String,
    /// The Rudiments tab: the rudiment chosen (PAS number, 1 to 40) and how often it is repeated.
    pub rudiment: u8,
    pub rudiment_repeats: usize,
    /// List only the original NARD Standard 26.
    pub rudiment_nard_only: bool,
    /// Raise the tempo after every clean take.
    pub rudiment_ladder: bool,
    /// The tempo (BPM) last used with each rudiment, and the fastest clean take of each.
    pub rudiment_bpm: BTreeMap<u8, f32>,
    pub rudiment_best: BTreeMap<u8, f32>,
    /// The start trigger: two hits in a row (these instruments, in this order) start a recording.
    pub trigger_enabled: bool,
    pub trigger_first: Lane,
    pub trigger_second: Lane,
}

/// How many scores the welcome panel remembers.
pub const MAX_RECENT: usize = 10;
/// How many scores keep their row layout.
pub const MAX_LAYOUTS: usize = 30;

/// How the bars of a score are split into rows: the same two numbers as `LayoutOpts` of the
/// renderer, without a dependency on it. 0 means "auto" and "off".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowLayout {
    pub bars_per_row: usize,
    pub align_bar: usize,
}

/// Whether two file paths name the same file (they compare case-insensitively on Windows).
fn same_path(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}
/// The controller preset used until another is chosen (or "none": an empty name).
pub const DEFAULT_CONTROLLER: &str = "Behringer X-Touch Mini";
/// Loudest instrument gain the mixer allows.
pub const MAX_LANE_GAIN: f32 = 1.5;

impl Default for Config {
    fn default() -> Self {
        Config {
            midi_in_port: "V31".into(),
            midi_out_port: "V31".into(),
            latency_ms: 0.0,
            tol: Tolerances::default(),
            metronome_during_take: true,
            count_in_bars: 1,
            listen_count_in: true,
            break_at_pause: true,
            ramp_percent: 0,
            ramp_passes: 2,
            click_audio: true,
            click_volume: 0.5,
            click_midi: false,
            click_note: 76,
            click_channel: 10,
            playback_audio: true,
            playback_volume: 0.7,
            playback_midi: false,
            playback_channel: 10,
            note_map: NoteMap::roland_v31(),
            hihat_cc_max: 127,
            keyboard_input: false,
            zoom: 1.0,
            expand_repeats: true,
            recent_files: Vec::new(),
            layouts: Vec::new(),
            audio_output: String::new(),
            bars_per_row: 0,
            layout_align_bar: 0,
            lane_gain: BTreeMap::new(),
            lane_mute: BTreeSet::new(),
            metro_bpm: 100.0,
            metro_beats: 4,
            metro_sub: 0,
            metro_preview: true,
            metro_ramp_bpm: 0,
            metro_ramp_bars: 4,
            controller_preset: DEFAULT_CONTROLLER.to_string(),
            controller_port: String::new(),
            rudiment: 1,
            rudiment_repeats: DEFAULT_REPEATS,
            rudiment_nard_only: false,
            rudiment_ladder: false,
            rudiment_bpm: BTreeMap::new(),
            rudiment_best: BTreeMap::new(),
            trigger_enabled: false,
            trigger_first: Lane::Crash1,
            trigger_second: Lane::Tom1,
        }
    }
}

/// Why the settings cannot be saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    NoDirectory,
    Write(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::NoDirectory => write!(f, "no config directory"),
            ConfigError::Write(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// How one kind of setting is read from its text and written back. Anything that does not read
/// (a bad number, an unknown name) leaves the value as it was.
trait Setting<T> {
    fn read(&self, slot: &mut T, text: &str);
    fn write(&self, value: &T) -> String;
}

struct Text;
struct Flag;
/// A number kept between `lo` and `hi` (and finite, for a float).
struct Number<T> {
    lo: T,
    hi: T,
}
/// An instrument by name; `Lane::Other` is not one the user can choose.
struct LaneName;
/// A rudiment by its PAS number.
struct RudimentNumber;

fn number<T>(lo: T, hi: T) -> Number<T> {
    Number { lo, hi }
}

impl Setting<String> for Text {
    fn read(&self, slot: &mut String, text: &str) {
        *slot = text.to_string();
    }
    fn write(&self, value: &String) -> String {
        value.clone()
    }
}

impl Setting<bool> for Flag {
    fn read(&self, slot: &mut bool, text: &str) {
        *slot = text.parse().unwrap_or(*slot);
    }
    fn write(&self, value: &bool) -> String {
        value.to_string()
    }
}

/// What `Number` needs of a number type: it can be parsed, compared and written, and a float can be NaN.
trait Numeric: Copy + PartialOrd + std::str::FromStr + std::fmt::Display {
    fn is_usable(self) -> bool;
}

macro_rules! numeric {
    ($($t:ty => $usable:expr),*) => {
        $(impl Numeric for $t {
            fn is_usable(self) -> bool {
                $usable(self)
            }
        })*
    };
}
numeric!(u8 => |_| true, u32 => |_| true, usize => |_| true, f32 => f32::is_finite, f64 => f64::is_finite);

impl<T: Numeric> Setting<T> for Number<T> {
    fn read(&self, slot: &mut T, text: &str) {
        if let Some(x) = text.parse::<T>().ok().filter(|x| x.is_usable()) {
            *slot = if x < self.lo {
                self.lo
            } else if x > self.hi {
                self.hi
            } else {
                x
            };
        }
    }
    fn write(&self, value: &T) -> String {
        value.to_string()
    }
}

impl Setting<Lane> for LaneName {
    fn read(&self, slot: &mut Lane, text: &str) {
        *slot = Lane::from_name(text).filter(|l| *l != Lane::Other).unwrap_or(*slot);
    }
    fn write(&self, value: &Lane) -> String {
        value.name().to_string()
    }
}

impl Setting<u8> for RudimentNumber {
    fn read(&self, slot: &mut u8, text: &str) {
        *slot = text.parse().ok().filter(|n| rudiments::by_number(*n).is_some()).unwrap_or(*slot);
    }
    fn write(&self, value: &u8) -> String {
        value.to_string()
    }
}

/// One setting of the file: its key, and how to read and write the field it belongs to.
struct Key {
    name: &'static str,
    read: fn(&mut Config, &str),
    write: fn(&Config) -> String,
}

/// `key!("zoom", c.zoom, number(0.3, 4.0))`: the key, the field (`c` stands for the `Config`) and
/// the kind of setting it is.
macro_rules! key {
    ($name:literal, $c:ident . $($field:ident).+, $kind:expr) => {
        Key {
            name: $name,
            read: |$c: &mut Config, text: &str| $kind.read(&mut $c.$($field).+, text),
            write: |$c: &Config| $kind.write(&$c.$($field).+),
        }
    };
}

/// The settings that are one line each, in the order they are written. A new one is a field, its
/// default and one line here (and its page). The lists and maps (`recent`, `gain.*`, `mute.*`,
/// `map.*`, `rudiment_bpm.*`, `rudiment_best.*`) are read and written by hand below.
const KEYS: &[Key] = &[
    key!("midi_in_port", c.midi_in_port, Text),
    key!("midi_out_port", c.midi_out_port, Text),
    key!("latency_ms", c.latency_ms, number(f64::MIN, f64::MAX)),
    key!("perfect_ms", c.tol.perfect_ms, number(f64::MIN, f64::MAX)),
    key!("good_ms", c.tol.good_ms, number(f64::MIN, f64::MAX)),
    key!("window_ms", c.tol.window_ms, number(f64::MIN, f64::MAX)),
    key!("metronome_during_take", c.metronome_during_take, Flag),
    key!("count_in_bars", c.count_in_bars, number(0, 4)),
    key!("listen_count_in", c.listen_count_in, Flag),
    key!("break_at_pause", c.break_at_pause, Flag),
    key!("ramp_percent", c.ramp_percent, number(0, 50)),
    key!("ramp_passes", c.ramp_passes, number(1, 32)),
    key!("click_audio", c.click_audio, Flag),
    key!("click_volume", c.click_volume, number(f32::MIN, f32::MAX)),
    key!("click_midi", c.click_midi, Flag),
    key!("click_note", c.click_note, number(0, 127)),
    key!("click_channel", c.click_channel, number(1, 16)),
    key!("playback_audio", c.playback_audio, Flag),
    key!("playback_volume", c.playback_volume, number(0.0, 1.0)),
    key!("playback_midi", c.playback_midi, Flag),
    key!("playback_channel", c.playback_channel, number(1, 16)),
    key!("hihat_cc_max", c.hihat_cc_max, number(1, 127)),
    key!("keyboard_input", c.keyboard_input, Flag),
    key!("zoom", c.zoom, number(0.3, 4.0)),
    key!("expand_repeats", c.expand_repeats, Flag),
    key!("audio_output", c.audio_output, Text),
    key!("bars_per_row", c.bars_per_row, number(0, 16)),
    key!("layout_align_bar", c.layout_align_bar, number(0, 9999)),
    key!("metro_bpm", c.metro_bpm, number(20.0, 300.0)),
    key!("metro_beats", c.metro_beats, number(1, 12)),
    key!("metro_sub", c.metro_sub, number(0, 3)),
    key!("metro_preview", c.metro_preview, Flag),
    key!("metro_ramp_bpm", c.metro_ramp_bpm, number(0, 60)),
    key!("metro_ramp_bars", c.metro_ramp_bars, number(1, 64)),
    key!("controller_preset", c.controller_preset, Text),
    key!("controller_port", c.controller_port, Text),
    key!("trigger_enabled", c.trigger_enabled, Flag),
    key!("trigger_first", c.trigger_first, LaneName),
    key!("trigger_second", c.trigger_second, LaneName),
    key!("rudiment", c.rudiment, RudimentNumber),
    key!("rudiment_repeats", c.rudiment_repeats, number(1, MAX_REPEATS)),
    key!("rudiment_nard_only", c.rudiment_nard_only, Flag),
    key!("rudiment_ladder", c.rudiment_ladder, Flag),
];

/// Where the settings, the replacement sounds and the user's controller presets live. The
/// application has one, found from the operating system; a test makes one on a temporary folder
/// (or `none`, which has no folder at all), so nothing reads or writes the user's own files.
#[derive(Clone, Debug, PartialEq)]
pub struct Paths {
    dir: Option<PathBuf>,
    /// Where the settings were kept when the application was called Drum Tutor.
    legacy: Option<PathBuf>,
}

impl Paths {
    /// The settings folder of the operating system's convention.
    pub fn system() -> Paths {
        Paths { dir: Paths::dir_named("RollModel", "roll-model"), legacy: Paths::dir_named("DrumTutor", "drum-tutor") }
    }

    /// `dir` as the settings folder.
    pub fn in_dir(dir: PathBuf) -> Paths {
        Paths { dir: Some(dir), legacy: None }
    }

    /// No folder: nothing is read, and saving fails.
    pub fn none() -> Paths {
        Paths { dir: None, legacy: None }
    }

    /// The settings folder of the application called `windows_mac` (Windows and macOS) or `xdg`
    /// (elsewhere).
    fn dir_named(windows_mac: &str, xdg: &str) -> Option<PathBuf> {
        if cfg!(windows) {
            std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(windows_mac))
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support").join(windows_mac))
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
                .map(|p| p.join(xdg))
        }
    }

    pub fn config_file(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join("config.txt"))
    }

    /// Folder with WAV files that replace built-in sounds.
    pub fn sounds_dir(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join("sounds"))
    }

    /// Controller presets written by the user.
    pub fn controllers_dir(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join("controllers"))
    }
}

impl Config {
    /// Volume of one instrument in Listen (1.0 unless changed).
    pub fn gain_of(&self, lane: Lane) -> f32 {
        self.lane_gain.get(&lane).copied().unwrap_or(1.0)
    }

    /// What an instrument is played at in Listen: its volume, or silence when
    /// it is muted, or when other instruments are soloed and it is not one.
    pub fn effective_gain(&self, solo: &BTreeSet<Lane>, lane: Lane) -> f32 {
        let heard = if solo.is_empty() { !self.lane_mute.contains(&lane) } else { solo.contains(&lane) };
        if heard {
            self.gain_of(lane)
        } else {
            0.0
        }
    }

    /// Puts `path` at the top of the recent list (without duplicates; paths
    /// compare case-insensitively on Windows) and trims the list.
    pub fn remember_recent(&mut self, path: &str) {
        self.recent_files.retain(|p| !same_path(p, path));
        self.recent_files.insert(0, path.to_string());
        self.recent_files.truncate(MAX_RECENT);
    }

    /// The row layout kept for the score at `path`, if it has one.
    pub fn layout_of(&self, path: &str) -> Option<RowLayout> {
        self.layouts.iter().find(|(p, _)| same_path(p, path)).map(|(_, l)| *l)
    }

    /// Keeps `layout` for the score at `path`. The automatic layout is the default and is not kept.
    pub fn remember_layout(&mut self, path: &str, layout: RowLayout) {
        self.layouts.retain(|(p, _)| !same_path(p, path));
        if layout != RowLayout::default() {
            self.layouts.insert(0, (path.to_string(), layout));
            self.layouts.truncate(MAX_LAYOUTS);
        }
    }

    /// Copies the settings folder (settings, sounds) of the old name to the new
    /// one, if only the old one exists. The old folder is left alone.
    fn migrate(old: &Path, new: &Path) -> std::io::Result<()> {
        if new.exists() || !old.is_dir() {
            return Ok(());
        }
        fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
            std::fs::create_dir_all(to)?;
            for entry in std::fs::read_dir(from)? {
                let entry = entry?;
                let target = to.join(entry.file_name());
                if entry.file_type()?.is_dir() {
                    copy(&entry.path(), &target)?;
                } else {
                    std::fs::copy(entry.path(), target)?;
                }
            }
            Ok(())
        }
        copy(old, new)
    }

    /// The settings kept in `paths` (the defaults when there is no file); the settings of the
    /// application's former name are copied across first, once.
    pub fn load_from(paths: &Paths) -> Config {
        if let (Some(old), Some(new)) = (&paths.legacy, &paths.dir) {
            let _ = Config::migrate(old, new);
        }
        paths
            .config_file()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| Config::from_text(&t))
            .unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) -> Result<PathBuf, ConfigError> {
        let p = paths.config_file().ok_or(ConfigError::NoDirectory)?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| ConfigError::Write(e.to_string()))?;
        }
        std::fs::write(&p, self.to_text()).map_err(|e| ConfigError::Write(e.to_string()))?;
        Ok(p)
    }

    pub fn to_text(&self) -> String {
        let mut s = String::from("# Roll Model settings\n");
        let mut kv = |k: &str, v: String| s.push_str(&format!("{k} = {v}\n"));
        for key in KEYS {
            kv(key.name, (key.write)(self));
        }
        for path in &self.recent_files {
            kv("recent", path.clone());
        }
        for (path, l) in &self.layouts {
            kv("layout", format!("{} {} {path}", l.bars_per_row, l.align_bar));
        }
        for (n, bpm) in &self.rudiment_bpm {
            kv(&format!("rudiment_bpm.{n}"), bpm.to_string());
        }
        for (n, bpm) in &self.rudiment_best {
            kv(&format!("rudiment_best.{n}"), bpm.to_string());
        }
        for lane in Lane::ALL {
            if self.gain_of(lane) != 1.0 {
                kv(&format!("gain.{}", lane.name()), self.gain_of(lane).to_string());
            }
            if self.lane_mute.contains(&lane) {
                kv(&format!("mute.{}", lane.name()), "true".to_string());
            }
        }
        for lane in Lane::ALL {
            kv(&format!("map.{}", lane.name()), format_note_list(&self.note_map.notes_for(lane)));
        }
        s
    }

    pub fn from_text(text: &str) -> Config {
        let mut c = Config::default();
        let mut map_seen = false;
        let mut map = NoteMap { map: Default::default() };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let b = |d: bool| v.parse::<bool>().unwrap_or(d);
            if let Some(key) = KEYS.iter().find(|key| key.name == k) {
                (key.read)(&mut c, v);
                continue;
            }
            match k {
                "layout" => {
                    // "bars align path"; the path may hold spaces.
                    let mut parts = v.splitn(3, ' ');
                    if let (Some(bars), Some(align), Some(path)) = (parts.next(), parts.next(), parts.next()) {
                        if let (Ok(bars), Ok(align)) = (bars.parse::<usize>(), align.parse::<usize>()) {
                            let layout = RowLayout { bars_per_row: bars.min(16), align_bar: align.min(9999) };
                            if !path.is_empty() && c.layouts.len() < MAX_LAYOUTS && c.layout_of(path).is_none() && layout != RowLayout::default() {
                                c.layouts.push((path.to_string(), layout));
                            }
                        }
                    }
                }
                "recent" => {
                    if !v.is_empty() && c.recent_files.len() < MAX_RECENT && !c.recent_files.iter().any(|p| p == v) {
                        c.recent_files.push(v.to_string());
                    }
                }
                _ => {
                    if let Some(lane) = k.strip_prefix("map.").and_then(Lane::from_name) {
                        map_seen = true;
                        for n in parse_note_list(v) {
                            map.assign(n, lane);
                        }
                    } else if let Some(lane) = k.strip_prefix("gain.").and_then(Lane::from_name) {
                        if let Some(g) = v.parse::<f32>().ok().filter(|g| g.is_finite()) {
                            c.lane_gain.insert(lane, g.clamp(0.0, MAX_LANE_GAIN));
                        }
                    } else if let Some(lane) = k.strip_prefix("mute.").and_then(Lane::from_name) {
                        if b(false) {
                            c.lane_mute.insert(lane);
                        }
                    } else if let Some(n) = k.strip_prefix("rudiment_bpm.").and_then(|n| n.parse::<u8>().ok()) {
                        if let (Some(_), Some(bpm)) = (rudiments::by_number(n), v.parse::<f32>().ok().filter(|x| x.is_finite())) {
                            c.rudiment_bpm.insert(n, bpm.clamp(20.0, 300.0));
                        }
                    } else if let Some(n) = k.strip_prefix("rudiment_best.").and_then(|n| n.parse::<u8>().ok()) {
                        if let (Some(_), Some(bpm)) = (rudiments::by_number(n), v.parse::<f32>().ok().filter(|x| x.is_finite())) {
                            c.rudiment_best.insert(n, bpm.clamp(20.0, 300.0));
                        }
                    }
                }
            }
        }
        if map_seen {
            c.note_map = map;
        }
        c
    }
}

#[cfg(test)]
mod tests;
