//! Score model: a flat, time-ordered list of drum onsets plus measures and
//! a tempo map. Positions are in quarter notes ("q") from the score start.

use std::num::NonZeroU8;

use crate::units::{Quarters, Seconds};

use crate::drums::{Lane, NoteHead};

/// The hand a stroke is played with, when the score says (rudiments do).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hand {
    Right,
    Left,
}

impl Hand {
    pub fn other(self) -> Hand {
        match self {
            Hand::Right => Hand::Left,
            Hand::Left => Hand::Right,
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            Hand::Right => "R",
            Hand::Left => "L",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Hand::Right => "Right hand",
            Hand::Left => "Left hand",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScoreNote {
    /// Onset in quarter notes from the start of the (unrolled) score.
    pub q: Quarters,
    /// Index into `Score::measures`.
    pub measure: usize,
    pub lane: Lane,
    pub staff_step: i32,
    pub head: NoteHead,
    pub dynamic: Dynamic,
    pub kind: Kind,
    /// Which hand plays it, if the score says; drawn as a letter under the note.
    pub hand: Option<Hand>,
    /// The stem direction the file gives, if it does.
    pub stem: Option<Stem>,
}

/// Which way a stem points: up for the hands, down for the feet, as a score is usually written. It
/// also tells the two voices apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stem {
    Up,
    Down,
}

impl Stem {
    pub fn is_up(self) -> bool {
        self == Stem::Up
    }
    pub fn from_up(up: bool) -> Stem {
        if up {
            Stem::Up
        } else {
            Stem::Down
        }
    }
}

/// How loud a note is meant to be played, against the notes around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Dynamic {
    #[default]
    Normal,
    Accent,
    /// Played softly, written in parentheses.
    Ghost,
}

/// A note of the music, or a grace note before one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Main,
    /// 1 = the one right before its main note, 2 = the one before that (a drag has two). Never 0.
    Grace { order: NonZeroU8 },
}

impl Kind {
    /// A grace note of this place before its main note; 0 is taken as 1.
    pub fn grace(order: u8) -> Kind {
        Kind::Grace { order: NonZeroU8::new(order).unwrap_or(NonZeroU8::MIN) }
    }
}

impl ScoreNote {
    pub fn is_accent(&self) -> bool {
        self.dynamic == Dynamic::Accent
    }
    pub fn is_ghost(&self) -> bool {
        self.dynamic == Dynamic::Ghost
    }
    pub fn is_grace(&self) -> bool {
        matches!(self.kind, Kind::Grace { .. })
    }
    /// For a grace note, how far before its main note it comes (1 or 2); 0 for any other note.
    pub fn grace_order(&self) -> u8 {
        match self.kind {
            Kind::Main => 0,
            Kind::Grace { order } => order.get(),
        }
    }

    /// Whether its stem points up: as the file says, else up for the hands and down for the feet.
    pub fn stem_is_up(&self) -> bool {
        self.stem.map_or(!self.lane.is_foot(), Stem::is_up)
    }
}

/// A rest the file prints. Rests are only drawn; nothing in grading or playback reads them.
#[derive(Clone, Debug, PartialEq)]
pub struct Rest {
    /// Index into `Score::measures`.
    pub measure: usize,
    pub q: Quarters,
    pub dur_q: Quarters,
    /// The voice it belongs to: true for the hands (stems up), false for the feet.
    pub stem: Stem,
    /// A rest for the whole bar, whatever its length.
    pub whole_bar: bool,
}

#[derive(Clone, Debug)]
pub struct Measure {
    /// Measure number as printed in the file.
    pub number: String,
    pub start_q: Quarters,
    pub len_q: Quarters,
    pub beats: u32,
    pub beat_type: u32,
}

impl Measure {
    pub fn end_q(&self) -> Quarters {
        self.start_q + self.len_q
    }
    /// Length of one beat of the time signature, in quarters.
    pub fn beat_q(&self) -> Quarters {
        Quarters(4.0 / self.beat_type.max(1) as f64)
    }
}

/// A click of the metronome: when (seconds from the start of the practice range, negative in the
/// count-in) and whether it is the first beat of a bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Click {
    pub t: Seconds,
    pub accent: bool,
}

/// Piecewise-constant tempo, in quarter-note BPM.
#[derive(Clone, Debug)]
pub struct TempoMap {
    /// (q, bpm), sorted by q, first entry at q = 0.
    points: Vec<(f64, f64)>,
}

impl Default for TempoMap {
    fn default() -> Self {
        TempoMap::constant(120.0)
    }
}

impl TempoMap {
    pub fn constant(bpm: f64) -> TempoMap {
        TempoMap { points: vec![(0.0, bpm.max(1.0))] }
    }

    pub fn from_points(pts: Vec<(Quarters, f64)>) -> TempoMap {
        let mut pts: Vec<(f64, f64)> = pts.into_iter().map(|(q, bpm)| (q.0, bpm)).collect();
        pts.retain(|(_, b)| *b > 0.0 && b.is_finite());
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out: Vec<(f64, f64)> = Vec::new();
        for (q, b) in pts {
            match out.last_mut() {
                Some(last) if (last.0 - q).abs() < 1e-9 => last.1 = b,
                Some(last) if (last.1 - b).abs() < 1e-9 => {}
                _ => out.push((q, b)),
            }
        }
        if out.is_empty() {
            return TempoMap::default();
        }
        if out[0].0 > 0.0 {
            let first = out[0].1;
            out.insert(0, (0.0, first));
        }
        TempoMap { points: out }
    }

    pub fn bpm_at(&self, q: Quarters) -> f64 {
        let q = q.0;
        let mut bpm = self.points[0].1;
        for (pq, b) in &self.points {
            if *pq <= q + 1e-9 {
                bpm = *b;
            } else {
                break;
            }
        }
        bpm
    }

    /// Seconds from q = 0 to `q`, with all tempos multiplied by `scale`.
    pub fn q_to_sec(&self, q: Quarters, scale: f64) -> Seconds {
        let q = q.0;
        let scale = scale.max(0.01);
        let mut sec = 0.0;
        for (i, (pq, bpm)) in self.points.iter().enumerate() {
            let next_q = self.points.get(i + 1).map(|p| p.0).unwrap_or(f64::INFINITY);
            if q <= *pq && i > 0 {
                break;
            }
            let seg_end = q.min(next_q);
            sec += (seg_end - pq) * 60.0 / (bpm * scale);
            if q <= next_q {
                break;
            }
        }
        Seconds(sec)
    }

    pub fn sec_to_q(&self, sec: Seconds, scale: f64) -> Quarters {
        Quarters(self.sec_to_quarters(sec.0, scale))
    }

    fn sec_to_quarters(&self, sec: f64, scale: f64) -> f64 {
        let scale = scale.max(0.01);
        let mut acc = 0.0;
        for (i, (pq, bpm)) in self.points.iter().enumerate() {
            let spq = 60.0 / (bpm * scale);
            let next_q = self.points.get(i + 1).map(|p| p.0);
            match next_q {
                Some(nq) => {
                    let seg = (nq - pq) * spq;
                    if sec < acc + seg || i == 0 && sec < 0.0 {
                        return pq + (sec - acc) / spq;
                    }
                    acc += seg;
                }
                None => return pq + (sec - acc) / spq,
            }
        }
        0.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct Score {
    pub title: String,
    pub part_name: String,
    pub(crate) measures: Vec<Measure>,
    /// Sorted by `q`.
    pub(crate) notes: Vec<ScoreNote>,
    /// The rests the file prints (none for a score made by the program).
    pub(crate) rests: Vec<Rest>,
    pub tempo: TempoMap,
    pub warnings: Vec<String>,
}

impl Score {
    /// The bars, one after the other without a gap.
    pub fn measures(&self) -> &[Measure] {
        &self.measures
    }

    /// The notes in time order, each inside its measure.
    pub fn notes(&self) -> &[ScoreNote] {
        &self.notes
    }

    /// The rests the file prints (none for a score made by the program).
    pub fn rests(&self) -> &[Rest] {
        &self.rests
    }

    /// The parts of a score, for a test to put together by hand without the rules of
    /// `import::check_invariants`.
    #[doc(hidden)]
    pub fn measures_mut(&mut self) -> &mut Vec<Measure> {
        &mut self.measures
    }

    #[doc(hidden)]
    pub fn notes_mut(&mut self) -> &mut Vec<ScoreNote> {
        &mut self.notes
    }

    #[doc(hidden)]
    pub fn rests_mut(&mut self) -> &mut Vec<Rest> {
        &mut self.rests
    }

    /// A score from its parts, or the rule they break (see `import::check_invariants`).
    pub fn from_parts(
        title: String,
        measures: Vec<Measure>,
        notes: Vec<ScoreNote>,
        rests: Vec<Rest>,
        tempo: TempoMap,
    ) -> Result<Score, String> {
        let score = Score { title, part_name: String::new(), measures, notes, rests, tempo, warnings: Vec::new() };
        crate::import::check_invariants(&score)?;
        Ok(score)
    }

    pub fn measure_at(&self, q: Quarters) -> Option<usize> {
        if self.measures.is_empty() || q < Quarters::ZERO {
            return None;
        }
        let idx = self.measures.partition_point(|m| m.start_q <= q + Quarters(1e-9));
        Some(idx.saturating_sub(1).min(self.measures.len() - 1))
    }

    /// Metronome clicks for measures `from..=to`: a count-in of
    /// `count_in_beats` beats before the first measure (negative times), then,
    /// if `beats` is set, one click per beat of the range. Times are seconds
    /// from the start of measure `from` with tempos multiplied by `scale`; the
    /// flag marks the first beat of a bar.
    pub fn click_track(&self, from: usize, to: usize, scale: f64, count_in_beats: u32, beats: bool) -> Vec<Click> {
        let Some(m0) = self.measures.get(from) else { return Vec::new() };
        let beat_sec = Seconds(60.0 / (self.tempo.bpm_at(m0.start_q) * scale) * m0.beat_q().0);
        let mut clicks: Vec<Click> =
            (0..count_in_beats).map(|k| Click { t: -beat_sec * f64::from(count_in_beats - k), accent: k % m0.beats.max(1) == 0 }).collect();
        if beats {
            let t0 = self.tempo.q_to_sec(m0.start_q, scale);
            for m in &self.measures[from..=to.clamp(from, self.measures.len() - 1)] {
                // A short first bar (a pickup) has fewer beats than its time signature says.
                let beats = (m.len_q / m.beat_q() - 1e-9).ceil().max(1.0) as u32;
                for b in 0..beats {
                    let q = m.start_q + m.beat_q() * f64::from(b);
                    clicks.push(Click { t: self.tempo.q_to_sec(q, scale) - t0, accent: b == 0 });
                }
            }
        }
        clicks
    }

    /// Staff position/notehead used for a lane in this file (so played notes
    /// are drawn the way the score draws the same instrument).
    /// One bar of a click track as a score, for the metronome's preview: a note
    /// on every click (`parts` per beat), the first beat accented and the
    /// subdivisions in brackets (soft).
    pub fn click_bar(beats: u32, parts: u32) -> Score {
        let (beats, parts) = (beats.max(1), parts.max(1));
        let mut notes = Vec::new();
        for b in 0..beats {
            for p in 0..parts {
                notes.push(ScoreNote {
                    q: Quarters(f64::from(b) + f64::from(p) / f64::from(parts)),
                    measure: 0,
                    lane: Lane::Other,
                    staff_step: 4,
                    head: NoteHead::Normal,
                    dynamic: if b == 0 && p == 0 {
                        Dynamic::Accent
                    } else if p > 0 {
                        Dynamic::Ghost
                    } else {
                        Dynamic::Normal
                    },
                    kind: Kind::Main,
                    hand: None,
                    stem: Some(Stem::Up),
                });
            }
        }
        Score {
            title: "Click".into(),
            measures: vec![Measure { number: "1".into(), start_q: Quarters::ZERO, len_q: Quarters(f64::from(beats)), beats, beat_type: 4 }],
            notes,
            ..Score::default()
        }
    }

    pub fn staff_for_lane(&self, lane: Lane) -> (i32, NoteHead) {
        self.notes.iter().find(|n| n.lane == lane && !n.is_grace()).map(|n| (n.staff_step, n.head)).unwrap_or_else(|| lane.default_staff())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_bar_has_a_note_per_click_and_an_accent_on_the_first() {
        let bar = Score::click_bar(3, 2);
        assert_eq!(bar.notes.len(), 6);
        assert_eq!(bar.measures[0].len_q, Quarters(3.0));
        assert!(bar.notes[0].is_accent() && bar.notes.iter().filter(|n| n.is_accent()).count() == 1);
        let soft: Vec<f64> = bar.notes.iter().filter(|n| n.is_ghost()).map(|n| n.q.0).collect();
        assert_eq!(soft, [0.5, 1.5, 2.5]);
        assert!(bar.notes.windows(2).all(|w| w[0].q < w[1].q), "in time order");
        assert_eq!(Score::click_bar(0, 0).notes.len(), 1, "never empty");
    }

    #[test]
    fn tempo_conversion_roundtrip() {
        let t = TempoMap::from_points(vec![(Quarters(0.0), 120.0), (Quarters(8.0), 60.0)]);
        let sec = |q: f64, scale: f64| t.q_to_sec(Quarters(q), scale).0;
        assert!((sec(4.0, 1.0) - 2.0).abs() < 1e-9);
        assert!((sec(8.0, 1.0) - 4.0).abs() < 1e-9);
        assert!((sec(10.0, 1.0) - 6.0).abs() < 1e-9);
        assert!((sec(4.0, 0.5) - 4.0).abs() < 1e-9);
        for q in [0.0, 1.5, 7.9, 8.0, 12.25] {
            let s = t.q_to_sec(Quarters(q), 0.8);
            assert!((t.sec_to_q(s, 0.8).0 - q).abs() < 1e-9, "q={q}");
        }
        // negative times (count-in) extrapolate with the first tempo
        assert!((t.sec_to_q(Seconds(-1.0), 1.0).0 + 2.0).abs() < 1e-9);
        assert!((sec(-2.0, 1.0) + 1.0).abs() < 1e-9);
    }

    #[test]
    fn click_track_counts_in_and_follows_beats() {
        let measure = |i: usize| Measure {
            number: (i + 1).to_string(),
            start_q: Quarters(4.0 * i as f64),
            len_q: Quarters(4.0),
            beats: 4,
            beat_type: 4,
        };
        let score = Score { measures: vec![measure(0), measure(1), measure(2)], tempo: TempoMap::constant(120.0), ..Default::default() };
        // Practise measures 2..=3 at 120 BPM (0.5 s per beat) with a one-bar count-in.
        let c = score.click_track(1, 2, 1.0, 4, true);
        let times: Vec<f64> = c.iter().map(|c| c.t.0).collect();
        assert_eq!(times.len(), 4 + 8);
        for (got, want) in times.iter().zip([-2.0, -1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0]) {
            assert!((got - want).abs() < 1e-9, "{got} vs {want}");
        }
        let accents: Vec<usize> = c.iter().enumerate().filter(|(_, c)| c.accent).map(|(i, _)| i).collect();
        assert_eq!(accents, vec![0, 4, 8]);
        // Half speed doubles every interval; no beat clicks and no count-in leaves nothing.
        assert!((score.click_track(0, 0, 0.5, 4, false)[0].t.0 + 4.0).abs() < 1e-9);
        assert!(score.click_track(0, 2, 1.0, 0, false).is_empty());
        assert!(Score::default().click_track(0, 0, 1.0, 4, true).is_empty());
    }

    #[test]
    fn a_score_made_from_parts_must_keep_the_rules_of_a_score() {
        let good = Score::click_bar(4, 1);
        let (measures, notes) = (good.measures().to_vec(), good.notes().to_vec());
        assert!(Score::from_parts("x".into(), measures.clone(), notes.clone(), vec![], TempoMap::default()).is_ok());
        let mut stray = notes;
        stray[1].q = Quarters(99.0);
        let err = Score::from_parts("x".into(), measures, stray, vec![], TempoMap::default()).unwrap_err();
        assert!(err.contains("outside its measure"), "{err}");
    }
}
