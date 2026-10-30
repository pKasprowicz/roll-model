//! What a page of drum notation shows that the score does not store: which stems are joined by
//! beams, which carry flags and which groups are tuplets. A score keeps only the onset of each
//! note, so everything is read from where the notes fall in the beat.
//!
//! Notes are grouped by beat (a dotted quarter in 6/8, 9/8 and 12/8) and by the direction of their
//! stem, because hands and feet are written as separate voices. A note lasts until the next onset
//! of its group, the last one until the end of the beat; so a lone note on an off-beat is written
//! with a flag, as if the rest before it were there. Rests are not drawn.

use std::collections::HashMap;

use crate::score::Score;

/// Notes that sound together, in a beamed group.
#[derive(Clone, Debug, PartialEq)]
pub struct Onset {
    pub q: f64,
    /// Indexes into `Score::notes`.
    pub notes: Vec<usize>,
    /// How many beams (or flags) this onset has: 1 for an eighth, 2 for a sixteenth, ...
    pub level: u8,
}

/// Stems joined by beams. The beams between two neighbouring onsets are the fewer of their levels;
/// an onset with more than its neighbours gets a stub.
#[derive(Clone, Debug, PartialEq)]
pub struct Beam {
    pub measure: usize,
    pub up: bool,
    pub onsets: Vec<Onset>,
}

/// A group of notes that are played in the time of fewer, with the number to write over it.
#[derive(Clone, Debug, PartialEq)]
pub struct Tuplet {
    pub measure: usize,
    pub up: bool,
    pub number: u8,
    /// Every note of the group, first onset to last.
    pub notes: Vec<usize>,
    pub first_q: f64,
    pub last_q: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notation {
    pub beams: Vec<Beam>,
    pub tuplets: Vec<Tuplet>,
    /// Per `Score::notes` index: the flags of a note that is not beamed (0 for none).
    pub flags: Vec<u8>,
    /// Per `Score::notes` index: the beam that holds it, if any.
    pub beam_of: Vec<Option<usize>>,
}

const EPS: f64 = 1e-6;

/// Beams, flags and tuplets of every note that is not a grace note.
pub fn analyse(score: &Score) -> Notation {
    let mut out = Notation { flags: vec![0; score.notes.len()], beam_of: vec![None; score.notes.len()], ..Notation::default() };
    // (measure, beat, stem up) -> note indexes; notes are sorted by q, so each group is in order.
    let mut groups: Vec<((usize, i64, bool), Vec<usize>)> = Vec::new();
    let mut index: HashMap<(usize, i64, bool), usize> = HashMap::new();
    for (i, n) in score.notes.iter().enumerate() {
        let Some(m) = score.measures.get(n.measure).filter(|_| !n.grace) else { continue };
        let beat = ((n.q - m.start_q + EPS) / group_len(m.beats, m.beat_type)).floor() as i64;
        let key = (n.measure, beat, n.stem_is_up());
        match index.get(&key) {
            Some(&g) => groups[g].1.push(i),
            None => {
                index.insert(key, groups.len());
                groups.push((key, vec![i]));
            }
        }
    }
    for ((measure, beat, up), notes) in groups {
        let m = &score.measures[measure];
        let len = group_len(m.beats, m.beat_type);
        let end = (m.start_q + (beat + 1) as f64 * len).min(m.end_q());
        let mut onsets: Vec<Onset> = Vec::new();
        for i in notes {
            let q = score.notes[i].q;
            match onsets.last_mut().filter(|o| (o.q - q).abs() < EPS) {
                Some(o) => o.notes.push(i),
                None => onsets.push(Onset { q, notes: vec![i], level: 0 }),
            }
        }
        for k in 0..onsets.len() {
            let next = onsets.get(k + 1).map_or(end, |o| o.q);
            onsets[k].level = level_of(next - onsets[k].q);
        }
        if let Some(number) = tuplet_of(&onsets, m.start_q + beat as f64 * len) {
            out.tuplets.push(Tuplet {
                measure,
                up,
                number,
                notes: onsets.iter().flat_map(|o| o.notes.iter().copied()).collect(),
                first_q: onsets[0].q,
                last_q: onsets[onsets.len() - 1].q,
            });
        }
        // Split at onsets that are not shorter than a quarter: they take no beam, and no flag.
        let mut run: Vec<Onset> = Vec::new();
        let finish = |run: &mut Vec<Onset>, out: &mut Notation| match run.len() {
            0 => {}
            1 => {
                for &i in &run[0].notes {
                    out.flags[i] = run[0].level;
                }
                run.clear();
            }
            _ => {
                for o in run.iter() {
                    for &i in &o.notes {
                        out.beam_of[i] = Some(out.beams.len());
                    }
                }
                out.beams.push(Beam { measure, up, onsets: std::mem::take(run) });
            }
        };
        for o in onsets {
            if o.level == 0 {
                finish(&mut run, &mut out);
            } else {
                run.push(o);
            }
        }
        finish(&mut run, &mut out);
    }
    out
}

/// The length of the group that is beamed together: a beat, or a dotted quarter in the compound
/// meters; never more than a quarter otherwise (a half-note beat is beamed in quarters).
fn group_len(beats: u32, beat_type: u32) -> f64 {
    if beat_type == 8 && beats >= 6 && beats.is_multiple_of(3) {
        1.5
    } else {
        1.0
    }
}

/// The beams (or flags) of a note that lasts `d` quarters: none from a quarter up; an eighth and an
/// eighth triplet have one, a sixteenth and a sixteenth triplet two, and so on. A note shorter
/// than a quarter has at least one.
fn level_of(d: f64) -> u8 {
    if d >= 1.0 - EPS {
        return 0;
    }
    const KNOWN: [(f64, u8); 8] = [(1.0 / 2.0, 1), (1.0 / 3.0, 1), (1.0 / 4.0, 2), (1.0 / 6.0, 2), (1.0 / 8.0, 3), (1.0 / 12.0, 3), (1.0 / 16.0, 4), (1.0 / 24.0, 4)];
    let d = d.max(1.0 / 64.0);
    KNOWN
        .iter()
        .min_by(|a, b| (d / a.0).ln().abs().total_cmp(&(d / b.0).ln().abs()))
        .map_or(1, |k| k.1)
}

/// The number over a group whose onsets do not fall on a sixteenth grid: 3 for thirds (and for
/// sixths that start on a third), 6 for sixths, 5 for fifths. `None` if the group is on the grid,
/// has a single onset, or fits no tuplet.
fn tuplet_of(onsets: &[Onset], start: f64) -> Option<u8> {
    if onsets.len() < 2 {
        return None;
    }
    let on = |o: &Onset, per: f64| {
        let x = (o.q - start) * per;
        (x - x.round()).abs() < 1e-4
    };
    if onsets.iter().all(|o| on(o, 4.0)) {
        return None;
    }
    if onsets.iter().all(|o| on(o, 3.0)) {
        Some(3)
    } else if onsets.iter().all(|o| on(o, 6.0)) {
        Some(6)
    } else if onsets.iter().all(|o| on(o, 5.0)) {
        Some(5)
    } else {
        None
    }
}

/// For each measure, whether it ends with a pause: a rest runs to the end of the bar and no note
/// starts during it (in any voice), or the whole bar is a rest. A rest under notes of another voice
/// is not a pause. A run of empty bars counts once, at its last bar.
pub fn pause_bars(score: &Score) -> Vec<bool> {
    let mut last_onset: Vec<Option<f64>> = vec![None; score.measures.len()];
    for n in score.notes.iter().filter(|n| !n.grace) {
        if let Some(slot) = last_onset.get_mut(n.measure) {
            *slot = Some(slot.map_or(n.q, |q| q.max(n.q)));
        }
    }
    let mut pauses = vec![false; score.measures.len()];
    for r in &score.rests {
        let Some(m) = score.measures.get(r.measure) else { continue };
        let reaches_end = r.whole_bar || r.q + r.dur_q >= m.end_q() - EPS;
        if reaches_end && last_onset[r.measure].is_none_or(|q| q < r.q - EPS) {
            pauses[r.measure] = true;
        }
    }
    // Bars with no note at all that follow one another are one pause: the break comes after the last.
    for i in 0..pauses.len().saturating_sub(1) {
        if pauses[i] && last_onset[i].is_none() && last_onset[i + 1].is_none() && pauses[i + 1] {
            pauses[i] = false;
        }
    }
    pauses
}

/// How a rest is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestKind {
    Whole,
    Half,
    Quarter,
    /// 1 flag: an eighth rest; 2: a sixteenth; 3: a thirty-second.
    Flagged(u8),
}

/// One rest sign: where it starts, what it looks like and whether it has a dot. A rest for the
/// whole bar is a single whole rest in the middle of the bar (`bar`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RestGlyph {
    pub q: f64,
    pub kind: RestKind,
    pub dotted: bool,
    pub bar: bool,
}

/// The signs a rest is written with: one for a plain, dotted or triplet length, else the longest
/// that fit one after another.
pub fn rest_glyphs(rest: &crate::score::Rest) -> Vec<RestGlyph> {
    const VALUES: [(f64, RestKind); 6] = [
        (4.0, RestKind::Whole),
        (2.0, RestKind::Half),
        (1.0, RestKind::Quarter),
        (0.5, RestKind::Flagged(1)),
        (0.25, RestKind::Flagged(2)),
        (0.125, RestKind::Flagged(3)),
    ];
    if rest.whole_bar {
        return vec![RestGlyph { q: rest.q, kind: RestKind::Whole, dotted: false, bar: true }];
    }
    let near = |a: f64, b: f64| (a - b).abs() < 1e-3;
    // One sign: its own length, with a dot, or in a tuplet (two thirds of it).
    for (len, kind) in VALUES {
        for (factor, dotted) in [(1.0, false), (1.5, true), (2.0 / 3.0, false)] {
            if near(rest.dur_q, len * factor) {
                return vec![RestGlyph { q: rest.q, kind, dotted, bar: false }];
            }
        }
    }
    let (mut q, mut left) = (rest.q, rest.dur_q);
    let mut out = Vec::new();
    for (len, kind) in VALUES {
        while left >= len - 1e-3 {
            out.push(RestGlyph { q, kind, dotted: false, bar: false });
            q += len;
            left -= len;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drums::{Lane, NoteHead};
    use crate::score::{Measure, ScoreNote};

    fn note(q: f64, lane: Lane) -> ScoreNote {
        ScoreNote { q, measure: (q / 4.0) as usize, lane, staff_step: 4, head: NoteHead::Normal, accent: false, ghost: false, grace: false, grace_order: 0, hand: None, stem_up: None }
    }

    fn score_of(beats: u32, beat_type: u32, bars: usize, notes: Vec<ScoreNote>) -> Score {
        let len = f64::from(beats) * 4.0 / f64::from(beat_type);
        let measures = (0..bars).map(|i| Measure { number: (i + 1).to_string(), start_q: i as f64 * len, len_q: len, beats, beat_type }).collect();
        let mut notes = notes;
        for n in &mut notes {
            n.measure = (n.q / len + EPS).floor() as usize;
        }
        Score { measures, notes, ..Score::default() }
    }

    fn hands(qs: &[f64]) -> Vec<ScoreNote> {
        qs.iter().map(|q| note(*q, Lane::Snare)).collect()
    }

    #[test]
    fn eighths_are_beamed_by_the_beat_and_a_quarter_has_no_flag() {
        let s = score_of(4, 4, 1, hands(&[0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 3.5]));
        let n = analyse(&s);
        let beams: Vec<Vec<f64>> = n.beams.iter().map(|b| b.onsets.iter().map(|o| o.q).collect()).collect();
        assert_eq!(beams, vec![vec![0.0, 0.5], vec![1.0, 1.5], vec![3.0, 3.5]]);
        assert!(n.beams.iter().all(|b| b.onsets.iter().all(|o| o.level == 1)));
        assert_eq!(n.flags[4], 0, "a quarter on beat 3 has neither flag nor beam");
        assert_eq!(n.beam_of[4], None);
    }

    #[test]
    fn sixteenths_have_two_beams_and_mixed_groups_keep_their_own_levels() {
        let s = score_of(4, 4, 1, hands(&[0.0, 0.25, 0.5, 0.75, 1.0, 1.5, 1.75]));
        let n = analyse(&s);
        assert_eq!(n.beams[0].onsets.iter().map(|o| o.level).collect::<Vec<_>>(), vec![2, 2, 2, 2]);
        // An eighth and two sixteenths: the beam levels are 1, 2, 2 and the last link has two beams.
        assert_eq!(n.beams[1].onsets.iter().map(|o| o.level).collect::<Vec<_>>(), vec![1, 2, 2]);
    }

    #[test]
    fn a_lone_note_on_the_off_beat_has_a_flag_and_a_lone_one_on_the_beat_has_none() {
        let s = score_of(4, 4, 1, hands(&[0.5, 1.0, 2.75]));
        let n = analyse(&s);
        assert!(n.beams.is_empty());
        assert_eq!(n.flags, vec![1, 0, 2]);
    }

    #[test]
    fn hands_and_feet_are_beamed_apart() {
        let mut notes = vec![note(0.0, Lane::HiHatClosed), note(0.0, Lane::Kick), note(0.5, Lane::HiHatClosed), note(0.5, Lane::Snare)];
        notes[1].stem_up = Some(false);
        let s = score_of(4, 4, 1, notes);
        let n = analyse(&s);
        // Hi-hat and snare both have their stems up and beam together (the snare at 0.5 is in
        // the same chord as the hi-hat), the kick is alone with its stem down on the beat.
        assert_eq!(n.beams.len(), 1);
        assert_eq!(n.beams[0].onsets[1].notes.len(), 2);
        assert!(n.beams[0].up);
        assert_eq!(n.flags[1], 0);
    }

    #[test]
    fn triplets_and_sextuplets_get_a_number_and_sixteenths_do_not() {
        let t = score_of(4, 4, 1, hands(&[0.0, 1.0 / 3.0, 2.0 / 3.0]));
        let n = analyse(&t);
        assert_eq!(n.tuplets.len(), 1);
        assert_eq!((n.tuplets[0].number, n.tuplets[0].notes.len()), (3, 3));
        assert_eq!(n.beams[0].onsets.iter().map(|o| o.level).collect::<Vec<_>>(), vec![1, 1, 1], "eighth triplets have one beam");

        let six: Vec<f64> = (0..6).map(|k| f64::from(k) / 6.0).collect();
        let n = analyse(&score_of(4, 4, 1, hands(&six)));
        assert_eq!(n.tuplets[0].number, 6);
        assert_eq!(n.beams[0].onsets[0].level, 2);

        let five: Vec<f64> = (0..5).map(|k| f64::from(k) / 5.0).collect();
        assert_eq!(analyse(&score_of(4, 4, 1, hands(&five))).tuplets[0].number, 5);

        assert!(analyse(&score_of(4, 4, 1, hands(&[0.0, 0.25, 0.5, 0.75]))).tuplets.is_empty());
    }

    #[test]
    fn compound_meters_beam_in_dotted_quarters_and_grace_notes_are_left_out() {
        let mut notes = hands(&[0.0, 0.5, 1.0, 1.5, 2.0, 2.5]);
        let mut grace = note(0.0, Lane::Snare);
        grace.grace = true;
        notes.insert(0, grace);
        let s = score_of(6, 8, 1, notes);
        let n = analyse(&s);
        let beams: Vec<usize> = n.beams.iter().map(|b| b.onsets.len()).collect();
        assert_eq!(beams, vec![3, 3], "two groups of three eighths");
        assert_eq!((n.flags[0], n.beam_of[0]), (0, None));
    }

    #[test]
    fn a_pickup_or_odd_bar_does_not_panic_and_a_second_bar_is_grouped_alone() {
        let s = score_of(2, 4, 2, hands(&[0.5, 1.0, 2.0, 2.5, 3.0]));
        let n = analyse(&s);
        // Notes at 2.0 and 2.5 are in the second bar (starts at 2.0) and beam together.
        assert!(n.beams.iter().any(|b| b.measure == 1 && b.onsets.len() == 2));
        assert_eq!(analyse(&Score::default()), Notation::default());
    }

    #[test]
    fn a_rest_is_written_with_one_sign_when_it_can_be() {
        use crate::score::Rest;
        let rest = |dur_q: f64, whole_bar: bool| Rest { measure: 0, q: 1.0, dur_q, up: true, whole_bar };
        let kinds = |r: &Rest| rest_glyphs(r).iter().map(|g| (g.kind, g.dotted)).collect::<Vec<_>>();
        assert_eq!(kinds(&rest(1.0, false)), vec![(RestKind::Quarter, false)]);
        assert_eq!(kinds(&rest(1.5, false)), vec![(RestKind::Quarter, true)]);
        assert_eq!(kinds(&rest(1.0 / 3.0, false)), vec![(RestKind::Flagged(1), false)], "an eighth triplet");
        assert_eq!(kinds(&rest(0.25, false)), vec![(RestKind::Flagged(2), false)]);
        // 1.25 quarters: a quarter, then a sixteenth, one after the other.
        let g = rest_glyphs(&rest(1.25, false));
        assert_eq!((g.len(), g[0].kind, g[1].kind, g[1].q), (2, RestKind::Quarter, RestKind::Flagged(2), 2.0));
        let bar = rest_glyphs(&rest(3.0, true));
        assert_eq!((bar.len(), bar[0].kind, bar[0].bar), (1, RestKind::Whole, true));
    }
}
