//! What a control can do (`Action`), where its message comes from (`Src`), and what the
//! application looks like to the lights (`State`).

use super::{group_label, Msg, PresetError, GROUPS};

/// Where a control's message comes from: the identity a binding is looked up by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Src {
    Note(u8),
    Cc(u8),
    /// Pitch bend on this channel (1-based).
    Bend(u8),
}

impl Src {
    pub(super) fn of(msg: Msg) -> Src {
        match msg {
            Msg::NoteOn { note, .. } | Msg::NoteOff { note, .. } => Src::Note(note),
            Msg::Cc { cc, .. } => Src::Cc(cc),
            Msg::Bend { ch, .. } => Src::Bend(ch),
        }
    }

    pub(super) fn parse(words: &[&str]) -> Result<Src, PresetError> {
        let [kind, n] = words else {
            return Err(PresetError::ExpectedMessage(words.join(" ")));
        };
        let n: u8 = n.parse().map_err(|_| PresetError::NotANumber(n.to_string()))?;
        match kind.to_ascii_lowercase().as_str() {
            "note" if n <= 127 => Ok(Src::Note(n)),
            "cc" if n <= 127 => Ok(Src::Cc(n)),
            "bend" if (1..=16).contains(&n) => Ok(Src::Bend(n)),
            "note" | "cc" | "bend" => Err(PresetError::OutOfRange(n)),
            other => Err(PresetError::UnknownMessageType(other.to_string())),
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

pub(super) const SIMPLE: &[(&str, Action)] = &[
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
    pub(super) fn lit(&self, a: Action) -> Option<bool> {
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
