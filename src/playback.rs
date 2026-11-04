//! Playing a score back ("Listen"), and the free metronome, so the drummer can
//! hear what to play. One thread schedules the notes and the metronome clicks
//! with the same sub-millisecond timing as `metronome.rs`, and sends them to
//! the sound card (software drum kit from `synth.rs`) and/or to a MIDI output
//! (the module's own sounds). A single thread owns the audio stream and the
//! MIDI connection, so the click and the drums can share one MIDI port.
//!
//! Settings that can change while playing (click on/off, notes on/off,
//! volumes, per-instrument gain, the number of passes) live in a shared [`SoundSwitches`]. What
//! gets played for a cue is decided by [`dispatch`], which talks to a [`Sink`];
//! the real sink drives the devices, the tests use a fake one.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use midir::MidiOutputConnection;

use crate::audio::click::ClickBank;
use crate::audio::mixer::NO_GROUP;
use crate::audio::resample::resample;
use crate::audio::EngineHandle;
use crate::control::{ErrorLog, SoundSwitches};
use crate::drums::Lane;
use crate::metronome::{at, wait_until, AUDIO_LOOKAHEAD};
use crate::practice::{ClickLevel, Ramp};
use crate::synth::{velocity_gain, Kit, SAMPLE_RATE};

pub use crate::clock::{clock_cues, metronome_clock, with_clock, ClockMsg, ClockOptions};
pub use crate::cues::{cues, metronome_cues, Cue, Sound};

/// A cue whose turn comes later than this (the thread stalled, or the sound
/// device took long to open) is dropped instead of being played as a burst.
const MAX_LATE: Duration = Duration::from_millis(250);
/// How long the last notes may ring out after the final cue (the longest
/// cymbal sample is two seconds).
const TAIL: Duration = Duration::from_millis(2200);

/// What is fixed for one run of the player. Everything that can change while
/// it plays is in the shared [`SoundSwitches`] instead.
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
    /// How much later than its time a MIDI note is sent when the same sound also goes through the sound
    /// card: the output delay, so that the two are heard together. Zero when only MIDI sounds.
    pub midi_delay: Duration,
    /// MIDI note that sounds each lane on the module.
    pub drum_notes: BTreeMap<Lane, u8>,
    /// Raises the tempo as the passes go by (only with more than one pass).
    pub ramp: Ramp,
    /// Send the clock the cues carry to the MIDI output; `None` when it is not wanted.
    pub clock: Option<ClockOptions>,
}

/// Where the sound goes. The real one drives the devices; tests record calls. `when` is the moment
/// the sound is meant to start: the sink may be told earlier than that (see [`Sink::lookahead`]).
pub trait Sink {
    fn audio_click(&mut self, level: ClickLevel, volume: f32, when: Instant);
    fn midi_click(&mut self, level: ClickLevel, when: Instant);
    /// `gain` includes the master volume, the instrument gain and the velocity.
    fn audio_drum(&mut self, lane: Lane, gain: f32, when: Instant);
    fn midi_drum(&mut self, note: u8, velocity: u8, when: Instant);
    fn midi_clock(&mut self, msg: ClockMsg, when: Instant);

    /// The music is over, or was stopped: the clock, if it ran, is stopped too.
    fn music_ended(&mut self) {}

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
fn dispatch(cue: &Cue, when: Instant, opts: &PlayOptions, control: &SoundSwitches, sink: &mut dyn Sink) {
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
        Sound::Clock(msg) => {
            if opts.clock.is_some() {
                sink.midi_clock(msg, when);
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
fn wait_for(wake: Instant, stop: &AtomicBool, sink: &mut dyn Sink, clock: &dyn Clock) -> bool {
    loop {
        let next = sink.next_due().map_or(wake, |due| due.min(wake));
        if !clock.wait_until(next, stop) {
            return false;
        }
        sink.flush(clock.now());
        if next >= wake {
            return true;
        }
    }
}

/// Plays `cues` relative to `zero`, pass after pass while `control.passes()` has more
/// stays on (a pass is `pass_len` seconds long). Sets `control.music_done()`
/// when the last pass has reached its end, then lets the tail ring out.
/// What one run of the scheduler plays and how it is controlled.
#[derive(Clone, Copy)]
struct Run<'a> {
    zero: Instant,
    cues: &'a [Cue],
    pass_len: f64,
    opts: &'a PlayOptions,
    control: &'a SoundSwitches,
    stop: &'a AtomicBool,
    /// How long the last sounds ring out after the music.
    tail: Duration,
}

/// Time as the scheduler sees it. The real clock reads the system clock and sleeps; a test brings
/// one that jumps, so the timing of the loop is checked without waiting for it.
pub trait Clock {
    fn now(&self) -> Instant;
    /// Waits until `t`; `false` if `stop` was set first.
    fn wait_until(&self, t: Instant, stop: &AtomicBool) -> bool;
    fn sleep(&self, duration: Duration);
}

/// The system clock.
struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn wait_until(&self, t: Instant, stop: &AtomicBool) -> bool {
        wait_until(t, stop)
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

fn run_with(run: &Run, sink: &mut dyn Sink, clock: &dyn Clock) {
    let Run { zero, cues, pass_len, opts, control, stop, tail } = *run;
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
            if !wait_for(wake, stop, sink, clock) {
                break 'passes;
            }
            if clock.now().saturating_duration_since(target) > MAX_LATE {
                continue;
            }
            dispatch(cue, target, opts, control, sink);
        }
        if let Some(end) = at(zero, offset + pass_len * squeeze) {
            // The next pass starts at `end` and its first cues are handed over ahead of their time
            // like any other, so when the music goes on the wait ends one look-ahead early. (Looping
            // switched off in that last moment still lets one more pass begin.)
            let hand_over = end.checked_sub(sink.lookahead()).unwrap_or(end);
            if !wait_for(hand_over, stop, sink, clock) {
                break;
            }
            if !control.passes().more_after(pass) {
                wait_for(end, stop, sink, clock);
                break;
            }
        } else if !control.passes().more_after(pass) {
            break;
        }
        pass += 1;
    }
    control.set_music_done(true);
    sink.music_ended();

    // Let the last notes ring out before the stream is dropped.
    let end = clock.now() + tail;
    while clock.now() < end && !stop.load(Ordering::Relaxed) {
        sink.flush(clock.now());
        clock.sleep(Duration::from_millis(20));
    }
}

pub struct Player {
    stop: Arc<AtomicBool>,
    errors: ErrorLog,
    handle: Option<JoinHandle<()>>,
}

impl Player {
    /// Starts a thread that plays `cues` relative to `zero`; one pass is
    /// `pass_len` seconds long and repeats as `control.passes()` says.
    pub fn start(zero: Instant, cues: Vec<Cue>, pass_len: f64, opts: PlayOptions, control: Arc<SoundSwitches>) -> Player {
        control.set_music_done(false);
        let stop = Arc::new(AtomicBool::new(false));
        let errors = ErrorLog::default();
        let (stop2, errors2) = (stop.clone(), errors.clone());
        let handle = std::thread::Builder::new()
            .name("playback".into())
            .spawn(move || run(zero, cues, pass_len, opts, control, stop2, errors2))
            .map_err(|e| errors.push(format!("Could not start the playback thread: {e}")))
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
        self.handle.as_ref().is_none_or(|h| h.is_finished())
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

fn note_on_off(conn: &mut MidiOutputConnection, errors: &ErrorLog, channel: u8, note: u8, velocity: u8) {
    for message in [[0x90 | channel, note, velocity.clamp(1, 127)], [0x80 | channel, note, 0]] {
        if let Err(e) = conn.send(&message) {
            errors.midi_failed(e);
        }
    }
}

/// The choke group of a lane (the bit a later sound can choke it by) and the groups it chokes itself.
fn choke_groups(lane: Lane) -> (u8, u32) {
    let group = if lane.can_be_choked() { Lane::ALL.iter().position(|l| *l == lane).map_or(NO_GROUP, |i| i as u8) } else { NO_GROUP };
    let chokes = Lane::ALL.iter().enumerate().filter(|(_, other)| lane.chokes(**other)).fold(0u32, |mask, (i, _)| mask | 1 << i);
    (group, chokes)
}

/// What a held message is.
enum Held {
    Note { channel: u8, note: u8, velocity: u8 },
    Clock(ClockMsg),
}

/// A message held back until its time, so MIDI keeps its exact timing while audio is handed over early.
struct HeldMessage {
    when: Instant,
    what: Held,
}

fn send_clock(conn: &mut MidiOutputConnection, errors: &ErrorLog, msg: ClockMsg) {
    let (bytes, len) = msg.bytes();
    if let Err(e) = conn.send(&bytes[..len]) {
        errors.midi_failed(e);
    }
}

fn send_held(conn: &mut MidiOutputConnection, errors: &ErrorLog, what: &Held) {
    match *what {
        Held::Note { channel, note, velocity } => note_on_off(conn, errors, channel, note, velocity),
        Held::Clock(msg) => send_clock(conn, errors, msg),
    }
}

/// The real output: the audio engine and the MIDI port.
struct DeviceSink {
    engine: Option<EngineHandle>,
    run: u32,
    /// The kit at the engine's rate, one sound per lane the cues use.
    kit: BTreeMap<Lane, Arc<[f32]>>,
    clicks: Option<ClickBank>,
    midi: Option<MidiOutputConnection>,
    held: Vec<HeldMessage>,
    /// A Start or Continue was sent and no Stop yet.
    clock_running: bool,
    click_note: u8,
    click_channel: u8,
    drum_channel: u8,
    /// How much later the MIDI click and the MIDI drums go out (see `PlayOptions::midi_delay`).
    click_delay: Duration,
    drum_delay: Duration,
    errors: ErrorLog,
}

/// The delays of the MIDI click and of the MIDI drums: `opts.midi_delay` for each that is also played
/// through the sound card in this run, otherwise none.
fn midi_delays(opts: &PlayOptions, has_clicks: bool) -> (Duration, Duration) {
    let delay = |also_audio: bool| if also_audio { opts.midi_delay } else { Duration::ZERO };
    (delay(opts.click_audio && has_clicks && opts.engine.is_some()), delay(opts.audio && opts.engine.is_some()))
}

impl DeviceSink {
    /// Sends `what` at `when` (after `delay`): at once if its time has come or nothing is handed over
    /// early, otherwise when its time comes.
    fn midi_message(&mut self, when: Instant, delay: Duration, what: Held) {
        if self.midi.is_none() {
            return;
        }
        let when = when + delay;
        if self.lookahead().is_zero() || when <= Instant::now() {
            if let Some(conn) = self.midi.as_mut() {
                send_held(conn, &self.errors, &what);
            }
        } else {
            self.held.push(HeldMessage { when, what });
        }
    }

    fn midi_note(&mut self, when: Instant, delay: Duration, channel: u8, note: u8, velocity: u8) {
        self.midi_message(when, delay, Held::Note { channel, note, velocity });
    }
}

impl Sink for DeviceSink {
    fn audio_click(&mut self, level: ClickLevel, volume: f32, when: Instant) {
        if let (Some(engine), Some(bank)) = (self.engine.as_ref(), self.clicks.as_ref()) {
            let (sound, gain) = bank.get(level, volume);
            if let Err(e) = engine.play(self.run, when, sound, gain, NO_GROUP, 0) {
                self.errors.play_failed(e);
            }
        }
    }

    fn midi_click(&mut self, level: ClickLevel, when: Instant) {
        self.midi_note(when, self.click_delay, self.click_channel, self.click_note, level.velocity());
    }

    fn audio_drum(&mut self, lane: Lane, gain: f32, when: Instant) {
        if let (Some(engine), Some(sound)) = (self.engine.as_ref(), self.kit.get(&lane)) {
            let (group, chokes) = choke_groups(lane);
            if let Err(e) = engine.play(self.run, when, sound, gain, group, chokes) {
                self.errors.play_failed(e);
            }
        }
    }

    fn midi_drum(&mut self, note: u8, velocity: u8, when: Instant) {
        self.midi_note(when, self.drum_delay, self.drum_channel, note, velocity);
    }

    /// The clock follows the click: it is delayed as the click is, so that the device's beat is heard
    /// with the computer's.
    fn midi_clock(&mut self, msg: ClockMsg, when: Instant) {
        self.clock_running |= msg.starts();
        self.midi_message(when, self.click_delay, Held::Clock(msg));
    }

    fn music_ended(&mut self) {
        if !std::mem::take(&mut self.clock_running) {
            return;
        }
        self.held.retain(|h| !matches!(h.what, Held::Clock(_)));
        if let Some(conn) = self.midi.as_mut() {
            if let Err(e) = conn.send(&[crate::clock::STOP]) {
                self.errors.midi_failed(e);
            }
        }
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
        let errors = &self.errors;
        self.held.retain(|h| {
            if h.when > now {
                return true;
            }
            send_held(conn, errors, &h.what);
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

fn run(
    zero: Instant,
    cues: Vec<Cue>,
    pass_len: f64,
    opts: PlayOptions,
    control: Arc<SoundSwitches>,
    stop: Arc<AtomicBool>,
    errors: ErrorLog,
) {
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

    let (click_delay, drum_delay) = midi_delays(&opts, has_clicks);
    let mut sink = DeviceSink {
        engine,
        run: run_id,
        kit: sounds,
        clicks,
        midi,
        held: Vec::new(),
        clock_running: false,
        click_note: opts.click_note,
        click_channel: opts.click_channel.clamp(1, 16) - 1,
        drum_channel: opts.drum_channel.clamp(1, 16) - 1,
        click_delay,
        drum_delay,
        errors: errors.clone(),
    };
    run_with(&Run { zero, cues: &cues, pass_len, opts: &opts, control: &control, stop: &stop, tail: TAIL }, &mut sink, &SystemClock);
}

#[cfg(test)]
mod tests;
