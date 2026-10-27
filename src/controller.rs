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
//! layer = A note 84                     # this message selects layer A
//! bind  = A note 89 -> record           # a button, on layer A only
//! bind  = cc 16 rel -> tempo            # an endless encoder, on every layer
//! bind  = bend 1 -> tempo               # a fader (pitch bend, channel 1)
//! led   = A record -> note 89           # light of a button
//! ring  = A tempo -> cc 48              # LED ring around an encoder
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
    pub fn text(self) -> String {
        match self {
            Msg::NoteOn { ch, note, vel } => format!("ch {ch} · note {note} on, velocity {vel}"),
            Msg::NoteOff { ch, note } => format!("ch {ch} · note {note} off"),
            Msg::Cc { ch, cc, val } => format!("ch {ch} · cc {cc} = {val}"),
            Msg::Bend { ch, value } => format!("ch {ch} · fader {value}"),
        }
    }
}

/// The turn of an endless encoder as Mackie Control sends it: 1..=63 clockwise
/// steps, 65..=127 counter-clockwise steps (value minus 64).
pub fn decode_relative(val: u8) -> i32 {
    let n = (val & 0x3F) as i32;
    if val & 0x40 != 0 {
        -n
    } else {
        n
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
    /// An endless encoder (Mackie relative CC) rather than an absolute control.
    pub relative: bool,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub layer: Option<String>,
    pub action: Action,
    pub out: Out,
    /// A ring around an encoder showing a level, rather than a button light.
    pub ring: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preset {
    pub name: String,
    /// Part of the MIDI port's name.
    pub port: String,
    /// Layer names, in the order of their selecting messages.
    pub layers: Vec<(String, Src)>,
    pub binds: Vec<Binding>,
    pub lights: Vec<Light>,
    /// Lines that could not be used, with their line numbers.
    pub warnings: Vec<String>,
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
        p
    }

    fn line(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "name" => self.name = value.to_string(),
            "port" => self.port = value.to_string(),
            "layer" => {
                let mut w: Vec<&str> = value.split_whitespace().collect();
                if w.len() < 2 {
                    return Err("expected \"layer = NAME note N\"".into());
                }
                let name = w.remove(0).to_uppercase();
                self.layers.push((name, Src::parse(&w)?));
            }
            "bind" => {
                let (left, right) = value.split_once("->").ok_or("expected \"->\"")?;
                let action = Action::parse(right).ok_or_else(|| format!("unknown action \"{}\"", right.trim()))?;
                let mut w: Vec<&str> = left.split_whitespace().collect();
                let layer = split_layer(&mut w, &["note", "cc", "bend"]);
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
                self.binds.push(Binding { layer, src, relative, action });
            }
            "led" | "ring" => {
                let (left, right) = value.split_once("->").ok_or("expected \"->\"")?;
                let mut w: Vec<&str> = left.split_whitespace().collect();
                let layer = split_layer(&mut w, &SIMPLE.iter().map(|(n, _)| *n).chain(["mute", "solo", "gain", "audition"]).collect::<Vec<_>>());
                let action = Action::parse(&w.join(" ")).ok_or_else(|| format!("unknown action \"{}\"", w.join(" ")))?;
                let ring = key == "ring";
                if ring && !action.is_level() || !ring && !action.has_light() {
                    return Err(format!("\"{}\" cannot be shown by a {}", action.text(), if ring { "ring" } else { "light" }));
                }
                let out = match Src::parse(&right.split_whitespace().collect::<Vec<_>>())? {
                    Src::Note(n) => Out::Note(n),
                    Src::Cc(n) => Out::Cc(n),
                    Src::Bend(_) => return Err("lights are set with a note or a cc".into()),
                };
                self.lights.push(Light { layer, action, out, ring });
            }
            other => return Err(format!("unknown key \"{other}\"")),
        }
        Ok(())
    }

    /// The binding of `src` on `layer` (`None`: only the lines for every layer): the line for
    /// the layer wins over one for all layers.
    pub fn position(&self, layer: Option<&str>, src: Src) -> Option<usize> {
        layer
            .and_then(|l| self.binds.iter().position(|b| b.src == src && b.layer.as_deref() == Some(l)))
            .or_else(|| self.binds.iter().position(|b| b.src == src && b.layer.is_none()))
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
        let mut seen: BTreeMap<(Option<String>, Src), Action> = BTreeMap::new();
        let mut repeated = Vec::new();
        for b in &self.binds {
            if let Some(first) = seen.insert((b.layer.clone(), b.src), b.action) {
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

pub struct Mapper {
    preset: Preset,
    layer: usize,
    shift: bool,
    /// The control that holds Shift down.
    shift_src: Option<Src>,
    pickups: Vec<Pickup>,
    /// What was last sent to each light, so that only changes are sent.
    sent: BTreeMap<(Out, bool), u8>,
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

    /// The layer in use, if the preset has layers.
    pub fn layer_name(&self) -> Option<&str> {
        self.preset.layers.get(self.layer).map(|l| l.0.as_str())
    }

    pub fn shift(&self) -> bool {
        self.shift
    }

    fn find(&self, src: Src) -> Option<usize> {
        self.preset.position(self.layer_name(), src)
    }

    /// What the message is bound to in the current layer, for the monitor.
    pub fn bound_to(&self, msg: Msg) -> Option<String> {
        let src = Src::of(msg);
        if self.preset.layers.iter().any(|l| l.1 == src) {
            return Some("layer switch".into());
        }
        self.find(src).map(|i| self.preset.binds[i].action.text())
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
        let Some(i) = self.find(src) else { return Vec::new() };
        let (action, relative) = (self.preset.binds[i].action, self.preset.binds[i].relative);
        match action {
            Action::Shift => {
                if down == Some(true) {
                    self.shift = true;
                    self.shift_src = Some(src);
                }
                Vec::new()
            }
            a if a.is_level() || a.is_browse() => match msg {
                Msg::Cc { val, .. } if relative => match decode_relative(val) {
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
        let mut want: BTreeMap<(Out, bool), u8> = BTreeMap::new();
        // Lines for every layer first, so a line for this layer overrides them.
        for specific in [false, true] {
            for l in self.preset.lights.iter().filter(|l| l.layer.is_some() == specific) {
                if specific && l.layer != layer {
                    continue;
                }
                let value = if l.ring {
                    state.level(l.action).map(ring_value)
                } else if l.action == Action::Shift {
                    Some(if self.shift { 127 } else { 0 })
                } else {
                    state.lit(l.action).map(|on| if on { 127 } else { 0 })
                };
                if let Some(v) = value {
                    want.insert((l.out, l.ring), v);
                }
            }
        }
        let mut out = Vec::new();
        let message = |o: Out, v: u8| match o {
            Out::Note(n) => [0x90, n, v],
            Out::Cc(n) => [0xB0, n, v],
        };
        for (&key, &v) in &want {
            if self.sent.get(&key) != Some(&v) {
                out.push(message(key.0, v));
                self.sent.insert(key, v);
            }
        }
        let gone: Vec<(Out, bool)> = self.sent.keys().filter(|k| !want.contains_key(k)).copied().collect();
        for key in gone {
            out.push(message(key.0, 0));
            self.sent.remove(&key);
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
        assert_eq!([1, 3, 63].map(decode_relative), [1, 3, 63]);
        assert_eq!([65, 67, 127].map(decode_relative), [-1, -3, -63]);
        assert_eq!(decode_relative(0), 0);
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
