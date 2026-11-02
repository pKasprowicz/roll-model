//! The neutral form every format is read into. A parser fills a [`RawScore`]; `finish` turns it
//! into the [`Score`](crate::score::Score). Nothing here belongs to one file format.

use super::MidiReport;
use crate::drums::{Lane, NoteHead};
use crate::score::{Dynamic, Stem};

/// An onset, as a parser found it: its place inside its measure, in quarter notes.
#[derive(Clone, Debug)]
pub struct RawNote {
    pub offset_q: f64,
    pub lane: Lane,
    pub staff_step: i32,
    pub head: NoteHead,
    pub dynamic: Dynamic,
    pub grace: bool,
    pub stem: Option<Stem>,
}

/// A rest the file prints.
#[derive(Clone, Debug)]
pub struct RawRest {
    pub offset_q: f64,
    pub dur_q: f64,
    /// The voice it was written in; `stem` is decided from the voices once all measures are read.
    pub voice: String,
    pub whole_bar: bool,
    pub stem: Stem,
}

/// One measure as written, before repeats are unrolled.
#[derive(Clone, Debug, Default)]
pub struct RawMeasure {
    pub number: String,
    pub len_q: f64,
    pub beats: u32,
    pub beat_type: u32,
    pub notes: Vec<RawNote>,
    pub rests: Vec<RawRest>,
    /// `(offset inside the measure in quarters, quarter-note BPM)`.
    pub tempos: Vec<(f64, f64)>,
    pub forward_repeat: bool,
    pub backward_repeat: Option<u32>,
    /// The numbers of the 1st/2nd endings this measure belongs to; empty if none.
    pub endings: Vec<u32>,
}

/// A part (staff group) the file holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartInfo {
    pub name: String,
    /// Looks like a drum part (unpitched notes or a percussion clef).
    pub drums: bool,
}

#[derive(Clone, Debug, Default)]
pub struct RawScore {
    pub title: String,
    pub part_name: String,
    pub parts: Vec<PartInfo>,
    /// Index into `parts` of the part that was read.
    pub part_used: usize,
    pub measures: Vec<RawMeasure>,
    /// What the parser had to guess.
    pub warnings: Vec<String>,
    /// What the quantiser did, for a MIDI file.
    pub midi: Option<MidiReport>,
}
