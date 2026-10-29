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

/// How an action is used: pressed, set to a level (a fader, or a turned encoder), or turned through a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Class {
    Button,
    Level,
    Browse,
}

/// Everything known about an action that is not a strip's: the one place to add a new one.
/// The order is the order in the preset editor.
pub(super) struct Info {
    pub(super) action: Action,
    /// The word in a preset file.
    pub(super) name: &'static str,
    /// The label on the help screen.
    pub(super) title: &'static str,
    /// The explanation on the help screen.
    pub(super) help: &'static str,
    pub(super) class: Class,
    /// A button's light can show this.
    pub(super) light: bool,
}

pub(super) const ACTIONS: &[Info] = &[
    Info { action: Action::Record, name: "record", title: "Record", help: "Starts a recording of the selected measures (count-in first); pressed again, it stops. The light is on while it records.", class: Class::Button, light: true },
    Info { action: Action::Listen, name: "listen", title: "Listen", help: "Plays the selected measures through the speakers or the MIDI output; pressed again, it stops. The light is on while it plays.", class: Class::Button, light: true },
    Info { action: Action::Stop, name: "stop", title: "Stop", help: "Stops a recording, Listen and the metronome.", class: Class::Button, light: true },
    Info { action: Action::Click, name: "click", title: "Click", help: "Switches the click on or off while you listen or record. The count-in always sounds.", class: Class::Button, light: true },
    Info { action: Action::Loop, name: "loop", title: "Loop", help: "Listen repeats the selected measures instead of stopping at the end.", class: Class::Button, light: true },
    Info { action: Action::Notes, name: "notes", title: "Notes", help: "Switches the drum notes on or off in Listen, to hear the click alone.", class: Class::Button, light: true },
    Info { action: Action::Metronome, name: "metronome", title: "Metronome", help: "Starts or stops the free metronome.", class: Class::Button, light: true },
    Info { action: Action::NextTab, name: "next_tab", title: "Next tab", help: "Goes to the next tab: Score, Rudiments, Metronome, Results, Settings.", class: Class::Button, light: false },
    Info { action: Action::LoopWorstBar, name: "loop_worst_bar", title: "Worst bar", help: "Selects the measure the last take went worst in and plays it in a loop.", class: Class::Button, light: false },
    Info { action: Action::RangeBack, name: "range_back", title: "Range back", help: "Moves the selected measures back by their own length. On the Rudiments tab: the previous rudiment.", class: Class::Button, light: false },
    Info { action: Action::RangeForward, name: "range_forward", title: "Range forward", help: "Moves the selected measures forward by their own length. On the Rudiments tab: the next rudiment.", class: Class::Button, light: false },
    Info { action: Action::TapTempo, name: "tap_tempo", title: "Tap tempo", help: "Tap it in time: from the second tap on it sets the tempo (the metronome's tempo on the Metronome tab).", class: Class::Button, light: false },
    Info { action: Action::TempoDown, name: "tempo_down", title: "Tempo −", help: "Lowers the practice tempo by 5 (per cent, or BPM for rudiments and the metronome).", class: Class::Button, light: false },
    Info { action: Action::TempoUp, name: "tempo_up", title: "Tempo +", help: "Raises the practice tempo by 5 (per cent, or BPM for rudiments and the metronome).", class: Class::Button, light: false },
    Info { action: Action::TempoReset, name: "tempo_reset", title: "Reset tempo", help: "Back to the written tempo (100 %), the rudiment's starting tempo, or the metronome's default.", class: Class::Button, light: false },
    Info { action: Action::DiscardTake, name: "discard_take", title: "Discard take", help: "Throws the last take away, so it is not in the results and the history.", class: Class::Button, light: false },
    Info { action: Action::OpenScore, name: "open_score", title: "Open score", help: "Opens the score chosen with the recent-scores encoder.", class: Class::Button, light: false },
    Info { action: Action::PrevRudiment, name: "prev_rudiment", title: "Prev rudiment", help: "Chooses the previous rudiment.", class: Class::Button, light: false },
    Info { action: Action::NextRudiment, name: "next_rudiment", title: "Next rudiment", help: "Chooses the next rudiment.", class: Class::Button, light: false },
    Info { action: Action::Shift, name: "shift", title: "Shift", help: "Hold it while turning an encoder: every step counts five times.", class: Class::Button, light: true },
    Info { action: Action::Tempo, name: "tempo", title: "Tempo", help: "The practice tempo: per cent of the written tempo, BPM for rudiments and on the Metronome tab. A fader sets it directly.", class: Class::Level, light: false },
    Info { action: Action::RangeFrom, name: "range_from", title: "First bar", help: "The first measure of the selected measures. On the Rudiments tab: how often the rudiment repeats.", class: Class::Level, light: false },
    Info { action: Action::RangeTo, name: "range_to", title: "Last bar", help: "The last measure of the selected measures. On the Rudiments tab: which hand leads.", class: Class::Level, light: false },
    Info { action: Action::ClickVolume, name: "click_volume", title: "Click volume", help: "The volume of the click.", class: Class::Level, light: false },
    Info { action: Action::MusicVolume, name: "music_volume", title: "Music volume", help: "The volume of the drum notes in Listen.", class: Class::Level, light: false },
    Info { action: Action::CountIn, name: "count_in", title: "Count-in", help: "The count-in: none, one bar or two.", class: Class::Level, light: false },
    Info { action: Action::BarsPerRow, name: "bars_per_row", title: "Bars per row", help: "Bars per row of the score; none means as many as fit.", class: Class::Level, light: false },
    Info { action: Action::Repeats, name: "repeats", title: "Repeats", help: "How often the rudiment repeats.", class: Class::Level, light: false },
    Info { action: Action::Lead, name: "lead", title: "Lead hand", help: "Which hand leads the rudiment: right, left or alternating.", class: Class::Level, light: false },
    Info { action: Action::Score, name: "score", title: "Recent scores", help: "Browses the recent scores; push the encoder to open the one shown.", class: Class::Browse, light: false },
    Info { action: Action::Rudiment, name: "rudiment", title: "Rudiment", help: "Browses the rudiments.", class: Class::Browse, light: false },
];

impl Action {
    /// "record", "tempo", "mute 3" (strips are numbered from 1 in the files).
    pub fn parse(text: &str) -> Option<Action> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let name = words.first()?.to_ascii_lowercase();
        match words.as_slice() {
            [_] => ACTIONS.iter().find(|i| i.name == name).map(|i| i.action),
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
            other => other.info().map(|i| i.name.to_string()).unwrap_or_default(),
        }
    }

    /// Every action a preset can bind, in sections for the preset editor: (title, actions).
    pub fn sections() -> Vec<(&'static str, Vec<Action>)> {
        let of = |wanted: fn(Class) -> bool| ACTIONS.iter().filter(|i| wanted(i.class)).map(|i| i.action).collect::<Vec<_>>();
        let strips = |make: fn(u8) -> Action| (0..GROUPS as u8).map(make).collect::<Vec<_>>();
        let mixer: Vec<Action> = [strips(Action::Gain), strips(Action::Mute), strips(Action::Solo), strips(Action::Audition)].concat();
        vec![
            ("Transport and practice", of(|c| c == Class::Button)),
            ("Values (knobs, encoders, faders)", of(|c| c != Class::Button)),
            ("Mixer: volume, mute, solo, play, per instrument group", mixer),
        ]
    }

    /// The table row of an action; none for a strip's.
    fn info(self) -> Option<&'static Info> {
        ACTIONS.iter().find(|i| i.action == self)
    }

    /// A few words for the control's label on the help screen.
    pub fn title(self) -> String {
        let group = |i: u8| group_label(i as usize);
        match self {
            Action::Mute(i) => format!("Mute {}", group(i)),
            Action::Solo(i) => format!("Solo {}", group(i)),
            Action::Audition(i) => format!("Play {}", group(i)),
            Action::Gain(i) => format!("{} volume", group(i)),
            other => other.info().map(|i| i.title.to_string()).unwrap_or_default(),
        }
    }

    /// What the control does, in a sentence or two, for the help screen.
    pub fn help(self) -> String {
        let group = |i: u8| group_label(i as usize);
        match self {
            Action::Mute(i) => format!("Mutes or unmutes {} in Listen. The light is on while it is muted.", group(i)),
            Action::Solo(i) => format!("Plays only the soloed instruments in Listen; {} is added or taken away. The light is on while it is solo.", group(i)),
            Action::Audition(i) => format!("Plays a hit of {}, to hear its volume.", group(i)),
            Action::Gain(i) => format!("The volume of {} in Listen.", group(i)),
            other => other.info().map(|i| i.help.to_string()).unwrap_or_default(),
        }
    }

    /// Set to a position by a fader, or turned by an encoder.
    pub fn is_level(self) -> bool {
        matches!(self, Action::Gain(_)) || self.info().is_some_and(|i| i.class == Class::Level)
    }

    /// Browses a list: only turned, and not sped up by Shift.
    pub fn is_browse(self) -> bool {
        self.info().is_some_and(|i| i.class == Class::Browse)
    }

    /// A button's light can show this.
    pub fn has_light(self) -> bool {
        matches!(self, Action::Mute(_) | Action::Solo(_)) || self.info().is_some_and(|i| i.light)
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
