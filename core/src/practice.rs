//! Practice logic with no window in it: the tap tempo, moving the selected bars, the measure a
//! take went worst in, the volume steps, the tempo ramp and the strength of a click. It is here so
//! that it can be tested without the application.

use std::time::{Duration, Instant};

use crate::analysis::Report;
use crate::score::Score;

/// Taps further apart than this start a new tap tempo.
pub const TAP_GAP: Duration = Duration::from_secs(2);
/// Taps that count towards the tempo.
pub const TAPS_KEPT: usize = 6;

/// Adds a tap and returns the tempo (beats per minute) they give, once there are two.
pub fn register_tap(taps: &mut Vec<Instant>, now: Instant) -> Option<f64> {
    if taps.last().is_some_and(|last| now.saturating_duration_since(*last) > TAP_GAP) {
        taps.clear();
    }
    taps.push(now);
    if taps.len() > TAPS_KEPT {
        taps.remove(0);
    }
    if taps.len() < 2 {
        return None;
    }
    let span = taps[taps.len() - 1].saturating_duration_since(taps[0]).as_secs_f64();
    (span > 0.0).then(|| 60.0 * (taps.len() - 1) as f64 / span)
}

/// Which way the selected bars move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Back,
    Forward,
}

/// The range of the same length before or after `from..=to` in a score of `n` measures.
pub fn shifted_range(from: usize, to: usize, n: usize, direction: Direction) -> Option<(usize, usize)> {
    let len = to.saturating_sub(from) + 1;
    if direction == Direction::Forward {
        (to + 1 < n).then(|| (from + len, (to + len).min(n - 1)))
    } else {
        (from > 0).then(|| {
            let from = from.saturating_sub(len);
            (from, from + len - 1)
        })
    }
}

/// The measure the last take went worst in: the most missed, wrong or
/// sloppy notes and extra hits (`extra_in`: the measure of each). `None` if all was perfect.
pub fn worst_measure(score: &Score, report: &Report, extra_in: &[usize]) -> Option<usize> {
    let mut loss = vec![0.0_f64; score.measures.len()];
    for (e, r) in report.expected.iter().zip(&report.results) {
        let Some(grade) = r.outcome.grade() else { continue };
        if let Some(slot) = score.notes.get(e.note_idx).and_then(|n| loss.get_mut(n.measure)) {
            *slot += 1.0 - grade.credit();
        }
    }
    for m in extra_in {
        if let Some(slot) = loss.get_mut(*m) {
            *slot += 1.0;
        }
    }
    let (worst, most) = loss.iter().copied().enumerate().fold((0, 0.0), |best, (i, l)| if l > best.1 { (i, l) } else { best });
    (most > 1e-9).then_some(worst)
}

/// Steps of 2 % on a 0..1 volume, without the rounding errors adding up.
pub fn nudged_volume(v: f32, steps: i32) -> f32 {
    (((v + 0.02 * steps as f32) * 50.0).round() / 50.0).clamp(0.0, 1.0)
}

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

/// A tempo ramp for a looping run: every `every` passes the tempo rises by `step`, a fraction of
/// the starting tempo (0.05 = five per cent of it), up to `MAX_FACTOR` times the starting tempo.
/// Cue times are given for the starting tempo; the player squeezes the later passes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ramp {
    /// 0 = no ramp.
    pub every: usize,
    pub step: f64,
}

impl Ramp {
    /// The tempo never rises beyond this many times the starting tempo.
    pub const MAX_FACTOR: f64 = 3.0;

    /// How many times faster than the start pass number `pass` (the first is 0) is played.
    pub fn factor(self, pass: usize) -> f64 {
        if self.every == 0 || self.step <= 0.0 {
            return 1.0;
        }
        (1.0 + self.step * (pass / self.every) as f64).min(Self::MAX_FACTOR)
    }

    pub fn is_off(self) -> bool {
        self.every == 0 || self.step <= 0.0
    }

    /// Where `elapsed` real seconds into the run are: the pass (from 0) and the seconds into that
    /// pass counted at the starting tempo. Before the start (the count-in) nothing is squeezed.
    pub fn locate(self, pass_len: f64, elapsed: f64) -> (usize, f64) {
        let pass_len = pass_len.max(0.001);
        if elapsed < 0.0 || self.is_off() {
            return if elapsed < 0.0 { (0, elapsed) } else { ((elapsed / pass_len) as usize, elapsed % pass_len) };
        }
        let (mut left, mut pass) = (elapsed, 0usize);
        loop {
            let f = self.factor(pass);
            let real_len = pass_len / f;
            if f >= Self::MAX_FACTOR {
                // From here on every pass is the same length.
                let whole = (left / real_len).floor();
                return (pass + whole as usize, (left - whole * real_len) * f);
            }
            if left < real_len {
                return (pass, left * f);
            }
            left -= real_len;
            pass += 1;
        }
    }

    /// `elapsed` real seconds into the run, as seconds at the starting tempo: what a clock or a
    /// playhead that does not know about the ramp would show.
    pub fn base_seconds(self, pass_len: f64, elapsed: f64) -> f64 {
        let (pass, into) = self.locate(pass_len, elapsed);
        if elapsed < 0.0 {
            return elapsed;
        }
        pass as f64 * pass_len.max(0.001) + into
    }

    /// When pass `pass` starts, in real seconds after the start of the first.
    pub fn start_of(self, pass_len: f64, pass: usize) -> f64 {
        (0..pass).map(|p| pass_len / self.factor(p)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{analyze, expected_notes, Hit, Tolerances};
    use crate::import::{import_bytes, ImportOptions};
    use crate::units::Seconds;

    const SAMPLE: &str = include_str!("../../examples/basic_rock_beat.musicxml");

    #[test]
    fn tapping_gives_the_tempo_and_a_pause_starts_over() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut taps = Vec::new();
        assert_eq!(register_tap(&mut taps, at(0)), None);
        let bpm = register_tap(&mut taps, at(500)).unwrap();
        assert!((bpm - 120.0).abs() < 1e-6);
        assert!((register_tap(&mut taps, at(1000)).unwrap() - 120.0).abs() < 1e-6);
        // Slowing down: the average follows.
        let slower = register_tap(&mut taps, at(1700)).unwrap();
        assert!(slower < 120.0 && slower > 90.0, "{slower}");
        // A long pause starts a new tap tempo.
        assert_eq!(register_tap(&mut taps, at(10_000)), None);
        assert_eq!(taps.len(), 1);
        // Only the last few taps count.
        for i in 1..20 {
            register_tap(&mut taps, at(10_000 + i * 400));
        }
        assert_eq!(taps.len(), TAPS_KEPT);
    }

    #[test]
    fn the_range_moves_by_its_own_length_and_stops_at_the_ends() {
        assert_eq!(shifted_range(0, 3, 16, Direction::Forward), Some((4, 7)));
        assert_eq!(shifted_range(4, 7, 16, Direction::Back), Some((0, 3)));
        assert_eq!(shifted_range(0, 3, 16, Direction::Back), None);
        assert_eq!(shifted_range(12, 15, 16, Direction::Forward), None);
        assert_eq!(shifted_range(10, 13, 16, Direction::Forward), Some((14, 15)), "the last chunk may be shorter");
        assert_eq!(shifted_range(2, 5, 16, Direction::Back), Some((0, 3)), "not below the first measure");
        assert_eq!(shifted_range(3, 3, 8, Direction::Forward), Some((4, 4)));
    }

    #[test]
    fn the_worst_measure_is_where_most_goes_wrong() {
        let score = &import_bytes(SAMPLE.as_bytes(), "sample.musicxml", &ImportOptions::default()).unwrap().score;
        let last = score.measures.len() - 1;
        let expected = expected_notes(score, 0, last, 1.0);
        let tol = &Tolerances::default();
        // Everything played perfectly, except that measure 2 (index 1) is left out.
        let hits: Vec<Hit> = expected
            .iter()
            .filter(|e| score.notes[e.note_idx].measure != 1)
            .map(|e| Hit { t: e.t, note: 36, velocity: 100, lane: e.lane })
            .collect();
        let report = analyze(&expected, &hits, tol, Seconds::INFINITY);
        assert_eq!(worst_measure(score, &report, &[]), Some(1));
        // Enough stray hits in measure 4 make it the worst one.
        let strays = vec![last; expected.len()];
        assert_eq!(worst_measure(score, &report, &strays), Some(last));
        // Nothing wrong, nothing to practise.
        let all: Vec<Hit> = expected.iter().map(|e| Hit { t: e.t, note: 36, velocity: 100, lane: e.lane }).collect();
        let perfect = analyze(&expected, &all, tol, Seconds::INFINITY);
        assert_eq!(worst_measure(score, &perfect, &[]), None);
    }

    #[test]
    fn click_levels() {
        assert_eq!(ClickLevel::from_accent(true), ClickLevel::Accent);
        assert_eq!(ClickLevel::from_accent(false), ClickLevel::Beat);
        assert!(ClickLevel::Accent.velocity() > ClickLevel::Beat.velocity());
        assert!(ClickLevel::Beat.velocity() > ClickLevel::Sub.velocity());
    }
}
