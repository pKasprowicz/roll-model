//! MIDI input capture (drum module -> app) and MIDI output (metronome click).

use std::sync::{Arc, Mutex};
use std::time::Instant;

use midir::{Ignore, MidiInput, MidiInputConnection, MidiInputPort, MidiOutput, MidiOutputConnection, MidiOutputPort};

use crate::controller::{self, Msg};

/// A note-on as received from the drum module, time-stamped on arrival.
#[derive(Clone, Copy, Debug)]
pub struct RawEvent {
    pub at: Instant,
    pub note: u8,
    pub velocity: u8,
    pub channel: u8,
    /// Last hi-hat pedal position (CC#4) received before this hit, if any.
    pub hihat: Option<u8>,
}

/// Controller number of the hi-hat pedal position.
pub const HIHAT_CC: u8 = 4;

/// Filled by the MIDI callback thread, drained by the UI every frame.
pub type EventQueue = Arc<Mutex<Vec<RawEvent>>>;

/// Most events or messages kept between two frames of the window, for the drums and for the
/// controller alike; a runaway device or a window that is not repainting cannot fill memory.
const MAX_QUEUED: usize = 2000;

/// Adds to a queue the window empties once a frame; what arrives while it is full is dropped.
fn push_capped<T>(queue: &Mutex<Vec<T>>, item: T) {
    let mut q = crate::sync::locked(queue);
    if q.len() < MAX_QUEUED {
        q.push(item);
    }
}

pub struct InputHandle {
    _conn: MidiInputConnection<()>,
    pub port_name: String,
}

/// Why a MIDI port cannot be listed or opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MidiError {
    /// The MIDI system itself would not start.
    Backend(String),
    InputNotFound(String),
    OutputNotFound(String),
    OpenInput {
        port: String,
        reason: String,
    },
    OpenOutput(String),
}

impl std::fmt::Display for MidiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MidiError::Backend(e) => write!(f, "{e}"),
            MidiError::InputNotFound(wanted) => write!(f, "MIDI input matching \"{wanted}\" not found"),
            MidiError::OutputNotFound(wanted) => write!(f, "MIDI output matching \"{wanted}\" not found"),
            MidiError::OpenInput { port, reason } => write!(f, "could not open \"{port}\": {reason}"),
            MidiError::OpenOutput(reason) => write!(f, "could not open MIDI output: {reason}"),
        }
    }
}

impl std::error::Error for MidiError {}

pub fn list_inputs() -> Result<Vec<String>, MidiError> {
    let mi = MidiInput::new("roll-model-list").map_err(|e| MidiError::Backend(e.to_string()))?;
    Ok(mi.ports().iter().filter_map(|p| mi.port_name(p).ok()).collect())
}

pub fn list_outputs() -> Result<Vec<String>, MidiError> {
    let mo = MidiOutput::new("roll-model-list").map_err(|e| MidiError::Backend(e.to_string()))?;
    Ok(mo.ports().iter().filter_map(|p| mo.port_name(p).ok()).collect())
}

/// The index of the port in `names` that `wanted` (an exact name or part of one) selects.
pub fn find_port(names: &[String], wanted: &str) -> Option<usize> {
    pick(names.iter().map(|s| s.as_str()).enumerate(), wanted)
}

/// Exact name match first, then case-insensitive substring.
fn pick<'a>(names: impl Iterator<Item = (usize, &'a str)> + Clone, wanted: &str) -> Option<usize> {
    let w = wanted.trim().to_lowercase();
    if w.is_empty() {
        return None;
    }
    names
        .clone()
        .find(|(_, n)| n.to_lowercase() == w)
        .or_else(|| names.clone().find(|(_, n)| n.to_lowercase().contains(&w)))
        .map(|(i, _)| i)
}

/// A MIDI client that sees the input ports, and the one of them that `wanted` selects. SysEx,
/// clock and active-sensing traffic is dropped.
fn find_input(client: &str, wanted: &str) -> Result<(MidiInput, MidiInputPort, String), MidiError> {
    let mut mi = MidiInput::new(client).map_err(|e| MidiError::Backend(e.to_string()))?;
    mi.ignore(Ignore::All);
    let ports = mi.ports();
    let names: Vec<String> = ports.iter().map(|p| mi.port_name(p).unwrap_or_default()).collect();
    let idx = find_port(&names, wanted).ok_or_else(|| MidiError::InputNotFound(wanted.to_string()))?;
    Ok((mi, ports[idx].clone(), names[idx].clone()))
}

/// A MIDI client that sees the output ports, and the one of them that `wanted` selects.
fn find_output(client: &str, wanted: &str) -> Result<(MidiOutput, MidiOutputPort, String), MidiError> {
    let mo = MidiOutput::new(client).map_err(|e| MidiError::Backend(e.to_string()))?;
    let ports = mo.ports();
    let names: Vec<String> = ports.iter().map(|p| mo.port_name(p).unwrap_or_default()).collect();
    let idx = find_port(&names, wanted).ok_or_else(|| MidiError::OutputNotFound(wanted.to_string()))?;
    Ok((mo, ports[idx].clone(), names[idx].clone()))
}

/// What a message from the drum module is: a hit (a note-on with a velocity), which takes the
/// last position of the hi-hat pedal; or that position itself, which is remembered for the hits
/// that follow. Note-offs and aftertouch (cymbal chokes) are not needed.
fn drum_hit(bytes: &[u8], at: Instant, pedal: &mut Option<u8>) -> Option<RawEvent> {
    match controller::parse(bytes)? {
        Msg::Cc { cc: HIHAT_CC, val, .. } => {
            *pedal = Some(val);
            None
        }
        Msg::NoteOn { ch, note, vel } if vel > 0 => Some(RawEvent { at, note, velocity: vel, channel: ch, hihat: *pedal }),
        _ => None,
    }
}

pub fn connect_input(wanted: &str, queue: EventQueue) -> Result<InputHandle, MidiError> {
    let (mi, port, port_name) = find_input("roll-model", wanted)?;
    let mut pedal: Option<u8> = None;
    let conn = mi
        .connect(
            &port,
            "roll-model-in",
            move |_stamp, msg, _| {
                if let Some(hit) = drum_hit(msg, Instant::now(), &mut pedal) {
                    push_capped(&queue, hit);
                }
            },
            (),
        )
        .map_err(|e| MidiError::OpenInput { port: port_name.clone(), reason: e.to_string() })?;
    Ok(InputHandle { _conn: conn, port_name })
}

/// Raw messages from a control surface, filled by its callback thread.
pub type MessageQueue = Arc<Mutex<Vec<Vec<u8>>>>;

/// A control surface: its messages arrive in a queue; the lights are set with `send`.
pub struct ControllerHandle {
    _conn: MidiInputConnection<()>,
    out: Option<MidiOutputConnection>,
    pub port_name: String,
}

impl ControllerHandle {
    /// Whether the lights can be set (the device has an output port that could be opened).
    pub fn has_output(&self) -> bool {
        self.out.is_some()
    }

    pub fn send(&mut self, message: &[u8]) {
        if let Some(out) = self.out.as_mut() {
            let _ = out.send(message);
        }
    }
}

/// Opens the input port whose name matches `wanted` and, if there is one with
/// the same name, its output port too (for the lights).
pub fn connect_controller(wanted: &str, queue: MessageQueue) -> Result<ControllerHandle, MidiError> {
    let (mi, port, port_name) = find_input("roll-model-controller", wanted)?;
    let conn = mi
        .connect(
            &port,
            "roll-model-controller-in",
            move |_stamp, msg, _| {
                push_capped(&queue, msg.to_vec());
            },
            (),
        )
        .map_err(|e| MidiError::OpenInput { port: port_name.clone(), reason: e.to_string() })?;
    let out =
        find_output("roll-model-controller", &port_name).ok().and_then(|(mo, port, _)| mo.connect(&port, "roll-model-controller-out").ok());
    Ok(ControllerHandle { _conn: conn, out, port_name })
}

pub fn connect_output(wanted: &str) -> Result<MidiOutputConnection, MidiError> {
    let (mo, port, _) = find_output("roll-model", wanted)?;
    mo.connect(&port, "roll-model-out").map_err(|e| MidiError::OpenOutput(e.to_string()))
}

/// Standard note name, e.g. 36 -> "C2" (Roland/Yamaha convention, C4 = 60).
pub fn note_name(n: u8) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", NAMES[(n % 12) as usize], (n as i32 / 12) - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drum_hit_has_the_pedal_position_that_came_before_it() {
        let at = Instant::now();
        let mut pedal = None;
        let hit = drum_hit(&[0x99, 38, 100], at, &mut pedal).expect("a note-on is a hit");
        assert_eq!((hit.note, hit.velocity, hit.channel, hit.hihat), (38, 100, 10, None));
        assert!(drum_hit(&[0xB9, HIHAT_CC, 64], at, &mut pedal).is_none(), "the pedal is not a hit");
        assert_eq!(drum_hit(&[0x99, 42, 90], at, &mut pedal).unwrap().hihat, Some(64));
        assert!(drum_hit(&[0xB9, 7, 99], at, &mut pedal).is_none(), "another cc is nothing");
        assert_eq!(pedal, Some(64));
    }

    #[test]
    fn what_is_not_a_hit_is_dropped() {
        let at = Instant::now();
        let mut pedal = None;
        for bytes in [&[0x99, 38, 0][..], &[0x89, 38, 64], &[0xD9, 40], &[0x99, 38], &[], &[0xF8]] {
            assert!(drum_hit(bytes, at, &mut pedal).is_none(), "{bytes:?}");
        }
    }

    #[test]
    fn a_queue_nobody_empties_stops_growing_at_the_cap() {
        let queue: Mutex<Vec<u32>> = Mutex::new(Vec::new());
        for n in 0..(MAX_QUEUED as u32 + 500) {
            push_capped(&queue, n);
        }
        let q = queue.lock().unwrap();
        assert_eq!(q.len(), MAX_QUEUED);
        assert_eq!(q.last(), Some(&(MAX_QUEUED as u32 - 1)), "what came first is kept, what came after the cap is dropped");
    }

    #[test]
    fn the_drum_and_controller_queues_have_the_same_cap() {
        let events: EventQueue = Arc::new(Mutex::new(Vec::new()));
        let at = Instant::now();
        for _ in 0..MAX_QUEUED * 2 {
            push_capped(&events, RawEvent { at, note: 38, velocity: 100, channel: 10, hihat: None });
        }
        assert_eq!(events.lock().unwrap().len(), MAX_QUEUED);
    }
}
