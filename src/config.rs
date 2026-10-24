//! Persistent settings, stored as a small `key = value` text file:
//! Windows `%APPDATA%\DrumTutor\config.txt`, macOS
//! `~/Library/Application Support/DrumTutor/config.txt`, otherwise
//! `$XDG_CONFIG_HOME/drum-tutor/config.txt` (or `~/.config/...`).

use std::path::PathBuf;

use crate::analysis::Tolerances;
use crate::drums::{format_note_list, parse_note_list, Lane, NoteMap};

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
    pub keyboard_input: bool,
    pub zoom: f32,
    pub expand_repeats: bool,
    /// Scores opened before, most recent first (absolute paths).
    pub recent_files: Vec<String>,
}

/// How many scores the welcome panel remembers.
pub const MAX_RECENT: usize = 10;

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
            keyboard_input: false,
            zoom: 1.0,
            expand_repeats: true,
            recent_files: Vec::new(),
        }
    }
}

impl Config {
    /// Puts `path` at the top of the recent list (without duplicates; paths
    /// compare case-insensitively on Windows) and trims the list.
    pub fn remember_recent(&mut self, path: &str) {
        let same = |a: &str, b: &str| if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b };
        self.recent_files.retain(|p| !same(p, path));
        self.recent_files.insert(0, path.to_string());
        self.recent_files.truncate(MAX_RECENT);
    }

    pub fn path() -> Option<PathBuf> {
        if cfg!(windows) {
            std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("DrumTutor").join("config.txt"))
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join("Library/Application Support/DrumTutor/config.txt"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
                .map(|p| p.join("drum-tutor").join("config.txt"))
        }
    }

    pub fn load() -> Config {
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
        let mut s = String::from("# Drum Tutor settings\n");
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
        kv("keyboard_input", self.keyboard_input.to_string());
        kv("zoom", self.zoom.to_string());
        kv("expand_repeats", self.expand_repeats.to_string());
        for path in &self.recent_files {
            kv("recent", path.clone());
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
                "keyboard_input" => c.keyboard_input = b(c.keyboard_input),
                "zoom" => c.zoom = v.parse::<f32>().unwrap_or(c.zoom).clamp(0.3, 4.0),
                "expand_repeats" => c.expand_repeats = b(c.expand_repeats),
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
            ..Default::default()
        };
        c.note_map.set_lane_notes(Lane::Kick, &[36, 33]);
        let back = Config::from_text(&c.to_text());
        assert_eq!(back, c);
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
