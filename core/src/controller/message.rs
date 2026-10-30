//! The messages a controller sends, and how an endless encoder counts.

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


/// How a control reports itself: by its position (a knob, a fader) or by how far it was turned
/// (an endless encoder, which counts in one of the `Encoding`s).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reading {
    #[default]
    Absolute,
    Relative(Encoding),
}

impl Reading {
    pub fn is_relative(self) -> bool {
        matches!(self, Reading::Relative(_))
    }

    /// How an endless encoder counts; Mackie's for a control that is not one (it is not used then).
    pub fn encoding(self) -> Encoding {
        match self {
            Reading::Relative(e) => e,
            Reading::Absolute => Encoding::Mackie,
        }
    }
}
