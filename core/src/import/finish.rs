//! The last stage, the same for every format: unroll repeats, lay the measures out in quarter
//! notes, build the tempo map, sort and clean the notes, and check what comes out.

use std::collections::HashMap;

use super::ir::{RawMeasure, RawScore};
use super::{ImportError, ImportOptions};
use crate::score::{Measure, Rest, Score, ScoreNote, TempoMap};

pub fn finish(raw: RawScore, opts: &ImportOptions) -> Result<Score, ImportError> {
    if raw.measures.is_empty() {
        return Err(ImportError::NoMeasures);
    }
    let limits = &opts.limits;
    if raw.measures.len() > limits.max_measures {
        return Err(ImportError::TooLarge(format!("more than {} measures", limits.max_measures)));
    }
    let order = playback_order(&raw.measures, opts.expand_repeats);
    if order.len() > limits.max_measures {
        return Err(ImportError::TooLarge(format!("more than {} measures once the repeats are played out", limits.max_measures)));
    }

    let mut score = Score { title: raw.title, part_name: raw.part_name, warnings: raw.warnings, ..Default::default() };
    let mut tempo_pts = Vec::new();
    let mut q = 0.0;
    for &mi in &order {
        let rm = &raw.measures[mi];
        let idx = score.measures.len();
        score.measures.push(Measure {
            number: rm.number.clone(),
            start_q: q,
            len_q: rm.len_q,
            beats: rm.beats,
            beat_type: rm.beat_type,
        });
        for n in &rm.notes {
            score.notes.push(ScoreNote {
                q: q + n.offset_q,
                measure: idx,
                lane: n.lane,
                staff_step: n.staff_step,
                head: n.head,
                accent: n.accent,
                ghost: n.ghost,
                grace: n.grace,
                grace_order: u8::from(n.grace),
                hand: None,
                stem_up: n.stem_up,
            });
        }
        for r in &rm.rests {
            score.rests.push(Rest { measure: idx, q: q + r.offset_q, dur_q: r.dur_q, up: r.up, whole_bar: r.whole_bar });
        }
        for (off, bpm) in &rm.tempos {
            tempo_pts.push((q + off, *bpm));
        }
        q += rm.len_q;
        if score.notes.len() > limits.max_notes {
            return Err(ImportError::TooLarge(format!("more than {} notes", limits.max_notes)));
        }
    }
    if tempo_pts.is_empty() {
        score.warnings.push("No tempo marking in the file; using 120 BPM".into());
    }
    score.tempo = TempoMap::from_points(tempo_pts);
    score.notes.sort_by(|a, b| a.q.total_cmp(&b.q).then(a.lane.cmp(&b.lane)));
    score.notes.dedup_by(|b, a| (a.q - b.q).abs() < 1e-6 && a.lane == b.lane && a.grace == b.grace);
    if score.notes.is_empty() {
        score.warnings.push("No drum notes found in the part".into());
    }
    // The rest of the program indexes measures and notes as if this held, so a parser that breaks it
    // is an error here and not a panic somewhere else.
    check_invariants(&score).map_err(ImportError::Invalid)?;
    Ok(score)
}

/// What every [`Score`] the program reads or builds must satisfy: measures follow one another
/// without a gap, notes and rests are in time order and inside their measure, and every index
/// points at a measure that exists. The error says which rule broke.
pub fn check_invariants(score: &Score) -> Result<(), String> {
    const EPS: f64 = 1e-6;
    let mut at = 0.0;
    for (i, m) in score.measures.iter().enumerate() {
        if (m.start_q - at).abs() > EPS {
            return Err(format!("measure {i} starts at {} but the one before ends at {at}", m.start_q));
        }
        if m.len_q <= 0.0 || !m.len_q.is_finite() {
            return Err(format!("measure {i} has length {}", m.len_q));
        }
        at = m.end_q();
    }
    let inside = |kind: &str, i: usize, measure: usize, q: f64| -> Result<(), String> {
        let m = score.measures.get(measure).ok_or_else(|| format!("{kind} {i} is in measure {measure}, which does not exist"))?;
        if q < m.start_q - EPS || q > m.end_q() + EPS {
            return Err(format!("{kind} {i} at q={q} lies outside its measure {measure} ({}..{})", m.start_q, m.end_q()));
        }
        Ok(())
    };
    for (i, n) in score.notes.iter().enumerate() {
        inside("note", i, n.measure, n.q)?;
        if i > 0 && n.q < score.notes[i - 1].q {
            return Err(format!("note {i} at q={} comes before the note before it (q={})", n.q, score.notes[i - 1].q));
        }
    }
    for (i, r) in score.rests.iter().enumerate() {
        inside("rest", i, r.measure, r.q)?;
    }
    Ok(())
}

/// Measure indices in playback order (repeats and 1st/2nd endings expanded).
pub(super) fn playback_order(raw: &[RawMeasure], expand: bool) -> Vec<usize> {
    if !expand {
        return (0..raw.len()).collect();
    }
    let mut order = Vec::new();
    let mut counts: HashMap<usize, u32> = HashMap::new();
    let mut repeat_start = 0usize;
    let mut pass = 1u32;
    let mut last_in_ending = false;
    let mut prev: Option<usize> = None;
    let mut i = 0usize;
    let mut guard = 0usize;
    while i < raw.len() && guard < 100_000 {
        guard += 1;
        let m = &raw[i];
        if m.forward_repeat && repeat_start != i {
            repeat_start = i;
            pass = 1;
        }
        let moving_forward = prev.is_some_and(|p| i > p);
        if m.endings.is_empty() && last_in_ending && pass > 1 && moving_forward {
            // Left the 1st/2nd-ending block: a new section begins here.
            repeat_start = i;
            pass = 1;
        }
        if !m.endings.is_empty() && !m.endings.contains(&pass) {
            i += 1;
            continue;
        }
        order.push(i);
        prev = Some(i);
        last_in_ending = !m.endings.is_empty();
        if let Some(times) = m.backward_repeat {
            let c = counts.entry(i).or_insert(1);
            if *c < times {
                *c += 1;
                pass += 1;
                i = repeat_start;
                continue;
            }
            counts.remove(&i);
            repeat_start = i + 1;
            pass = 1;
        }
        i += 1;
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rm(fwd: bool, back: Option<u32>, endings: &[u32]) -> RawMeasure {
        RawMeasure {
            forward_repeat: fwd,
            backward_repeat: back,
            endings: endings.to_vec(),
            len_q: 4.0,
            beats: 4,
            beat_type: 4,
            ..Default::default()
        }
    }

    #[test]
    fn repeats_and_voltas() {
        // |: A B [1. C :| [2. D | E
        let ms = vec![rm(true, None, &[]), rm(false, None, &[]), rm(false, Some(2), &[1]), rm(false, None, &[2]), rm(false, None, &[])];
        assert_eq!(playback_order(&ms, true), vec![0, 1, 2, 0, 1, 3, 4]);
        // A |: B :| x3  C
        let ms = vec![rm(false, None, &[]), rm(true, Some(3), &[]), rm(false, None, &[])];
        assert_eq!(playback_order(&ms, true), vec![0, 1, 1, 1, 2]);
        // A :| B (repeat from start)
        let ms = vec![rm(false, Some(2), &[]), rm(false, None, &[])];
        assert_eq!(playback_order(&ms, true), vec![0, 0, 1]);
        assert_eq!(playback_order(&ms, false), vec![0, 1]);
    }

    #[test]
    fn a_parser_that_breaks_the_invariants_is_an_error_not_a_bad_score() {
        let measure = RawMeasure { number: "1".into(), len_q: 4.0, beats: 4, beat_type: 4, ..Default::default() };
        let mut stray = measure.clone();
        stray.notes.push(super::super::RawNote {
            offset_q: 9.0,
            lane: crate::drums::Lane::Kick,
            staff_step: 1,
            head: crate::drums::NoteHead::Normal,
            accent: false,
            ghost: false,
            grace: false,
            stem_up: None,
        });
        let err = finish(RawScore { measures: vec![stray], ..Default::default() }, &ImportOptions::default()).unwrap_err();
        assert!(matches!(&err, ImportError::Invalid(why) if why.contains("outside its measure")), "{err:?}");
        assert!(err.to_string().contains("not consistent"));
        assert!(finish(RawScore { measures: vec![measure], ..Default::default() }, &ImportOptions::default()).is_ok());
    }

    #[test]
    fn a_score_without_measures_is_refused() {
        let raw = RawScore::default();
        assert_eq!(finish(raw, &ImportOptions::default()).unwrap_err(), ImportError::NoMeasures);
    }

    #[test]
    fn a_runaway_repeat_count_is_stopped_by_the_limits() {
        let ms = RawScore { measures: vec![rm(false, Some(u32::MAX), &[])], ..Default::default() };
        let err = finish(ms, &ImportOptions::default()).unwrap_err();
        assert!(matches!(err, ImportError::TooLarge(_)), "{err:?}");
    }

    #[test]
    fn the_invariants_name_the_rule_that_broke() {
        let mut score = Score::click_bar(4, 2);
        assert_eq!(check_invariants(&score), Ok(()));
        score.notes.swap(0, 3);
        assert!(check_invariants(&score).unwrap_err().contains("comes before"));
        let mut score = Score::click_bar(4, 1);
        score.notes[1].measure = 5;
        assert!(check_invariants(&score).unwrap_err().contains("does not exist"));
        let mut score = Score::click_bar(4, 1);
        score.notes[1].q = 9.0;
        assert!(check_invariants(&score).unwrap_err().contains("outside its measure"));
        let mut score = Score::click_bar(4, 1);
        score.measures[0].start_q = 1.0;
        assert!(check_invariants(&score).unwrap_err().contains("starts at"));
    }
}
