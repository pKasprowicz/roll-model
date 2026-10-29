//! MIDI input capture (drum module -> app) and MIDI output (metronome click).

use std::sync::{Arc, Mutex};
use std::time::Instant;

use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

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
    OpenInput { port: String, reason: String },
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

pub fn connect_input(wanted: &str, queue: EventQueue) -> Result<InputHandle, MidiError> {
    let mut mi = MidiInput::new("roll-model").map_err(|e| MidiError::Backend(e.to_string()))?;
    // Drop SysEx, clock and active-sensing traffic.
    mi.ignore(Ignore::All);
    let ports = mi.ports();
    let names: Vec<String> = ports.iter().map(|p| mi.port_name(p).unwrap_or_default()).collect();
    let idx = pick(names.iter().map(|s| s.as_str()).enumerate(), wanted)
        .ok_or_else(|| MidiError::InputNotFound(wanted.to_string()))?;
    let port = ports[idx].clone();
    let port_name = names[idx].clone();
    let mut pedal: Option<u8> = None;
    let conn = mi
        .connect(
            &port,
            "roll-model-in",
            move |_stamp, msg, _| {
                let at = Instant::now();
                // Note-on with velocity > 0 on any channel, and the hi-hat pedal
                // (CC#4), which is remembered and attached to the hits that follow.
                // Note-offs and aftertouch (cymbal chokes) are not needed.
                if msg.len() >= 3 && msg[0] & 0xF0 == 0xB0 && msg[1] == HIHAT_CC {
                    pedal = Some(msg[2]);
                } else if msg.len() >= 3 && msg[0] & 0xF0 == 0x90 && msg[2] > 0 {
                    if let Ok(mut q) = queue.lock() {
                        q.push(RawEvent { at, note: msg[1], velocity: msg[2], channel: (msg[0] & 0x0F) + 1, hihat: pedal });
                    }
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

/// Most messages kept between two frames of the window; a runaway device cannot fill memory.
const MAX_QUEUED: usize = 2000;

/// Opens the input port whose name matches `wanted` and, if there is one with
/// the same name, its output port too (for the lights).
pub fn connect_controller(wanted: &str, queue: MessageQueue) -> Result<ControllerHandle, MidiError> {
    let mut mi = MidiInput::new("roll-model-controller").map_err(|e| MidiError::Backend(e.to_string()))?;
    mi.ignore(Ignore::All);
    let ports = mi.ports();
    let names: Vec<String> = ports.iter().map(|p| mi.port_name(p).unwrap_or_default()).collect();
    let idx = pick(names.iter().map(|s| s.as_str()).enumerate(), wanted)
        .ok_or_else(|| MidiError::InputNotFound(wanted.to_string()))?;
    let port_name = names[idx].clone();
    let conn = mi
        .connect(
            &ports[idx],
            "roll-model-controller-in",
            move |_stamp, msg, _| {
                if let Ok(mut q) = queue.lock() {
                    if q.len() < MAX_QUEUED {
                        q.push(msg.to_vec());
                    }
                }
            },
            (),
        )
        .map_err(|e| MidiError::OpenInput { port: port_name.clone(), reason: e.to_string() })?;
    let out = MidiOutput::new("roll-model-controller").ok().and_then(|mo| {
        let ports = mo.ports();
        let names: Vec<String> = ports.iter().map(|p| mo.port_name(p).unwrap_or_default()).collect();
        let i = pick(names.iter().map(|s| s.as_str()).enumerate(), &port_name)?;
        mo.connect(&ports[i], "roll-model-controller-out").ok()
    });
    Ok(ControllerHandle { _conn: conn, out, port_name })
}

pub fn connect_output(wanted: &str) -> Result<MidiOutputConnection, MidiError> {
    let mo = MidiOutput::new("roll-model").map_err(|e| MidiError::Backend(e.to_string()))?;
    let ports = mo.ports();
    let names: Vec<String> = ports.iter().map(|p| mo.port_name(p).unwrap_or_default()).collect();
    let idx = pick(names.iter().map(|s| s.as_str()).enumerate(), wanted)
        .ok_or_else(|| MidiError::OutputNotFound(wanted.to_string()))?;
    let port = ports[idx].clone();
    mo.connect(&port, "roll-model-out")
        .map_err(|e| MidiError::OpenOutput(e.to_string()))
}

/// Standard note name, e.g. 36 -> "C2" (Roland/Yamaha convention, C4 = 60).
pub fn note_name(n: u8) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", NAMES[(n % 12) as usize], (n as i32 / 12) - 1)
}
