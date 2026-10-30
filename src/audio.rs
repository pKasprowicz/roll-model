//! The audio engine that will replace rodio (tasks AUD-2 to AUD-6, see `docs/audio-engine-plan.md`).
//!
//! One persistent output stream plays sounds that start at an exact frame instead of at the next
//! callback, so the onsets no longer move with the callback period. This module holds the parts
//! that need no sound device and are tested offline:
//!
//! - [`mixer`]: the voice pool that is rendered into the buffer of each callback.

pub mod mixer;
