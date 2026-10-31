//! The sound output of the program: one engine kept open from the first sound on, shared by takes,
//! Listen, the metronome tab, auditions and the calibration. It is opened again when the output
//! device in the settings changes or the stream has failed.

use super::DrumApp;
use crate::audio::{default_output_device, Engine, EngineHandle};
use crate::metronome::HAS_AUDIO;

#[derive(Default)]
pub(super) struct AudioOutput {
    engine: Option<Engine>,
    /// The device name the engine was asked for (empty = the system default).
    device: String,
    /// What was said when the engine was opened (a fallback to the default output, or why there is none).
    note: Option<String>,
}

/// What the Sound page shows about the open output.
pub(super) struct AudioInfo {
    pub device: String,
    pub rate: u32,
    /// What the driver reports between the callback and the sound leaving; `None` when it reports none.
    pub latency_ms: Option<f64>,
    pub late: u64,
    /// How late the worst of them was.
    pub worst_late_ms: f64,
    pub dropped: u64,
}

/// Whether the open engine has to be replaced: its stream failed, it was opened for another
/// setting, or the setting is "System default" and the default has moved to another output since.
/// `default_now` is the system's default output, read only when the setting asks for it; `None`
/// when it was not read or is unknown, which never forces a new engine.
fn must_reopen(opened_for: &str, wanted: &str, open_device: &str, failed: bool, default_now: Option<&str>) -> bool {
    failed || opened_for != wanted || default_now.is_some_and(|now| now != open_device)
}

impl AudioOutput {
    /// Opens an engine for `device` unless a working one is open already. Returns the message to
    /// show: a fallback to the default output, or that there is no output at all.
    fn ensure(&mut self, device: &str) -> Option<String> {
        if let Some(engine) = &self.engine {
            // Windows leaves a running stream on the output it opened when only the default moves,
            // so the default is looked at again at each start.
            let default_now = if device.trim().is_empty() { default_output_device() } else { None };
            if !must_reopen(&self.device, device, engine.device(), engine.has_failed(), default_now.as_deref()) {
                return self.note.clone();
            }
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

    pub(super) fn is_open(&self) -> bool {
        self.engine.is_some()
    }

    pub(super) fn info(&self) -> Option<AudioInfo> {
        let engine = self.engine.as_ref()?;
        let handle = engine.handle();
        let shared = handle.shared();
        let latency = shared.latency().as_secs_f64() * 1000.0;
        let counters = shared.counters();
        Some(AudioInfo {
            device: engine.device().to_string(),
            rate: shared.rate(),
            latency_ms: (latency > 0.0).then_some(latency),
            late: counters.late,
            worst_late_ms: counters.worst_late as f64 * 1000.0 / shared.rate().max(1) as f64,
            dropped: counters.dropped_pending + counters.dropped_voices,
        })
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
        // With "System default" chosen the open output is the default that was just looked at, so
        // the label of the Sound page follows a change of it without a Refresh.
        if self.cfg.audio_output.trim().is_empty() {
            if let Some(info) = self.audio.info() {
                self.audio_default = Some(info.device);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::must_reopen;

    #[test]
    fn a_working_engine_for_the_same_setting_and_default_is_kept() {
        assert!(!must_reopen("", "", "Speakers", false, Some("Speakers")));
        assert!(!must_reopen("Headphones", "Headphones", "Headphones", false, None));
    }

    #[test]
    fn a_move_of_the_system_default_opens_the_new_output() {
        // The stream is still alive on the old output: nothing failed, only the default changed.
        assert!(must_reopen("", "", "Speakers", false, Some("Headphones")));
    }

    #[test]
    fn a_named_output_does_not_follow_the_default() {
        // `default_now` is not read for a named output.
        assert!(!must_reopen("Headphones", "Headphones", "Headphones", false, None));
    }

    #[test]
    fn an_unknown_default_does_not_force_a_new_engine() {
        assert!(!must_reopen("", "", "Speakers", false, None));
    }

    #[test]
    fn a_failed_stream_or_another_setting_opens_a_new_engine() {
        assert!(must_reopen("", "", "Speakers", true, Some("Speakers")));
        assert!(must_reopen("", "Headphones", "Speakers", false, None));
        assert!(must_reopen("Headphones", "", "Headphones", false, Some("Headphones")));
    }
}
