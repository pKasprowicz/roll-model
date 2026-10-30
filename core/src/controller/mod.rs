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

mod action;
mod error;
mod mapper;
mod message;
mod preset;
#[cfg(test)]
mod tests;

pub use action::{Action, Out, Src, State};
pub use error::{LayerError, LoadPresetError, PresetError};
pub use mapper::{learn, Command, Mapper};
pub use message::{parse, Encoding, Msg, Reading};
pub use preset::{
    builtin_text, custom_presets, load_preset, preset_source, Binding, Input, Light, Preset, PresetKind, PresetSource, BUILTIN,
};

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
