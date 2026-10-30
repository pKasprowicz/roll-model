//! What stands in for the backend in a build without the `audio` feature: nothing can be opened.

use super::handle::EngineHandle;

/// With no value, so there is never an open engine in this build.
pub enum Engine {}

impl Engine {
    pub fn open(_device: &str) -> Result<(Engine, Option<String>), String> {
        Err("This build has no sound output: send playback to a MIDI output instead.".into())
    }

    pub fn handle(&self) -> EngineHandle {
        match *self {}
    }

    pub fn device(&self) -> &str {
        match *self {}
    }

    pub fn has_failed(&self) -> bool {
        match *self {}
    }

    pub fn take_errors(&self) -> Vec<String> {
        match *self {}
    }
}

pub fn output_devices() -> Vec<String> {
    Vec::new()
}

pub fn default_output_device() -> Option<String> {
    None
}
