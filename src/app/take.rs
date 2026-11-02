//! One recorded attempt over a range of measures, and the plan it follows: when it starts, which
//! hits it holds, how it is graded as it goes and when it is over. Nothing here knows the
//! application; a take can be driven in a test with the times given to it.

use std::time::Instant;

use crate::analysis::{analyze_with, Expected, Grade, Hit, Report, Tolerances, Window};
use crate::drums::Lane;
use crate::render::{HitStatus, PlayedNote};
use crate::score::{Click, Score};
use crate::units::{Quarters, Seconds};

/// How often, while a take records, it is graded again when no hit came: grading the whole take is
/// not something to do on every frame.
const GRADING_INTERVAL: Seconds = Seconds(0.05);

/// How long after its last note a take goes on listening before it ends by itself.
const TAIL: Seconds = Seconds(1.0);

/// Why a take ends: the player stopped it, or its music came to the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StopReason {
    Aborted,
    Completed,
}

/// Time `t` against `zero`, negative before it.
pub(super) fn signed_secs(t: Instant, zero: Instant) -> Seconds {
    if t >= zero {
        Seconds((t - zero).as_secs_f64())
    } else {
        Seconds(-(zero - t).as_secs_f64())
    }
}

/// What a take or a playback of the selected range needs: where time zero is and the click track.
pub(super) struct Plan {
    pub(super) from: usize,
    pub(super) to: usize,
    pub(super) scale: f64,
    pub(super) bpm: f64,
    pub(super) beat_sec: Seconds,
    pub(super) count_in_beats: u32,
    pub(super) zero: Instant,
    pub(super) t0: Seconds,
    pub(super) end_sec: Seconds,
    /// Count-in and beat clicks, seconds from the first measure.
    pub(super) clicks: Vec<Click>,
}

/// One recorded attempt over a measure range.
pub(super) struct Take {
    number: usize,
    /// What was played: the score's title, or the rudiment with its repetitions.
    label: String,
    /// How far from a note a hit counts for it: adaptive for rudiments, whose notes are close.
    window: Window,
    /// Score time zero = start of `from` measure.
    zero: Instant,
    from: usize,
    to: usize,
    tempo_scale: f64,
    /// Seconds from score start to the `from` measure (at `tempo_scale`).
    t0: Seconds,
    end_sec: Seconds,
    expected: Vec<Expected>,
    hits: Vec<Hit>,
    report: Report,
    /// How many hits there were, and how far into the take it was, when `report` was worked out.
    graded_at: Option<(usize, Seconds)>,
    running: bool,
    count_in_beat_sec: Seconds,
}

impl Take {
    /// A take that has begun, following `plan`: nothing is due yet, so nothing is graded.
    pub(super) fn start(number: usize, label: String, window: Window, plan: &Plan, expected: Vec<Expected>, tol: &Tolerances) -> Take {
        Take {
            number,
            zero: plan.zero,
            from: plan.from,
            to: plan.to,
            tempo_scale: plan.scale,
            t0: plan.t0,
            end_sec: plan.end_sec,
            report: analyze_with(&expected, &[], tol, Seconds::NEG_INFINITY, window),
            graded_at: None,
            label,
            window,
            expected,
            hits: Vec::new(),
            running: true,
            count_in_beat_sec: plan.beat_sec,
        }
    }

    pub(super) fn number(&self) -> usize {
        self.number
    }

    pub(super) fn label(&self) -> &str {
        &self.label
    }

    pub(super) fn window(&self) -> Window {
        self.window
    }

    pub(super) fn is_running(&self) -> bool {
        self.running
    }

    pub(super) fn report(&self) -> &Report {
        &self.report
    }

    pub(super) fn expected(&self) -> &[Expected] {
        &self.expected
    }

    pub(super) fn hits(&self) -> &[Hit] {
        &self.hits
    }

    #[cfg(test)]
    pub(super) fn set_hits(&mut self, hits: Vec<Hit>) {
        self.hits = hits;
    }

    pub(super) fn zero(&self) -> Instant {
        self.zero
    }

    /// Seconds of one beat of the first measure, for the count-in.
    pub(super) fn count_in_beat_sec(&self) -> Seconds {
        self.count_in_beat_sec
    }

    /// The position in the score of the moment `elapsed` seconds from time zero.
    pub(super) fn q_at(&self, score: &Score, elapsed: Seconds) -> Quarters {
        score.tempo.sec_to_q(elapsed + self.t0, self.tempo_scale)
    }

    /// The tempo it was played at, in BPM.
    pub(super) fn bpm(&self, score: &Score) -> f64 {
        score.measures().get(self.from).map_or(0.0, |m| score.tempo.bpm_at(m.start_q) * self.tempo_scale)
    }

    /// The measures it covered, as the score prints their numbers.
    pub(super) fn range_text(&self, score: &Score) -> String {
        match (score.measures().get(self.from), score.measures().get(self.to)) {
            (Some(a), Some(b)) => format!("{}–{}", a.number, b.number),
            _ => String::new(),
        }
    }

    /// A hit of the drums: `latency` is taken off, and a hit long before the count-in is no part of
    /// the take. Does nothing once the take is over.
    pub(super) fn record_hit(&mut self, at: Instant, note: u8, velocity: u8, lane: Lane, latency: Seconds) {
        if !self.running {
            return;
        }
        let t = signed_secs(at, self.zero) - latency;
        if t > -self.count_in_beat_sec * 0.75 {
            self.hits.push(Hit { t, note, velocity, lane });
        }
    }

    /// One look, every frame, at `now`: a new hit is graded at once, otherwise only now and then,
    /// for the notes that fall due. `true` when the take has run past its end and is to be stopped.
    pub(super) fn update(&mut self, tol: &Tolerances, now: Instant) -> bool {
        if !self.running {
            return false;
        }
        let elapsed = signed_secs(now, self.zero);
        let due = self.graded_at.is_none_or(|(hits, at)| hits != self.hits.len() || elapsed - at >= GRADING_INTERVAL);
        if due {
            self.report = analyze_with(&self.expected, &self.hits, tol, elapsed, self.window);
            self.graded_at = Some((self.hits.len(), elapsed));
        }
        elapsed > self.end_sec + TAIL
    }

    /// Ends the take and grades it for good: the notes after the stop are left ungraded, unless the
    /// whole range was played (`Completed`). `false` if it was over already.
    pub(super) fn finish(&mut self, kind: StopReason, tol: &Tolerances, now: Instant) -> bool {
        if !self.running {
            return false;
        }
        self.running = false;
        let elapsed = if kind == StopReason::Completed { Seconds::INFINITY } else { signed_secs(now, self.zero) };
        self.report = analyze_with(&self.expected, &self.hits, tol, elapsed, self.window);
        true
    }

    /// The hits drawn under the score, each with how it was graded.
    pub(super) fn played_notes(&self, score: &Score) -> Vec<PlayedNote> {
        self.hits
            .iter()
            .enumerate()
            .map(|(hi, h)| {
                let (status, dt) = match self.report.hit_match.get(hi).copied().flatten() {
                    Some(ei) => {
                        let r = self.report.results[ei];
                        (r.outcome.grade().map_or(HitStatus::Neutral, HitStatus::Graded), r.dt)
                    }
                    None if h.t > self.end_sec + Seconds(0.2) || h.t < Seconds(-0.2) => (HitStatus::Neutral, None),
                    None => (HitStatus::Extra, None),
                };
                PlayedNote { q: self.q_at(score, h.t).0, lane: h.lane, velocity: h.velocity, status, dt }
            })
            .collect()
    }

    /// How each note of the score was graded (`None`: not in the range, or not due yet).
    pub(super) fn grades(&self, notes: usize) -> Vec<Option<Grade>> {
        let mut g = vec![None; notes];
        for (e, r) in self.expected.iter().zip(&self.report.results) {
            if let Some(slot) = g.get_mut(e.note_idx) {
                *slot = r.outcome.grade();
            }
        }
        g
    }

    /// The measure of each hit that matched no note.
    pub(super) fn measures_of_extra_hits(&self, score: &Score) -> Vec<usize> {
        self.hits
            .iter()
            .zip(&self.report.hit_match)
            .filter(|(_, matched)| matched.is_none())
            .filter_map(|(h, _)| score.measure_at(self.q_at(score, h.t)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::expected_notes;
    use crate::import::{import_bytes, ImportOptions};
    use std::time::Duration;

    const SAMPLE: &str = include_str!("../../examples/basic_rock_beat.musicxml");

    fn score() -> Score {
        import_bytes(SAMPLE.as_bytes(), "sample.musicxml", &ImportOptions::default()).unwrap().score
    }

    /// A take of the whole sample at its own tempo, with time zero at `zero`.
    fn take(score: &Score, zero: Instant) -> Take {
        let last = score.measures().len() - 1;
        let expected = expected_notes(score, 0, last, 1.0);
        let end_sec = score.tempo.q_to_sec(score.measures()[last].end_q(), 1.0);
        let plan = Plan {
            from: 0,
            to: last,
            scale: 1.0,
            bpm: 100.0,
            beat_sec: Seconds(0.6),
            count_in_beats: 4,
            zero,
            t0: Seconds::ZERO,
            end_sec,
            clicks: Vec::new(),
        };
        Take::start(1, "Sample".into(), Window::Fixed, &plan, expected, &Tolerances::default())
    }

    #[test]
    fn a_take_begins_with_nothing_graded_and_records_hits_against_its_zero() {
        let (score, zero) = (score(), Instant::now() + Duration::from_secs(3));
        let mut t = take(&score, zero);
        assert!(t.is_running() && t.hits().is_empty() && t.report().evaluated == 0);
        t.record_hit(zero + Duration::from_millis(10), 38, 100, Lane::Snare, Seconds(0.004));
        assert_eq!(t.hits().len(), 1);
        assert!((t.hits()[0].t.0 - 0.006).abs() < 1e-6, "the latency is taken off: {}", t.hits()[0].t);
        t.record_hit(zero - Duration::from_secs(10), 38, 100, Lane::Snare, Seconds::ZERO);
        assert_eq!(t.hits().len(), 1, "a hit long before the count-in is not part of it");
    }

    #[test]
    fn a_new_hit_is_graded_at_once_and_otherwise_only_every_fifty_milliseconds() {
        let (score, zero) = (score(), Instant::now());
        let tol = Tolerances::default();
        let mut t = take(&score, zero);
        let at = |ms: u64| zero + Duration::from_millis(ms);
        t.update(&tol, at(100));
        assert_eq!(t.graded_at.map(|(n, _)| n), Some(0));
        let first = t.graded_at;
        t.update(&tol, at(120));
        assert_eq!(t.graded_at, first, "20 ms later and no hit: not again");
        t.record_hit(at(130), 36, 100, Lane::Kick, Seconds::ZERO);
        t.update(&tol, at(131));
        assert_eq!(t.graded_at.map(|(n, _)| n), Some(1), "a hit: at once");
        t.update(&tol, at(200));
        assert!(t.graded_at.unwrap().1 > Seconds(0.19), "50 ms later: again");
    }

    #[test]
    fn a_take_ends_by_itself_a_second_after_its_last_note() {
        let (score, zero) = (score(), Instant::now());
        let tol = Tolerances::default();
        let mut t = take(&score, zero);
        let end = t.end_sec;
        assert!(!t.update(&tol, zero + Duration::from_secs_f64(end.0 + 0.5)));
        assert!(t.update(&tol, zero + Duration::from_secs_f64(end.0 + 1.1)));
    }

    #[test]
    fn finishing_grades_for_good_once_and_a_perfect_play_is_graded_perfect() {
        let (score, zero) = (score(), Instant::now());
        let tol = Tolerances::default();
        let mut t = take(&score, zero);
        let hits: Vec<(Instant, Lane)> = t.expected().iter().map(|e| (zero + Duration::from_secs_f64(e.t.0), e.lane)).collect();
        for (at, lane) in hits {
            t.record_hit(at, 36, 100, lane, Seconds::ZERO);
        }
        assert!(t.finish(StopReason::Completed, &tol, zero));
        assert!(!t.is_running());
        assert_eq!((t.report().missed, t.report().extra), (0, 0));
        assert_eq!(t.report().perfect, t.expected().len());
        assert!(!t.finish(StopReason::Completed, &tol, zero), "already over");
        assert_eq!(t.grades(score.notes().len()).iter().flatten().count(), t.expected().len());
        assert!(t.measures_of_extra_hits(&score).is_empty());
        assert_eq!(t.played_notes(&score).len(), t.hits().len());
        assert_eq!(t.range_text(&score), "1–4");
        assert!((t.bpm(&score) - 100.0).abs() < 1e-9);
    }
}
