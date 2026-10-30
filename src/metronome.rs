//! Metronome / count-in. Runs on its own thread with sub-millisecond
//! scheduling; clicks go to the sound card (feature `audio`) and/or to a
//! MIDI output (e.g. back into the V31 so the click is in the drummer's
//! headphones).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::control::{ClickLevel, Control, ErrorLog};

#[derive(Clone, Debug)]
pub struct ClickOptions {
    pub audio: bool,
    /// Sound output device by name; empty = the system default.
    pub audio_device: String,
    /// Used when no `Control` is given.
    pub volume: f32,
    pub midi_port: Option<String>,
    pub midi_note: u8,
    /// 1..=16
    pub midi_channel: u8,
}

pub struct Metronome {
    stop: Arc<AtomicBool>,
    errors: ErrorLog,
    handle: Option<JoinHandle<()>>,
}

impl Metronome {
    /// `clicks`: (seconds relative to `zero`, accented) in ascending order.
    /// With a `control`, clicks at or after `zero` follow its click switch and
    /// volume while they play; the count-in before `zero` always sounds.
    pub fn start(zero: Instant, clicks: Vec<(f64, bool)>, opts: ClickOptions, control: Option<Arc<Control>>) -> Metronome {
        let stop = Arc::new(AtomicBool::new(false));
        let errors = ErrorLog::default();
        let (stop2, errors2) = (stop.clone(), errors.clone());
        let handle = std::thread::Builder::new()
            .name("metronome".into())
            .spawn(move || run(zero, clicks, opts, control, stop2, errors2))
            .ok();
        Metronome { stop, errors, handle }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    /// Problems the thread ran into (sound device missing, MIDI port missing).
    pub fn take_errors(&self) -> Vec<String> {
        self.errors.take()
    }
}

impl Drop for Metronome {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(crate) fn at(zero: Instant, t: f64) -> Option<Instant> {
    if t >= 0.0 {
        zero.checked_add(Duration::from_secs_f64(t))
    } else {
        zero.checked_sub(Duration::from_secs_f64(-t))
    }
}

/// Coarse sleep, then spin for the last ~1.5 ms. `false` if `stop` was set first.
pub(crate) fn wait_until(target: Instant, stop: &AtomicBool) -> bool {
    loop {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let now = Instant::now();
        if now >= target {
            return true;
        }
        let rem = target - now;
        if rem > Duration::from_micros(2500) {
            std::thread::sleep((rem - Duration::from_micros(1500)).min(Duration::from_millis(10)));
        } else {
            std::hint::spin_loop();
        }
    }
}

#[cfg(feature = "audio")]
pub(crate) type Audio = Option<(rodio::OutputStream, rodio::OutputStreamHandle)>;
/// Without the `audio` feature there is never an output: a type with no value, so `None` is the only one.
#[cfg(not(feature = "audio"))]
pub(crate) type Audio = Option<std::convert::Infallible>;

/// Opens the sound output called `device` (empty = the system default). If
/// that device is missing or will not open, falls back to the default one and
/// returns a message saying so; the message is also returned when there is no
/// usable output at all.
#[cfg(feature = "audio")]
pub(crate) fn open_audio(enabled: bool, device: &str) -> (Audio, Option<String>) {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    if !enabled {
        return (None, None);
    }
    let wanted = device.trim();
    let mut note = None;
    if !wanted.is_empty() {
        let found = rodio::cpal::default_host()
            .output_devices()
            .ok()
            .and_then(|mut devices| devices.find(|d| d.name().is_ok_and(|n| n == wanted)));
        match found {
            Some(d) => match rodio::OutputStream::try_from_device(&d) {
                Ok(stream) => return (Some(stream), None),
                Err(e) => note = Some(format!("Sound output \"{wanted}\" could not be opened ({e}); using the system default.")),
            },
            None => note = Some(format!("Sound output \"{wanted}\" was not found; using the system default.")),
        }
    }
    match rodio::OutputStream::try_default() {
        Ok(stream) => (Some(stream), note),
        Err(e) => (None, Some(format!("No sound output could be opened ({e}), so nothing can be played through the computer."))),
    }
}

#[cfg(not(feature = "audio"))]
pub(crate) fn open_audio(_enabled: bool, _device: &str) -> (Audio, Option<String>) {
    (None, None)
}

/// Names of the sound outputs the computer offers (without duplicates).
#[cfg(feature = "audio")]
pub fn output_devices() -> Vec<String> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    let mut names: Vec<String> = Vec::new();
    if let Ok(devices) = rodio::cpal::default_host().output_devices() {
        for name in devices.filter_map(|d| d.name().ok()) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    // Some hosts (ALSA without a sound card, for one) have a default output they do not enumerate.
    if let Some(default) = default_output_device() {
        if !names.contains(&default) {
            names.insert(0, default);
        }
    }
    names
}

#[cfg(not(feature = "audio"))]
pub fn output_devices() -> Vec<String> {
    Vec::new()
}

/// Name of the system's default sound output, for display.
#[cfg(feature = "audio")]
pub fn default_output_device() -> Option<String> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    rodio::cpal::default_host().default_output_device().and_then(|d| d.name().ok())
}

#[cfg(not(feature = "audio"))]
pub fn default_output_device() -> Option<String> {
    None
}

#[cfg(feature = "audio")]
pub(crate) fn click_audio(audio: &Audio, level: ClickLevel, volume: f32) {
    use rodio::Source;
    if let Some((_stream, handle)) = audio.as_ref() {
        let (freq, soften) = match level {
            ClickLevel::Accent => (1760.0, 1.0),
            ClickLevel::Beat => (1175.0, 1.0),
            ClickLevel::Sub => (880.0, 0.6),
        };
        let src = rodio::source::SineWave::new(freq)
            .take_duration(Duration::from_millis(30))
            .amplify((volume * soften).clamp(0.0, 1.0));
        let _ = handle.play_raw(src);
    }
}

#[cfg(not(feature = "audio"))]
pub(crate) fn click_audio(_audio: &Audio, _level: ClickLevel, _volume: f32) {}

/// Whether this build can play clicks through the sound card.
pub const HAS_AUDIO: bool = cfg!(feature = "audio");

fn run(
    zero: Instant,
    clicks: Vec<(f64, bool)>,
    opts: ClickOptions,
    control: Option<Arc<Control>>,
    stop: Arc<AtomicBool>,
    errors: ErrorLog,
) {
    let (audio, audio_note) = open_audio(opts.audio, &opts.audio_device);
    if let Some(message) = audio_note {
        errors.push(message);
    }

    let mut midi = None;
    if let Some(port) = opts.midi_port.as_deref() {
        match crate::midi::connect_output(port) {
            Ok(conn) => midi = Some(conn),
            Err(e) => errors.push(e.to_string()),
        }
    }
    let ch = opts.midi_channel.clamp(1, 16) - 1;

    for (t, accent) in clicks {
        let Some(target) = at(zero, t) else { continue };
        if !wait_until(target, &stop) {
            break;
        }
        if t >= 0.0 && control.as_ref().is_some_and(|c| !c.click()) {
            continue; // switched off while the music runs
        }
        let level = ClickLevel::from_accent(accent);
        if let Some(conn) = midi.as_mut() {
            let _ = conn.send(&[0x90 | ch, opts.midi_note, level.velocity()]);
            let _ = conn.send(&[0x80 | ch, opts.midi_note, 0]);
        }
        let volume = control.as_ref().map_or(opts.volume, |c| c.click_volume());
        click_audio(&audio, level, volume);
    }
    // Let the last click ring out before the audio stream is dropped.
    std::thread::sleep(Duration::from_millis(80));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_devices_are_listed_once_and_include_the_default() {
        let devices = output_devices();
        println!("sound outputs: {devices:?}; default: {:?}", default_output_device());
        let mut unique = devices.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), devices.len(), "no duplicates");
        if let Some(default) = default_output_device() {
            assert!(devices.contains(&default), "the default output {default:?} is one of the outputs");
        }
    }

    #[test]
    fn a_missing_device_falls_back_to_the_default_and_says_so() {
        // Nothing is opened when sound is not wanted.
        let (_, note) = open_audio(false, "Speakers that do not exist");
        assert_eq!(note, None);
        // With sound wanted, an unknown name must be reported rather than silently ignored
        // (or, on a machine without any sound output, the lack of one).
        let (audio, note) = open_audio(true, "Speakers that do not exist");
        #[cfg(feature = "audio")]
        {
            let note = note.expect("a message either way");
            assert!(note.contains("Speakers that do not exist") || note.contains("No sound output"), "{note}");
            let _ = audio;
        }
        #[cfg(not(feature = "audio"))]
        {
            let _ = (audio, note);
        }
    }
}
