//! Metronome / count-in. Runs on its own thread with sub-millisecond
//! scheduling; clicks go to the sound card (feature `audio`) and/or to a
//! MIDI output (e.g. back into the V31 so the click is in the drummer's
//! headphones).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::audio::click::ClickBank;
use crate::audio::mixer::NO_GROUP;
use crate::audio::EngineHandle;
use crate::control::{ClickLevel, Control, ErrorLog};

#[derive(Clone, Debug)]
pub struct ClickOptions {
    pub audio: bool,
    /// The open sound output; `None` when there is none to play through.
    pub engine: Option<EngineHandle>,
    /// Used when no `Control` is given.
    pub volume: f32,
    pub midi_port: Option<String>,
    pub midi_note: u8,
    /// 1..=16
    pub midi_channel: u8,
    /// How much later than its time the MIDI click goes out when the click also sounds through the sound
    /// card (the output delay). Zero for the calibration clicks.
    pub midi_delay: Duration,
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

/// The MIDI delay, cut short where the next click would otherwise be handed to the engine late: this thread
/// waits for the MIDI note, and the next click is due `AUDIO_LOOKAHEAD` before its time.
fn midi_delay_before_next(delay: Duration, gap_to_next: Option<f64>) -> Duration {
    match gap_to_next {
        Some(gap) => delay.min(Duration::from_secs_f64(gap.max(0.0)).saturating_sub(AUDIO_LOOKAHEAD)),
        None => delay,
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

/// Whether this build can play clicks through the sound card.
pub const HAS_AUDIO: bool = cfg!(feature = "audio");

/// How far ahead of its time a sound is handed to the audio engine: longer than a callback period
/// plus the lateness of a scheduling thread, so the engine can place it at its exact frame.
pub(crate) const AUDIO_LOOKAHEAD: Duration = Duration::from_millis(30);

fn run(
    zero: Instant,
    clicks: Vec<(f64, bool)>,
    opts: ClickOptions,
    control: Option<Arc<Control>>,
    stop: Arc<AtomicBool>,
    errors: ErrorLog,
) {
    let engine = opts.engine.clone().filter(|_| opts.audio);
    let bank = engine.as_ref().map(|e| ClickBank::new(e.shared().rate()));
    let run_id = engine.as_ref().map(|e| e.new_run());
    if let Some(e) = engine.as_ref() {
        if !e.wait_ready(Duration::from_millis(500)) {
            errors.push("The sound output did not start in time, so the click may be late.".into());
        }
    }

    let mut midi = None;
    if let Some(port) = opts.midi_port.as_deref() {
        match crate::midi::connect_output(port) {
            Ok(conn) => midi = Some(conn),
            Err(e) => errors.push(e.to_string()),
        }
    }
    let ch = opts.midi_channel.clamp(1, 16) - 1;

    let times: Vec<f64> = clicks.iter().map(|c| c.0).collect();
    for (i, (t, accent)) in clicks.into_iter().enumerate() {
        let Some(target) = at(zero, t) else { continue };
        // The engine needs the click ahead of its time; MIDI goes out at the time itself.
        let wake = if engine.is_some() { target.checked_sub(AUDIO_LOOKAHEAD).unwrap_or(target) } else { target };
        if !wait_until(wake, &stop) {
            break;
        }
        if t >= 0.0 && control.as_ref().is_some_and(|c| !c.click()) {
            continue; // switched off while the music runs
        }
        let level = ClickLevel::from_accent(accent);
        if let (Some(engine), Some(bank), Some(run)) = (engine.as_ref(), bank.as_ref(), run_id) {
            let volume = control.as_ref().map_or(opts.volume, |c| c.click_volume());
            let (sound, gain) = bank.get(level, volume);
            let _ = engine.play(run, target, sound, gain, NO_GROUP, 0);
        }
        if let Some(conn) = midi.as_mut() {
            let delay = if engine.is_some() { midi_delay_before_next(opts.midi_delay, times.get(i + 1).map(|n| n - t)) } else { Duration::ZERO };
            if !wait_until(target + delay, &stop) {
                break;
            }
            let _ = conn.send(&[0x90 | ch, opts.midi_note, level.velocity()]);
            let _ = conn.send(&[0x80 | ch, opts.midi_note, 0]);
        }
    }
    // Let the last click ring out; a stop cuts it off.
    if !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(80));
    }
    if let (Some(engine), Some(run)) = (engine.as_ref(), run_id) {
        let _ = engine.cancel_run(run);
    }
}

#[cfg(test)]
mod delay_tests {
    use super::*;

    #[test]
    fn the_midi_delay_leaves_the_next_click_its_lookahead() {
        let ms = Duration::from_millis;
        assert_eq!(midi_delay_before_next(ms(45), Some(0.6)), ms(45));
        assert_eq!(midi_delay_before_next(ms(45), None), ms(45));
        // Clicks 62.5 ms apart: only 32.5 ms are left once the next one must be handed over 30 ms ahead.
        assert_eq!(midi_delay_before_next(ms(45), Some(0.0625)), Duration::from_micros(32_500));
        assert_eq!(midi_delay_before_next(ms(45), Some(0.02)), Duration::ZERO);
    }
}
