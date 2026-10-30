//! The audio engine (tasks AUD-2 to AUD-6, see `docs/audio-engine-plan.md`).
//!
//! One persistent output stream plays sounds that start at an exact frame instead of at the next
//! callback, so the onsets no longer move with the callback period. The parts that need no sound
//! device are tested offline:
//!
//! - [`mixer`]: the voice pool that is rendered into the buffer of each callback;
//! - [`clock`]: where in the stream's frames a moment of the program's clock falls;
//! - [`handle`]: the end the scheduler threads use and the callback the backend runs, with the
//!   queue between them;
//! - [`resample`] and [`click`]: the sounds at the stream's rate.
//!
//! `backend` (feature `audio`) is the part that opens the sound card; without the feature [`Engine`]
//! can never be opened and the program can only send its sound to a MIDI output.

pub mod click;
pub mod clock;
pub mod handle;
pub mod mixer;
pub mod resample;

#[cfg(feature = "audio")]
mod backend;
#[cfg(feature = "audio")]
pub use backend::{default_output_device, output_devices, Engine};

#[cfg(not(feature = "audio"))]
mod silent;
#[cfg(not(feature = "audio"))]
pub use silent::{default_output_device, output_devices, Engine};

pub use handle::EngineHandle;

/// Which generation of audio engine this is. A latency compensation set with another one is
/// probably wrong (`Config::latency_is_stale`): raise it when a change moves the click in time.
/// 1 was rodio; 2 is this engine.
pub const ENGINE_GENERATION: u32 = 2;
