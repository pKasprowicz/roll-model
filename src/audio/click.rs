//! The metronome's click sounds at the stream's rate. The same three tones the program has always
//! used: a sine of 30 ms, higher for the accent and lower and softer between the beats.

use std::f64::consts::TAU;
use std::sync::Arc;

use crate::control::ClickLevel;

/// How long a click sounds.
pub const CLICK_SECS: f64 = 0.030;

/// Pitch in Hz and how much of the click volume the level gets.
fn tone(level: ClickLevel) -> (f64, f32) {
    match level {
        ClickLevel::Accent => (1760.0, 1.0),
        ClickLevel::Beat => (1175.0, 1.0),
        ClickLevel::Sub => (880.0, 0.6),
    }
}

/// The three clicks, made once for a rate.
pub struct ClickBank {
    sounds: [(Arc<[f32]>, f32); 3],
}

impl ClickBank {
    pub fn new(rate: u32) -> ClickBank {
        let make = |level| {
            let (freq, soften) = tone(level);
            let frames = (CLICK_SECS * rate as f64).round() as usize;
            let sine: Arc<[f32]> = (0..frames).map(|i| (TAU * freq * i as f64 / rate as f64).sin() as f32).collect::<Vec<_>>().into();
            (sine, soften)
        };
        ClickBank { sounds: [make(ClickLevel::Accent), make(ClickLevel::Beat), make(ClickLevel::Sub)] }
    }

    /// The sound of a click, at unit level, and the gain that goes with `volume` (0 to 1) for it.
    pub fn get(&self, level: ClickLevel, volume: f32) -> (&Arc<[f32]>, f32) {
        let (sound, soften) = &self.sounds[match level {
            ClickLevel::Accent => 0,
            ClickLevel::Beat => 1,
            ClickLevel::Sub => 2,
        }];
        (sound, (volume * soften).clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crossings(s: &[f32]) -> usize {
        s.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    #[test]
    fn a_click_is_thirty_milliseconds_of_a_sine_at_the_rate() {
        for rate in [44_100, 48_000, 96_000] {
            let bank = ClickBank::new(rate);
            let (accent, gain) = bank.get(ClickLevel::Accent, 0.5);
            assert_eq!(accent.len(), (0.030 * rate as f64).round() as usize);
            assert_eq!(accent[0], 0.0);
            assert!(accent.iter().all(|s| s.abs() <= 1.0));
            assert_eq!(gain, 0.5);
            // 1760 Hz for 30 ms is 52.8 periods.
            assert!((crossings(accent) as i32 - 53).abs() <= 1, "{}", crossings(accent));
        }
    }

    #[test]
    fn the_levels_differ_in_pitch_and_the_soft_one_in_loudness() {
        let bank = ClickBank::new(48_000);
        let (beat, beat_gain) = bank.get(ClickLevel::Beat, 0.8);
        let (sub, sub_gain) = bank.get(ClickLevel::Sub, 0.8);
        assert!(crossings(beat) > crossings(sub));
        assert!((beat_gain - 0.8).abs() < 1e-6 && (sub_gain - 0.48).abs() < 1e-6);
        assert_eq!(bank.get(ClickLevel::Accent, 3.0).1, 1.0, "the volume is kept in range");
    }
}
