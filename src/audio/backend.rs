//! The sound card: opens an output stream with cpal and runs the engine's callback in it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamError};

use super::handle::{channel, Callback, EngineHandle};
use crate::control::ErrorLog;

/// An open output stream. Dropping it stops the sound.
pub struct Engine {
    _stream: cpal::Stream,
    handle: EngineHandle,
    device: String,
    failed: Arc<AtomicBool>,
    errors: ErrorLog,
}

impl Engine {
    /// Opens the output called `device` (empty = the system default). If that device is missing or
    /// will not open, falls back to the default one and returns a message saying so.
    pub fn open(device: &str) -> Result<(Engine, Option<String>), String> {
        let wanted = device.trim();
        let host = cpal::default_host();
        let mut note = None;
        if !wanted.is_empty() {
            let found = host.output_devices().ok().and_then(|mut all| all.find(|d| name_of(d).is_some_and(|n| n == wanted)));
            match found {
                Some(d) => match open_device(&d) {
                    Ok(engine) => return Ok((engine, None)),
                    Err(e) => note = Some(format!("Sound output \"{wanted}\" could not be opened ({e}); using the system default.")),
                },
                None => note = Some(format!("Sound output \"{wanted}\" was not found; using the system default.")),
            }
        }
        let default = host.default_output_device().ok_or("No sound output could be found, so nothing can be played through the computer.")?;
        match open_device(&default) {
            Ok(engine) => Ok((engine, note)),
            Err(e) => Err(format!("No sound output could be opened ({e}), so nothing can be played through the computer.")),
        }
    }

    pub fn handle(&self) -> EngineHandle {
        self.handle.clone()
    }

    /// The name the device was opened with.
    pub fn device(&self) -> &str {
        &self.device
    }

    /// The device went away or the stream has to be built again: open a new engine.
    pub fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    /// Problems the stream reported since the last call.
    pub fn take_errors(&self) -> Vec<String> {
        self.errors.take()
    }
}

/// The name a device is listed and saved under. cpal before 0.17 gave Windows' friendly name
/// ("Speakers (Realtek(R) Audio)"), which settings files hold; since then `name()` is the short
/// description ("Speakers") and the friendly name is the first extended line.
fn name_of(device: &cpal::Device) -> Option<String> {
    let description = device.description().ok()?;
    #[cfg(windows)]
    if let Some(full) = description.extended().first() {
        return Some(full.clone());
    }
    Some(description.name().to_string())
}

fn open_device(device: &cpal::Device) -> Result<Engine, String> {
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let (rate, channels, format) = (supported.sample_rate(), supported.channels() as usize, supported.sample_format());
    let config: cpal::StreamConfig = supported.into();

    let origin = Instant::now();
    let (handle, callback) = channel(rate, origin);
    let failed = Arc::new(AtomicBool::new(false));
    let errors = ErrorLog::default();
    let (failed2, errors2) = (failed.clone(), errors.clone());
    let on_error = move |e: StreamError| {
        // Underruns are glitches the stream survives; the rest mean it has to be opened again.
        if !matches!(e, StreamError::BufferUnderrun) {
            failed2.store(true, Ordering::Relaxed);
        }
        errors2.push(format!("Sound output problem: {e}"));
    };

    let stream = match format {
        SampleFormat::F32 => build::<f32>(device, &config, callback, origin, channels, on_error),
        SampleFormat::I16 => build::<i16>(device, &config, callback, origin, channels, on_error),
        SampleFormat::I32 => build::<i32>(device, &config, callback, origin, channels, on_error),
        SampleFormat::U16 => build::<u16>(device, &config, callback, origin, channels, on_error),
        other => return Err(format!("the sample format {other:?} is not supported")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(Engine { _stream: stream, handle, device: name_of(device).unwrap_or_default(), failed, errors })
}

/// Longest block the scratch buffer is made for, in samples; a longer one grows it once.
const SCRATCH: usize = 16_384;

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut callback: Callback,
    origin: Instant,
    channels: usize,
    on_error: impl FnMut(StreamError) + Send + 'static,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let rate = config.sample_rate as f64;
    let mut scratch: Vec<f32> = Vec::with_capacity(SCRATCH);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                // As early as possible: the callback's own lateness is what the clock estimate measures.
                let wall = origin.elapsed().as_secs_f64();
                let ts = info.timestamp();
                if let Some(ahead) = ts.playback.duration_since(&ts.callback) {
                    callback.shared().set_latency_frames((ahead.as_secs_f64() * rate) as u32);
                }
                scratch.resize(data.len(), 0.0);
                callback.render(&mut scratch, channels, wall);
                for (out, s) in data.iter_mut().zip(scratch.iter()) {
                    *out = T::from_sample(*s);
                }
            },
            on_error,
            None,
        )
        .map_err(|e| e.to_string())
}

/// Names of the sound outputs the computer offers (without duplicates).
pub fn output_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut names: Vec<String> = Vec::new();
    if let Ok(devices) = host.output_devices() {
        for name in devices.filter_map(|d| name_of(&d)) {
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

/// Name of the system's default sound output, for display.
pub fn default_output_device() -> Option<String> {
    cpal::default_host().default_output_device().and_then(|d| name_of(&d))
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
        // On a machine without any sound output there is nothing to fall back to, and that is said too.
        match Engine::open("Speakers that do not exist") {
            Ok((engine, note)) => {
                let note = note.expect("an unknown name is reported, not ignored");
                assert!(note.contains("Speakers that do not exist"), "{note}");
                assert!(!engine.has_failed());
            }
            Err(e) => assert!(e.contains("No sound output"), "{e}"),
        }
    }

    #[test]
    fn an_open_engine_runs_its_callback() {
        let Ok((engine, _)) = Engine::open("") else { return }; // no sound output on this machine
        let handle = engine.handle();
        assert!(handle.wait_ready(std::time::Duration::from_secs(3)), "the first callback ran");
        assert!(handle.shared().latency().as_secs_f64() < 1.0);
    }
}
