//! What a run of the music is made of: the cues (a click or a drum note, `t` seconds from the
//! start, negative in the count-in) that the scheduler hands to the sound devices. Nothing here
//! plays anything.

use crate::analysis::Expected;
use crate::clock::ClockMsg;
use crate::config::Subdivision;
use crate::drums::Lane;
use crate::practice::ClickLevel;
use crate::score::{Click, Dynamic};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sound {
    Click {
        level: ClickLevel,
    },
    Drum {
        lane: Lane,
        velocity: u8,
    },
    /// A message of the MIDI clock (see `clock.rs`).
    Clock(ClockMsg),
}

/// Something to play `t` seconds after the player's zero (negative = count-in).
#[derive(Clone, Copy, Debug)]
pub struct Cue {
    pub t: f64,
    pub sound: Sound,
}

/// MIDI velocity for a score note.
pub fn velocity_for(e: &Expected) -> u8 {
    if e.dynamic == Dynamic::Ghost {
        45
    } else if e.dynamic == Dynamic::Accent {
        120
    } else if e.grace {
        70
    } else {
        90
    }
}

/// The notes of a take and its click track as one list ordered by time. At
/// the same instant the click comes first.
pub fn cues(expected: &[Expected], clicks: &[Click]) -> Vec<Cue> {
    let mut v: Vec<Cue> = clicks
        .iter()
        .map(|c| Cue { t: c.t.0, sound: Sound::Click { level: ClickLevel::from_accent(c.accent) } })
        .chain(expected.iter().map(|e| Cue { t: e.t.0, sound: Sound::Drum { lane: e.lane, velocity: velocity_for(e) } }))
        .collect();
    v.sort_by(|a, b| a.t.total_cmp(&b.t)); // stable
    v
}

/// One bar of the free metronome and its length in seconds: an accent on the
/// first beat, normal clicks on the other beats, soft clicks between them.
pub fn metronome_cues(bpm: f64, beats: u32, sub: Subdivision) -> (Vec<Cue>, f64) {
    let beat_sec = 60.0 / bpm.max(1.0);
    let parts = sub.parts();
    let mut v = Vec::new();
    for b in 0..beats.max(1) {
        for s in 0..parts {
            let level = match (b, s) {
                (0, 0) => ClickLevel::Accent,
                (_, 0) => ClickLevel::Beat,
                _ => ClickLevel::Sub,
            };
            v.push(Cue { t: (b * parts + s) as f64 * beat_sec / parts as f64, sound: Sound::Click { level } });
        }
    }
    (v, beats.max(1) as f64 * beat_sec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Seconds;

    fn note(lane: Lane, t: f64) -> Expected {
        Expected { note_idx: 0, t: Seconds(t), lane, dynamic: Dynamic::Normal, grace: false, hand: None }
    }

    #[test]
    fn cues_are_ordered_and_clicks_lead_at_ties() {
        let mut ghost = note(Lane::Snare, 1.0);
        ghost.dynamic = Dynamic::Ghost;
        let mut accent = note(Lane::Crash1, 0.0);
        accent.dynamic = Dynamic::Accent;
        let c = cues(
            &[ghost, accent, note(Lane::Kick, 0.0)],
            &[Click { t: Seconds(-1.0), accent: true }, Click { t: Seconds(0.0), accent: true }, Click { t: Seconds(1.0), accent: false }],
        );
        let times: Vec<f64> = c.iter().map(|c| c.t).collect();
        assert_eq!(times, vec![-1.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
        assert_eq!(c[1].sound, Sound::Click { level: ClickLevel::Accent }); // the click, then the two notes
        assert_eq!(c[2].sound, Sound::Drum { lane: Lane::Crash1, velocity: 120 });
        assert_eq!(c[3].sound, Sound::Drum { lane: Lane::Kick, velocity: 90 });
        assert_eq!(c[4].sound, Sound::Click { level: ClickLevel::Beat });
        assert_eq!(c[5].sound, Sound::Drum { lane: Lane::Snare, velocity: 45 });
    }

    #[test]
    fn metronome_bar_has_an_accent_beats_and_soft_subdivisions() {
        let (bar, len) = metronome_cues(120.0, 3, Subdivision::Eighths);
        assert!((len - 1.5).abs() < 1e-9, "three beats at 120 BPM");
        let levels: Vec<ClickLevel> = bar
            .iter()
            .map(|c| match c.sound {
                Sound::Click { level } => level,
                _ => panic!("only clicks"),
            })
            .collect();
        use ClickLevel::*;
        assert_eq!(levels, [Accent, Sub, Beat, Sub, Beat, Sub]);
        let times: Vec<f64> = bar.iter().map(|c| c.t).collect();
        for (got, want) in times.iter().zip([0.0, 0.25, 0.5, 0.75, 1.0, 1.25]) {
            assert!((got - want).abs() < 1e-9, "{got} vs {want}");
        }
        assert_eq!(metronome_cues(60.0, 4, Subdivision::None).0.len(), 4);
        assert_eq!(metronome_cues(60.0, 4, Subdivision::Triplets).0.len(), 12);
        assert_eq!(metronome_cues(60.0, 2, Subdivision::Sixteenths).0.len(), 8);
        assert_eq!(metronome_cues(60.0, 0, Subdivision::None).0.len(), 1, "at least one beat");
    }
}
