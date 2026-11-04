//! State shared between the UI and the sound threads, so settings can change
//! while sound is playing. Plain atomics: the sound threads read them for
//! every cue and never block on the UI.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

use crate::config::Passes;
use crate::drums::Lane;

fn load(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}

fn store(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Relaxed);
}

pub struct SoundSwitches {
    notes: AtomicBool,
    click: AtomicBool,
    /// The number of passes; 0 is unlimited.
    passes: AtomicU32,
    music_done: AtomicBool,
    volume: AtomicU32,
    click_volume: AtomicU32,
    /// Effective gain per instrument (volume, mute and solo already applied).
    lanes: [AtomicU32; Lane::ALL.len()],
}

impl SoundSwitches {
    pub fn new() -> Arc<SoundSwitches> {
        Arc::new(SoundSwitches {
            notes: AtomicBool::new(true),
            click: AtomicBool::new(true),
            passes: AtomicU32::new(1),
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

    /// How many passes the music goes through. Read at every pass boundary, so lowering it lets the
    /// current pass finish.
    pub fn passes(&self) -> Passes {
        match self.passes.load(Relaxed) {
            0 => Passes::Unlimited,
            n => Passes::new(n),
        }
    }
    pub fn set_passes(&self, passes: Passes) {
        self.passes.store(
            match passes {
                Passes::Count(n) => n.max(1),
                Passes::Unlimited => 0,
            },
            Relaxed,
        );
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
pub struct ErrorLog {
    messages: Arc<Mutex<Vec<String>>>,
    /// Sounds the thread could not hand to the audio engine, or MIDI messages the port refused.
    failed: Arc<AtomicU32>,
}

impl ErrorLog {
    pub fn push(&self, message: String) {
        crate::sync::locked(&self.messages).push(message);
    }

    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut *crate::sync::locked(&self.messages))
    }

    /// A sound that was not played: counted, and said once (the thread would otherwise say it for
    /// every note).
    pub fn play_failed(&self, why: crate::audio::PlayError) {
        if self.failed.fetch_add(1, Relaxed) == 0 {
            let why = match why {
                crate::audio::PlayError::NotReady => "the sound output has not started",
                crate::audio::PlayError::QueueFull => "the sound output is not keeping up",
            };
            self.push(format!("A sound was not played because {why}; more may follow."));
        }
    }

    /// A MIDI message the port refused: counted, and said once.
    pub fn midi_failed(&self, why: impl std::fmt::Display) {
        if self.failed.fetch_add(1, Relaxed) == 0 {
            self.push(format!("A MIDI message was not sent: {why}; more may follow."));
        }
    }

    /// How many sounds and MIDI messages were lost so far.
    #[cfg(test)]
    pub fn failed(&self) -> u32 {
        self.failed.load(Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_round_trips() {
        let c = SoundSwitches::new();
        assert!(c.notes() && c.click() && c.passes() == Passes::ONCE && !c.music_done());
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
        let c = SoundSwitches::new();
        for (i, lane) in Lane::ALL.iter().enumerate() {
            c.set_lane_gain(*lane, i as f32 / 10.0);
        }
        for (i, lane) in Lane::ALL.iter().enumerate() {
            assert_eq!(c.lane_gain(*lane), i as f32 / 10.0, "{lane:?}");
        }
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

    #[test]
    fn a_lost_sound_is_counted_every_time_and_said_once() {
        use crate::audio::PlayError;
        let log = ErrorLog::default();
        log.play_failed(PlayError::QueueFull);
        log.play_failed(PlayError::NotReady);
        log.midi_failed("port closed");
        assert_eq!(log.failed(), 3);
        let said = log.take();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].contains("not keeping up"), "{said:?}");
    }
}
