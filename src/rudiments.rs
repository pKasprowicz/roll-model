//! The PAS international drum rudiments (see `docs/rudiments.md`) as strokes, and a
//! builder that turns one of them into a `Score`, so the Rudiments tab can show,
//! play and grade it like any other score. No GUI or MIDI dependencies.
//!
//! A rudiment is written as a *pattern*: whitespace-separated strokes on a grid of
//! `unit` quarter notes.
//!
//! - `R` / `L`: a stroke with that hand; `Z`: a buzz (closed) roll, one held note.
//! - A lower-case prefix is the grace notes before it, played with the other hand:
//!   `lR` is a flam, `llR` a drag.
//! - `>` after the hand accents the stroke.
//! - `:n` after it makes the stroke `n` grid units long instead of one (the next
//!   stroke is that much later); `n` may be fractional for two strokes in one unit.
//!
//! A pattern is one *cell* in the right-hand lead. The cell's length is rounded up to
//! whole quarter notes and it is repeated; the left lead is the mirror image.

use crate::drums::Lane;
use crate::score::{Hand, Measure, Score, ScoreNote, TempoMap};

/// The tempo the rudiment scores are written at; the practice tempo (percent) is then the BPM.
pub const BASE_BPM: f64 = 100.0;
/// How many times a cell can be repeated.
pub const MAX_REPEATS: usize = 32;
/// How many times it is repeated at first.
pub const DEFAULT_REPEATS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Roll,
    Diddle,
    Flam,
    Drag,
}

impl Family {
    pub const ALL: [Family; 4] = [Family::Roll, Family::Diddle, Family::Flam, Family::Drag];

    pub fn label(self) -> &'static str {
        match self {
            Family::Roll => "Roll",
            Family::Diddle => "Diddle",
            Family::Flam => "Flam",
            Family::Drag => "Drag",
        }
    }
}

/// Which hand starts each repetition of a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lead {
    Right,
    Left,
    /// Right, then left, then right again, and so on.
    Alternate,
}

impl Lead {
    pub const ALL: [Lead; 3] = [Lead::Right, Lead::Left, Lead::Alternate];

    pub fn label(self) -> &'static str {
        match self {
            Lead::Right => "Right",
            Lead::Left => "Left",
            Lead::Alternate => "Alternate",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lead::Right => "right",
            Lead::Left => "left",
            Lead::Alternate => "alternate",
        }
    }

    pub fn from_name(s: &str) -> Option<Lead> {
        Lead::ALL.into_iter().find(|l| l.name() == s)
    }

    /// The next one in the list, wrapping around.
    pub fn next(self) -> Lead {
        match self {
            Lead::Right => Lead::Left,
            Lead::Left => Lead::Alternate,
            Lead::Alternate => Lead::Right,
        }
    }
}

/// One stroke of a cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    /// Where it starts in the cell, in quarter notes.
    pub at: f64,
    /// How long it is written for, in quarter notes.
    pub len: f64,
    pub hand: Hand,
    pub accent: bool,
    /// Grace notes before it (0, 1 = flam, 2 = drag), played with the other hand.
    pub graces: u8,
    /// A buzz roll: one held note, no particular hand.
    pub buzz: bool,
}

/// A rudiment's strokes and how long one repetition lasts.
#[derive(Clone, Debug)]
pub struct Cell {
    pub strokes: Vec<Stroke>,
    /// Whole quarter notes per repetition.
    pub beats: u32,
}

pub struct Rudiment {
    /// 1 to 40, the PAS numbering.
    pub number: u8,
    pub name: &'static str,
    pub family: Family,
    /// One of the original NARD Standard 26 American rudiments.
    pub nard: bool,
    /// One grid step in quarter notes.
    unit: f64,
    pattern: &'static str,
    /// What it is, in a line.
    pub about: &'static str,
}

const S16: f64 = 0.25;
const S8: f64 = 0.5;
const T16: f64 = 1.0 / 6.0;

const fn rud(number: u8, name: &'static str, family: Family, nard: bool, unit: f64, pattern: &'static str, about: &'static str) -> Rudiment {
    Rudiment { number, name, family, nard, unit, pattern, about }
}

/// The 40 rudiments, in the PAS order.
static CATALOGUE: [Rudiment; 40] = [
    rud(1, "Single stroke roll", Family::Roll, true, S16, "R L R L R L R L", "Alternating single strokes."),
    rud(2, "Single stroke four", Family::Roll, false, T16, "R L R L:3 R L R L:3", "Three quick strokes and one on the second half of the beat, twice."),
    rud(3, "Single stroke seven", Family::Roll, false, T16, "R L R L R L R:6", "Six quick strokes in a beat, then a seventh on the next."),
    rud(4, "Multiple bounce roll", Family::Roll, false, S16, "Z:8", "A closed (buzz) roll: one held note. It can only be listened to, not graded stroke by stroke."),
    rud(5, "Triple stroke roll", Family::Roll, false, T16, "R R R L L L R R R L L L", "Three strokes with each hand in turn."),
    rud(6, "Double stroke open roll", Family::Roll, true, S16, "R R L L R R L L", "Double strokes: two with each hand."),
    rud(7, "Five stroke roll", Family::Roll, true, S16, "R R L L R>", "Two diddles and an accent."),
    rud(8, "Six stroke roll", Family::Roll, false, S16, "R> L L R R L>", "An accent, two diddles, an accent."),
    rud(9, "Seven stroke roll", Family::Roll, true, S16, "R R L L R R L>", "Three diddles and an accent."),
    rud(10, "Nine stroke roll", Family::Roll, true, S16, "R R L L R R L L R>", "Four diddles and an accent."),
    rud(11, "Ten stroke roll", Family::Roll, true, S16, "R R L L R R L L R> L>", "Four diddles and two accents."),
    rud(12, "Eleven stroke roll", Family::Roll, true, S16, "R R L L R R L L R R L>", "Five diddles and an accent."),
    rud(13, "Thirteen stroke roll", Family::Roll, true, S16, "R R L L R R L L R R L L R>", "Six diddles and an accent."),
    rud(14, "Fifteen stroke roll", Family::Roll, true, S16, "R R L L R R L L R R L L R R L>", "Seven diddles and an accent."),
    rud(15, "Seventeen stroke roll", Family::Roll, false, S16, "R R L L R R L L R R L L R R L L R>", "Eight diddles and an accent."),
    rud(16, "Single paradiddle", Family::Diddle, true, S16, "R> L R R L> R L L", "Two singles and a diddle; the accent starts each group."),
    rud(17, "Double paradiddle", Family::Diddle, true, S16, "R> L R L R R L> R L R L L", "Four singles and a diddle."),
    rud(18, "Triple paradiddle", Family::Diddle, false, S16, "R> L R L R L R R L> R L R L R L L", "Six singles and a diddle."),
    rud(19, "Single paradiddle-diddle", Family::Diddle, false, S16, "R> L R R L L R> L R R L L", "Two singles and two diddles."),
    rud(20, "Flam", Family::Flam, true, S16, "lR>:4 rL>:4", "A soft grace note just before a main stroke with the other hand."),
    rud(21, "Flam accent", Family::Flam, true, S8, "lR> L R rL> R L", "A flam and two taps, in threes."),
    rud(22, "Flam tap", Family::Flam, true, S16, "lR> R rL> L lR> R rL> L", "A flam on the first stroke of each diddle."),
    rud(23, "Flamacue", Family::Flam, true, S16, "lR L> R L lR:4", "A flam, an accented tap, two taps, and a flam on the next beat."),
    rud(24, "Flam paradiddle", Family::Flam, true, S16, "lR> L R R rL> R L L", "A paradiddle with a flam on the first stroke."),
    rud(25, "Single flammed mill", Family::Flam, false, S16, "lR> R L R rL> L R L", "An inverted paradiddle with a flam on the first stroke."),
    rud(26, "Flam paradiddle-diddle", Family::Flam, true, S16, "lR> L R R L L rL> R L L R R", "A paradiddle-diddle with a flam on the first stroke."),
    rud(27, "Pataflafla", Family::Flam, false, S16, "lR> L R rL> lR> L R rL>", "Four strokes with a flam on the first and the last."),
    rud(28, "Swiss army triplet", Family::Flam, false, T16, "lR> R L lR> R L", "A flam and two taps as a triplet."),
    rud(29, "Inverted flam tap", Family::Flam, false, S16, "lR> L rL> R lR> L rL> R", "A flam on the second stroke of each diddle."),
    rud(30, "Flam drag", Family::Flam, false, S16, "lR>:2 L L R:2 rL>:2 R R L:2", "A flam, a drag (a double stroke) and a tap."),
    rud(31, "Drag", Family::Drag, true, S16, "llR:4 rrL:4", "Two soft grace notes before a main stroke with the other hand."),
    rud(32, "Single drag tap", Family::Drag, true, S8, "llR L> rrL R>", "A drag, then an accented tap with the other hand."),
    rud(33, "Double drag tap", Family::Drag, true, S8, "llR llR L> rrL rrL R>", "Two drags, then an accented tap."),
    rud(34, "Lesson 25", Family::Drag, true, S16, "llR L R>:2 llR L R>:2", "A drag, a tap and an accented tap, in threes."),
    rud(35, "Single dragadiddle", Family::Drag, false, S16, "R>:0.5 R:0.5 L R R L>:0.5 L:0.5 R L L", "A paradiddle whose first stroke is a diddle."),
    rud(36, "Drag paradiddle #1", Family::Drag, true, S16, "R>:2 llR L R R L>:2 rrL R L L", "An accented stroke, then a paradiddle with a drag on its first stroke."),
    rud(37, "Drag paradiddle #2", Family::Drag, true, S16, "R>:2 llR:2 llR L R R L>:2 rrL:2 rrL R L L", "An accented stroke, a drag, then a paradiddle with a drag on its first stroke."),
    rud(38, "Single ratamacue", Family::Drag, true, T16, "llR L R L>:3 rrL R L R>:3", "A drag, two taps and an accented tap."),
    rud(39, "Double ratamacue", Family::Drag, true, T16, "llR:3 llR L R L>:3 rrL:3 rrL R L R>:3", "A drag, then a single ratamacue."),
    rud(40, "Triple ratamacue", Family::Drag, true, T16, "llR:3 llR:3 llR L R L>:3 rrL:3 rrL:3 rrL R L R>:3", "Two drags, then a single ratamacue."),
];

pub fn all() -> &'static [Rudiment] {
    &CATALOGUE
}

pub fn by_number(number: u8) -> Option<&'static Rudiment> {
    CATALOGUE.iter().find(|r| r.number == number)
}

/// Reads a pattern; the strokes lie on a grid of `unit` quarter notes.
fn parse(unit: f64, pattern: &str) -> Result<Vec<Stroke>, String> {
    let mut strokes = Vec::new();
    let mut at = 0.0;
    for token in pattern.split_whitespace() {
        let (body, len) = match token.split_once(':') {
            Some((body, n)) => (body, n.parse::<f64>().map_err(|_| format!("bad length in \"{token}\""))?),
            None => (token, 1.0),
        };
        if len <= 0.0 {
            return Err(format!("bad length in \"{token}\""));
        }
        let (body, accent) = match body.strip_suffix('>') {
            Some(b) => (b, true),
            None => (body, false),
        };
        let graces = body.chars().take_while(|c| c.is_ascii_lowercase()).count();
        let (prefix, main) = body.split_at(graces);
        let (hand, buzz) = match main {
            "R" => (Hand::Right, false),
            "L" => (Hand::Left, false),
            "Z" => (Hand::Right, true),
            _ => return Err(format!("bad stroke \"{token}\"")),
        };
        // The grace notes are played with the other hand; the letters only say so for the reader.
        let grace_letter = if hand == Hand::Right { 'l' } else { 'r' };
        if graces > 2 || prefix.chars().any(|c| c != grace_letter) {
            return Err(format!("bad grace notes in \"{token}\""));
        }
        strokes.push(Stroke { at: at * unit, len: len * unit, hand, accent, graces: graces as u8, buzz });
        at += len;
    }
    if strokes.is_empty() {
        return Err("no strokes".into());
    }
    Ok(strokes)
}

impl Rudiment {
    /// Whether a take of it can be graded: a buzz roll cannot.
    pub fn gradable(&self) -> bool {
        !self.cell().strokes.iter().any(|s| s.buzz)
    }

    /// One repetition in the right-hand lead. The patterns are fixed and tested, so a bad one is a bug.
    pub fn cell(&self) -> Cell {
        let strokes = parse(self.unit, self.pattern).unwrap_or_else(|e| panic!("rudiment {}: {e}", self.name));
        let end = strokes.last().map_or(0.0, |s| s.at + s.len);
        Cell { strokes, beats: (end - 1e-9).ceil().max(1.0) as u32 }
    }

    /// The sticking as it is usually written: `lR L R`, grace notes in lower case.
    pub fn sticking(&self) -> String {
        self.cell()
            .strokes
            .iter()
            .map(|s| {
                let mut t = String::new();
                if s.buzz {
                    return "buzz".to_string();
                }
                let grace = if s.hand == Hand::Right { "l" } else { "r" };
                t.push_str(&grace.repeat(usize::from(s.graces)));
                t.push_str(s.hand.letter());
                t
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The lead that makes repeating the cell flow: a cell that ends with the hand it begins with
    /// would play that hand twice in a row, so its repetitions alternate; the others repeat as they are.
    pub fn default_lead(&self) -> Lead {
        let cell = self.cell();
        match (cell.strokes.first(), cell.strokes.last()) {
            (Some(a), Some(b)) if a.hand == b.hand && !a.buzz => Lead::Alternate,
            _ => Lead::Right,
        }
    }

    /// A tempo to start at: about six strokes a second on the closest strokes, in BPM.
    pub fn default_bpm(&self) -> f64 {
        let cell = self.cell();
        let closest = cell.strokes.windows(2).map(|w| w[1].at - w[0].at).fold(f64::INFINITY, f64::min);
        if !closest.is_finite() {
            return 60.0;
        }
        ((360.0 * closest / 5.0).round() * 5.0).clamp(40.0, 120.0)
    }

    /// Whether the name or the number contains what was typed (case-insensitive).
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        q.is_empty() || self.name.to_lowercase().contains(&q) || self.number.to_string() == q
    }

    /// The rudiment as a score: `repeats` bars, one per repetition of the cell, written at
    /// `BASE_BPM`, so the practice tempo in percent is the tempo in BPM.
    pub fn build(&self, repeats: usize, lead: Lead) -> Score {
        let cell = self.cell();
        let repeats = repeats.clamp(1, MAX_REPEATS);
        let (step, head) = Lane::Snare.default_staff();
        let beats = f64::from(cell.beats);
        let mut score = Score {
            title: self.name.to_string(),
            part_name: format!("{} rudiment {}{}", self.family.label(), self.number, if self.nard { " · NARD 26" } else { "" }),
            tempo: TempoMap::constant(BASE_BPM),
            ..Score::default()
        };
        for i in 0..repeats {
            let start_q = i as f64 * beats;
            score.measures.push(Measure { number: (i + 1).to_string(), start_q, len_q: beats, beats: cell.beats, beat_type: 4 });
            let mirrored = match lead {
                Lead::Right => false,
                Lead::Left => true,
                Lead::Alternate => i % 2 == 1,
            };
            for stroke in &cell.strokes {
                let hand = if mirrored { stroke.hand.other() } else { stroke.hand };
                let note = |grace_order: u8, hand: Option<Hand>, accent: bool| ScoreNote {
                    q: start_q + stroke.at,
                    measure: i,
                    lane: Lane::Snare,
                    staff_step: step,
                    head,
                    accent,
                    ghost: false,
                    grace: grace_order > 0,
                    grace_order,
                    hand,
                    stem_up: Some(true),
                };
                // The earliest grace note first, so the notes stay in the order they sound.
                for order in (1..=stroke.graces).rev() {
                    score.notes.push(note(order, Some(hand.other()), false));
                }
                score.notes.push(note(0, (!stroke.buzz).then_some(hand), stroke.accent));
            }
        }
        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rudiment(number: u8) -> &'static Rudiment {
        by_number(number).unwrap()
    }

    #[test]
    fn there_are_forty_numbered_in_order_with_the_known_counts() {
        assert_eq!(all().len(), 40);
        assert!(all().iter().enumerate().all(|(i, r)| r.number as usize == i + 1));
        let count = |f: Family, nard: bool| all().iter().filter(|r| r.family == f && (!nard || r.nard)).count();
        assert_eq!([Family::Roll, Family::Diddle, Family::Flam, Family::Drag].map(|f| count(f, false)), [15, 4, 11, 10]);
        assert_eq!([Family::Roll, Family::Diddle, Family::Flam, Family::Drag].map(|f| count(f, true)), [9, 2, 6, 9]);
        assert_eq!(all().iter().filter(|r| r.nard).count(), 26, "the NARD Standard 26");
        // Grouped by family in the PAS order.
        assert!(all().windows(2).all(|w| (w[0].family as u8) <= (w[1].family as u8)));
        let mut names: Vec<_> = all().iter().map(|r| r.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 40, "names are unique");
    }

    #[test]
    fn every_pattern_reads_and_has_the_length_of_the_catalogue() {
        // Beats per repetition, as listed in docs/rudiments.md (rolls on a 16th grid).
        let beats = [
            2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 4, 4, 5, // rolls
            2, 3, 4, 3, // diddles
            2, 3, 2, 2, 2, 2, 3, 2, 1, 2, 3, // flams
            2, 2, 3, 2, 2, 3, 4, 2, 3, 4, // drags
        ];
        for (r, want) in all().iter().zip(beats) {
            let cell = r.cell();
            assert_eq!(cell.beats, want, "{} (#{})", r.name, r.number);
            let mut end = 0.0;
            for s in &cell.strokes {
                assert!((s.at - end).abs() < 1e-9, "{}: strokes follow each other", r.name);
                end = s.at + s.len;
            }
            assert!(end <= f64::from(cell.beats) + 1e-9 && end > f64::from(cell.beats) - 1.0, "{}: {end}", r.name);
        }
    }

    #[test]
    fn the_sticking_is_the_one_of_the_chart() {
        let want = [
            (1, "R L R L R L R L"),
            (2, "R L R L R L R L"),
            (3, "R L R L R L R"),
            (5, "R R R L L L R R R L L L"),
            (6, "R R L L R R L L"),
            (7, "R R L L R"),
            (8, "R L L R R L"),
            (11, "R R L L R R L L R L"),
            (15, "R R L L R R L L R R L L R R L L R"),
            (16, "R L R R L R L L"),
            (17, "R L R L R R L R L R L L"),
            (18, "R L R L R L R R L R L R L R L L"),
            (19, "R L R R L L R L R R L L"),
            (20, "lR rL"),
            (21, "lR L R rL R L"),
            (22, "lR R rL L lR R rL L"),
            (23, "lR L R L lR"),
            (24, "lR L R R rL R L L"),
            (25, "lR R L R rL L R L"),
            (26, "lR L R R L L rL R L L R R"),
            (27, "lR L R rL lR L R rL"),
            (28, "lR R L lR R L"),
            (29, "lR L rL R lR L rL R"),
            (30, "lR L L R rL R R L"),
            (31, "llR rrL"),
            (32, "llR L rrL R"),
            (33, "llR llR L rrL rrL R"),
            (34, "llR L R llR L R"),
            (35, "R R L R R L L R L L"),
            (36, "R llR L R R L rrL R L L"),
            (37, "R llR llR L R R L rrL rrL R L L"),
            (38, "llR L R L rrL R L R"),
            (39, "llR llR L R L rrL rrL R L R"),
            (40, "llR llR llR L R L rrL rrL rrL R L R"),
        ];
        for (n, sticking) in want {
            assert_eq!(rudiment(n).sticking(), sticking, "#{n} {}", rudiment(n).name);
        }
        assert_eq!(rudiment(4).sticking(), "buzz");
    }

    #[test]
    fn accents_and_note_values_follow_the_chart() {
        let accents = |n: u8| rudiment(n).cell().strokes.iter().filter(|s| s.accent).map(|s| s.at).collect::<Vec<_>>();
        assert_eq!(accents(16), [0.0, 1.0], "a paradiddle accents the first stroke of each group");
        assert_eq!(accents(17), [0.0, 1.5]);
        assert_eq!(accents(18), [0.0, 2.0]);
        assert_eq!(accents(1), Vec::<f64>::new());
        assert_eq!(accents(23), [0.25], "the flamacue accents its second stroke");
        assert_eq!(accents(7), [1.0], "a roll ends on an accent");
        assert_eq!(accents(11), [2.0, 2.25], "the ten stroke roll has two");
        // Flams and drags.
        let graces = |n: u8| rudiment(n).cell().strokes.iter().map(|s| s.graces).collect::<Vec<_>>();
        assert_eq!(graces(20), [1, 1]);
        assert_eq!(graces(31), [2, 2]);
        assert_eq!(graces(40), [2, 2, 2, 0, 0, 0, 2, 2, 2, 0, 0, 0]);
        assert!(rudiment(16).cell().strokes.iter().all(|s| s.graces == 0));
        // A triplet rudiment is on a triplet grid, the single dragadiddle has two strokes in one 16th.
        let at = |n: u8| rudiment(n).cell().strokes.iter().map(|s| s.at).collect::<Vec<_>>();
        assert!((at(38)[1] - 1.0 / 6.0).abs() < 1e-9 && (at(38)[3] - 0.5).abs() < 1e-9);
        assert_eq!(&at(35)[..4], [0.0, 0.125, 0.25, 0.5]);
    }

    #[test]
    fn a_bad_pattern_is_refused() {
        assert!(parse(0.25, "R L").is_ok());
        for bad in ["", "X", "R:0", "R:x", "lL", "rR", "lllR", "R>>", "Rl", "R: 1"] {
            assert!(parse(0.25, bad).is_err(), "{bad:?}");
        }
        assert!(parse(0.25, "lR rL llR rrL R> Z:8").is_ok());
    }

    #[test]
    fn a_score_has_a_bar_per_repetition_and_the_notes_in_order() {
        let r = rudiment(24); // flam paradiddle
        let score = r.build(3, Lead::Right);
        assert_eq!(score.measures.len(), 3);
        assert!(score.measures.iter().all(|m| m.len_q == 2.0 && m.beats == 2 && m.beat_type == 4));
        assert_eq!(score.measures[1].start_q, 2.0);
        assert_eq!(score.notes.len(), 3 * (8 + 2), "eight strokes and two grace notes per bar");
        assert!(score.notes.windows(2).all(|w| w[0].q <= w[1].q), "in time order");
        assert!(score.notes.iter().all(|n| n.lane == Lane::Snare));
        assert!((score.tempo.bpm_at(0.0) - BASE_BPM).abs() < 1e-9);
        // A grace note comes right before its main note, with the other hand, and is not accented.
        let (g, m) = (&score.notes[0], &score.notes[1]);
        assert!(g.grace && g.grace_order == 1 && !g.accent && g.hand == Some(Hand::Left));
        assert!(!m.grace && m.accent && m.hand == Some(Hand::Right) && g.q == m.q);
        // Every note is in the bar its time says.
        for n in &score.notes {
            assert_eq!(score.measure_at(n.q), Some(n.measure));
        }
        // A drag has two, the earlier one first.
        let drag = rudiment(31).build(1, Lead::Right);
        assert_eq!(drag.notes.iter().map(|n| n.grace_order).collect::<Vec<_>>(), [2, 1, 0, 2, 1, 0]);
        // The repetitions are limited.
        assert_eq!(r.build(0, Lead::Right).measures.len(), 1);
        assert_eq!(r.build(1000, Lead::Right).measures.len(), MAX_REPEATS);
    }

    #[test]
    fn the_left_lead_is_the_mirror_image_and_alternate_alternates() {
        let hands = |s: &Score| s.notes.iter().filter(|n| !n.grace).filter_map(|n| n.hand).map(|h| h.letter()).collect::<String>();
        let r = rudiment(16);
        assert_eq!(hands(&r.build(1, Lead::Right)), "RLRRLRLL");
        assert_eq!(hands(&r.build(1, Lead::Left)), "LRLLRLRR");
        assert_eq!(hands(&r.build(2, Lead::Alternate)), "RLRRLRLLLRLLRLRR");
        // The grace notes switch with the hands.
        let flam = rudiment(20).build(1, Lead::Left);
        assert_eq!(flam.notes[0].hand, Some(Hand::Right));
        assert_eq!(flam.notes[1].hand, Some(Hand::Left));
        // Only the hands change, not the timing.
        let (a, b) = (r.build(2, Lead::Right), r.build(2, Lead::Left));
        assert!(a.notes.iter().zip(&b.notes).all(|(x, y)| x.q == y.q && x.accent == y.accent));
    }

    #[test]
    fn a_buzz_roll_is_one_note_and_cannot_be_graded() {
        let r = rudiment(4);
        assert!(!r.gradable());
        assert!(all().iter().filter(|r| r.gradable()).count() == 39);
        let score = r.build(2, Lead::Right);
        assert_eq!(score.notes.len(), 2);
        assert!(score.notes.iter().all(|n| n.hand.is_none() && !n.grace));
    }

    #[test]
    fn the_default_lead_makes_the_repetitions_flow() {
        // A cell that ends with the hand it starts with would play it twice in a row.
        assert_eq!(rudiment(7).default_lead(), Lead::Alternate, "five stroke roll: R R L L R");
        assert_eq!(rudiment(3).default_lead(), Lead::Alternate);
        assert_eq!(rudiment(16).default_lead(), Lead::Right, "a paradiddle repeats as it is");
        assert_eq!(rudiment(1).default_lead(), Lead::Right);
        assert_eq!(rudiment(4).default_lead(), Lead::Right);
        for r in all().iter().filter(|r| r.gradable()) {
            let score = r.build(4, r.default_lead());
            let main: Vec<Hand> = score.notes.iter().filter(|n| !n.grace).filter_map(|n| n.hand).collect();
            let per_cell = r.cell().strokes.len();
            assert_eq!(main.len(), 4 * per_cell);
            for join in 1..4 {
                let at = join * per_cell;
                assert_ne!(main[at - 1], main[at], "{}: the hands alternate where a repetition ends and the next begins", r.name);
            }
        }
    }

    #[test]
    fn a_tempo_to_start_at_is_sensible_for_every_rudiment() {
        for r in all() {
            let bpm = r.default_bpm();
            assert!((40.0..=120.0).contains(&bpm) && bpm % 5.0 == 0.0, "{}: {bpm}", r.name);
        }
        assert_eq!(rudiment(16).default_bpm(), 90.0, "16ths: six strokes a second");
        assert_eq!(rudiment(28).default_bpm(), 60.0, "triplets");
        assert_eq!(rudiment(35).default_bpm(), 45.0, "two strokes in a 16th");
    }

    #[test]
    fn the_search_finds_names_and_numbers() {
        assert!(rudiment(16).matches("para"));
        assert!(rudiment(16).matches("PARADIDDLE"));
        assert!(rudiment(16).matches("16") && !rudiment(16).matches("17"));
        assert!(rudiment(16).matches("  "));
        assert_eq!(all().iter().filter(|r| r.matches("paradiddle")).count(), 8);
        assert_eq!(Lead::from_name("alternate"), Some(Lead::Alternate));
        assert_eq!(Lead::from_name("both"), None);
    }
}
