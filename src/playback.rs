//! Playing a score back ("Listen"), and the free metronome, so the drummer can
//! hear what to play. One thread schedules the notes and the metronome clicks
//! with the same sub-millisecond timing as `metronome.rs`, and sends them to
//! the sound card (software drum kit from `synth.rs`) and/or to a MIDI output
//! (the module's own sounds). A single thread owns the audio stream and the
//! MIDI connection, so the click and the drums can share one MIDI port.
//!
//! Settings that can change while playing (click on/off, notes on/off,
//! volumes, per-instrument gain, looping) live in a shared [`Control`]. What
//! gets played for a cue is decided by [`dispatch`], which talks to a [`Sink`];
//! the real sink drives the devices, the tests use a fake one.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use midir::MidiOutputConnection;

use crate::analysis::Expected;
use crate::control::{ClickLevel, Control, ErrorLog};
use crate::drums::Lane;
use crate::metronome::{at, click_audio, open_audio, wait_until, Audio};
use crate::synth::{velocity_gain, Kit};

/// A cue whose turn comes later than this (the thread stalled, or the sound
/// device took long to open) is dropped instead of being played as a burst.
const MAX_LATE: Duration = Duration::from_millis(250);
/// How long the last notes may ring out after the final cue (the longest
/// cymbal sample is two seconds).
const TAIL: Duration = Duration::from_millis(2200);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sound {
    Click { level: ClickLevel },
    Drum { lane: Lane, velocity: u8 },
}

/// Something to play `t` seconds after the player's zero (negative = count-in).
#[derive(Clone, Copy, Debug)]
pub struct Cue {
    pub t: f64,
    pub sound: Sound,
}

/// What is fixed for one run of the player. Everything that can change while
/// it plays is in the shared [`Control`] instead.
#[derive(Clone, Debug)]
pub struct PlayOptions {
    /// Play the drums through the sound card.
    pub audio: bool,
    /// Sound output device by name; empty = the system default.
    pub audio_device: String,
    /// Folder with WAV files that replace built-in instrument sounds.
    pub sounds_dir: Option<PathBuf>,
    pub click_audio: bool,
    /// MIDI output to open; `None` when neither drums nor click go to MIDI.
    pub midi_port: Option<String>,
    pub midi_drums: bool,
    /// 1..=16
    pub drum_channel: u8,
    pub click_midi: bool,
    pub click_note: u8,
    /// 1..=16
    pub click_channel: u8,
    /// MIDI note that sounds each lane on the module.
    pub drum_notes: BTreeMap<Lane, u8>,
}

/// MIDI velocity for a score note.
pub fn velocity_for(e: &Expected) -> u8 {
    if e.ghost {
        45
    } else if e.accent {
        120
    } else if e.grace {
        70
    } else {
        90
    }
}

/// The notes of a take and its click track as one list ordered by time. At
/// the same instant the click comes first.
pub fn cues(expected: &[Expected], clicks: &[(f64, bool)]) -> Vec<Cue> {
    let mut v: Vec<Cue> = clicks
        .iter()
        .map(|&(t, accent)| Cue { t, sound: Sound::Click { level: ClickLevel::from_accent(accent) } })
        .chain(expected.iter().map(|e| Cue { t: e.t, sound: Sound::Drum { lane: e.lane, velocity: velocity_for(e) } }))
        .collect();
    v.sort_by(|a, b| a.t.total_cmp(&b.t)); // stable
    v
}

/// How a beat of the free metronome is divided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subdivision {
    None,
    Eighths,
    Triplets,
    Sixteenths,
}

impl Subdivision {
    pub const ALL: [Subdivision; 4] = [Subdivision::None, Subdivision::Eighths, Subdivision::Triplets, Subdivision::Sixteenths];

    pub fn label(self) -> &'static str {
        match self {
            Subdivision::None => "none",
            Subdivision::Eighths => "eighths",
            Subdivision::Triplets => "triplets",
            Subdivision::Sixteenths => "sixteenths",
        }
    }

    /// Clicks per beat.
    pub fn parts(self) -> u32 {
        match self {
            Subdivision::None => 1,
            Subdivision::Eighths => 2,
            Subdivision::Triplets => 3,
            Subdivision::Sixteenths => 4,
        }
    }

    /// Position in `ALL`, as stored in the settings file.
    pub fn index(self) -> usize {
        Subdivision::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    pub fn from_index(i: usize) -> Subdivision {
        Subdivision::ALL.get(i).copied().unwrap_or(Subdivision::None)
    }
}

/// One bar of the free metronome and its length in seconds: an accent on the
/// first beat, normal clicks on the other beats, soft clicks between them.
pub fn metronome_cues(bpm: f64, beats: u32, sub: Subdivision) -> (Vec<Cue>, f64) {
    let beat_sec = 60.0 / bpm.max(1.0);
    let parts = sub.parts();
    let mut v = Vec::new();
    for b in 0..beats.max(1) {
        for s in 0..parts {
            let level = match (b, s) {
                (0, 0) => ClickLevel::Accent,
                (_, 0) => ClickLevel::Beat,
                _ => ClickLevel::Sub,
            };
            v.push(Cue { t: (b * parts + s) as f64 * beat_sec / parts as f64, sound: Sound::Click { level } });
        }
    }
    (v, beats.max(1) as f64 * beat_sec)
}

/// Where the sound goes. The real one drives the devices; tests record calls.
pub trait Sink {
    fn audio_click(&mut self, level: ClickLevel, volume: f32);
    fn midi_click(&mut self, level: ClickLevel);
    /// `gain` includes the master volume, the instrument gain and the velocity.
    fn audio_drum(&mut self, lane: Lane, gain: f32);
    fn midi_drum(&mut self, note: u8, velocity: u8);
}

/// Decides what a cue sounds like right now, from the options and the live
/// controls, and hands it to the sink.
fn dispatch(cue: &Cue, opts: &PlayOptions, control: &Control, sink: &mut dyn Sink) {
    match cue.sound {
        Sound::Click { level } => {
            // The count-in (before zero) always sounds; the click during the music can be switched off.
            if cue.t >= 0.0 && !control.click() {
                return;
            }
            if opts.click_midi {
                sink.midi_click(level);
            }
            if opts.click_audio {
                sink.audio_click(level, control.click_volume());
            }
        }
        Sound::Drum { lane, velocity } => {
            let gain = control.lane_gain(lane);
            if !control.notes() || gain <= 0.0 {
                return;
            }
            if opts.midi_drums {
                if let Some(note) = opts.drum_notes.get(&lane) {
                    sink.midi_drum(*note, (velocity as f32 * gain).round().clamp(1.0, 127.0) as u8);
                }
            }
            if opts.audio {
                sink.audio_drum(lane, control.volume() * gain * velocity_gain(velocity));
            }
        }
    }
}

/// The cues of one pass. Only the first pass has the count-in.
fn pass_cues(cues: &[Cue], pass: usize) -> impl Iterator<Item = &Cue> {
    cues.iter().filter(move |c| pass == 0 || c.t >= 0.0)
}

/// Plays `cues` relative to `zero`, pass after pass while `control.looping()`
/// stays on (a pass is `pass_len` seconds long). Sets `control.music_done()`
/// when the last pass has reached its end, then lets the tail ring out.
fn run_with(
    zero: Instant,
    cues: &[Cue],
    pass_len: f64,
    opts: &PlayOptions,
    control: &Control,
    stop: &AtomicBool,
    sink: &mut dyn Sink,
    tail: Duration,
) {
    let pass_len = pass_len.max(0.001);
    let mut pass = 0usize;
    'passes: loop {
        let offset = pass as f64 * pass_len;
        for cue in pass_cues(cues, pass) {
            let Some(target) = at(zero, cue.t + offset) else { continue };
            if !wait_until(target, stop) {
                break 'passes;
            }
            if Instant::now().saturating_duration_since(target) > MAX_LATE {
                continue;
            }
            dispatch(cue, opts, control, sink);
        }
        if let Some(end) = at(zero, offset + pass_len) {
            if !wait_until(end, stop) {
                break;
            }
        }
        if !control.looping() {
            break;
        }
        pass += 1;
    }
    control.set_music_done(true);

    // Let the last notes ring out before the stream is dropped.
    let end = Instant::now() + tail;
    while Instant::now() < end && !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub struct Player {
    stop: Arc<AtomicBool>,
    errors: ErrorLog,
    handle: Option<JoinHandle<()>>,
}

impl Player {
    /// Starts a thread that plays `cues` relative to `zero`; one pass is
    /// `pass_len` seconds long and repeats while `control.looping()` is on.
    pub fn start(zero: Instant, cues: Vec<Cue>, pass_len: f64, opts: PlayOptions, control: Arc<Control>) -> Player {
        control.set_music_done(false);
        let stop = Arc::new(AtomicBool::new(false));
        let errors = ErrorLog::default();
        let (stop2, errors2) = (stop.clone(), errors.clone());
        let handle = std::thread::Builder::new()
            .name("playback".into())
            .spawn(move || run(zero, cues, pass_len, opts, control, stop2, errors2))
            .ok();
        Player { stop, errors, handle }
    }

    /// Stops at once (cutting off ringing cymbals) and waits for the thread.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    /// The last cue was played and the tail has rung out.
    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().map_or(true, |h| h.is_finished())
    }

    /// Problems the thread ran into (no sound device, MIDI port missing).
    pub fn take_errors(&self) -> Vec<String> {
        self.errors.take()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

fn note_on_off(conn: &mut MidiOutputConnection, channel: u8, note: u8, velocity: u8) {
    let _ = conn.send(&[0x90 | channel, note, velocity.clamp(1, 127)]);
    let _ = conn.send(&[0x80 | channel, note, 0]);
}

/// One drum sample, shared between the hits of a lane, played at `gain`. When
/// `choke` is set the voice fades out over a few milliseconds and ends.
#[cfg(feature = "audio")]
struct Voice {
    data: Arc<Vec<f32>>,
    pos: usize,
    gain: f32,
    choke: Arc<AtomicBool>,
    /// Samples of fade-out left once choked; `None` while the voice plays normally.
    fade_left: Option<u32>,
}

#[cfg(feature = "audio")]
const FADE_SAMPLES: u32 = 256;

#[cfg(feature = "audio")]
impl Iterator for Voice {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.fade_left.is_none() && self.choke.load(Ordering::Relaxed) {
            self.fade_left = Some(FADE_SAMPLES);
        }
        let ramp = match self.fade_left {
            Some(0) => return None,
            Some(left) => {
                self.fade_left = Some(left - 1);
                (left - 1) as f32 / FADE_SAMPLES as f32
            }
            None => 1.0,
        };
        let s = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(s * self.gain * ramp)
    }
}

#[cfg(feature = "audio")]
impl rodio::Source for Voice {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        crate::synth::SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(self.data.len() as f64 / crate::synth::SAMPLE_RATE as f64))
    }
}

#[cfg(feature = "audio")]
fn play_voice(audio: &Audio, sample: &Arc<Vec<f32>>, gain: f32, choke: Arc<AtomicBool>) {
    if let Some((_stream, handle)) = audio.as_ref() {
        let _ = handle.play_raw(Voice { data: sample.clone(), pos: 0, gain, choke, fade_left: None });
    }
}

/// A voice that may still be sounding and could be choked by a later hit.
#[cfg(feature = "audio")]
struct Ringing {
    lane: Lane,
    choke: Arc<AtomicBool>,
    since: Instant,
}

/// The real output: the sound card and the MIDI port.
struct DeviceSink {
    audio: Audio,
    midi: Option<MidiOutputConnection>,
    kit: Kit,
    click_note: u8,
    click_channel: u8,
    drum_channel: u8,
    #[cfg(feature = "audio")]
    ringing: Vec<Ringing>,
}

impl Sink for DeviceSink {
    fn audio_click(&mut self, level: ClickLevel, volume: f32) {
        click_audio(&self.audio, level, volume);
    }

    fn midi_click(&mut self, level: ClickLevel) {
        if let Some(conn) = self.midi.as_mut() {
            note_on_off(conn, self.click_channel, self.click_note, level.velocity());
        }
    }

    fn audio_drum(&mut self, lane: Lane, gain: f32) {
        #[cfg(feature = "audio")]
        {
            // Closing the hi-hat ends the open one.
            self.ringing.retain(|r| r.since.elapsed() < Duration::from_millis(2500));
            for r in &self.ringing {
                if lane.chokes(r.lane) {
                    r.choke.store(true, Ordering::Relaxed);
                }
            }
            if let Some(sample) = self.kit.sample(lane) {
                let choke = Arc::new(AtomicBool::new(false));
                if lane.can_be_choked() {
                    self.ringing.push(Ringing { lane, choke: choke.clone(), since: Instant::now() });
                }
                play_voice(&self.audio, sample, gain, choke);
            }
        }
        #[cfg(not(feature = "audio"))]
        {
            let _ = (lane, gain, &self.kit, &self.audio);
        }
    }

    fn midi_drum(&mut self, note: u8, velocity: u8) {
        if let Some(conn) = self.midi.as_mut() {
            note_on_off(conn, self.drum_channel, note, velocity);
        }
    }
}

fn run(zero: Instant, cues: Vec<Cue>, pass_len: f64, opts: PlayOptions, control: Arc<Control>, stop: Arc<AtomicBool>, errors: ErrorLog) {
    let lanes = cues.iter().filter_map(|c| match c.sound {
        Sound::Drum { lane, .. } if opts.audio => Some(lane),
        _ => None,
    });
    let (kit, warnings) = Kit::load(lanes, opts.sounds_dir.as_deref());
    for w in warnings {
        errors.push(w);
    }

    let has_clicks = cues.iter().any(|c| matches!(c.sound, Sound::Click { .. }));
    let want_audio = opts.audio || (opts.click_audio && has_clicks);
    let (audio, audio_note) = open_audio(want_audio, &opts.audio_device);
    if let Some(message) = audio_note {
        errors.push(message);
    }
    #[cfg(not(feature = "audio"))]
    if want_audio {
        errors.push("This build has no sound output: send playback to a MIDI output instead.".into());
    }
    let mut midi = None;
    if let Some(port) = opts.midi_port.as_deref() {
        match crate::midi::connect_output(port) {
            Ok(conn) => midi = Some(conn),
            Err(e) => errors.push(e.to_string()),
        }
    }

    let mut sink = DeviceSink {
        audio,
        midi,
        kit,
        click_note: opts.click_note,
        click_channel: opts.click_channel.clamp(1, 16) - 1,
        drum_channel: opts.drum_channel.clamp(1, 16) - 1,
        #[cfg(feature = "audio")]
        ringing: Vec::new(),
    };
    run_with(zero, &cues, pass_len, &opts, &control, &stop, &mut sink, TAIL);
}

#[cfg(test)]
mod tests;
