//! Metronome / count-in. Runs on its own thread with sub-millisecond
//! scheduling; clicks go to the sound card (feature `audio`) and/or to a
//! MIDI output (e.g. back into the V31 so the click is in the drummer's
//! headphones).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct ClickOptions {
    pub audio: bool,
    pub volume: f32,
    pub midi_port: Option<String>,
    pub midi_note: u8,
    /// 1..=16
    pub midi_channel: u8,
}

pub struct Metronome {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Metronome {
    /// `clicks`: (seconds relative to `zero`, accented) in ascending order.
    pub fn start(zero: Instant, clicks: Vec<(f64, bool)>, opts: ClickOptions) -> Metronome {
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let handle = std::thread::Builder::new()
            .name("metronome".into())
            .spawn(move || run(zero, clicks, opts, stop2))
            .ok();
        Metronome { stop, handle }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Metronome {
    fn drop(&mut self) {
        self.stop();
    }
}

fn at(zero: Instant, t: f64) -> Option<Instant> {
    if t >= 0.0 {
        zero.checked_add(Duration::from_secs_f64(t))
    } else {
        zero.checked_sub(Duration::from_secs_f64(-t))
    }
}

/// Coarse sleep, then spin for the last ~1.5 ms.
fn wait_until(target: Instant, stop: &AtomicBool) -> bool {
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
type Audio = Option<(rodio::OutputStream, rodio::OutputStreamHandle)>;
#[cfg(not(feature = "audio"))]
type Audio = ();

#[cfg(feature = "audio")]
fn open_audio(enabled: bool) -> Audio {
    if enabled {
        rodio::OutputStream::try_default().ok()
    } else {
        None
    }
}

#[cfg(not(feature = "audio"))]
fn open_audio(_enabled: bool) -> Audio {}

#[cfg(feature = "audio")]
fn click_audio(audio: &Audio, accent: bool, volume: f32) {
    use rodio::Source;
    if let Some((_stream, handle)) = audio.as_ref() {
        let freq = if accent { 1760.0 } else { 1175.0 };
        let src = rodio::source::SineWave::new(freq)
            .take_duration(Duration::from_millis(30))
            .amplify(volume.clamp(0.0, 1.0));
        let _ = handle.play_raw(src);
    }
}

#[cfg(not(feature = "audio"))]
fn click_audio(_audio: &Audio, _accent: bool, _volume: f32) {}

/// Whether this build can play clicks through the sound card.
pub const HAS_AUDIO: bool = cfg!(feature = "audio");

fn run(zero: Instant, clicks: Vec<(f64, bool)>, opts: ClickOptions, stop: Arc<AtomicBool>) {
    #[allow(clippy::let_unit_value)]
    let audio = open_audio(opts.audio);

    let mut midi = opts.midi_port.as_deref().and_then(|p| crate::midi::connect_output(p).ok());
    let ch = opts.midi_channel.clamp(1, 16) - 1;

    for (t, accent) in clicks {
        let Some(target) = at(zero, t) else { continue };
        if !wait_until(target, &stop) {
            break;
        }
        if let Some(conn) = midi.as_mut() {
            let vel = if accent { 127 } else { 90 };
            let _ = conn.send(&[0x90 | ch, opts.midi_note, vel]);
            let _ = conn.send(&[0x80 | ch, opts.midi_note, 0]);
        }
        click_audio(&audio, accent, opts.volume);
    }
    // Let the last click ring out before the audio stream is dropped.
    std::thread::sleep(Duration::from_millis(80));
}
