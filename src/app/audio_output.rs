//! The sound output of the program: one engine kept open from the first sound on, shared by takes,
//! Listen, the metronome tab, auditions and the calibration. It is opened again when the output
//! device in the settings changes or the stream has failed.

use super::DrumApp;
use crate::audio::{Engine, EngineHandle};
use crate::metronome::HAS_AUDIO;

#[derive(Default)]
pub(super) struct AudioOutput {
    engine: Option<Engine>,
    /// The device name the engine was asked for (empty = the system default).
    device: String,
    /// What was said when the engine was opened (a fallback to the default output, or why there is none).
    note: Option<String>,
}

impl AudioOutput {
    /// Opens an engine for `device` unless a working one is open already. Returns the message to
    /// show: a fallback to the default output, or that there is no output at all.
    fn ensure(&mut self, device: &str) -> Option<String> {
        if self.engine.as_ref().is_some_and(|e| self.device == device && !e.has_failed()) {
            return self.note.clone();
        }
        // The old stream goes first: the device is not opened twice.
        self.engine = None;
        self.device = device.to_string();
        match Engine::open(device) {
            Ok((engine, note)) => {
                self.engine = Some(engine);
                self.note = note;
            }
            Err(message) => self.note = Some(message),
        }
        self.note.clone()
    }

    pub(super) fn handle(&self) -> Option<EngineHandle> {
        self.engine.as_ref().map(|e| e.handle())
    }

    /// Problems the stream reported since the last call.
    pub(super) fn take_errors(&self) -> Vec<String> {
        self.engine.as_ref().map(|e| e.take_errors()).unwrap_or_default()
    }
}

impl DrumApp {
    /// Opens the sound output if some sound is going to need it. Called before every start, so a
    /// device that was missing or has been changed in the settings is picked up.
    pub(super) fn ensure_audio(&mut self) {
        if !HAS_AUDIO || !(self.cfg.click_audio || self.cfg.playback_audio) {
            return;
        }
        if let Some(message) = self.audio.ensure(&self.cfg.audio_output) {
            self.view.error = Some(message);
        }
    }
}
