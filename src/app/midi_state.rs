//! The drum module's MIDI input: the ports, the connection, the events waiting, and what the
//! settings page shows of them (the monitor, the lane that waits to learn a note, the note lists
//! being typed).

use std::collections::{BTreeMap, VecDeque};

use crate::drums::Lane;
use crate::midi::{self, EventQueue, InputHandle, MidiError, RawEvent};

pub(super) struct MidiState {
    inputs: Vec<String>,
    outputs: Vec<String>,
    input: Option<InputHandle>,
    queue: EventQueue,
    monitor: VecDeque<(RawEvent, Option<Lane>)>,
    learn: Option<Lane>,
    lane_text: BTreeMap<Lane, String>,
}

impl MidiState {
    pub(super) fn new() -> MidiState {
        MidiState {
            inputs: Vec::new(),
            outputs: Vec::new(),
            input: None,
            queue: EventQueue::default(),
            monitor: VecDeque::new(),
            learn: None,
            lane_text: BTreeMap::new(),
        }
    }

    /// Looks at the ports again.
    pub(super) fn refresh_ports(&mut self) {
        self.inputs = midi::list_inputs().unwrap_or_default();
        self.outputs = midi::list_outputs().unwrap_or_default();
    }

    pub(super) fn inputs(&self) -> &[String] {
        &self.inputs
    }

    pub(super) fn outputs(&self) -> &[String] {
        &self.outputs
    }

    #[cfg(test)]
    pub(super) fn set_inputs(&mut self, inputs: Vec<String>) {
        self.inputs = inputs;
    }

    /// The port the drum module is connected on.
    pub(super) fn connected_port(&self) -> Option<&str> {
        self.input.as_ref().map(|h| h.port_name.as_str())
    }

    pub(super) fn is_connected(&self) -> bool {
        self.input.is_some()
    }

    /// Connects to the port `name`, closing the old one first (Windows ports are exclusive).
    pub(super) fn connect(&mut self, name: &str) -> Result<String, MidiError> {
        self.input = None;
        let handle = midi::connect_input(name, self.queue.clone())?;
        let port = handle.port_name.clone();
        self.input = Some(handle);
        Ok(port)
    }

    pub(super) fn disconnect(&mut self) {
        self.input = None;
    }

    /// The events that arrived since the last time.
    pub(super) fn drain(&mut self) -> Vec<RawEvent> {
        std::mem::take(&mut *crate::sync::locked(&self.queue))
    }

    /// Puts a hit on the monitor, which keeps `lines` of them, the latest first.
    pub(super) fn show_on_monitor(&mut self, ev: RawEvent, lane: Option<Lane>, lines: usize) {
        self.monitor.push_front((ev, lane));
        self.monitor.truncate(lines);
    }

    pub(super) fn monitor(&self) -> impl Iterator<Item = &(RawEvent, Option<Lane>)> {
        self.monitor.iter()
    }

    pub(super) fn latest(&self) -> Option<&(RawEvent, Option<Lane>)> {
        self.monitor.front()
    }

    /// The lane that waits for the next note to be its own.
    pub(super) fn learning(&self) -> Option<Lane> {
        self.learn
    }

    /// Waits for a note for `lane`, or stops waiting if it already did.
    pub(super) fn toggle_learning(&mut self, lane: Lane) {
        self.learn = if self.learn == Some(lane) { None } else { Some(lane) };
    }

    /// The lane that was waiting for a note, which now has none to wait for.
    pub(super) fn take_learning(&mut self) -> Option<Lane> {
        self.learn.take()
    }

    /// The text of the note list of `lane` as it is being typed; `initial` makes it the first time.
    pub(super) fn note_text(&mut self, lane: Lane, initial: impl FnOnce() -> String) -> &mut String {
        self.lane_text.entry(lane).or_insert_with(initial)
    }

    /// The typed lists are out of date (the map changed): they are made again from it.
    pub(super) fn forget_note_texts(&mut self) {
        self.lane_text.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn event(note: u8) -> RawEvent {
        RawEvent { at: Instant::now(), channel: 10, note, velocity: 90, hihat: None }
    }

    #[test]
    fn the_monitor_keeps_the_latest_lines_first() {
        let mut m = MidiState::new();
        for note in 0..5 {
            m.show_on_monitor(event(note), None, 3);
        }
        assert_eq!(m.monitor().map(|(e, _)| e.note).collect::<Vec<_>>(), [4, 3, 2]);
        assert_eq!(m.latest().map(|(e, _)| e.note), Some(4));
    }

    #[test]
    fn learning_waits_for_one_lane_and_ends_with_a_note_or_a_second_click() {
        let mut m = MidiState::new();
        assert_eq!(m.learning(), None);
        m.toggle_learning(Lane::Snare);
        assert_eq!(m.learning(), Some(Lane::Snare));
        m.toggle_learning(Lane::Kick);
        assert_eq!(m.learning(), Some(Lane::Kick), "another lane takes over");
        m.toggle_learning(Lane::Kick);
        assert_eq!(m.learning(), None, "the same lane again stops it");
        m.toggle_learning(Lane::Ride);
        assert_eq!(m.take_learning(), Some(Lane::Ride));
        assert_eq!(m.take_learning(), None);
    }

    #[test]
    fn a_typed_note_list_is_made_once_and_forgotten_when_the_map_changes() {
        let mut m = MidiState::new();
        assert_eq!(m.note_text(Lane::Snare, || "38, 40".into()), "38, 40");
        m.note_text(Lane::Snare, || unreachable!("already made")).push_str(", 37");
        assert_eq!(m.note_text(Lane::Snare, || unreachable!()), "38, 40, 37");
        m.forget_note_texts();
        assert_eq!(m.note_text(Lane::Snare, || "38".into()), "38");
    }

    #[test]
    fn nothing_is_connected_and_nothing_waits_at_the_start() {
        let mut m = MidiState::new();
        assert!(!m.is_connected() && m.connected_port().is_none());
        assert!(m.drain().is_empty());
    }
}
