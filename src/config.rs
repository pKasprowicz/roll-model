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
}

/// How many scores the welcome panel remembers.
pub const MAX_RECENT: usize = 10;
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
            audio_output: String::new(),
            bars_per_row: 0,
            layout_align_bar: 0,
            lane_gain: BTreeMap::new(),
            lane_mute: BTreeSet::new(),
            metro_bpm: 100.0,
            metro_beats: 4,
            metro_sub: 0,
            metro_preview: true,
            controller_preset: String::new(),
            controller_port: String::new(),
            rudiment: 1,
            rudiment_repeats: DEFAULT_REPEATS,
            rudiment_nard_only: false,
            rudiment_ladder: false,
            rudiment_bpm: BTreeMap::new(),
            rudiment_best: BTreeMap::new(),
        }
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

    /// Folder with WAV files that replace built-in sounds, next to the settings file.
    pub fn sounds_dir() -> Option<PathBuf> {
        Config::path().and_then(|p| p.parent().map(|d| d.join("sounds")))
    }

    /// Puts `path` at the top of the recent list (without duplicates; paths
    /// compare case-insensitively on Windows) and trims the list.
    pub fn remember_recent(&mut self, path: &str) {
        let same = |a: &str, b: &str| if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b };
        self.recent_files.retain(|p| !same(p, path));
        self.recent_files.insert(0, path.to_string());
        self.recent_files.truncate(MAX_RECENT);
    }

    /// The settings folder of the application called `windows_mac` (Windows and
    /// macOS) or `xdg` (elsewhere).
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

    fn dir() -> Option<PathBuf> {
        Config::dir_named("RollModel", "roll-model")
    }

    /// Where the settings were kept when the application was called Drum Tutor.
    fn legacy_dir() -> Option<PathBuf> {
        Config::dir_named("DrumTutor", "drum-tutor")
    }

    /// Controller presets written by the user, next to the settings file.
    pub fn controllers_dir() -> Option<PathBuf> {
        Config::dir().map(|d| d.join("controllers"))
    }

    pub fn path() -> Option<PathBuf> {
        Config::dir().map(|d| d.join("config.txt"))
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

    pub fn load() -> Config {
        if let (Some(old), Some(new)) = (Config::legacy_dir(), Config::dir()) {
            let _ = Config::migrate(&old, &new);
        }
        Config::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| Config::from_text(&t))
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<PathBuf, String> {
        let p = Config::path().ok_or("no config directory")?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, self.to_text()).map_err(|e| e.to_string())?;
        Ok(p)
    }

    pub fn to_text(&self) -> String {
        let mut s = String::from("# Roll Model settings\n");
        let mut kv = |k: &str, v: String| s.push_str(&format!("{k} = {v}\n"));
        kv("midi_in_port", self.midi_in_port.clone());
        kv("midi_out_port", self.midi_out_port.clone());
        kv("latency_ms", self.latency_ms.to_string());
        kv("perfect_ms", self.tol.perfect_ms.to_string());
        kv("good_ms", self.tol.good_ms.to_string());
        kv("window_ms", self.tol.window_ms.to_string());
        kv("metronome_during_take", self.metronome_during_take.to_string());
        kv("count_in_bars", self.count_in_bars.to_string());
        kv("click_audio", self.click_audio.to_string());
        kv("click_volume", self.click_volume.to_string());
        kv("click_midi", self.click_midi.to_string());
        kv("click_note", self.click_note.to_string());
        kv("click_channel", self.click_channel.to_string());
        kv("playback_audio", self.playback_audio.to_string());
        kv("playback_volume", self.playback_volume.to_string());
        kv("playback_midi", self.playback_midi.to_string());
        kv("playback_channel", self.playback_channel.to_string());
        kv("hihat_cc_max", self.hihat_cc_max.to_string());
        kv("keyboard_input", self.keyboard_input.to_string());
        kv("zoom", self.zoom.to_string());
        kv("expand_repeats", self.expand_repeats.to_string());
        for path in &self.recent_files {
            kv("recent", path.clone());
        }
        kv("audio_output", self.audio_output.clone());
        kv("bars_per_row", self.bars_per_row.to_string());
        kv("layout_align_bar", self.layout_align_bar.to_string());
        kv("metro_bpm", self.metro_bpm.to_string());
        kv("metro_beats", self.metro_beats.to_string());
        kv("metro_sub", self.metro_sub.to_string());
        kv("metro_preview", self.metro_preview.to_string());
        kv("controller_preset", self.controller_preset.clone());
        kv("controller_port", self.controller_port.clone());
        kv("rudiment", self.rudiment.to_string());
        kv("rudiment_repeats", self.rudiment_repeats.to_string());
        kv("rudiment_nard_only", self.rudiment_nard_only.to_string());
        kv("rudiment_ladder", self.rudiment_ladder.to_string());
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
            let f = |d: f64| v.parse::<f64>().unwrap_or(d);
            let b = |d: bool| v.parse::<bool>().unwrap_or(d);
            match k {
                "midi_in_port" => c.midi_in_port = v.to_string(),
                "midi_out_port" => c.midi_out_port = v.to_string(),
                "latency_ms" => c.latency_ms = f(c.latency_ms),
                "perfect_ms" => c.tol.perfect_ms = f(c.tol.perfect_ms),
                "good_ms" => c.tol.good_ms = f(c.tol.good_ms),
                "window_ms" => c.tol.window_ms = f(c.tol.window_ms),
                "metronome_during_take" => c.metronome_during_take = b(c.metronome_during_take),
                "count_in_bars" => c.count_in_bars = v.parse().unwrap_or(c.count_in_bars).min(4),
                "click_audio" => c.click_audio = b(c.click_audio),
                "click_volume" => c.click_volume = v.parse().unwrap_or(c.click_volume),
                "click_midi" => c.click_midi = b(c.click_midi),
                "click_note" => c.click_note = v.parse().unwrap_or(c.click_note).min(127),
                "click_channel" => c.click_channel = v.parse().unwrap_or(c.click_channel).clamp(1, 16),
                "playback_audio" => c.playback_audio = b(c.playback_audio),
                "playback_volume" => {
                    let x = v.parse::<f32>().ok().filter(|x| x.is_finite());
                    c.playback_volume = x.unwrap_or(c.playback_volume).clamp(0.0, 1.0)
                }
                "playback_midi" => c.playback_midi = b(c.playback_midi),
                "playback_channel" => c.playback_channel = v.parse().unwrap_or(c.playback_channel).clamp(1, 16),
                "hihat_cc_max" => c.hihat_cc_max = v.parse().unwrap_or(c.hihat_cc_max).clamp(1, 127),
                "keyboard_input" => c.keyboard_input = b(c.keyboard_input),
                "zoom" => c.zoom = v.parse::<f32>().unwrap_or(c.zoom).clamp(0.3, 4.0),
                "expand_repeats" => c.expand_repeats = b(c.expand_repeats),
                "recent" => {
                    if !v.is_empty() && c.recent_files.len() < MAX_RECENT && !c.recent_files.iter().any(|p| p == v) {
                        c.recent_files.push(v.to_string());
                    }
                }
                "audio_output" => c.audio_output = v.to_string(),
                "bars_per_row" => c.bars_per_row = v.parse().unwrap_or(c.bars_per_row).min(16),
                "layout_align_bar" => c.layout_align_bar = v.parse().unwrap_or(c.layout_align_bar).min(9999),
                "metro_bpm" => {
                    let x = v.parse::<f32>().ok().filter(|x| x.is_finite());
                    c.metro_bpm = x.unwrap_or(c.metro_bpm).clamp(20.0, 300.0)
                }
                "metro_beats" => c.metro_beats = v.parse().unwrap_or(c.metro_beats).clamp(1, 12),
                "metro_preview" => c.metro_preview = b(c.metro_preview),
                "controller_preset" => c.controller_preset = v.to_string(),
                "controller_port" => c.controller_port = v.to_string(),
                "rudiment" => c.rudiment = v.parse().ok().filter(|n| rudiments::by_number(*n).is_some()).unwrap_or(c.rudiment),
                "rudiment_repeats" => c.rudiment_repeats = v.parse().unwrap_or(c.rudiment_repeats).clamp(1, MAX_REPEATS),
                "rudiment_nard_only" => c.rudiment_nard_only = b(c.rudiment_nard_only),
                "rudiment_ladder" => c.rudiment_ladder = b(c.rudiment_ladder),
                "metro_sub" => c.metro_sub = v.parse().unwrap_or(c.metro_sub).min(3),
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
mod tests {
    use super::*;

    #[test]
    fn settings_of_the_old_name_are_copied_to_the_new_folder_once() {
        let base = std::env::temp_dir().join(format!("roll-model-migrate-{}", std::process::id()));
        let (old, new) = (base.join("DrumTutor"), base.join("RollModel"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(old.join("sounds")).unwrap();
        std::fs::write(old.join("config.txt"), "latency_ms = 7\nrecent = a.musicxml\n").unwrap();
        std::fs::write(old.join("sounds").join("Kick.wav"), b"wav").unwrap();

        Config::migrate(&old, &new).unwrap();
        let c = Config::from_text(&std::fs::read_to_string(new.join("config.txt")).unwrap());
        assert_eq!(c.latency_ms, 7.0);
        assert_eq!(c.recent_files, ["a.musicxml"]);
        assert!(new.join("sounds").join("Kick.wav").is_file());
        assert!(old.join("config.txt").is_file(), "the old folder is left alone");

        // Settings changed under the new name are never overwritten by the old ones.
        std::fs::write(new.join("config.txt"), "latency_ms = 9\n").unwrap();
        Config::migrate(&old, &new).unwrap();
        assert_eq!(std::fs::read_to_string(new.join("config.txt")).unwrap(), "latency_ms = 9\n");
        // Nothing to migrate is not an error.
        Config::migrate(&base.join("missing"), &base.join("other")).unwrap();
        assert!(!base.join("other").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn roundtrip() {
        let mut c = Config {
            latency_ms: 12.5,
            midi_in_port: "Roland V31 MIDI 1".into(),
            metronome_during_take: false,
            count_in_bars: 2,
            playback_audio: false,
            playback_volume: 0.25,
            playback_midi: true,
            playback_channel: 1,
            audio_output: "Speakers (USB Audio)".into(),
            bars_per_row: 4,
            layout_align_bar: 5,
            metro_bpm: 133.5,
            metro_beats: 7,
            metro_sub: 2,
            metro_preview: false,
            hihat_cc_max: 90,
            controller_preset: "Behringer X-Touch Mini".into(),
            controller_port: "X-TOUCH".into(),
            ..Default::default()
        };
        c.note_map.set_lane_notes(Lane::Kick, &[36, 33]);
        c.lane_gain.insert(Lane::Crash1, 0.4);
        c.lane_gain.insert(Lane::Kick, 1.25);
        c.lane_mute.insert(Lane::HiHatClosed);
        c.rudiment = 24;
        c.rudiment_repeats = 8;
        c.rudiment_nard_only = true;
        c.rudiment_ladder = true;
        c.rudiment_bpm.insert(24, 72.5);
        c.rudiment_best.insert(16, 96.0);
        let back = Config::from_text(&c.to_text());
        assert_eq!(back, c);
    }

    #[test]
    fn rudiment_settings_are_clamped_and_default_when_missing() {
        let c = Config::from_text(
            "rudiment = 99\nrudiment_repeats = 500\nrudiment_bpm.16 = 1000\nrudiment_bpm.99 = 80\nrudiment_bpm.x = 80\n\
             rudiment_best.20 = abc\nrudiment_best.21 = 88\nrudiment_ladder = maybe\n",
        );
        assert_eq!(c.rudiment, 1, "not a rudiment: keeps the default");
        assert_eq!(c.rudiment_repeats, MAX_REPEATS);
        assert_eq!(c.rudiment_bpm.get(&16), Some(&300.0));
        assert_eq!(c.rudiment_bpm.len(), 1, "unknown rudiments are dropped");
        assert_eq!(c.rudiment_best.len(), 1);
        assert_eq!(c.rudiment_best.get(&21), Some(&88.0));
        assert!(!c.rudiment_ladder);
        let old = Config::from_text("latency_ms = 5\n");
        assert_eq!((old.rudiment, old.rudiment_repeats), (1, DEFAULT_REPEATS));
        assert!(old.rudiment_bpm.is_empty() && old.rudiment_best.is_empty());
        assert!(!Config::default().to_text().contains("rudiment_bpm."), "nothing is written for a rudiment never practised");
    }

    #[test]
    fn mute_and_solo_decide_what_is_heard() {
        let mut c = Config::default();
        c.lane_gain.insert(Lane::Kick, 0.5);
        c.lane_mute.insert(Lane::Snare);
        let none = BTreeSet::new();
        assert_eq!(c.effective_gain(&none, Lane::Kick), 0.5);
        assert_eq!(c.effective_gain(&none, Lane::Snare), 0.0, "muted");
        assert_eq!(c.effective_gain(&none, Lane::Ride), 1.0);
        // Soloing silences everything else, even instruments that are not muted...
        let solo: BTreeSet<Lane> = [Lane::Snare, Lane::Ride].into_iter().collect();
        assert_eq!(c.effective_gain(&solo, Lane::Kick), 0.0);
        assert_eq!(c.effective_gain(&solo, Lane::Ride), 1.0);
        // ...and a solo wins over a mute.
        assert_eq!(c.effective_gain(&solo, Lane::Snare), 1.0);
    }

    #[test]
    fn mixer_layout_and_metronome_settings_are_clamped_and_default_when_missing() {
        let c = Config::from_text(
            "bars_per_row = 99\nlayout_align_bar = 1000000\nmetro_bpm = 5\nmetro_beats = 0\nmetro_sub = 9\n\
             gain.Ride = 9\ngain.Snare = abc\ngain.Nope = 1\nmute.Kick = true\nmute.Snare = maybe\naudio_output =\n",
        );
        assert_eq!((c.bars_per_row, c.layout_align_bar), (16, 9999));
        assert_eq!((c.metro_bpm, c.metro_beats, c.metro_sub), (20.0, 1, 3));
        assert_eq!(c.gain_of(Lane::Ride), MAX_LANE_GAIN);
        assert_eq!(c.gain_of(Lane::Snare), 1.0, "an unreadable gain keeps the default");
        assert!(c.lane_mute.contains(&Lane::Kick) && !c.lane_mute.contains(&Lane::Snare));
        assert_eq!(c.audio_output, "", "an empty device name means the system default");
        // A settings file from before these existed loads with the defaults.
        let old = Config::from_text("latency_ms = 5\n");
        assert_eq!((old.bars_per_row, old.layout_align_bar, old.metro_bpm, old.metro_beats), (0, 0, 100.0, 4));
        assert!(old.lane_gain.is_empty() && old.lane_mute.is_empty() && old.audio_output.is_empty());
        assert!(old.controller_preset.is_empty() && old.controller_port.is_empty(), "no controller unless chosen");
        // Defaults are not written out for the mixer.
        assert!(!Config::default().to_text().contains("gain."));
    }

    #[test]
    fn recent_files_keep_order_without_duplicates() {
        let mut c = Config::default();
        for p in ["a.musicxml", "b.musicxml", "c.musicxml", "a.musicxml"] {
            c.remember_recent(p);
        }
        assert_eq!(c.recent_files, ["a.musicxml", "c.musicxml", "b.musicxml"]);
        // The list survives a save and load in the same order, including paths with spaces and '='.
        c.remember_recent("D:\\My scores\\a=b.musicxml");
        assert_eq!(Config::from_text(&c.to_text()).recent_files, c.recent_files);
        for i in 0..2 * MAX_RECENT {
            c.remember_recent(&format!("score{i}.musicxml"));
        }
        assert_eq!(c.recent_files.len(), MAX_RECENT);
        assert_eq!(c.recent_files[0], format!("score{}.musicxml", 2 * MAX_RECENT - 1));
        // A hand-edited file with too many or repeated entries is cut down.
        let many: String = (0..30).map(|i| format!("recent = f{}.xml\n", i % 15)).collect();
        assert_eq!(Config::from_text(&many).recent_files.len(), MAX_RECENT);
        if cfg!(windows) {
            c.remember_recent("D:\\Scores\\Beat.musicxml");
            c.remember_recent("d:\\scores\\beat.musicxml");
            assert_eq!(c.recent_files.iter().filter(|p| p.to_lowercase() == "d:\\scores\\beat.musicxml").count(), 1);
        }
    }

    #[test]
    fn playback_settings_are_clamped_and_default_when_missing() {
        let c = Config::from_text("playback_volume = 7\nplayback_channel = 99\nplayback_audio = nonsense\n");
        assert_eq!(c.playback_volume, 1.0);
        assert_eq!(c.playback_channel, 16);
        assert!(c.playback_audio, "an unreadable value keeps the default");
        // A settings file from before playback existed still loads.
        let old = Config::from_text("latency_ms = 5\n");
        assert_eq!((old.playback_audio, old.playback_midi, old.playback_channel), (true, false, 10));
    }
}
