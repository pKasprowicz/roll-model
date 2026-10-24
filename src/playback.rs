//! Playing a score back ("Listen") so the drummer can hear what to play.
//! One thread schedules the notes and the metronome clicks with the same
//! sub-millisecond timing as `metronome.rs`, and sends them to the sound card
//! (software drum kit from `synth.rs`) and/or to a MIDI output (the module's
//! own sounds). A single thread owns the audio stream and the MIDI connection,
//! so the click and the drums can share one MIDI port.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use midir::MidiOutputConnection;

use crate::analysis::Expected;
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
    Click { accent: bool },
    Drum { lane: Lane, velocity: u8 },
}

/// Something to play `t` seconds after the player's zero (negative = count-in).
#[derive(Clone, Copy, Debug)]
pub struct Cue {
    pub t: f64,
    pub sound: Sound,
}

#[derive(Clone, Debug)]
pub struct PlayOptions {
    /// Play the drums through the sound card.
    pub audio: bool,
    /// 0..=1
    pub volume: f32,
    pub click_audio: bool,
    pub click_volume: f32,
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
        .map(|&(t, accent)| Cue { t, sound: Sound::Click { accent } })
        .chain(expected.iter().map(|e| Cue { t: e.t, sound: Sound::Drum { lane: e.lane, velocity: velocity_for(e) } }))
        .collect();
    v.sort_by(|a, b| a.t.total_cmp(&b.t)); // stable
    v
}

pub struct Player {
    stop: Arc<AtomicBool>,
    errors: Arc<Mutex<Vec<String>>>,
    handle: Option<JoinHandle<()>>,
}

impl Player {
    /// Starts a thread that plays `cues` relative to `zero`.
    pub fn start(zero: Instant, cues: Vec<Cue>, opts: PlayOptions) -> Player {
        let stop = Arc::new(AtomicBool::new(false));
        let errors = Arc::new(Mutex::new(Vec::new()));
        let (stop2, errors2) = (stop.clone(), errors.clone());
        let handle = std::thread::Builder::new()
            .name("playback".into())
            .spawn(move || run(zero, cues, opts, stop2, errors2))
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
        self.errors.lock().map(|mut e| std::mem::take(&mut *e)).unwrap_or_default()
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

#[cfg(feature = "audio")]
fn audio_ready(audio: &Audio) -> bool {
    audio.is_some()
}

#[cfg(not(feature = "audio"))]
fn audio_ready(_audio: &Audio) -> bool {
    false
}

/// One drum sample, shared between the hits of a lane, played at `gain`.
#[cfg(feature = "audio")]
struct Voice {
    data: Arc<Vec<f32>>,
    pos: usize,
    gain: f32,
}

#[cfg(feature = "audio")]
impl Iterator for Voice {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let s = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(s * self.gain)
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
fn play_voice(audio: &Audio, sample: &Arc<Vec<f32>>, gain: f32) {
    if let Some((_stream, handle)) = audio.as_ref() {
        let _ = handle.play_raw(Voice { data: sample.clone(), pos: 0, gain });
    }
}

#[cfg(not(feature = "audio"))]
fn play_voice(_audio: &Audio, _sample: &Arc<Vec<f32>>, _gain: f32) {}

fn run(zero: Instant, cues: Vec<Cue>, opts: PlayOptions, stop: Arc<AtomicBool>, errors: Arc<Mutex<Vec<String>>>) {
    let fail = |msg: String| {
        if let Ok(mut e) = errors.lock() {
            e.push(msg);
        }
    };
    let has_clicks = cues.iter().any(|c| matches!(c.sound, Sound::Click { .. }));
    let kit = Kit::for_lanes(cues.iter().filter_map(|c| match c.sound {
        Sound::Drum { lane, .. } if opts.audio => Some(lane),
        _ => None,
    }));

    let want_audio = opts.audio || (opts.click_audio && has_clicks);
    #[allow(clippy::let_unit_value)]
    let audio = open_audio(want_audio);
    if want_audio && !audio_ready(&audio) {
        fail("No sound output could be opened, so the score can't be played through the computer.".into());
    }
    let mut midi = None;
    if let Some(port) = opts.midi_port.as_deref() {
        match crate::midi::connect_output(port) {
            Ok(conn) => midi = Some(conn),
            Err(e) => fail(e),
        }
    }
    let drum_channel = opts.drum_channel.clamp(1, 16) - 1;
    let click_channel = opts.click_channel.clamp(1, 16) - 1;

    for cue in cues {
        let Some(target) = at(zero, cue.t) else { continue };
        if !wait_until(target, &stop) {
            break;
        }
        if Instant::now().saturating_duration_since(target) > MAX_LATE {
            continue;
        }
        match cue.sound {
            Sound::Click { accent } => {
                if let (true, Some(conn)) = (opts.click_midi, midi.as_mut()) {
                    note_on_off(conn, click_channel, opts.click_note, if accent { 127 } else { 90 });
                }
                if opts.click_audio {
                    click_audio(&audio, accent, opts.click_volume);
                }
            }
            Sound::Drum { lane, velocity } => {
                if let (true, Some(conn), Some(note)) = (opts.midi_drums, midi.as_mut(), opts.drum_notes.get(&lane)) {
                    note_on_off(conn, drum_channel, *note, velocity);
                }
                if let Some(sample) = kit.sample(lane) {
                    play_voice(&audio, sample, opts.volume * velocity_gain(velocity));
                }
            }
        }
    }

    // Let the last notes ring out before the stream is dropped.
    let end = Instant::now() + TAIL;
    while Instant::now() < end && !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::expected_notes;
    use crate::musicxml::{parse_musicxml, ParseOptions};
    use crate::synth::SAMPLE_RATE;

    fn note(lane: Lane, t: f64) -> Expected {
        Expected { note_idx: 0, t, lane, accent: false, ghost: false, grace: false }
    }

    #[test]
    fn cues_are_ordered_and_clicks_lead_at_ties() {
        let mut ghost = note(Lane::Snare, 1.0);
        ghost.ghost = true;
        let mut accent = note(Lane::Crash1, 0.0);
        accent.accent = true;
        let c = cues(&[ghost, accent, note(Lane::Kick, 0.0)], &[(-1.0, true), (0.0, true), (1.0, false)]);
        let times: Vec<f64> = c.iter().map(|c| c.t).collect();
        assert_eq!(times, vec![-1.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
        assert_eq!(c[1].sound, Sound::Click { accent: true }); // the click, then the two notes
        assert_eq!(c[2].sound, Sound::Drum { lane: Lane::Crash1, velocity: 120 });
        assert_eq!(c[3].sound, Sound::Drum { lane: Lane::Kick, velocity: 90 });
        assert_eq!(c[4].sound, Sound::Click { accent: false });
        assert_eq!(c[5].sound, Sound::Drum { lane: Lane::Snare, velocity: 45 });
    }

    /// Mixes the drum cues into a buffer the way the player would, so the
    /// sample, the timing and the velocity handling can be checked without a
    /// sound card.
    fn mix(cues: &[Cue], lead_sec: f64, volume: f32) -> Vec<f32> {
        let kit = Kit::for_lanes(cues.iter().filter_map(|c| match c.sound {
            Sound::Drum { lane, .. } => Some(lane),
            _ => None,
        }));
        let mut out = Vec::new();
        for cue in cues {
            let Sound::Drum { lane, velocity } = cue.sound else { continue };
            let start = ((cue.t + lead_sec) * SAMPLE_RATE as f64).round() as usize;
            let sample = kit.sample(lane).unwrap();
            if out.len() < start + sample.len() {
                out.resize(start + sample.len(), 0.0);
            }
            for (i, s) in sample.iter().enumerate() {
                out[start + i] += s * volume * velocity_gain(velocity);
            }
        }
        out
    }

    #[test]
    fn sample_score_renders_a_note_at_every_onset() {
        let score = parse_musicxml(include_str!("../examples/basic_rock_beat.musicxml"), &ParseOptions::default()).unwrap();
        let expected = expected_notes(&score, 0, score.measures.len() - 1, 1.0);
        let clicks = score.click_track(0, score.measures.len() - 1, 1.0, 4, true);
        let all = cues(&expected, &clicks);
        let lead = 2.4; // one count-in bar at 100 BPM
        let out = mix(&all, lead, 1.0);

        // Nothing but clicks (which `mix` skips) before the first note.
        let first = (lead * SAMPLE_RATE as f64) as usize;
        assert!(out[..first].iter().all(|s| *s == 0.0));
        assert!(out.iter().all(|s| s.abs() <= 1.0), "the mix clips at full volume");

        // Every note starts audibly within 5 ms of its time.
        let window = (0.005 * SAMPLE_RATE as f64) as usize;
        for e in &expected {
            let at = ((e.t + lead) * SAMPLE_RATE as f64).round() as usize;
            let peak = out[at..at + window].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.02, "silent at {:.3}s for {:?}", e.t, e.lane);
        }
        // And the take is not far longer than the music plus the last ring-out.
        let last = expected.iter().map(|e| e.t).fold(0.0, f64::max) + lead;
        assert!(out.len() as f64 / (SAMPLE_RATE as f64) < last + 2.5);
    }

    #[test]
    fn accents_are_louder_than_ghost_notes() {
        let mut accent = note(Lane::Snare, 0.0);
        accent.accent = true;
        let mut ghost = note(Lane::Snare, 0.0);
        ghost.ghost = true;
        let loud = mix(&cues(&[accent], &[]), 0.0, 1.0);
        let soft = mix(&cues(&[ghost], &[]), 0.0, 1.0);
        let peak = |v: &[f32]| v.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak(&loud) > 2.0 * peak(&soft), "{} vs {}", peak(&loud), peak(&soft));
    }
}
