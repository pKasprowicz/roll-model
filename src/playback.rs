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
use crate::audio::click::ClickBank;
use crate::audio::mixer::NO_GROUP;
use crate::audio::resample::resample;
use crate::audio::EngineHandle;
use crate::metronome::{at, wait_until, AUDIO_LOOKAHEAD};
use crate::synth::{velocity_gain, Kit, SAMPLE_RATE};

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

/// A tempo ramp for a looping run: every `every` passes the tempo rises by `step`, a fraction of
/// the starting tempo (0.05 = five per cent of it), up to `MAX_FACTOR` times the starting tempo.
/// Cue times are given for the starting tempo; the player squeezes the later passes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ramp {
    /// 0 = no ramp.
    pub every: usize,
    pub step: f64,
}

impl Ramp {
    /// The tempo never rises beyond this many times the starting tempo.
    pub const MAX_FACTOR: f64 = 3.0;

    /// How many times faster than the start pass number `pass` (the first is 0) is played.
    pub fn factor(self, pass: usize) -> f64 {
        if self.every == 0 || self.step <= 0.0 {
            return 1.0;
        }
        (1.0 + self.step * (pass / self.every) as f64).min(Self::MAX_FACTOR)
    }

    fn is_off(self) -> bool {
        self.every == 0 || self.step <= 0.0
    }

    /// Where `elapsed` real seconds into the run are: the pass (from 0) and the seconds into that
    /// pass counted at the starting tempo. Before the start (the count-in) nothing is squeezed.
    pub fn locate(self, pass_len: f64, elapsed: f64) -> (usize, f64) {
        let pass_len = pass_len.max(0.001);
        if elapsed < 0.0 || self.is_off() {
            return if elapsed < 0.0 { (0, elapsed) } else { ((elapsed / pass_len) as usize, elapsed % pass_len) };
        }
        let (mut left, mut pass) = (elapsed, 0usize);
        loop {
            let f = self.factor(pass);
            let real_len = pass_len / f;
            if f >= Self::MAX_FACTOR {
                // From here on every pass is the same length.
                let whole = (left / real_len).floor();
                return (pass + whole as usize, (left - whole * real_len) * f);
            }
            if left < real_len {
                return (pass, left * f);
            }
            left -= real_len;
            pass += 1;
        }
    }

    /// `elapsed` real seconds into the run, as seconds at the starting tempo: what a clock or a
    /// playhead that does not know about the ramp would show.
    pub fn base_seconds(self, pass_len: f64, elapsed: f64) -> f64 {
        let (pass, into) = self.locate(pass_len, elapsed);
        if elapsed < 0.0 {
            return elapsed;
        }
        pass as f64 * pass_len.max(0.001) + into
    }

    /// When pass `pass` starts, in real seconds after the start of the first.
    fn start_of(self, pass_len: f64, pass: usize) -> f64 {
        (0..pass).map(|p| pass_len / self.factor(p)).sum()
    }
}

/// What is fixed for one run of the player. Everything that can change while
/// it plays is in the shared [`Control`] instead.
#[derive(Clone, Debug)]
pub struct PlayOptions {
    /// Play the drums through the sound card.
    pub audio: bool,
    /// The open sound output; `None` when there is none to play through.
    pub engine: Option<EngineHandle>,
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
    /// Raises the tempo as the passes go by (only with looping on).
    pub ramp: Ramp,
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

/// Where the sound goes. The real one drives the devices; tests record calls. `when` is the moment
/// the sound is meant to start: the sink may be told earlier than that (see [`Sink::lookahead`]).
pub trait Sink {
    fn audio_click(&mut self, level: ClickLevel, volume: f32, when: Instant);
    fn midi_click(&mut self, level: ClickLevel, when: Instant);
    /// `gain` includes the master volume, the instrument gain and the velocity.
    fn audio_drum(&mut self, lane: Lane, gain: f32, when: Instant);
    fn midi_drum(&mut self, note: u8, velocity: u8, when: Instant);

    /// How long before its time a cue is handed to the sink: the audio engine places a sound at an
    /// exact frame only if it is told ahead. Zero when nothing needs that.
    fn lookahead(&self) -> Duration {
        Duration::ZERO
    }

    /// When the sink next has something of its own to send (MIDI that was held back to its time).
    fn next_due(&self) -> Option<Instant> {
        None
    }

    /// Sends what has come due at `now`.
    fn flush(&mut self, _now: Instant) {}
}

/// Decides what a cue sounds like, from the options and the live controls, and hands it to the
/// sink to start at `when`.
fn dispatch(cue: &Cue, when: Instant, opts: &PlayOptions, control: &Control, sink: &mut dyn Sink) {
    match cue.sound {
        Sound::Click { level } => {
            // The count-in (before zero) always sounds; the click during the music can be switched off.
            if cue.t >= 0.0 && !control.click() {
                return;
            }
            if opts.click_midi {
                sink.midi_click(level, when);
            }
            if opts.click_audio {
                sink.audio_click(level, control.click_volume(), when);
            }
        }
        Sound::Drum { lane, velocity } => {
            let gain = control.lane_gain(lane);
            if !control.notes() || gain <= 0.0 {
                return;
            }
            if opts.midi_drums {
                if let Some(note) = opts.drum_notes.get(&lane) {
                    sink.midi_drum(*note, (velocity as f32 * gain).round().clamp(1.0, 127.0) as u8, when);
                }
            }
            if opts.audio {
                sink.audio_drum(lane, control.volume() * gain * velocity_gain(velocity), when);
            }
        }
    }
}

/// The cues of one pass. Only the first pass has the count-in.
fn pass_cues(cues: &[Cue], pass: usize) -> impl Iterator<Item = &Cue> {
    cues.iter().filter(move |c| pass == 0 || c.t >= 0.0)
}

/// Waits until `wake`, sending what the sink has to send of its own on the way. `false` if `stop`
/// was set first.
fn wait_for(wake: Instant, stop: &AtomicBool, sink: &mut dyn Sink) -> bool {
    loop {
        let next = sink.next_due().map_or(wake, |due| due.min(wake));
        if !wait_until(next, stop) {
            return false;
        }
        sink.flush(Instant::now());
        if next >= wake {
            return true;
        }
    }
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
        let offset = opts.ramp.start_of(pass_len, pass);
        let squeeze = 1.0 / opts.ramp.factor(pass);
        for cue in pass_cues(cues, pass) {
            // The count-in (before zero, in the first pass only) is never squeezed.
            let t = if cue.t < 0.0 { cue.t } else { cue.t * squeeze };
            let Some(target) = at(zero, t + offset) else { continue };
            let wake = target.checked_sub(sink.lookahead()).unwrap_or(target);
            if !wait_for(wake, stop, sink) {
                break 'passes;
            }
            if Instant::now().saturating_duration_since(target) > MAX_LATE {
                continue;
            }
            dispatch(cue, target, opts, control, sink);
        }
        if let Some(end) = at(zero, offset + pass_len * squeeze) {
            // The next pass starts at `end` and its first cues are handed over ahead of their time
            // like any other, so when the music goes on the wait ends one look-ahead early. (Looping
            // switched off in that last moment still lets one more pass begin.)
            let hand_over = end.checked_sub(sink.lookahead()).unwrap_or(end);
            if !wait_for(hand_over, stop, sink) {
                break;
            }
            if !control.looping() {
                wait_for(end, stop, sink);
                break;
            }
        } else if !control.looping() {
            break;
        }
        pass += 1;
    }
    control.set_music_done(true);

    // Let the last notes ring out before the stream is dropped.
    let end = Instant::now() + tail;
    while Instant::now() < end && !stop.load(Ordering::Relaxed) {
        sink.flush(Instant::now());
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

/// The choke group of a lane (the bit a later sound can choke it by) and the groups it chokes itself.
fn choke_groups(lane: Lane) -> (u8, u32) {
    let group = if lane.can_be_choked() { Lane::ALL.iter().position(|l| *l == lane).map_or(NO_GROUP, |i| i as u8) } else { NO_GROUP };
    let chokes = Lane::ALL.iter().enumerate().filter(|(_, other)| lane.chokes(**other)).fold(0u32, |mask, (i, _)| mask | 1 << i);
    (group, chokes)
}

/// A note held back until its time, so MIDI keeps its exact timing while audio is handed over early.
struct HeldNote {
    when: Instant,
    channel: u8,
    note: u8,
    velocity: u8,
}

/// The real output: the audio engine and the MIDI port.
struct DeviceSink {
    engine: Option<EngineHandle>,
    run: u32,
    /// The kit at the engine's rate, one sound per lane the cues use.
    kit: BTreeMap<Lane, Arc<[f32]>>,
    clicks: Option<ClickBank>,
    midi: Option<MidiOutputConnection>,
    held: Vec<HeldNote>,
    click_note: u8,
    click_channel: u8,
    drum_channel: u8,
}

impl DeviceSink {
    fn midi_note(&mut self, when: Instant, channel: u8, note: u8, velocity: u8) {
        if self.midi.is_none() {
            return;
        }
        if self.lookahead().is_zero() || when <= Instant::now() {
            if let Some(conn) = self.midi.as_mut() {
                note_on_off(conn, channel, note, velocity);
            }
        } else {
            self.held.push(HeldNote { when, channel, note, velocity });
        }
    }
}

impl Sink for DeviceSink {
    fn audio_click(&mut self, level: ClickLevel, volume: f32, when: Instant) {
        if let (Some(engine), Some(bank)) = (self.engine.as_ref(), self.clicks.as_ref()) {
            let (sound, gain) = bank.get(level, volume);
            let _ = engine.play(self.run, when, sound, gain, NO_GROUP, 0);
        }
    }

    fn midi_click(&mut self, level: ClickLevel, when: Instant) {
        self.midi_note(when, self.click_channel, self.click_note, level.velocity());
    }

    fn audio_drum(&mut self, lane: Lane, gain: f32, when: Instant) {
        if let (Some(engine), Some(sound)) = (self.engine.as_ref(), self.kit.get(&lane)) {
            let (group, chokes) = choke_groups(lane);
            let _ = engine.play(self.run, when, sound, gain, group, chokes);
        }
    }

    fn midi_drum(&mut self, note: u8, velocity: u8, when: Instant) {
        self.midi_note(when, self.drum_channel, note, velocity);
    }

    fn lookahead(&self) -> Duration {
        if self.engine.is_some() {
            AUDIO_LOOKAHEAD
        } else {
            Duration::ZERO
        }
    }

    fn next_due(&self) -> Option<Instant> {
        self.held.iter().map(|h| h.when).min()
    }

    fn flush(&mut self, now: Instant) {
        let Some(conn) = self.midi.as_mut() else { return };
        self.held.retain(|h| {
            if h.when > now {
                return true;
            }
            note_on_off(conn, h.channel, h.note, h.velocity);
            false
        });
    }
}

impl Drop for DeviceSink {
    /// A stop cuts the sound at once: the engine fades out what this run has playing.
    fn drop(&mut self) {
        if let Some(engine) = self.engine.as_ref() {
            let _ = engine.cancel_run(self.run);
        }
    }
}

fn run(zero: Instant, cues: Vec<Cue>, pass_len: f64, opts: PlayOptions, control: Arc<Control>, stop: Arc<AtomicBool>, errors: ErrorLog) {
    let lanes = cues.iter().filter_map(|c| match c.sound {
        Sound::Drum { lane, .. } if opts.audio => Some(lane),
        _ => None,
    });
    let (kit, warnings) = Kit::load(lanes.clone(), opts.sounds_dir.as_deref());
    for w in warnings {
        errors.push(w);
    }

    let has_clicks = cues.iter().any(|c| matches!(c.sound, Sound::Click { .. }));
    let engine = opts.engine.clone().filter(|_| opts.audio || (opts.click_audio && has_clicks));
    let (mut sounds, mut clicks, mut run_id) = (BTreeMap::new(), None, 0);
    if let Some(engine) = engine.as_ref() {
        let rate = engine.shared().rate();
        for lane in lanes {
            if let Some(sample) = kit.sample(lane) {
                sounds.entry(lane).or_insert_with(|| resample(sample, SAMPLE_RATE, rate).into());
            }
        }
        clicks = Some(ClickBank::new(rate));
        run_id = engine.new_run();
        if !engine.wait_ready(Duration::from_millis(500)) {
            errors.push("The sound output did not start in time, so the first sounds may be late.".into());
        }
    }
    let mut midi = None;
    if let Some(port) = opts.midi_port.as_deref() {
        match crate::midi::connect_output(port) {
            Ok(conn) => midi = Some(conn),
            Err(e) => errors.push(e.to_string()),
        }
    }

    let mut sink = DeviceSink {
        engine,
        run: run_id,
        kit: sounds,
        clicks,
        midi,
        held: Vec::new(),
        click_note: opts.click_note,
        click_channel: opts.click_channel.clamp(1, 16) - 1,
        drum_channel: opts.drum_channel.clamp(1, 16) - 1,
    };
    run_with(zero, &cues, pass_len, &opts, &control, &stop, &mut sink, TAIL);
}

#[cfg(test)]
mod tests;
