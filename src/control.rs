//! State shared between the UI and the sound threads, so settings can change
//! while sound is playing. Plain atomics: the sound threads read them for
//! every cue and never block on the UI.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

use crate::drums::Lane;

/// How strong a metronome click is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickLevel {
    /// First beat of a bar.
    Accent,
    Beat,
    /// A subdivision between beats (eighths, sixteenths, triplets).
    Sub,
}

impl ClickLevel {
    pub fn from_accent(accent: bool) -> ClickLevel {
        if accent {
            ClickLevel::Accent
        } else {
            ClickLevel::Beat
        }
    }

    /// MIDI velocity of the click note.
    pub fn velocity(self) -> u8 {
        match self {
            ClickLevel::Accent => 127,
            ClickLevel::Beat => 90,
            ClickLevel::Sub => 55,
        }
    }
}

fn load(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}

fn store(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Relaxed);
}

pub struct Control {
    notes: AtomicBool,
    click: AtomicBool,
    looping: AtomicBool,
    music_done: AtomicBool,
    volume: AtomicU32,
    click_volume: AtomicU32,
    /// Effective gain per instrument (volume, mute and solo already applied).
    lanes: [AtomicU32; Lane::ALL.len()],
}

impl Control {
    pub fn new() -> Arc<Control> {
        Arc::new(Control {
            notes: AtomicBool::new(true),
            click: AtomicBool::new(true),
            looping: AtomicBool::new(false),
            music_done: AtomicBool::new(false),
            volume: AtomicU32::new(0.7_f32.to_bits()),
            click_volume: AtomicU32::new(0.5_f32.to_bits()),
            lanes: std::array::from_fn(|_| AtomicU32::new(1.0_f32.to_bits())),
        })
    }

    /// Play the drum notes of the score (off = a click track).
    pub fn notes(&self) -> bool {
        self.notes.load(Relaxed)
    }
    pub fn set_notes(&self, on: bool) {
        self.notes.store(on, Relaxed);
    }

    /// Play the click while the music runs. The count-in is not affected.
    pub fn click(&self) -> bool {
        self.click.load(Relaxed)
    }
    pub fn set_click(&self, on: bool) {
        self.click.store(on, Relaxed);
    }

    /// Start another pass when the music ends. Read at every pass boundary,
    /// so switching it off lets the current pass finish.
    pub fn looping(&self) -> bool {
        self.looping.load(Relaxed)
    }
    pub fn set_looping(&self, on: bool) {
        self.looping.store(on, Relaxed);
    }

    /// Set by the playback thread when the last pass has reached its end.
    pub fn music_done(&self) -> bool {
        self.music_done.load(Relaxed)
    }
    pub fn set_music_done(&self, done: bool) {
        self.music_done.store(done, Relaxed);
    }

    /// 0..=1, for the drums.
    pub fn volume(&self) -> f32 {
        load(&self.volume)
    }
    pub fn set_volume(&self, v: f32) {
        store(&self.volume, v);
    }

    /// 0..=1, for the click.
    pub fn click_volume(&self) -> f32 {
        load(&self.click_volume)
    }
    pub fn set_click_volume(&self, v: f32) {
        store(&self.click_volume, v);
    }

    /// Gain of one instrument: 0 = silent, 1 = as rendered.
    pub fn lane_gain(&self, lane: Lane) -> f32 {
        load(&self.lanes[lane as usize])
    }
    pub fn set_lane_gain(&self, lane: Lane, gain: f32) {
        store(&self.lanes[lane as usize], gain.max(0.0));
    }
}

/// Problems a sound thread ran into (no sound device, MIDI port missing),
/// collected for the UI to show.
#[derive(Clone, Default)]
pub struct ErrorLog(Arc<Mutex<Vec<String>>>);

impl ErrorLog {
    pub fn push(&self, message: String) {
        if let Ok(mut v) = self.0.lock() {
            v.push(message);
        }
    }

    pub fn take(&self) -> Vec<String> {
        self.0.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_round_trips() {
        let c = Control::new();
        assert!(c.notes() && c.click() && !c.looping() && !c.music_done());
        for lane in Lane::ALL {
            assert_eq!(c.lane_gain(lane), 1.0);
        }
        c.set_click(false);
        c.set_lane_gain(Lane::Ride, 0.25);
        c.set_lane_gain(Lane::Kick, -3.0);
        c.set_volume(0.3);
        c.set_click_volume(0.9);
        assert!(!c.click());
        assert_eq!((c.lane_gain(Lane::Ride), c.lane_gain(Lane::Kick)), (0.25, 0.0), "negative gain is clamped to silence");
        assert_eq!((c.volume(), c.click_volume()), (0.3, 0.9));
        assert_eq!(c.lane_gain(Lane::Snare), 1.0, "other instruments are untouched");
    }

    #[test]
    fn every_lane_has_its_own_slot() {
        let c = Control::new();
        for (i, lane) in Lane::ALL.iter().enumerate() {
            c.set_lane_gain(*lane, i as f32 / 10.0);
        }
        for (i, lane) in Lane::ALL.iter().enumerate() {
            assert_eq!(c.lane_gain(*lane), i as f32 / 10.0, "{lane:?}");
        }
    }

    #[test]
    fn click_levels() {
        assert_eq!(ClickLevel::from_accent(true), ClickLevel::Accent);
        assert_eq!(ClickLevel::from_accent(false), ClickLevel::Beat);
        assert!(ClickLevel::Accent.velocity() > ClickLevel::Beat.velocity());
        assert!(ClickLevel::Beat.velocity() > ClickLevel::Sub.velocity());
    }

    #[test]
    fn error_log_hands_each_message_out_once() {
        let log = ErrorLog::default();
        let other = log.clone();
        log.push("a".into());
        other.push("b".into());
        assert_eq!(log.take(), ["a", "b"]);
        assert!(other.take().is_empty());
    }
}
