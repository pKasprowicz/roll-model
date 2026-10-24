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
}

/// Filled by the MIDI callback thread, drained by the UI every frame.
pub type EventQueue = Arc<Mutex<Vec<RawEvent>>>;

pub struct InputHandle {
    _conn: MidiInputConnection<()>,
    pub port_name: String,
}

pub fn list_inputs() -> Result<Vec<String>, String> {
    let mi = MidiInput::new("drum-tutor-list").map_err(|e| e.to_string())?;
    Ok(mi.ports().iter().filter_map(|p| mi.port_name(p).ok()).collect())
}

pub fn list_outputs() -> Result<Vec<String>, String> {
    let mo = MidiOutput::new("drum-tutor-list").map_err(|e| e.to_string())?;
    Ok(mo.ports().iter().filter_map(|p| mo.port_name(p).ok()).collect())
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

pub fn connect_input(wanted: &str, queue: EventQueue) -> Result<InputHandle, String> {
    let mut mi = MidiInput::new("drum-tutor").map_err(|e| e.to_string())?;
    // Drop SysEx, clock and active-sensing traffic.
    mi.ignore(Ignore::All);
    let ports = mi.ports();
    let names: Vec<String> = ports.iter().map(|p| mi.port_name(p).unwrap_or_default()).collect();
    let idx = pick(names.iter().map(|s| s.as_str()).enumerate(), wanted)
        .ok_or_else(|| format!("MIDI input matching \"{wanted}\" not found"))?;
    let port = ports[idx].clone();
    let port_name = names[idx].clone();
    let conn = mi
        .connect(
            &port,
            "drum-tutor-in",
            move |_stamp, msg, _| {
                let at = Instant::now();
                // Note-on with velocity > 0 on any channel. Note-offs, hi-hat
                // pedal CC#4 and aftertouch (cymbal chokes) are not needed.
                if msg.len() >= 3 && msg[0] & 0xF0 == 0x90 && msg[2] > 0 {
                    if let Ok(mut q) = queue.lock() {
                        q.push(RawEvent { at, note: msg[1], velocity: msg[2], channel: (msg[0] & 0x0F) + 1 });
                    }
                }
            },
            (),
        )
        .map_err(|e| format!("could not open \"{port_name}\": {e}"))?;
    Ok(InputHandle { _conn: conn, port_name })
}

pub fn connect_output(wanted: &str) -> Result<MidiOutputConnection, String> {
    let mo = MidiOutput::new("drum-tutor").map_err(|e| e.to_string())?;
    let ports = mo.ports();
    let names: Vec<String> = ports.iter().map(|p| mo.port_name(p).unwrap_or_default()).collect();
    let idx = pick(names.iter().map(|s| s.as_str()).enumerate(), wanted)
        .ok_or_else(|| format!("MIDI output matching \"{wanted}\" not found"))?;
    let port = ports[idx].clone();
    mo.connect(&port, "drum-tutor-out")
        .map_err(|e| format!("could not open MIDI output: {e}"))
}

/// Standard note name, e.g. 36 -> "C2" (Roland/Yamaha convention, C4 = 60).
pub fn note_name(n: u8) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", NAMES[(n % 12) as usize], (n as i32 / 12) - 1)
}
