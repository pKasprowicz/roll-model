//! Score model: a flat, time-ordered list of drum onsets plus measures and
//! a tempo map. Positions are in quarter notes ("q") from the score start.

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
    pub q: f64,
    /// Index into `Score::measures`.
    pub measure: usize,
    pub lane: Lane,
    pub staff_step: i32,
    pub head: NoteHead,
    pub accent: bool,
    pub ghost: bool,
    pub grace: bool,
    /// For a grace note: 1 = the one right before its main note, 2 = the one before that
    /// (a drag has two). 0 for every other note.
    pub grace_order: u8,
    /// Which hand plays it, if the score says; drawn as a letter under the note.
    pub hand: Option<Hand>,
    pub stem_up: Option<bool>,
}

impl ScoreNote {
    /// Whether its stem points up: as the file says, else up for the hands and down for the feet.
    pub fn stem_is_up(&self) -> bool {
        self.stem_up.unwrap_or(!self.lane.is_foot())
    }
}

/// A rest the file prints. Rests are only drawn; nothing in grading or playback reads them.
#[derive(Clone, Debug, PartialEq)]
pub struct Rest {
    /// Index into `Score::measures`.
    pub measure: usize,
    pub q: f64,
    pub dur_q: f64,
    /// The voice it belongs to: true for the hands (stems up), false for the feet.
    pub up: bool,
    /// A rest for the whole bar, whatever its length.
    pub whole_bar: bool,
}

#[derive(Clone, Debug)]
pub struct Measure {
    /// Measure number as printed in the file.
    pub number: String,
    pub start_q: f64,
    pub len_q: f64,
    pub beats: u32,
    pub beat_type: u32,
}

impl Measure {
    pub fn end_q(&self) -> f64 {
        self.start_q + self.len_q
    }
    /// Length of one beat of the time signature, in quarters.
    pub fn beat_q(&self) -> f64 {
        4.0 / self.beat_type.max(1) as f64
    }
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

    pub fn from_points(mut pts: Vec<(f64, f64)>) -> TempoMap {
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

    pub fn bpm_at(&self, q: f64) -> f64 {
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
    pub fn q_to_sec(&self, q: f64, scale: f64) -> f64 {
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
        sec
    }

    pub fn sec_to_q(&self, sec: f64, scale: f64) -> f64 {
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
    pub measures: Vec<Measure>,
    /// Sorted by `q`.
    pub notes: Vec<ScoreNote>,
    /// The rests the file prints (none for a score made by the program).
    pub rests: Vec<Rest>,
    pub tempo: TempoMap,
    pub warnings: Vec<String>,
}

impl Score {
    pub fn measure_at(&self, q: f64) -> Option<usize> {
        if self.measures.is_empty() || q < 0.0 {
            return None;
        }
        let idx = self.measures.partition_point(|m| m.start_q <= q + 1e-9);
        Some(idx.saturating_sub(1).min(self.measures.len() - 1))
    }

    /// Metronome clicks for measures `from..=to`: a count-in of
    /// `count_in_beats` beats before the first measure (negative times), then,
    /// if `beats` is set, one click per beat of the range. Times are seconds
    /// from the start of measure `from` with tempos multiplied by `scale`; the
    /// flag marks the first beat of a bar.
    pub fn click_track(&self, from: usize, to: usize, scale: f64, count_in_beats: u32, beats: bool) -> Vec<(f64, bool)> {
        let Some(m0) = self.measures.get(from) else { return Vec::new() };
        let beat_sec = 60.0 / (self.tempo.bpm_at(m0.start_q) * scale) * m0.beat_q();
        let mut clicks: Vec<(f64, bool)> = (0..count_in_beats)
            .map(|k| (-((count_in_beats - k) as f64) * beat_sec, k % m0.beats.max(1) == 0))
            .collect();
        if beats {
            let t0 = self.tempo.q_to_sec(m0.start_q, scale);
            for m in &self.measures[from..=to.clamp(from, self.measures.len() - 1)] {
                for b in 0..m.beats {
                    let q = m.start_q + b as f64 * m.beat_q();
                    clicks.push((self.tempo.q_to_sec(q, scale) - t0, b == 0));
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
                    q: f64::from(b) + f64::from(p) / f64::from(parts),
                    measure: 0,
                    lane: Lane::Other,
                    staff_step: 4,
                    head: NoteHead::Normal,
                    accent: b == 0 && p == 0,
                    ghost: p > 0,
                    grace: false,
                    grace_order: 0,
                    hand: None,
                    stem_up: Some(true),
                });
            }
        }
        Score {
            title: "Click".into(),
            measures: vec![Measure { number: "1".into(), start_q: 0.0, len_q: f64::from(beats), beats, beat_type: 4 }],
            notes,
            ..Score::default()
        }
    }

    pub fn staff_for_lane(&self, lane: Lane) -> (i32, NoteHead) {
        self.notes
            .iter()
            .find(|n| n.lane == lane && !n.grace)
            .map(|n| (n.staff_step, n.head))
            .unwrap_or_else(|| lane.default_staff())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_bar_has_a_note_per_click_and_an_accent_on_the_first() {
        let bar = Score::click_bar(3, 2);
        assert_eq!(bar.notes.len(), 6);
        assert_eq!(bar.measures[0].len_q, 3.0);
        assert!(bar.notes[0].accent && bar.notes.iter().filter(|n| n.accent).count() == 1);
        let soft: Vec<f64> = bar.notes.iter().filter(|n| n.ghost).map(|n| n.q).collect();
        assert_eq!(soft, [0.5, 1.5, 2.5]);
        assert!(bar.notes.windows(2).all(|w| w[0].q < w[1].q), "in time order");
        assert_eq!(Score::click_bar(0, 0).notes.len(), 1, "never empty");
    }

    #[test]
    fn tempo_conversion_roundtrip() {
        let t = TempoMap::from_points(vec![(0.0, 120.0), (8.0, 60.0)]);
        assert!((t.q_to_sec(4.0, 1.0) - 2.0).abs() < 1e-9);
        assert!((t.q_to_sec(8.0, 1.0) - 4.0).abs() < 1e-9);
        assert!((t.q_to_sec(10.0, 1.0) - 6.0).abs() < 1e-9);
        assert!((t.q_to_sec(4.0, 0.5) - 4.0).abs() < 1e-9);
        for q in [0.0, 1.5, 7.9, 8.0, 12.25] {
            let s = t.q_to_sec(q, 0.8);
            assert!((t.sec_to_q(s, 0.8) - q).abs() < 1e-9, "q={q}");
        }
        // negative times (count-in) extrapolate with the first tempo
        assert!((t.sec_to_q(-1.0, 1.0) + 2.0).abs() < 1e-9);
        assert!((t.q_to_sec(-2.0, 1.0) + 1.0).abs() < 1e-9);
    }

    #[test]
    fn click_track_counts_in_and_follows_beats() {
        let measure = |i: usize| Measure {
            number: (i + 1).to_string(),
            start_q: 4.0 * i as f64,
            len_q: 4.0,
            beats: 4,
            beat_type: 4,
        };
        let score = Score {
            measures: vec![measure(0), measure(1), measure(2)],
            tempo: TempoMap::constant(120.0),
            ..Default::default()
        };
        // Practise measures 2..=3 at 120 BPM (0.5 s per beat) with a one-bar count-in.
        let c = score.click_track(1, 2, 1.0, 4, true);
        let times: Vec<f64> = c.iter().map(|(t, _)| *t).collect();
        assert_eq!(times.len(), 4 + 8);
        for (got, want) in times.iter().zip([-2.0, -1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0]) {
            assert!((got - want).abs() < 1e-9, "{got} vs {want}");
        }
        let accents: Vec<usize> = c.iter().enumerate().filter(|(_, (_, a))| *a).map(|(i, _)| i).collect();
        assert_eq!(accents, vec![0, 4, 8]);
        // Half speed doubles every interval; no beat clicks and no count-in leaves nothing.
        assert!((score.click_track(0, 0, 0.5, 4, false)[0].0 + 4.0).abs() < 1e-9);
        assert!(score.click_track(0, 2, 1.0, 0, false).is_empty());
        assert!(Score::default().click_track(0, 0, 1.0, 4, true).is_empty());
    }
}
