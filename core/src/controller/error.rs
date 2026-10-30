//! Why a preset cannot be used.

use std::path::PathBuf;

/// Why a line of a preset file cannot be used. `Display` is the text the Controller page shows
/// (with the line number in front).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresetError {
    ExpectedKeyValue,
    UnknownKey(String),
    UnknownKind(String),
    ExpectedLayer,
    ExpectedArrow,
    UnknownAction(String),
    /// A message that is not "note N", "cc N" or "bend CHANNEL"; the words found.
    ExpectedMessage(String),
    NotANumber(String),
    OutOfRange(u8),
    UnknownMessageType(String),
    BadChannel,
    EncodingNeedsRel(&'static str),
    OnlyCcRelative,
    FaderOnlyValue,
    OnlyTurned(String),
    FaderChannel(u8),
    CannotBeShown { action: String, ring: bool },
    BadLightNumber(String),
    RingTakesScale(String),
    LightTakesOnOff,
    UnknownSetting(String),
    LightNeedsNoteOrCc,
}

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PresetError::ExpectedKeyValue => write!(f, "expected \"key = value\""),
            PresetError::UnknownKey(key) => write!(f, "unknown key \"{key}\""),
            PresetError::UnknownKind(value) => write!(f, "unknown kind \"{value}\": expected mackie or midi"),
            PresetError::ExpectedLayer => write!(f, "expected \"layer = NAME note N\""),
            PresetError::ExpectedArrow => write!(f, "expected \"->\""),
            PresetError::UnknownAction(action) => write!(f, "unknown action \"{action}\""),
            PresetError::ExpectedMessage(found) => write!(f, "expected \"note N\", \"cc N\" or \"bend CHANNEL\", found \"{found}\""),
            PresetError::NotANumber(word) => write!(f, "\"{word}\" is not a number"),
            PresetError::OutOfRange(n) => write!(f, "{n} is out of range"),
            PresetError::UnknownMessageType(kind) => write!(f, "unknown message type \"{kind}\""),
            PresetError::BadChannel => write!(f, "expected \"ch\" and a channel from 1 to 16"),
            PresetError::EncodingNeedsRel(word) => write!(f, "\"{word}\" needs \"rel\" before it"),
            PresetError::OnlyCcRelative => write!(f, "only a cc can be relative"),
            PresetError::FaderOnlyValue => write!(f, "a fader can only set a value"),
            PresetError::OnlyTurned(action) => write!(f, "\"{action}\" is only turned: add \"rel\""),
            PresetError::FaderChannel(c) => write!(f, "a fader's channel is in \"bend {c}\", not in \"ch\""),
            PresetError::CannotBeShown { action, ring } => write!(f, "\"{action}\" cannot be shown by a {}", if *ring { "ring" } else { "light" }),
            PresetError::BadLightNumber(word) => write!(f, "\"{word}\": expected a number from 0 to 127"),
            PresetError::RingTakesScale(name) => write!(f, "\"{name}=\" is for a light; a ring takes \"scale=\""),
            PresetError::LightTakesOnOff => write!(f, "\"scale=\" is for a ring; a light takes \"on=\" and \"off=\""),
            PresetError::UnknownSetting(word) => write!(f, "unknown setting \"{word}\""),
            PresetError::LightNeedsNoteOrCc => write!(f, "lights are set with a note or a cc"),
        }
    }
}

impl std::error::Error for PresetError {}

/// Why a layer cannot be added to a preset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayerError {
    /// No name, or one with spaces or a `#` in it.
    BadName,
    /// A word of the file format (`note`, `rel`, the name of an action, ...) cannot be a layer's name.
    Reserved(String),
    Taken(String),
    /// The control that would select the layer already does something; the text says what.
    ControlInUse(String),
}

impl std::fmt::Display for LayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayerError::BadName => write!(f, "A layer needs a name of one word, without spaces or #."),
            LayerError::Reserved(word) => write!(f, "\"{word}\" is a word of the preset format; choose another name."),
            LayerError::Taken(name) => write!(f, "There is a layer called \"{name}\" already."),
            LayerError::ControlInUse(what) => write!(f, "That control already selects or does something ({what}); clear it first, or press another."),
        }
    }
}

impl std::error::Error for LayerError {}

/// Why a preset cannot be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadPresetError {
    NotFound(String),
    Unreadable { path: PathBuf, reason: String },
}

impl std::fmt::Display for LoadPresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadPresetError::NotFound(name) => write!(f, "controller preset \"{name}\" not found"),
            LoadPresetError::Unreadable { path, reason } => write!(f, "could not read {}: {reason}", path.display()),
        }
    }
}

impl std::error::Error for LoadPresetError {}
