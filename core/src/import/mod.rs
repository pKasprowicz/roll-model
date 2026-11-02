//! Opening a score file: bytes in, a [`Score`] and a [`Report`] out.
//!
//! ```text
//! bytes --> sniff --> unwrap --> parse --> finish --> Loaded { score, report }
//!           Format    container  per format  shared
//! ```
//!
//! A format is a parser that fills the neutral [`RawScore`]; everything after it is shared, so a
//! new format adds one parser and one `Format` and nothing else. The result is a pure function of
//! the bytes, the name and the [`ImportOptions`].

mod container;
mod finish;
mod ir;
mod midi;
mod musicxml;
mod quantise;
#[doc(hidden)]
pub mod smf;
mod sniff;

use std::path::Path;

pub use finish::check_invariants;
pub use ir::{PartInfo, RawMeasure, RawNote, RawRest, RawScore};
pub use quantise::{Grid, MidiOptions, MidiReport, MOVED_MS};
pub use sniff::{sniff, Detected, Format};

use crate::score::Score;

/// The file extensions the program opens; the one list behind the open dialog and the hints.
pub const EXTENSIONS: &[&str] = &["musicxml", "xml", "mxl", "mid", "midi"];

/// Bounds on what one file may ask of the program. Loading runs on the window's thread, so a file
/// that expands without end must be refused instead of read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The size of the file, and of the document inside a compressed one.
    pub max_bytes: usize,
    /// Measures, after repeats are played out.
    pub max_measures: usize,
    pub max_notes: usize,
    /// Events of a MIDI file, whatever they are.
    pub max_events: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_bytes: 64 << 20, max_measures: 20_000, max_notes: 500_000, max_events: 2_000_000 }
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ImportOptions {
    /// Play repeats and 1st/2nd endings out, so the score is the music as it is played.
    pub expand_repeats: bool,
    /// Which part of the file to read (its place in `Report::parts`); `None` picks the drum part.
    pub part: Option<usize>,
    pub limits: Limits,
    pub midi: MidiOptions,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions { expand_repeats: true, part: None, limits: Limits::default(), midi: MidiOptions::default() }
    }
}

impl ImportOptions {
    pub fn with_repeats(mut self, expand: bool) -> Self {
        self.expand_repeats = expand;
        self
    }

    pub fn with_part(mut self, part: Option<usize>) -> Self {
        self.part = part;
        self
    }

    pub fn with_midi(mut self, midi: MidiOptions) -> Self {
        self.midi = midi;
        self
    }
}

/// What the import knows about the file besides the score.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub format: Format,
    /// The parts the file holds.
    pub parts: Vec<PartInfo>,
    /// The place in `parts` of the one that was read.
    pub part_used: usize,
    /// For a MIDI file: how its notes were placed.
    pub midi: Option<MidiReport>,
}

#[derive(Clone, Debug)]
pub struct Loaded {
    pub score: Score,
    pub report: Report,
}

/// Why a file is not a score this program can open. The texts are what the user reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportError {
    /// The file could not be read.
    Read(String),
    /// Nothing in the file or its name says it is a score the program reads.
    Unrecognised,
    /// A format known by name but not read: export the score as one that is.
    Recognised {
        format: &'static str,
    },
    TooLarge(String),
    /// The archive or the text encoding around the score is damaged.
    Container(String),
    Parse {
        format: Format,
        reason: String,
    },
    NoDrumPart,
    NoMeasures,
    /// The parser produced a score that breaks a rule of [`check_invariants`]; a defect of the
    /// importer, not of the file.
    Invalid(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Read(e) | ImportError::Container(e) => write!(f, "{e}"),
            ImportError::Unrecognised => write!(
                f,
                "not a score file this program reads (it opens {})",
                EXTENSIONS.iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(", ")
            ),
            ImportError::Recognised { format } => {
                write!(f, "{format} files are not read directly; export the score as MusicXML from {format} and open that file")
            }
            ImportError::TooLarge(what) => write!(f, "the file is too large: {what}"),
            ImportError::Parse { reason, .. } => write!(f, "{reason}"),
            ImportError::NoDrumPart => write!(f, "the file has no drum part"),
            ImportError::NoMeasures => write!(f, "the drum part has no measures"),
            ImportError::Invalid(why) => {
                write!(f, "the score read from the file is not consistent ({why}); this is a defect of the importer")
            }
        }
    }
}

impl std::error::Error for ImportError {}

pub fn load_score(path: &Path, opts: &ImportOptions) -> Result<Loaded, ImportError> {
    let too_large = |size: u64| ImportError::TooLarge(format!("{size} bytes, the limit is {}", opts.limits.max_bytes));
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > opts.limits.max_bytes as u64 {
            return Err(too_large(meta.len()));
        }
    }
    let bytes = std::fs::read(path).map_err(|e| ImportError::Read(e.to_string()))?;
    import_bytes(&bytes, &path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), opts)
}

/// Import a file already in memory; `name` is its file name (only the extension and the stem are used).
pub fn import_bytes(bytes: &[u8], name: &str, opts: &ImportOptions) -> Result<Loaded, ImportError> {
    if bytes.len() > opts.limits.max_bytes {
        return Err(ImportError::TooLarge(format!("{} bytes, the limit is {}", bytes.len(), opts.limits.max_bytes)));
    }
    let format = match sniff(bytes, name) {
        Detected::Known(format) => format,
        Detected::Recognised { format } => return Err(ImportError::Recognised { format }),
        Detected::Unknown => return Err(ImportError::Unrecognised),
    };
    let raw = match format {
        Format::MusicXml => musicxml::parse(&container::musicxml_text(bytes, &opts.limits)?, opts)?,
        Format::Midi => midi::parse(bytes, opts)?,
    };
    let report = Report { format, parts: raw.parts.clone(), part_used: raw.part_used, midi: raw.midi.clone() };
    let mut score = finish::finish(raw, opts)?;
    if score.title == "Untitled" {
        if let Some(stem) = Path::new(name).file_stem() {
            score.title = stem.to_string_lossy().into_owned();
        }
    }
    Ok(Loaded { score, report })
}
