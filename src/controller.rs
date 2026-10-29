//! Hardware controllers that speak the Mackie Control protocol (Behringer
//! X-Touch Mini and Compact, and similar): which control does what, and what
//! the lights show. Bytes in, commands and LED messages out; no MIDI port, no GUI.
//!
//! A *preset* is a small text file (see `presets/`) that lists the controller's
//! messages and the actions they trigger:
//!
//! ```text
//! name  = Behringer X-Touch Mini
//! port  = X-TOUCH MINI                  # part of the MIDI port's name
//! kind  = mackie                        # or midi: plain notes, cc and values; mackie when missing
//! layer = A note 84                     # this message selects layer A
//! bind  = A note 89 -> record           # a button, on layer A only
//! bind  = cc 16 rel -> tempo            # an endless encoder (Mackie relative values), on every layer
//! bind  = cc 14 ch 2 rel twos -> tempo  # only on MIDI channel 2; rel mackie | twos | offset | signbit2
//! bind  = cc 13 abs -> tempo            # a knob that sends its position
//! bind  = bend 1 -> tempo               # a fader (pitch bend, channel 1)
//! led   = A record -> note 89           # light of a button (on=127 off=0 by default)
//! led   = record -> note 36 ch 10 on=21 off=0   # another channel, other values (a pad's colour)
//! ring  = A tempo -> cc 48              # LED ring around an encoder (Mackie ring values)
//! ring  = tempo -> cc 20 scale=127      # a plain value 0..scale instead
//! ```
//!
//! A line without a layer name applies to all layers; a line for the current
//! layer wins over one for all layers. `#` starts a comment.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::drums::Lane;

/// Strips of the mixer: groups of instruments that are controlled together.
pub const GROUPS: usize = 8;

/// The instruments of group `i` (0-based). The first one is the group's representative.
/// Every instrument but `Lane::Other` is in exactly one group.
pub fn group_lanes(i: usize) -> &'static [Lane] {
    match i {
        0 => &[Lane::Kick],
        1 => &[Lane::Snare, Lane::CrossStick],
        2 => &[Lane::HiHatClosed, Lane::HiHatOpen, Lane::HiHatPedal],
        3 => &[Lane::Tom1],
        4 => &[Lane::Tom2],
        5 => &[Lane::Tom3, Lane::Tom4],
        6 => &[Lane::Crash1, Lane::Crash2, Lane::China, Lane::Splash],
        _ => &[Lane::Ride, Lane::RideBell, Lane::Cowbell],
    }
}

pub fn group_label(i: usize) -> &'static str {
    ["Kick", "Snare", "Hi-hat", "Tom 1", "Tom 2", "Tom 3/4", "Crash", "Ride"][i.min(GROUPS - 1)]
}

// ------------------------------------------------------------------ messages

/// A MIDI message from the controller. Channels are 1-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    NoteOn { ch: u8, note: u8, vel: u8 },
    NoteOff { ch: u8, note: u8 },
    Cc { ch: u8, cc: u8, val: u8 },
    /// 14-bit pitch bend (0..=16383): the faders of a Mackie Control.
    Bend { ch: u8, value: u16 },
}

pub fn parse(bytes: &[u8]) -> Option<Msg> {
    let (&status, rest) = bytes.split_first()?;
    let ch = (status & 0x0F) + 1;
    match (status & 0xF0, rest) {
        (0x90, &[note, vel, ..]) => Some(Msg::NoteOn { ch, note: note & 0x7F, vel: vel & 0x7F }),
        (0x80, &[note, ..]) => Some(Msg::NoteOff { ch, note: note & 0x7F }),
        (0xB0, &[cc, val, ..]) => Some(Msg::Cc { ch, cc: cc & 0x7F, val: val & 0x7F }),
        (0xE0, &[lsb, msb, ..]) => Some(Msg::Bend { ch, value: (lsb & 0x7F) as u16 | ((msb & 0x7F) as u16) << 7 }),
        _ => None,
    }
}

impl Msg {
    /// The MIDI channel (1-based).
    pub fn channel(self) -> u8 {
        match self {
            Msg::NoteOn { ch, .. } | Msg::NoteOff { ch, .. } | Msg::Cc { ch, .. } | Msg::Bend { ch, .. } => ch,
        }
    }

    pub fn text(self) -> String {
        match self {
            Msg::NoteOn { ch, note, vel } => format!("ch {ch} · note {note} on, velocity {vel}"),
            Msg::NoteOff { ch, note } => format!("ch {ch} · note {note} off"),
            Msg::Cc { ch, cc, val } => format!("ch {ch} · cc {cc} = {val}"),
            Msg::Bend { ch, value } => format!("ch {ch} · fader {value}"),
        }
    }
}

/// How an endless encoder says how far it was turned, in a cc value of 0..=127.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Encoding {
    /// Mackie Control ("signed bit"): 1..=63 clockwise steps, 65..=127 counter-clockwise (value minus 64).
    #[default]
    Mackie,
    /// Two's complement: 1..=63 clockwise, 127 down to 65 counter-clockwise (127 = one step back).
    Twos,
    /// Binary offset: 64 is no turn, above it clockwise, below it counter-clockwise.
    Offset,
    /// "Signed bit 2": the opposite sign bit to Mackie, 65..=127 clockwise, 1..=63 counter-clockwise.
    SignBit2,
}

impl Encoding {
    pub fn parse(word: &str) -> Option<Encoding> {
        match word.to_ascii_lowercase().as_str() {
            "mackie" => Some(Encoding::Mackie),
            "twos" => Some(Encoding::Twos),
            "offset" => Some(Encoding::Offset),
            "signbit2" => Some(Encoding::SignBit2),
            _ => None,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Encoding::Mackie => "mackie",
            Encoding::Twos => "twos",
            Encoding::Offset => "offset",
            Encoding::SignBit2 => "signbit2",
        }
    }

    /// The signed number of steps a cc value means.
    pub fn decode(self, val: u8) -> i32 {
        let val = val & 0x7F;
        match self {
            Encoding::Mackie => {
                let n = (val & 0x3F) as i32;
                if val & 0x40 != 0 {
                    -n
                } else {
                    n
                }
            }
            Encoding::Twos => {
                if val < 64 {
                    val as i32
                } else {
                    val as i32 - 128
                }
            }
            Encoding::Offset => val as i32 - 64,
            Encoding::SignBit2 => {
                let n = (val & 0x3F) as i32;
                if val & 0x40 != 0 {
                    n
                } else {
                    -n
                }
            }
        }
    }
}


/// Where a control's message comes from: the identity a binding is looked up by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Src {
    Note(u8),
    Cc(u8),
    /// Pitch bend on this channel (1-based).
    Bend(u8),
}

impl Src {
    fn of(msg: Msg) -> Src {
        match msg {
            Msg::NoteOn { note, .. } | Msg::NoteOff { note, .. } => Src::Note(note),
            Msg::Cc { cc, .. } => Src::Cc(cc),
            Msg::Bend { ch, .. } => Src::Bend(ch),
        }
    }

    fn parse(words: &[&str]) -> Result<Src, String> {
        let [kind, n] = words else {
            return Err(format!("expected \"note N\", \"cc N\" or \"bend CHANNEL\", found \"{}\"", words.join(" ")));
        };
        let n: u8 = n.parse().map_err(|_| format!("\"{n}\" is not a number"))?;
        match kind.to_ascii_lowercase().as_str() {
            "note" if n <= 127 => Ok(Src::Note(n)),
            "cc" if n <= 127 => Ok(Src::Cc(n)),
            "bend" if (1..=16).contains(&n) => Ok(Src::Bend(n)),
            "note" | "cc" | "bend" => Err(format!("{n} is out of range")),
            other => Err(format!("unknown message type \"{other}\"")),
        }
    }

    pub fn text(self) -> String {
        match self {
            Src::Note(n) => format!("note {n}"),
            Src::Cc(n) => format!("cc {n}"),
            Src::Bend(c) => format!("bend {c}"),
        }
    }
}

/// A message sent to the controller to light something.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Out {
    Note(u8),
    Cc(u8),
}

// ------------------------------------------------------------------- actions

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    // Buttons
    Record,
    Listen,
    Stop,
    Click,
    Loop,
    Notes,
    Metronome,
    NextTab,
    LoopWorstBar,
    RangeBack,
    RangeForward,
    TapTempo,
    TempoDown,
    TempoUp,
    DiscardTake,
    TempoReset,
    OpenScore,
    /// Rudiments: the previous and the next rudiment.
    PrevRudiment,
    NextRudiment,
    /// Held to make encoder turns five times as large.
    Shift,
    /// Strip number `0..GROUPS`.
    Mute(u8),
    Solo(u8),
    Audition(u8),
    // Values, turned by an encoder or set by a fader
    Tempo,
    RangeFrom,
    RangeTo,
    ClickVolume,
    MusicVolume,
    CountIn,
    BarsPerRow,
    Gain(u8),
    /// Rudiments: how often the rudiment repeats, and which hand starts it.
    Repeats,
    Lead,
    /// Browses the recent scores; only turned.
    Score,
    /// Browses the rudiments; only turned.
    Rudiment,
}

const SIMPLE: &[(&str, Action)] = &[
    ("record", Action::Record),
    ("listen", Action::Listen),
    ("stop", Action::Stop),
    ("click", Action::Click),
    ("loop", Action::Loop),
    ("notes", Action::Notes),
    ("metronome", Action::Metronome),
    ("next_tab", Action::NextTab),
    ("loop_worst_bar", Action::LoopWorstBar),
    ("range_back", Action::RangeBack),
    ("range_forward", Action::RangeForward),
    ("tap_tempo", Action::TapTempo),
    ("tempo_down", Action::TempoDown),
    ("tempo_up", Action::TempoUp),
    ("discard_take", Action::DiscardTake),
    ("tempo_reset", Action::TempoReset),
    ("open_score", Action::OpenScore),
    ("prev_rudiment", Action::PrevRudiment),
    ("next_rudiment", Action::NextRudiment),
    ("shift", Action::Shift),
    ("tempo", Action::Tempo),
    ("range_from", Action::RangeFrom),
    ("range_to", Action::RangeTo),
    ("click_volume", Action::ClickVolume),
    ("music_volume", Action::MusicVolume),
    ("count_in", Action::CountIn),
    ("bars_per_row", Action::BarsPerRow),
    ("repeats", Action::Repeats),
    ("lead", Action::Lead),
    ("score", Action::Score),
    ("rudiment", Action::Rudiment),
];

impl Action {
    /// "record", "tempo", "mute 3" (strips are numbered from 1 in the files).
    pub fn parse(text: &str) -> Option<Action> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let name = words.first()?.to_ascii_lowercase();
        match words.as_slice() {
            [_] => SIMPLE.iter().find(|(n, _)| *n == name).map(|(_, a)| *a),
            [_, n] => {
                let strip = n.parse::<u8>().ok().filter(|n| (1..=GROUPS as u8).contains(n))? - 1;
                match name.as_str() {
                    "mute" => Some(Action::Mute(strip)),
                    "solo" => Some(Action::Solo(strip)),
                    "audition" => Some(Action::Audition(strip)),
                    "gain" => Some(Action::Gain(strip)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    pub fn text(self) -> String {
        let numbered = |name: &str, i: u8| format!("{name} {}", i + 1);
        match self {
            Action::Mute(i) => numbered("mute", i),
            Action::Solo(i) => numbered("solo", i),
            Action::Audition(i) => numbered("audition", i),
            Action::Gain(i) => numbered("gain", i),
            other => SIMPLE.iter().find(|(_, a)| *a == other).map(|(n, _)| n.to_string()).unwrap_or_default(),
        }
    }

    /// Every action a preset can bind, in sections for the preset editor: (title, actions).
    pub fn sections() -> Vec<(&'static str, Vec<Action>)> {
        let strips = |make: fn(u8) -> Action| (0..GROUPS as u8).map(make).collect::<Vec<_>>();
        let mixer: Vec<Action> = [strips(Action::Gain), strips(Action::Mute), strips(Action::Solo), strips(Action::Audition)].concat();
        vec![
            (
                "Transport and practice",
                vec![
                    Action::Record,
                    Action::Listen,
                    Action::Stop,
                    Action::Click,
                    Action::Loop,
                    Action::Notes,
                    Action::Metronome,
                    Action::NextTab,
                    Action::LoopWorstBar,
                    Action::RangeBack,
                    Action::RangeForward,
                    Action::TapTempo,
                    Action::TempoDown,
                    Action::TempoUp,
                    Action::TempoReset,
                    Action::DiscardTake,
                    Action::OpenScore,
                    Action::PrevRudiment,
                    Action::NextRudiment,
                    Action::Shift,
                ],
            ),
            (
                "Values (knobs, encoders, faders)",
                vec![
                    Action::Tempo,
                    Action::RangeFrom,
                    Action::RangeTo,
                    Action::ClickVolume,
                    Action::MusicVolume,
                    Action::CountIn,
                    Action::BarsPerRow,
                    Action::Repeats,
                    Action::Lead,
                    Action::Score,
                    Action::Rudiment,
                ],
            ),
            ("Mixer: volume, mute, solo, play, per instrument group", mixer),
        ]
    }

    /// A few words for the control's label on the help screen.
    pub fn title(self) -> String {
        let group = |i: u8| group_label(i as usize);
        match self {
            Action::Record => "Record".into(),
            Action::Listen => "Listen".into(),
            Action::Stop => "Stop".into(),
            Action::Click => "Click".into(),
            Action::Loop => "Loop".into(),
            Action::Notes => "Notes".into(),
            Action::Metronome => "Metronome".into(),
            Action::NextTab => "Next tab".into(),
            Action::LoopWorstBar => "Worst bar".into(),
            Action::RangeBack => "Range back".into(),
            Action::RangeForward => "Range forward".into(),
            Action::TapTempo => "Tap tempo".into(),
            Action::TempoDown => "Tempo −".into(),
            Action::TempoUp => "Tempo +".into(),
            Action::DiscardTake => "Discard take".into(),
            Action::TempoReset => "Reset tempo".into(),
            Action::OpenScore => "Open score".into(),
            Action::PrevRudiment => "Prev rudiment".into(),
            Action::NextRudiment => "Next rudiment".into(),
            Action::Shift => "Shift".into(),
            Action::Mute(i) => format!("Mute {}", group(i)),
            Action::Solo(i) => format!("Solo {}", group(i)),
            Action::Audition(i) => format!("Play {}", group(i)),
            Action::Tempo => "Tempo".into(),
            Action::RangeFrom => "First bar".into(),
            Action::RangeTo => "Last bar".into(),
            Action::ClickVolume => "Click volume".into(),
            Action::MusicVolume => "Music volume".into(),
            Action::CountIn => "Count-in".into(),
            Action::BarsPerRow => "Bars per row".into(),
            Action::Gain(i) => format!("{} volume", group(i)),
            Action::Repeats => "Repeats".into(),
            Action::Lead => "Lead hand".into(),
            Action::Score => "Recent scores".into(),
            Action::Rudiment => "Rudiment".into(),
        }
    }

    /// What the control does, in a sentence or two, for the help screen.
    pub fn help(self) -> String {
        let group = |i: u8| group_label(i as usize);
        match self {
            Action::Record => "Starts a recording of the selected measures (count-in first); pressed again, it stops. The light is on while it records.".into(),
            Action::Listen => "Plays the selected measures through the speakers or the MIDI output; pressed again, it stops. The light is on while it plays.".into(),
            Action::Stop => "Stops a recording, Listen and the metronome.".into(),
            Action::Click => "Switches the click on or off while you listen or record. The count-in always sounds.".into(),
            Action::Loop => "Listen repeats the selected measures instead of stopping at the end.".into(),
            Action::Notes => "Switches the drum notes on or off in Listen, to hear the click alone.".into(),
            Action::Metronome => "Starts or stops the free metronome.".into(),
            Action::NextTab => "Goes to the next tab: Score, Rudiments, Metronome, Results, Settings.".into(),
            Action::LoopWorstBar => "Selects the measure the last take went worst in and plays it in a loop.".into(),
            Action::RangeBack => "Moves the selected measures back by their own length. On the Rudiments tab: the previous rudiment.".into(),
            Action::RangeForward => "Moves the selected measures forward by their own length. On the Rudiments tab: the next rudiment.".into(),
            Action::TapTempo => "Tap it in time: from the second tap on it sets the tempo (the metronome's tempo on the Metronome tab).".into(),
            Action::TempoDown => "Lowers the practice tempo by 5 (per cent, or BPM for rudiments and the metronome).".into(),
            Action::TempoUp => "Raises the practice tempo by 5 (per cent, or BPM for rudiments and the metronome).".into(),
            Action::DiscardTake => "Throws the last take away, so it is not in the results and the history.".into(),
            Action::TempoReset => "Back to the written tempo (100 %), the rudiment's starting tempo, or the metronome's default.".into(),
            Action::OpenScore => "Opens the score chosen with the recent-scores encoder.".into(),
            Action::PrevRudiment => "Chooses the previous rudiment.".into(),
            Action::NextRudiment => "Chooses the next rudiment.".into(),
            Action::Shift => "Hold it while turning an encoder: every step counts five times.".into(),
            Action::Mute(i) => format!("Mutes or unmutes {} in Listen. The light is on while it is muted.", group(i)),
            Action::Solo(i) => format!("Plays only the soloed instruments in Listen; {} is added or taken away. The light is on while it is solo.", group(i)),
            Action::Audition(i) => format!("Plays a hit of {}, to hear its volume.", group(i)),
            Action::Tempo => "The practice tempo: per cent of the written tempo, BPM for rudiments and on the Metronome tab. A fader sets it directly.".into(),
            Action::RangeFrom => "The first measure of the selected measures. On the Rudiments tab: how often the rudiment repeats.".into(),
            Action::RangeTo => "The last measure of the selected measures. On the Rudiments tab: which hand leads.".into(),
            Action::ClickVolume => "The volume of the click.".into(),
            Action::MusicVolume => "The volume of the drum notes in Listen.".into(),
            Action::CountIn => "The count-in: none, one bar or two.".into(),
            Action::BarsPerRow => "Bars per row of the score; none means as many as fit.".into(),
            Action::Gain(i) => format!("The volume of {} in Listen.", group(i)),
            Action::Repeats => "How often the rudiment repeats.".into(),
            Action::Lead => "Which hand leads the rudiment: right, left or alternating.".into(),
            Action::Score => "Browses the recent scores; push the encoder to open the one shown.".into(),
            Action::Rudiment => "Browses the rudiments.".into(),
        }
    }

    /// Set to a position by a fader, or turned by an encoder.
    pub fn is_level(self) -> bool {
        matches!(
            self,
            Action::Tempo
                | Action::RangeFrom
                | Action::RangeTo
                | Action::ClickVolume
                | Action::MusicVolume
                | Action::CountIn
                | Action::BarsPerRow
                | Action::Repeats
                | Action::Lead
                | Action::Gain(_)
        )
    }

    /// Browses a list: only turned, and not sped up by Shift.
    pub fn is_browse(self) -> bool {
        matches!(self, Action::Score | Action::Rudiment)
    }

    /// A button's light can show this.
    pub fn has_light(self) -> bool {
        matches!(
            self,
            Action::Record
                | Action::Listen
                | Action::Stop
                | Action::Click
                | Action::Loop
                | Action::Notes
                | Action::Metronome
                | Action::Shift
                | Action::Mute(_)
                | Action::Solo(_)
        )
    }
}

/// What the application looks like right now, for the controller's lights and
/// for catching up a fader. Levels run from 0 to 1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub record: bool,
    pub listen: bool,
    pub click: bool,
    pub looping: bool,
    pub notes: bool,
    pub metronome: bool,
    pub mute: [bool; GROUPS],
    pub solo: [bool; GROUPS],
    pub tempo: f32,
    pub range_from: f32,
    pub range_to: f32,
    pub click_volume: f32,
    pub music_volume: f32,
    pub count_in: f32,
    pub bars_per_row: f32,
    pub repeats: f32,
    pub lead: f32,
    pub gain: [f32; GROUPS],
}

impl State {
    fn lit(&self, a: Action) -> Option<bool> {
        Some(match a {
            Action::Record => self.record,
            Action::Listen => self.listen,
            Action::Stop => self.record || self.listen || self.metronome,
            Action::Click => self.click,
            Action::Loop => self.looping,
            Action::Notes => self.notes,
            Action::Metronome => self.metronome,
            Action::Mute(i) => self.mute[i as usize % GROUPS],
            Action::Solo(i) => self.solo[i as usize % GROUPS],
            _ => return None,
        })
    }

    pub fn level(&self, a: Action) -> Option<f32> {
        Some(match a {
            Action::Tempo => self.tempo,
            Action::RangeFrom => self.range_from,
            Action::RangeTo => self.range_to,
            Action::ClickVolume => self.click_volume,
            Action::MusicVolume => self.music_volume,
            Action::CountIn => self.count_in,
            Action::BarsPerRow => self.bars_per_row,
            Action::Repeats => self.repeats,
            Action::Lead => self.lead,
            Action::Gain(i) => self.gain[i as usize % GROUPS],
            _ => return None,
        })
    }
}

// ------------------------------------------------------------------- presets

#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    /// `None`: on every layer.
    pub layer: Option<String>,
    pub src: Src,
    /// Only messages on this MIDI channel (1-based); `None`: on any channel.
    pub channel: Option<u8>,
    /// An endless encoder (relative CC) rather than an absolute control.
    pub relative: bool,
    /// How a relative encoder counts (`Mackie` unless the line says otherwise).
    pub encoding: Encoding,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub layer: Option<String>,
    pub action: Action,
    pub out: Out,
    /// A ring around an encoder showing a level, rather than a button light.
    pub ring: bool,
    /// The MIDI channel the light is set on (1-based).
    pub channel: u8,
    /// Values sent to light and to darken a button (127 and 0 unless the line says otherwise).
    pub on: u8,
    pub off: u8,
    /// A ring shows its level as a plain value 0..=scale; `None`: Mackie ring values.
    pub scale: Option<u8>,
}

/// What a preset's controller speaks: Mackie Control (layers, ring and light values as the
/// Mackie table has them) or plain MIDI (notes, cc and pitch bend as the device sends them).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum PresetKind {
    #[default]
    Mackie,
    Midi,
}

impl PresetKind {
    pub fn parse(word: &str) -> Option<PresetKind> {
        match word.trim().to_ascii_lowercase().as_str() {
            "mackie" => Some(PresetKind::Mackie),
            "midi" => Some(PresetKind::Midi),
            _ => None,
        }
    }

    /// The word used in preset files.
    pub fn word(self) -> &'static str {
        match self {
            PresetKind::Mackie => "mackie",
            PresetKind::Midi => "midi",
        }
    }

    /// For the screen.
    pub fn label(self) -> &'static str {
        match self {
            PresetKind::Mackie => "Mackie Control",
            PresetKind::Midi => "Generic MIDI",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preset {
    pub name: String,
    /// Part of the MIDI port's name.
    pub port: String,
    /// Mackie Control or plain MIDI; a ring of a MIDI preset shows a plain value 0 to 127 unless `scale=` says otherwise.
    pub kind: PresetKind,
    /// Layer names, in the order of their selecting messages.
    pub layers: Vec<(String, Src)>,
    pub binds: Vec<Binding>,
    pub lights: Vec<Light>,
    /// Lines that could not be used, with their line numbers.
    pub warnings: Vec<String>,
}

/// Takes "ch N" (a MIDI channel from 1 to 16) out of the words of a line.
fn take_channel(words: &mut Vec<&str>) -> Result<Option<u8>, String> {
    let Some(i) = words.iter().position(|w| w.eq_ignore_ascii_case("ch")) else { return Ok(None) };
    let n: u8 = words.get(i + 1).and_then(|w| w.parse().ok()).filter(|n| (1..=16).contains(n)).ok_or("expected \"ch\" and a channel from 1 to 16")?;
    words.drain(i..=i + 1);
    Ok(Some(n))
}

fn split_layer<'a>(words: &mut Vec<&'a str>, first_is_not: &[&str]) -> Option<String> {
    let first = words.first()?;
    if first_is_not.iter().any(|k| first.eq_ignore_ascii_case(k)) {
        return None;
    }
    Some(words.remove(0).to_uppercase())
}

impl Preset {
    pub fn parse(text: &str) -> Preset {
        let mut p = Preset::default();
        let kind_given = text.lines().filter_map(|l| l.split('#').next()?.split_once('=')).any(|(key, _)| key.trim() == "kind");
        for (i, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let result = match line.split_once('=') {
                None => Err("expected \"key = value\"".to_string()),
                Some((k, v)) => p.line(k.trim(), v.trim()),
            };
            if let Err(e) = result {
                p.warnings.push(format!("line {}: {e}", i + 1));
            }
        }
        p.validate();
        if !kind_given {
            // A file from before presets said what they speak: layers and lights are Mackie Control's, plain notes and cc are MIDI.
            p.kind = if p.layers.is_empty() && p.lights.is_empty() { PresetKind::Midi } else { PresetKind::Mackie };
        }
        p
    }

    fn line(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "name" => self.name = value.to_string(),
            "port" => self.port = value.to_string(),
            "kind" => self.kind = PresetKind::parse(value).ok_or_else(|| format!("unknown kind \"{value}\": expected mackie or midi"))?,
            "layer" => self.parse_layer(value)?,
            "bind" => self.parse_bind(value)?,
            "led" => self.parse_light(false, value)?,
            "ring" => self.parse_light(true, value)?,
            other => return Err(format!("unknown key \"{other}\"")),
        }
        Ok(())
    }

    /// `layer = NAME note N`
    fn parse_layer(&mut self, value: &str) -> Result<(), String> {
        let mut w: Vec<&str> = value.split_whitespace().collect();
        if w.len() < 2 {
            return Err("expected \"layer = NAME note N\"".into());
        }
        let name = w.remove(0).to_uppercase();
        self.layers.push((name, Src::parse(&w)?));
        Ok(())
    }

    /// `bind = [LAYER] note|cc|bend N [ch C] [abs | rel [ENCODING]] -> action`
    fn parse_bind(&mut self, value: &str) -> Result<(), String> {
        let (left, right) = value.split_once("->").ok_or("expected \"->\"")?;
        let action = Action::parse(right).ok_or_else(|| format!("unknown action \"{}\"", right.trim()))?;
        let mut w: Vec<&str> = left.split_whitespace().collect();
        let layer = split_layer(&mut w, &["note", "cc", "bend"]);
        let channel = take_channel(&mut w)?;
        let mut encoding = Encoding::Mackie;
        if let Some(e) = w.last().and_then(|l| Encoding::parse(l)) {
            w.pop();
            if !w.last().is_some_and(|x| x.eq_ignore_ascii_case("rel")) {
                return Err(format!("\"{}\" needs \"rel\" before it", e.word()));
            }
            encoding = e;
        }
        let mut relative = false;
        if let Some(last) = w.last() {
            if last.eq_ignore_ascii_case("rel") {
                relative = true;
                w.pop();
            } else if last.eq_ignore_ascii_case("abs") {
                w.pop();
            }
        }
        let src = Src::parse(&w)?;
        if relative && !matches!(src, Src::Cc(_)) {
            return Err("only a cc can be relative".into());
        }
        if matches!(src, Src::Bend(_)) && !action.is_level() {
            return Err("a fader can only set a value".into());
        }
        if action.is_browse() && !relative {
            return Err(format!("\"{}\" is only turned: add \"rel\"", action.text()));
        }
        if let (Src::Bend(c), Some(_)) = (src, channel) {
            return Err(format!("a fader's channel is in \"bend {c}\", not in \"ch\""));
        }
        self.binds.push(Binding { layer, src, channel, relative, encoding, action });
        Ok(())
    }

    /// `led = [LAYER] action -> note|cc N [ch C] [on=V] [off=V]` and `ring = ... [scale=V]`
    fn parse_light(&mut self, ring: bool, value: &str) -> Result<(), String> {
        let (left, right) = value.split_once("->").ok_or("expected \"->\"")?;
        let mut w: Vec<&str> = left.split_whitespace().collect();
        let layer = split_layer(&mut w, &SIMPLE.iter().map(|(n, _)| *n).chain(["mute", "solo", "gain", "audition"]).collect::<Vec<_>>());
        let action = Action::parse(&w.join(" ")).ok_or_else(|| format!("unknown action \"{}\"", w.join(" ")))?;
        if ring && !action.is_level() || !ring && !action.has_light() {
            return Err(format!("\"{}\" cannot be shown by a {}", action.text(), if ring { "ring" } else { "light" }));
        }
        let mut r: Vec<&str> = right.split_whitespace().collect();
        let channel = take_channel(&mut r)?.unwrap_or(1);
        let (mut on, mut off, mut scale) = (127u8, 0u8, None);
        let mut words = Vec::new();
        for word in r {
            let Some((name, number)) = word.split_once('=') else {
                words.push(word);
                continue;
            };
            let n: u8 = number.parse().ok().filter(|n| *n <= 127).ok_or_else(|| format!("\"{word}\": expected a number from 0 to 127"))?;
            match (name.to_ascii_lowercase().as_str(), ring) {
                ("on", false) => on = n,
                ("off", false) => off = n,
                ("scale", true) => scale = Some(n),
                ("on" | "off", true) => return Err(format!("\"{name}=\" is for a light; a ring takes \"scale=\"")),
                ("scale", false) => return Err("\"scale=\" is for a ring; a light takes \"on=\" and \"off=\"".into()),
                _ => return Err(format!("unknown setting \"{word}\"")),
            }
        }
        let out = match Src::parse(&words)? {
            Src::Note(n) => Out::Note(n),
            Src::Cc(n) => Out::Cc(n),
            Src::Bend(_) => return Err("lights are set with a note or a cc".into()),
        };
        self.lights.push(Light { layer, action, out, ring, channel, on, off, scale });
        Ok(())
    }

    /// The binding of `src` on `layer` (`None`: only the lines for every layer) for a message on
    /// MIDI channel `channel` (`None`: any binding, whatever its channel, as a picture of the
    /// controller needs). The line for the layer wins over one for all layers, and a line for the
    /// channel over one for any channel.
    pub fn position(&self, layer: Option<&str>, src: Src, channel: Option<u8>) -> Option<usize> {
        self.binds
            .iter()
            .enumerate()
            .filter(|(_, b)| b.src == src && channel.map_or(true, |c| b.channel.map_or(true, |bc| bc == c)))
            .filter_map(|(i, b)| {
                let layer_rank = if layer.is_some() && b.layer.as_deref() == layer {
                    2
                } else if b.layer.is_none() {
                    1
                } else {
                    return None;
                };
                let exact_channel = b.channel.is_some() && b.channel == channel;
                // The best rank wins; among equals the first line of the file.
                Some(((layer_rank, exact_channel), std::cmp::Reverse(i)))
            })
            .max()
            .map(|(_, std::cmp::Reverse(i))| i)
    }

    /// The preset as a file: what `Preset::parse` reads back as the same preset (comments are lost).
    pub fn to_text(&self) -> String {
        let mut s = String::from("# Edited in Roll Model; the lines are explained in the README (\"Your own preset\").\n");
        s.push_str(&format!("name = {}\nport = {}\nkind = {}\n", self.name, self.port, self.kind.word()));
        for (name, src) in &self.layers {
            s.push_str(&format!("layer = {name} {}\n", src.text()));
        }
        for b in &self.binds {
            let mut left = String::new();
            if let Some(layer) = &b.layer {
                left.push_str(&format!("{layer} "));
            }
            left.push_str(&b.src.text());
            if let Some(ch) = b.channel {
                left.push_str(&format!(" ch {ch}"));
            }
            if matches!(b.src, Src::Cc(_)) {
                if !b.relative {
                    left.push_str(" abs");
                } else if b.encoding == Encoding::Mackie {
                    left.push_str(" rel");
                } else {
                    left.push_str(&format!(" rel {}", b.encoding.word()));
                }
            }
            s.push_str(&format!("bind = {left} -> {}\n", b.action.text()));
        }
        for l in &self.lights {
            let key = if l.ring { "ring" } else { "led" };
            let layer = l.layer.as_ref().map(|x| format!("{x} ")).unwrap_or_default();
            let out = match l.out {
                Out::Note(n) => format!("note {n}"),
                Out::Cc(n) => format!("cc {n}"),
            };
            let mut right = out;
            if l.channel != 1 {
                right.push_str(&format!(" ch {}", l.channel));
            }
            if let Some(scale) = l.scale {
                right.push_str(&format!(" scale={scale}"));
            }
            if !l.ring && (l.on, l.off) != (127, 0) {
                right.push_str(&format!(" on={} off={}", l.on, l.off));
            }
            s.push_str(&format!("{key} = {layer}{} -> {right}\n", l.action.text()));
        }
        s
    }

    /// The bindings of `action` on exactly `layer` (`None`: the lines for every layer).
    pub fn bindings_of(&self, layer: Option<&str>, action: Action) -> Vec<&Binding> {
        self.binds.iter().filter(|b| b.layer.as_deref() == layer && b.action == action).collect()
    }

    /// Binds `action` to the control `found` on `layer`. The control is taken from whatever it was
    /// bound to on that layer (returned), and the action's old control of the same kind (note, cc or
    /// fader) is replaced; a control of another kind, such as a fader next to an encoder, stays.
    pub fn assign(&mut self, layer: Option<&str>, action: Action, found: Learned) -> Option<Action> {
        let layer = layer.map(str::to_string);
        let kind = |src: Src| match src {
            Src::Note(_) => 0,
            Src::Cc(_) => 1,
            Src::Bend(_) => 2,
        };
        let mut taken_from = None;
        self.binds.retain(|b| {
            let same = b.layer == layer && b.src == found.src && b.channel == found.channel;
            if same && b.action != action {
                taken_from = Some(b.action);
            }
            !same
        });
        self.binds.retain(|b| !(b.layer == layer && b.action == action && kind(b.src) == kind(found.src)));
        self.binds.push(Binding { layer, src: found.src, channel: found.channel, relative: found.relative, encoding: found.encoding, action });
        taken_from
    }

    /// Takes every control of `action` on `layer` away.
    pub fn clear(&mut self, layer: Option<&str>, action: Action) {
        self.binds.retain(|b| !(b.layer.as_deref() == layer && b.action == action));
    }

    /// Changes how the cc controls of `action` on `layer` are read: as positions, or as an endless encoder.
    pub fn set_kind(&mut self, layer: Option<&str>, action: Action, relative: bool, encoding: Encoding) {
        for b in self.binds.iter_mut().filter(|b| b.layer.as_deref() == layer && b.action == action && matches!(b.src, Src::Cc(_))) {
            b.relative = relative;
            b.encoding = encoding;
        }
    }

    /// Drops lines that name a layer the preset does not declare, and reports repeated controls.
    fn validate(&mut self) {
        let known: Vec<String> = self.layers.iter().map(|l| l.0.clone()).collect();
        let ok = |l: &Option<String>| l.as_ref().map_or(true, |l| known.contains(l));
        let before = (self.binds.len(), self.lights.len());
        self.binds.retain(|b| ok(&b.layer));
        self.lights.retain(|l| ok(&l.layer));
        if before != (self.binds.len(), self.lights.len()) {
            self.warnings.push("some lines name a layer that no \"layer =\" line declares; they were ignored".into());
        }
        let mut seen: BTreeMap<(Option<String>, Src, Option<u8>), Action> = BTreeMap::new();
        let mut repeated = Vec::new();
        for b in &self.binds {
            if let Some(first) = seen.insert((b.layer.clone(), b.src, b.channel), b.action) {
                repeated.push(format!("{} is bound to both \"{}\" and \"{}\"", b.src.text(), first.text(), b.action.text()));
            }
        }
        for r in repeated {
            self.warnings.push(format!("{r}; the first one is used"));
        }
    }
}

/// The presets that come with the application: (name, file text).
pub const BUILTIN: &[(&str, &str)] = &[
    ("Behringer X-Touch Mini", include_str!("../presets/x-touch-mini.txt")),
    ("Behringer X-Touch Compact", include_str!("../presets/x-touch-compact.txt")),
    // Generic MIDI controllers (no Mackie Control); written from memory, not verified on the devices.
    ("Novation Launch Control XL", include_str!("../presets/novation-launch-control-xl.txt")),
    ("Korg nanoKONTROL2", include_str!("../presets/korg-nanokontrol2.txt")),
    ("Akai MPK Mini", include_str!("../presets/akai-mpk-mini.txt")),
    ("Arturia BeatStep", include_str!("../presets/arturia-beatstep.txt")),
];

pub fn builtin_text(name: &str) -> Option<&'static str> {
    BUILTIN.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

/// A preset the user wrote: a `.txt` file in the controllers folder, named after its file.
pub fn custom_presets(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<(String, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("txt")))
        .filter_map(|p| Some((p.file_stem()?.to_string_lossy().into_owned(), p)))
        .collect();
    found.sort();
    found
}

/// A built-in preset by name, or the custom file with that name in `dir`.
pub fn load_preset(name: &str, dir: Option<&Path>) -> Result<Preset, String> {
    if let Some(text) = builtin_text(name) {
        return Ok(Preset::parse(text));
    }
    let path = dir
        .and_then(|d| custom_presets(d).into_iter().find(|(n, _)| n == name))
        .map(|(_, p)| p)
        .ok_or_else(|| format!("controller preset \"{name}\" not found"))?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let mut p = Preset::parse(&text);
    if p.name.is_empty() {
        p.name = name.to_string();
    }
    Ok(p)
}

// -------------------------------------------------------------------- mapper

/// A control found by pressing or turning it, ready to be bound to an action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Learned {
    pub src: Src,
    pub channel: Option<u8>,
    pub relative: bool,
    pub encoding: Encoding,
}

/// What a message from the controller says about the control that sent it, when `action` is
/// waiting for one. A button needs a press (not the release), a value a cc or a fader, and the
/// lists (`Score`, `Rudiment`) an endless encoder. `relative` and `encoding` are the choice for
/// a cc that sets a value; `remember_channel` limits the binding to the channel the message came on.
/// `None`: not a control this action can use (a note off, a note for a knob's value, ...).
pub fn learn(msg: Msg, action: Action, relative: bool, encoding: Encoding, remember_channel: bool) -> Option<Learned> {
    let channel = |src: Src| (remember_channel && !matches!(src, Src::Bend(_))).then(|| msg.channel());
    let value_like = action.is_level() || action.is_browse();
    match msg {
        Msg::NoteOn { note, vel, .. } if vel > 0 && !value_like => {
            Some(Learned { src: Src::Note(note), channel: channel(Src::Note(note)), relative: false, encoding: Encoding::Mackie })
        }
        Msg::Cc { cc, .. } if value_like => {
            let relative = relative || action.is_browse();
            Some(Learned { src: Src::Cc(cc), channel: channel(Src::Cc(cc)), relative, encoding: if relative { encoding } else { Encoding::Mackie } })
        }
        Msg::Cc { cc, val, .. } if val > 0 => Some(Learned { src: Src::Cc(cc), channel: channel(Src::Cc(cc)), relative: false, encoding: Encoding::Mackie }),
        Msg::Bend { ch, .. } if action.is_level() => Some(Learned { src: Src::Bend(ch), channel: None, relative: false, encoding: Encoding::Mackie }),
        _ => None,
    }
}

/// What the application should do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Press(Action),
    /// Encoder turned by this many steps (negative = counter-clockwise).
    Turn(Action, i32),
    /// Fader moved to this position (0..=1).
    Set(Action, f32),
}

/// A fader that does not move by itself must be caught up with the value it
/// controls before it takes over, or the value would jump.
#[derive(Clone, Copy, Debug, Default)]
struct Pickup {
    engaged: bool,
    last: Option<f32>,
}

impl Pickup {
    /// How near the fader must be to take over, and how far the value may drift
    /// (changed with the mouse, say) before the fader has to catch up again.
    const CATCH: f32 = 0.03;
    const DRIFT: f32 = 0.25;

    fn take(&mut self, pos: f32, target: f32) -> bool {
        if self.engaged && (pos - target).abs() > Self::DRIFT {
            self.engaged = false;
        }
        if !self.engaged {
            let crossed = self.last.is_some_and(|l| (l - target) * (pos - target) <= 0.0);
            self.engaged = (pos - target).abs() <= Self::CATCH || crossed;
        }
        self.last = Some(pos);
        self.engaged
    }
}

/// What a light is told apart by: where it is set, whether it is a ring, and the MIDI channel.
type LightKey = (Out, bool, u8);

pub struct Mapper {
    preset: Preset,
    layer: usize,
    shift: bool,
    /// The control that holds Shift down.
    shift_src: Option<Src>,
    pickups: Vec<Pickup>,
    /// What was last sent to each light (the value and the value that darkens it), so that only changes are sent.
    sent: BTreeMap<LightKey, (u8, u8)>,
}

/// How much one click of an encoder moves a value while Shift is held, relative to normal.
const SHIFT_FACTOR: i32 = 5;

fn pressed(msg: Msg) -> Option<bool> {
    match msg {
        Msg::NoteOn { vel, .. } => Some(vel > 0),
        Msg::NoteOff { .. } => Some(false),
        Msg::Cc { val, .. } => Some(val > 0),
        Msg::Bend { .. } => None,
    }
}

/// The value that makes the LED ring of a Mackie Control show a level as a bar.
fn ring_value(level: f32) -> u8 {
    let position = if level <= 0.001 { 0 } else { (level.clamp(0.0, 1.0) * 10.0).round() as u8 + 1 };
    0x20 | position
}

impl Mapper {
    pub fn new(preset: Preset) -> Mapper {
        let pickups = vec![Pickup::default(); preset.binds.len()];
        Mapper { preset, layer: 0, shift: false, shift_src: None, pickups, sent: BTreeMap::new() }
    }

    pub fn preset(&self) -> &Preset {
        &self.preset
    }

    /// Puts an edited preset in place of the one in use. The layer and Shift stay; faders have to
    /// catch up again and every light is sent again.
    pub fn replace_preset(&mut self, preset: Preset) {
        self.layer = self.layer.min(preset.layers.len().saturating_sub(1));
        self.pickups = vec![Pickup::default(); preset.binds.len()];
        self.sent.clear();
        self.preset = preset;
    }

    /// The layer in use, if the preset has layers.
    pub fn layer_name(&self) -> Option<&str> {
        self.preset.layers.get(self.layer).map(|l| l.0.as_str())
    }

    pub fn shift(&self) -> bool {
        self.shift
    }

    fn find(&self, src: Src, channel: u8) -> Option<usize> {
        self.preset.position(self.layer_name(), src, Some(channel))
    }

    /// What the message is bound to in the current layer, for the monitor.
    pub fn bound_to(&self, msg: Msg) -> Option<String> {
        let src = Src::of(msg);
        if self.preset.layers.iter().any(|l| l.1 == src) {
            return Some("layer switch".into());
        }
        self.find(src, msg.channel()).map(|i| self.preset.binds[i].action.text())
    }

    pub fn handle(&mut self, msg: Msg, state: &State) -> Vec<Command> {
        let src = Src::of(msg);
        let down = pressed(msg);
        if let Some(i) = self.preset.layers.iter().position(|l| l.1 == src) {
            if down == Some(true) && i != self.layer {
                self.layer = i;
                self.pickups.iter_mut().for_each(|p| *p = Pickup::default());
            }
            return Vec::new();
        }
        // Shift ends when its own button is let go, also after the layer changed under it.
        if down == Some(false) && self.shift_src == Some(src) {
            self.shift = false;
            self.shift_src = None;
            return Vec::new();
        }
        let Some(i) = self.find(src, msg.channel()) else { return Vec::new() };
        let (action, relative, encoding) = (self.preset.binds[i].action, self.preset.binds[i].relative, self.preset.binds[i].encoding);
        match action {
            Action::Shift => {
                if down == Some(true) {
                    self.shift = true;
                    self.shift_src = Some(src);
                }
                Vec::new()
            }
            a if a.is_level() || a.is_browse() => match msg {
                Msg::Cc { val, .. } if relative => match encoding.decode(val) {
                    0 => Vec::new(),
                    n => vec![Command::Turn(a, if self.shift && !a.is_browse() { n * SHIFT_FACTOR } else { n })],
                },
                Msg::Cc { val, .. } if a.is_level() => self.absolute(i, a, val as f32 / 127.0, state),
                Msg::Bend { value, .. } if a.is_level() => self.absolute(i, a, value as f32 / 16383.0, state),
                _ => Vec::new(),
            },
            a if down == Some(true) => vec![Command::Press(a)],
            _ => Vec::new(),
        }
    }

    fn absolute(&mut self, bind: usize, action: Action, pos: f32, state: &State) -> Vec<Command> {
        let Some(target) = state.level(action) else { return Vec::new() };
        if self.pickups[bind].take(pos, target) {
            vec![Command::Set(action, pos)]
        } else {
            Vec::new()
        }
    }

    /// The messages that bring the controller's lights up to date with `state`:
    /// only what changed since the last call, and a light of another layer is put out.
    pub fn lights(&mut self, state: &State) -> Vec<[u8; 3]> {
        let layer = self.layer_name().map(str::to_string);
        let mut want: BTreeMap<LightKey, (u8, u8)> = BTreeMap::new();
        // Lines for every layer first, so a line for this layer overrides them.
        for specific in [false, true] {
            for l in self.preset.lights.iter().filter(|l| l.layer.is_some() == specific) {
                if specific && l.layer != layer {
                    continue;
                }
                let scale = l.scale.or((self.preset.kind == PresetKind::Midi).then_some(127));
                let value = if l.ring {
                    state.level(l.action).map(|x| match scale {
                        Some(scale) => (x.clamp(0.0, 1.0) * scale as f32).round() as u8,
                        None => ring_value(x),
                    })
                } else if l.action == Action::Shift {
                    Some(if self.shift { l.on } else { l.off })
                } else {
                    state.lit(l.action).map(|on| if on { l.on } else { l.off })
                };
                if let Some(v) = value {
                    want.insert((l.out, l.ring, l.channel), (v, if l.ring { 0 } else { l.off }));
                }
            }
        }
        let mut out = Vec::new();
        let message = |(o, _, ch): LightKey, v: u8| {
            let ch = ch.clamp(1, 16) - 1;
            match o {
                Out::Note(n) => [0x90 | ch, n, v],
                Out::Cc(n) => [0xB0 | ch, n, v],
            }
        };
        for (&key, &(v, off)) in &want {
            if self.sent.get(&key).map(|s| s.0) != Some(v) {
                out.push(message(key, v));
                self.sent.insert(key, (v, off));
            }
        }
        let gone: Vec<LightKey> = self.sent.keys().filter(|k| !want.contains_key(k)).copied().collect();
        for key in gone {
            let off = self.sent.remove(&key).map_or(0, |s| s.1);
            out.push(message(key, off));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(n: u8) -> Msg {
        Msg::NoteOn { ch: 1, note: n, vel: 127 }
    }
    fn release(n: u8) -> Msg {
        Msg::NoteOn { ch: 1, note: n, vel: 0 }
    }
    fn cc(n: u8, v: u8) -> Msg {
        Msg::Cc { ch: 1, cc: n, val: v }
    }

    fn mini() -> Mapper {
        Mapper::new(load_preset("Behringer X-Touch Mini", None).unwrap())
    }

    #[test]
    fn messages_are_parsed() {
        assert_eq!(parse(&[0x90, 89, 127]), Some(Msg::NoteOn { ch: 1, note: 89, vel: 127 }));
        assert_eq!(parse(&[0x89, 5, 0]), Some(Msg::NoteOff { ch: 10, note: 5 }));
        assert_eq!(parse(&[0xB0, 16, 65]), Some(Msg::Cc { ch: 1, cc: 16, val: 65 }));
        assert_eq!(parse(&[0xE0, 0x7F, 0x7F]), Some(Msg::Bend { ch: 1, value: 16383 }));
        assert_eq!(parse(&[0xE2, 0, 0x40]), Some(Msg::Bend { ch: 3, value: 8192 }));
        assert_eq!(parse(&[0xF8]), None, "clock");
        assert_eq!(parse(&[0x90, 3]), None, "truncated");
        assert_eq!(parse(&[]), None);
    }

    #[test]
    fn encoder_turns_are_signed() {
        assert_eq!([1, 3, 63].map(|v| Encoding::Mackie.decode(v)), [1, 3, 63]);
        assert_eq!([65, 67, 127].map(|v| Encoding::Mackie.decode(v)), [-1, -3, -63]);
        assert_eq!(Encoding::Mackie.decode(0), 0);
    }

    #[test]
    fn the_rudiment_actions_can_be_bound_in_a_custom_preset() {
        let p = Preset::parse(
            "layer = A note 84\n\
             bind = A cc 16 rel -> repeats\n\
             bind = A cc 17 rel -> lead\n\
             bind = A cc 18 rel -> rudiment\n\
             bind = A note 70 -> next_rudiment\n\
             bind = A note 71 -> prev_rudiment\n\
             bind = A bend 1 -> repeats\n\
             ring = A repeats -> cc 48\n\
             ring = A lead -> cc 49\n",
        );
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        for text in ["repeats", "lead", "rudiment", "next_rudiment", "prev_rudiment"] {
            assert_eq!(Action::parse(text).unwrap().text(), text);
        }
        // Browsing the rudiments is only turned, like browsing the scores.
        let bad = Preset::parse("layer = A note 84\nbind = A cc 18 -> rudiment\nbind = A bend 1 -> rudiment\n");
        assert_eq!(bad.warnings.len(), 2, "{:?}", bad.warnings);
        assert!(Preset::parse("layer = A note 84\nring = A rudiment -> cc 48\n").warnings.len() == 1, "no ring for a list");

        let mut m = Mapper::new(p);
        let s = State { repeats: 0.25, lead: 0.5, ..State::default() };
        assert!(m.handle(note(84), &s).is_empty(), "the layer switch");
        assert_eq!(m.handle(cc(16, 2), &s), [Command::Turn(Action::Repeats, 2)]);
        assert_eq!(m.handle(cc(18, 65), &s), [Command::Turn(Action::Rudiment, -1)]);
        assert_eq!(m.handle(note(70), &s), [Command::Press(Action::NextRudiment)]);
        assert_eq!(s.level(Action::Repeats), Some(0.25));
        assert_eq!(s.level(Action::Lead), Some(0.5));
        // A fader catches up with the repetitions before it takes over.
        assert!(m.handle(Msg::Bend { ch: 1, value: 0 }, &s).is_empty());
        assert_eq!(m.lights(&s), [[0xB0, 48, ring_value(0.25)], [0xB0, 49, ring_value(0.5)]], "the rings show the values");
    }

    #[test]
    fn built_in_presets_have_no_problems() {
        for (name, text) in BUILTIN {
            let p = Preset::parse(text);
            assert_eq!(&p.name, name);
            assert!(p.warnings.is_empty(), "{name}: {:?}", p.warnings);
            assert!(!p.binds.is_empty() && !p.port.is_empty());
        }
    }

    #[test]
    fn the_mini_has_two_layers_and_every_button_does_something() {
        let p = Preset::parse(builtin_text("Behringer X-Touch Mini").unwrap());
        assert_eq!(p.layers.iter().map(|l| l.0.as_str()).collect::<Vec<_>>(), ["A", "B"]);
        for layer in ["A", "B"] {
            let on_layer = |src: Src| p.binds.iter().any(|b| b.layer.as_deref() == Some(layer) && b.src == src);
            let buttons = [89, 90, 40, 41, 42, 43, 44, 45, 87, 88, 91, 92, 86, 93, 94, 95];
            assert!(buttons.iter().all(|n| on_layer(Src::Note(*n))), "layer {layer}");
            assert!((16..24).all(|n| on_layer(Src::Cc(n))), "eight encoders on layer {layer}");
            let pushes = if layer == "A" { vec![32, 39] } else { (32..40).collect() };
            assert!(pushes.iter().all(|n| on_layer(Src::Note(*n))), "encoder pushes on layer {layer}");
            assert!(on_layer(Src::Bend(1)), "the fader on layer {layer}");
        }
    }

    #[test]
    fn a_layer_message_switches_what_the_controls_do() {
        let mut m = mini();
        let s = State::default();
        assert_eq!(m.layer_name(), Some("A"));
        assert_eq!(m.handle(note(95), &s), [Command::Press(Action::Record)]);
        assert!(m.handle(note(85), &s).is_empty(), "the switch itself does nothing else");
        assert_eq!(m.layer_name(), Some("B"));
        assert_eq!(m.handle(note(89), &s), [Command::Press(Action::Mute(0))]);
        assert_eq!(m.handle(note(87), &s), [Command::Press(Action::Solo(0))]);
        assert_eq!(m.handle(cc(18, 65), &s), [Command::Turn(Action::Gain(2), -1)]);
        m.handle(note(84), &s);
        assert_eq!(m.handle(cc(18, 2), &s), [Command::Turn(Action::RangeTo, 2)]);
    }

    #[test]
    fn what_is_printed_on_the_minis_transport_buttons_is_what_they_do_on_layer_a() {
        let mut m = mini();
        let s = State::default();
        // Rewind, fast forward, LOOP, stop, play and record are printed beside buttons 3 to 8 of the bottom row.
        for (note_number, action) in [
            (91, Action::RangeBack),
            (92, Action::RangeForward),
            (86, Action::Loop),
            (93, Action::Stop),
            (94, Action::Listen),
            (95, Action::Record),
        ] {
            assert_eq!(m.handle(note(note_number), &s), [Command::Press(action)], "note {note_number}");
        }
    }

    #[test]
    fn a_button_acts_when_pressed_not_when_released() {
        let mut m = mini();
        let s = State::default();
        assert_eq!(m.handle(note(94), &s), [Command::Press(Action::Listen)]);
        assert!(m.handle(release(94), &s).is_empty());
        assert!(m.handle(Msg::NoteOff { ch: 1, note: 94 }, &s).is_empty());
        assert!(m.handle(note(1), &s).is_empty(), "an unbound button");
    }

    #[test]
    fn shift_makes_encoder_turns_larger_while_held() {
        let mut m = mini();
        let s = State::default();
        assert_eq!(m.handle(cc(16, 1), &s), [Command::Turn(Action::Tempo, 1)]);
        assert!(m.handle(note(89), &s).is_empty());
        assert!(m.shift());
        assert_eq!(m.handle(cc(16, 1), &s), [Command::Turn(Action::Tempo, 5)]);
        assert_eq!(m.handle(cc(16, 65), &s), [Command::Turn(Action::Tempo, -5)]);
        assert_eq!(m.handle(cc(23, 1), &s), [Command::Turn(Action::Score, 1)], "browsing the scores is not sped up");
        m.handle(release(89), &s);
        assert!(!m.shift());
        assert_eq!(m.handle(cc(16, 1), &s), [Command::Turn(Action::Tempo, 1)]);
        // Let go on the other layer (where the same button is something else): Shift still ends.
        m.handle(note(89), &s);
        m.handle(note(85), &s);
        assert!(m.shift());
        assert!(m.handle(release(89), &s).is_empty());
        assert!(!m.shift());
    }

    #[test]
    fn the_compact_has_no_layers_and_its_select_row_holds_the_function_buttons() {
        let mut m = Mapper::new(load_preset("Behringer X-Touch Compact", None).unwrap());
        let s = State::default();
        assert_eq!(m.layer_name(), None);
        assert_eq!(m.handle(note(25), &s), [Command::Press(Action::Click)]);
        assert_eq!(m.handle(note(29), &s), [Command::Press(Action::LoopWorstBar)]);
        assert_eq!(m.handle(note(94), &s), [Command::Press(Action::Listen)]);
        assert!(m.handle(cc(16, 1), &s) == [Command::Turn(Action::Tempo, 1)]);
        // The first SELECT button is Shift, held while turning.
        m.handle(note(24), &s);
        assert!(m.shift());
        assert_eq!(m.handle(cc(16, 1), &s), [Command::Turn(Action::Tempo, 5)]);
        m.handle(release(24), &s);
        assert!(!m.shift());
        // What the Compact does not have is no longer bound.
        for n in [84, 85, 87, 88, 89, 54, 47, 70] {
            assert!(m.handle(note(n), &s).is_empty(), "note {n}");
        }
        assert!(m.handle(cc(60, 1), &s).is_empty(), "there is no jog wheel");
    }

    #[test]
    fn the_generic_midi_presets_use_absolute_knobs_and_plain_buttons() {
        let s = State::default();
        // (preset, a message, what it must do)
        let cases: [(&str, Msg, Option<Command>); 8] = [
            ("Korg nanoKONTROL2", cc(41, 127), Some(Command::Press(Action::Listen))),
            ("Korg nanoKONTROL2", cc(41, 0), None),
            ("Korg nanoKONTROL2", cc(32, 127), Some(Command::Press(Action::Solo(0)))),
            ("Novation Launch Control XL", note(73), Some(Command::Press(Action::Mute(0)))),
            ("Novation Launch Control XL", cc(108, 127), Some(Command::Press(Action::Record))),
            ("Akai MPK Mini", note(36), Some(Command::Press(Action::Record))),
            ("Akai MPK Mini", cc(70, 64), None), // a fader-like control far from the value: waits
            ("Arturia BeatStep", note(52), Some(Command::Press(Action::Mute(0)))),
        ];
        for (name, msg, expected) in cases {
            let mut m = Mapper::new(load_preset(name, None).unwrap());
            let got = m.handle(msg, &s);
            assert_eq!(got.first().copied(), expected, "{name}: {msg:?}");
        }
        // An absolute knob moves a value once it has reached it (here the tempo is 0.5).
        let mut m = Mapper::new(load_preset("Korg nanoKONTROL2", None).unwrap());
        let s = State { tempo: 0.5, ..Default::default() };
        let got = m.handle(cc(16, 64), &s);
        assert!(matches!(got[..], [Command::Set(Action::Tempo, x)] if (x - 0.5).abs() < 0.01), "{got:?}");
        // Controls the Mackie presets rely on are not bound by the generic ones.
        for name in ["Korg nanoKONTROL2", "Novation Launch Control XL", "Akai MPK Mini", "Arturia BeatStep"] {
            let p = load_preset(name, None).unwrap();
            assert!(p.layers.is_empty() && p.lights.is_empty(), "{name}: no layers and no lights");
            assert!(p.warnings.is_empty(), "{name}: {:?}", p.warnings);
        }
    }

    #[test]
    fn every_relative_encoding_counts_its_own_way() {
        // (encoding, values for one step up, three steps up, one step down, three steps down, no turn)
        let table = [
            (Encoding::Mackie, [1, 3, 65, 67, 0]),
            (Encoding::Twos, [1, 3, 127, 125, 0]),
            (Encoding::Offset, [65, 67, 63, 61, 64]),
            (Encoding::SignBit2, [65, 67, 1, 3, 0]),
        ];
        for (e, [up1, up3, down1, down3, none]) in table {
            assert_eq!([up1, up3, down1, down3, none].map(|v| e.decode(v)), [1, 3, -1, -3, 0], "{e:?}");
        }
        assert_eq!(Encoding::Offset.decode(64 + 63), 63);
        assert_eq!(Encoding::Twos.decode(64), -64, "the far end of two's complement");
        for e in [Encoding::Mackie, Encoding::Twos, Encoding::Offset, Encoding::SignBit2] {
            assert_eq!(Encoding::parse(e.word()), Some(e), "the word of {e:?} parses back");
        }
        assert_eq!(Encoding::parse("TWOS"), Some(Encoding::Twos));
        assert_eq!(Encoding::parse("binary"), None);
    }

    #[test]
    fn an_encoder_line_can_name_its_encoding_and_a_knob_moves_the_value_it_is_bound_to() {
        let p = Preset::parse("bind = cc 1 rel twos -> tempo\nbind = cc 2 rel offset -> range_from\nbind = cc 3 rel -> range_to\nbind = cc 4 rel signbit2 -> click_volume\n");
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        let mut m = Mapper::new(p);
        let s = State::default();
        assert_eq!(m.handle(cc(1, 127), &s), [Command::Turn(Action::Tempo, -1)], "two's complement");
        assert_eq!(m.handle(cc(2, 66), &s), [Command::Turn(Action::RangeFrom, 2)], "offset");
        assert!(m.handle(cc(2, 64), &s).is_empty(), "offset: 64 is no turn");
        assert_eq!(m.handle(cc(3, 65), &s), [Command::Turn(Action::RangeTo, -1)], "Mackie unless said otherwise");
        assert_eq!(m.handle(cc(4, 65), &s), [Command::Turn(Action::ClickVolume, 1)], "signed bit 2");
        // Shift still makes the steps larger, whatever the encoding.
        let p = Preset::parse("bind = note 9 -> shift\nbind = cc 1 rel twos -> tempo\n");
        let mut m = Mapper::new(p);
        m.handle(note(9), &s);
        assert_eq!(m.handle(cc(1, 127), &s), [Command::Turn(Action::Tempo, -5)]);
    }

    #[test]
    fn a_line_can_be_for_one_midi_channel_and_a_channel_line_wins() {
        let p = Preset::parse("bind = note 36 ch 10 -> record\nbind = note 36 -> stop\nbind = cc 7 ch 2 abs -> tempo\n");
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        assert_eq!(p.binds[0].channel, Some(10));
        assert_eq!(p.binds[1].channel, None);
        let mut m = Mapper::new(p);
        let s = State { tempo: 0.5, ..Default::default() };
        let on = |ch: u8, n: u8| Msg::NoteOn { ch, note: n, vel: 100 };
        assert_eq!(m.handle(on(10, 36), &s), [Command::Press(Action::Record)], "the line for channel 10");
        assert_eq!(m.handle(on(1, 36), &s), [Command::Press(Action::Stop)], "any other channel");
        let cc_on = |ch: u8, val: u8| Msg::Cc { ch, cc: 7, val };
        assert!(m.handle(cc_on(1, 64), &s).is_empty(), "cc 7 on channel 1 is not bound");
        assert_eq!(m.handle(cc_on(2, 64), &s).len(), 1, "cc 7 on channel 2 is");
        assert_eq!(m.bound_to(on(10, 36)).as_deref(), Some("record"));
        assert_eq!(m.bound_to(on(3, 36)).as_deref(), Some("stop"));
        assert_eq!(m.bound_to(cc_on(1, 1)), None);
        // The same note on two channels is two controls, not a repeated line.
        let two = Preset::parse("bind = note 36 ch 1 -> record\nbind = note 36 ch 2 -> stop\n");
        assert!(two.warnings.is_empty(), "{:?}", two.warnings);
        let again = Preset::parse("bind = note 36 ch 1 -> record\nbind = note 36 ch 1 -> stop\n");
        assert_eq!(again.warnings.len(), 1, "{:?}", again.warnings);
    }

    #[test]
    fn bad_channel_encoding_and_light_words_are_reported() {
        for line in [
            "bind = note 1 ch 0 -> record",
            "bind = note 1 ch 17 -> record",
            "bind = note 1 ch -> record",
            "bind = cc 1 twos -> tempo",
            "bind = cc 1 abs twos -> tempo",
            "bind = note 1 rel twos -> record",
            "bind = bend 1 ch 2 -> tempo",
            "led = record -> note 1 on=200",
            "led = record -> note 1 scale=10",
            "ring = tempo -> cc 1 on=10",
            "led = record -> note 1 colour=3",
            "led = record -> note 1 ch 20",
        ] {
            let p = Preset::parse(line);
            assert_eq!(p.warnings.len(), 1, "{line}: {:?}", p.warnings);
            assert!(p.binds.is_empty() && p.lights.is_empty(), "{line}: the line is not used");
        }
    }

    #[test]
    fn a_light_can_have_other_values_another_channel_and_a_ring_a_plain_scale() {
        let p = Preset::parse("led = record -> note 36 ch 10 on=21 off=3\nring = tempo -> cc 20 scale=100\nled = listen -> note 5\n");
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        let mut m = Mapper::new(p);
        let mut s = State { record: true, tempo: 0.5, ..Default::default() };
        let first = m.lights(&s);
        assert!(first.contains(&[0x99, 36, 21]), "record is on, channel 10, value 21: {first:?}");
        assert!(first.contains(&[0xB0, 20, 50]), "the ring shows 0.5 of 100: {first:?}");
        assert!(first.contains(&[0x90, 5, 0]), "an ordinary light: channel 1, off = 0: {first:?}");
        s.record = false;
        assert_eq!(m.lights(&s), [[0x99, 36, 3]], "off is the line's own off value");
        // A light that is no longer wanted gets its own off value, not 0.
        let p = Preset::parse("layer = A note 1\nlayer = B note 2\nled = A record -> note 36 ch 10 on=21 off=3\n");
        let mut m = Mapper::new(p);
        let s = State { record: true, ..Default::default() };
        assert_eq!(m.lights(&s), [[0x99, 36, 21]]);
        m.handle(note(2), &s);
        assert_eq!(m.lights(&s), [[0x99, 36, 3]], "put out with the preset's off value");
    }

    #[test]
    fn a_preset_written_as_text_reads_back_the_same() {
        for (name, text) in BUILTIN {
            let original = Preset::parse(text);
            let again = Preset::parse(&original.to_text());
            assert!(again.warnings.is_empty(), "{name}: {:?}", again.warnings);
            assert_eq!(original, again, "{name}");
        }
        let odd = Preset::parse(
            "name = X\nport = Y\nlayer = A note 1\nlayer = B note 2\nbind = A cc 5 ch 3 rel twos -> tempo\nbind = cc 6 abs -> click_volume\nbind = B bend 2 -> music_volume\n\
             led = A record -> note 9 ch 10 on=21 off=3\nring = B tempo -> cc 4 scale=100\nled = shift -> cc 8\n",
        );
        assert!(odd.warnings.is_empty(), "{:?}", odd.warnings);
        assert_eq!(Preset::parse(&odd.to_text()), odd);
    }

    #[test]
    fn every_action_is_in_the_editor_once() {
        let all: Vec<Action> = Action::sections().into_iter().flat_map(|(_, a)| a).collect();
        let mut texts: Vec<String> = all.iter().map(|a| a.text()).collect();
        let n = texts.len();
        texts.sort();
        texts.dedup();
        assert_eq!(texts.len(), n, "no action twice");
        for (name, _) in SIMPLE {
            assert!(all.iter().any(|a| a.text() == *name), "{name} is missing");
        }
        for i in 0..GROUPS as u8 {
            for a in [Action::Gain(i), Action::Mute(i), Action::Solo(i), Action::Audition(i)] {
                assert!(all.contains(&a), "{}", a.text());
            }
        }
        for a in &all {
            assert_eq!(Action::parse(&a.text()), Some(*a), "{}", a.text());
        }
    }

    #[test]
    fn a_pressed_control_is_learned_the_way_its_action_needs() {
        let on = |ch: u8, n: u8, vel: u8| Msg::NoteOn { ch, note: n, vel };
        let (m, t) = (Encoding::Mackie, Encoding::Twos);
        // A button wants a press: a note on, or a cc that is not zero (a button that sends cc).
        assert_eq!(learn(on(1, 36, 100), Action::Record, false, m, false).map(|l| l.src), Some(Src::Note(36)));
        assert_eq!(learn(on(1, 36, 0), Action::Record, false, m, false), None, "the release");
        assert_eq!(learn(Msg::NoteOff { ch: 1, note: 36 }, Action::Record, false, m, false), None);
        assert_eq!(learn(cc(7, 127), Action::Record, false, m, false).map(|l| l.src), Some(Src::Cc(7)));
        assert_eq!(learn(cc(7, 0), Action::Record, false, m, false), None);
        assert_eq!(learn(Msg::Bend { ch: 1, value: 5 }, Action::Record, false, m, false), None, "a fader is no button");
        // A value takes a cc (any value) or a fader, but not a note.
        assert_eq!(learn(on(1, 36, 100), Action::Tempo, false, m, false), None);
        let knob = learn(cc(13, 0), Action::Tempo, false, m, false).unwrap();
        assert_eq!((knob.src, knob.relative), (Src::Cc(13), false));
        let endless = learn(cc(14, 65), Action::Tempo, true, t, false).unwrap();
        assert_eq!((endless.relative, endless.encoding), (true, t));
        assert_eq!(learn(Msg::Bend { ch: 3, value: 5 }, Action::Tempo, false, m, true).map(|l| (l.src, l.channel)), Some((Src::Bend(3), None)));
        // The lists are only turned, so they are always relative.
        assert!(learn(cc(15, 1), Action::Score, false, m, false).unwrap().relative);
        assert_eq!(learn(Msg::Bend { ch: 1, value: 5 }, Action::Score, false, m, false), None);
        // The channel is remembered only when asked for.
        assert_eq!(learn(on(10, 36, 100), Action::Record, false, m, true).and_then(|l| l.channel), Some(10));
        assert_eq!(learn(on(10, 36, 100), Action::Record, false, m, false).and_then(|l| l.channel), None);
    }

    #[test]
    fn assigning_replaces_the_old_control_of_the_same_kind_and_takes_a_used_one() {
        let mut p = Preset::parse("bind = note 1 -> record\nbind = note 2 -> listen\nbind = cc 5 abs -> tempo\nbind = bend 1 -> tempo\n");
        let found = |src| Learned { src, channel: None, relative: false, encoding: Encoding::Mackie };
        // Record moves to note 3: the old note goes.
        assert_eq!(p.assign(None, Action::Record, found(Src::Note(3))), None);
        assert_eq!(p.bindings_of(None, Action::Record).len(), 1);
        assert_eq!(p.bindings_of(None, Action::Record)[0].src, Src::Note(3));
        // Stop takes note 2 from Listen.
        assert_eq!(p.assign(None, Action::Stop, found(Src::Note(2))), Some(Action::Listen));
        assert!(p.bindings_of(None, Action::Listen).is_empty());
        // Tempo gets a new knob; the fader stays.
        p.assign(None, Action::Tempo, found(Src::Cc(6)));
        let tempo: Vec<Src> = p.bindings_of(None, Action::Tempo).iter().map(|b| b.src).collect();
        assert_eq!(tempo, [Src::Bend(1), Src::Cc(6)]);
        // Another layer's lines are not touched, and clearing is per layer.
        let mut q = Preset::parse("layer = A note 8\nlayer = B note 9\nbind = A note 1 -> record\nbind = B note 1 -> mute 1\n");
        assert_eq!(q.assign(Some("A"), Action::Stop, found(Src::Note(1))), Some(Action::Record));
        assert_eq!(q.bindings_of(Some("B"), Action::Mute(0)).len(), 1);
        q.clear(Some("B"), Action::Mute(0));
        assert!(q.bindings_of(Some("B"), Action::Mute(0)).is_empty() && q.bindings_of(Some("A"), Action::Stop).len() == 1);
        // The kind of a cc can be changed afterwards.
        p.set_kind(None, Action::Tempo, true, Encoding::Offset);
        let cc_bind = p.binds.iter().find(|b| b.src == Src::Cc(6)).unwrap();
        assert_eq!((cc_bind.relative, cc_bind.encoding), (true, Encoding::Offset));
        assert!(!p.binds.iter().find(|b| b.src == Src::Bend(1)).unwrap().relative, "a fader is not touched");
    }

    #[test]
    fn an_edited_preset_replaces_the_one_in_use_and_keeps_the_layer() {
        let p = Preset::parse("layer = A note 8\nlayer = B note 9\nbind = A note 1 -> record\nbind = B note 1 -> stop\n");
        let mut m = Mapper::new(p.clone());
        let s = State::default();
        m.handle(note(9), &s);
        assert_eq!(m.layer_name(), Some("B"));
        let mut edited = p;
        edited.assign(Some("B"), Action::Listen, Learned { src: Src::Note(1), channel: None, relative: false, encoding: Encoding::Mackie });
        m.replace_preset(edited);
        assert_eq!(m.layer_name(), Some("B"), "still on layer B");
        assert_eq!(m.handle(note(1), &s), [Command::Press(Action::Listen)]);
    }

    #[test]
    fn a_preset_says_whether_it_speaks_mackie_or_plain_midi() {
        // Without a kind line the file is read as Mackie if it has layers or lights, as MIDI otherwise.
        assert_eq!(Preset::parse("layer = A note 1\nbind = A note 2 -> record\n").kind, PresetKind::Mackie);
        assert_eq!(Preset::parse("led = record -> note 1\n").kind, PresetKind::Mackie);
        assert_eq!(Preset::parse("bind = note 2 -> record\n").kind, PresetKind::Midi);
        assert_eq!(Preset::parse("kind = mackie\nbind = note 2 -> record\n").kind, PresetKind::Mackie, "a kind line wins");
        assert_eq!(Preset::parse("kind = midi\n").kind, PresetKind::Midi);
        assert_eq!(Preset::parse("kind = MIDI\n").kind, PresetKind::Midi);
        let bad = Preset::parse("kind = osc\n");
        assert_eq!((bad.kind, bad.warnings.len()), (PresetKind::Mackie, 1), "{:?}", bad.warnings);
        for (name, text) in BUILTIN {
            let kind = Preset::parse(text).kind;
            let generic = ["Novation Launch Control XL", "Korg nanoKONTROL2", "Akai MPK Mini", "Arturia BeatStep"].contains(name);
            assert_eq!(kind, if generic { PresetKind::Midi } else { PresetKind::Mackie }, "{name}");
        }
        // The kind survives a trip through a file.
        let p = Preset::parse("kind = midi\nbind = note 1 -> record\n");
        assert_eq!(Preset::parse(&p.to_text()).kind, PresetKind::Midi);
        // A ring of a MIDI preset shows a plain value, unless it says how far.
        let midi = |extra: &str| {
            let mut m = Mapper::new(Preset::parse(&format!("kind = midi\nring = tempo -> cc 20{extra}\n")));
            m.lights(&State { tempo: 0.5, ..Default::default() })
        };
        assert_eq!(midi(""), [[0xB0, 20, 64]]);
        assert_eq!(midi(" scale=100"), [[0xB0, 20, 50]]);
        let mut mackie = Mapper::new(Preset::parse("ring = tempo -> cc 20\n"));
        assert_eq!(mackie.lights(&State { tempo: 0.5, ..Default::default() }), [[0xB0, 20, 0x20 | 6]], "Mackie ring values");
    }

    #[test]
    fn a_fader_waits_until_it_reaches_the_value() {
        let mut m = mini();
        let s = State { tempo: 0.5, ..Default::default() };
        let bend = |v: f32| Msg::Bend { ch: 1, value: (v * 16383.0) as u16 };
        assert!(m.handle(bend(0.0), &s).is_empty(), "far below: ignored, no jump");
        assert!(m.handle(bend(0.3), &s).is_empty());
        assert_eq!(m.handle(bend(0.52), &s).len(), 1, "close enough: takes over");
        let followed = State { tempo: 0.52, ..Default::default() };
        let moved = m.handle(bend(0.6), &followed);
        assert!(matches!(moved[..], [Command::Set(Action::Tempo, x)] if (x - 0.6).abs() < 0.01), "{moved:?}");
        // The value changed elsewhere: the fader has to catch up again.
        let elsewhere = State { tempo: 0.1, ..Default::default() };
        assert!(m.handle(bend(0.62), &elsewhere).is_empty());
        assert_eq!(m.handle(bend(0.12), &elsewhere).len(), 1, "back near the value");
        // Crossing the value in one big move also takes over.
        let mut m = mini();
        assert!(m.handle(bend(0.1), &s).is_empty());
        assert_eq!(m.handle(bend(0.9), &s).len(), 1, "jumped across the value");
        // A new layer starts waiting again.
        m.handle(note(85), &s);
        assert!(m.handle(bend(0.0), &State { music_volume: 0.8, ..Default::default() }).is_empty());
    }

    #[test]
    fn lights_follow_the_state_and_only_changes_are_sent() {
        let mut m = mini();
        let mut s = State { record: true, tempo: 1.0, ..Default::default() };
        let first = m.lights(&s);
        assert!(first.contains(&[0x90, 95, 127]), "record lit");
        assert!(first.contains(&[0x90, 94, 0]), "listen dark");
        assert!(first.contains(&[0xB0, 48, 0x20 | 11]), "tempo ring full");
        assert!(m.lights(&s).is_empty(), "nothing changed");
        s.record = false;
        s.listen = true;
        let changed = m.lights(&s);
        assert_eq!(changed.len(), 2, "record and listen changed, stop is lit in both cases: {changed:?}");
        assert!(changed.contains(&[0x90, 95, 0]) && changed.contains(&[0x90, 94, 127]));
        // Layer B: the mute and solo lights replace the transport lights.
        s.mute[0] = true;
        m.handle(note(85), &s);
        let b = m.lights(&s);
        assert!(b.contains(&[0x90, 89, 127]), "mute 1 is lit on the button that was Shift: {b:?}");
        assert!(b.contains(&[0x90, 93, 0]), "Stop's button now belongs to solo 6: {b:?}");
        assert!(b.contains(&[0xB0, 48, 0x20]), "the first ring now shows gain 1, which is 0 here: {b:?}");
    }

    #[test]
    fn a_light_of_another_layer_is_put_out() {
        let p = Preset::parse("layer = A note 1\nlayer = B note 2\nled = A record -> note 10\nled = B click -> note 11\n");
        let mut m = Mapper::new(p);
        let s = State { record: true, click: true, ..Default::default() };
        assert_eq!(m.lights(&s), [[0x90, 10, 127]]);
        m.handle(note(2), &s);
        let mut b = m.lights(&s);
        b.sort();
        assert_eq!(b, [[0x90, 10, 0], [0x90, 11, 127]]);
    }

    #[test]
    fn a_line_for_the_layer_overrides_one_for_all_layers() {
        let p = Preset::parse("layer = A note 1\nlayer = B note 2\nbind = note 9 -> record\nbind = B note 9 -> listen\n");
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        let mut m = Mapper::new(p);
        let s = State::default();
        assert_eq!(m.handle(note(9), &s), [Command::Press(Action::Record)]);
        m.handle(note(2), &s);
        assert_eq!(m.handle(note(9), &s), [Command::Press(Action::Listen)]);
        assert_eq!(m.bound_to(note(9)).as_deref(), Some("listen"));
        assert_eq!(m.bound_to(note(2)).as_deref(), Some("layer switch"));
    }

    #[test]
    fn bad_lines_are_reported_and_the_rest_is_used() {
        let p = Preset::parse(
            "name = Test # a comment\n\
             port = TEST\n\
             bind = note 1 -> record\n\
             bind = note 2 -> fly\n\
             bind = note 300 -> stop\n\
             bind = bend 1 -> record\n\
             bind = note 3 rel -> tempo\n\
             bind = C note 4 -> stop\n\
             bind = note 1 -> listen\n\
             led = record -> note 5\n\
             led = tempo -> note 6\n\
             ring = record -> cc 7\n\
             nonsense\n\
             colour = red\n",
        );
        assert_eq!(p.name, "Test");
        assert_eq!(p.binds.len(), 2, "record on note 1, and listen on note 1 (a repeat)");
        assert_eq!(p.lights.len(), 1);
        for fragment in ["line 4", "line 5", "line 6", "line 7", "line 11", "line 12", "line 13", "line 14", "layer", "both"] {
            assert!(p.warnings.iter().any(|w| w.contains(fragment)), "no warning with \"{fragment}\": {:?}", p.warnings);
        }
    }

    #[test]
    fn actions_round_trip_through_their_names() {
        for (_, a) in SIMPLE {
            assert_eq!(Action::parse(&a.text()), Some(*a));
        }
        for i in 0..GROUPS as u8 {
            for a in [Action::Mute(i), Action::Solo(i), Action::Audition(i), Action::Gain(i)] {
                assert_eq!(Action::parse(&a.text()), Some(a));
            }
        }
        assert_eq!(Action::parse("MUTE 1"), Some(Action::Mute(0)));
        assert_eq!(Action::parse("mute 9"), None);
        assert_eq!(Action::parse("mute 0"), None);
        assert_eq!(Action::parse("mute"), None);
        assert_eq!(Action::parse("record 1"), None);
    }

    #[test]
    fn the_groups_cover_every_instrument_once() {
        let mut seen = Vec::new();
        for i in 0..GROUPS {
            seen.extend_from_slice(group_lanes(i));
        }
        for lane in Lane::ALL {
            let n = seen.iter().filter(|l| **l == lane).count();
            assert_eq!(n, if lane == Lane::Other { 0 } else { 1 }, "{lane:?}");
        }
    }

    #[test]
    fn custom_presets_are_read_from_a_folder() {
        let dir = std::env::temp_dir().join(format!("roll-model-presets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("My pads.txt"), "port = PADS\nbind = note 36 -> record\n").unwrap();
        std::fs::write(dir.join("readme.md"), "not a preset").unwrap();
        assert_eq!(custom_presets(&dir).iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["My pads"]);
        let p = load_preset("My pads", Some(&dir)).unwrap();
        assert_eq!((p.name.as_str(), p.port.as_str(), p.binds.len()), ("My pads", "PADS", 1));
        assert!(load_preset("Nope", Some(&dir)).is_err());
        assert!(load_preset("Nope", None).is_err());
        assert!(custom_presets(&dir.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
