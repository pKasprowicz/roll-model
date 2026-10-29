//! The core of Roll Model: everything that needs no window, no MIDI port and no sound card.
//! Scores (`musicxml`, `xml`, `loader`, `score`, `drums`), grading (`analysis`), rudiments, the
//! start trigger, the hardware controller (`controller`, `controller_layout`) and the settings
//! file (`config`). Positions in a score are quarter notes (`q`); seconds come only from
//! `score::TempoMap`.
//!
//! The application (the `roll-model` crate) adds the window, the MIDI ports and the sound on top
//! of this crate; nothing here may depend on them, so every part can be tested without a device.

pub mod analysis;
pub mod config;
pub mod controller;
pub mod controller_layout;
pub mod drums;
pub mod loader;
pub mod musicxml;
pub mod rudiments;
pub mod score;
pub mod trigger;
pub mod xml;
