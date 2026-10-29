//! The hardware controller (a Mackie Control compatible surface such as the
//! Behringer X-Touch Mini): the connection, what its commands do to the
//! application, the lights, and the Controller page of Settings.
//! Which control sends which command is in the preset (`controller.rs`).

mod apply;
mod editor;
mod page;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};


use super::DrumApp;
use crate::controller::{self, Action, Mapper};
use crate::midi::{self, ControllerHandle, MessageQueue};
use crate::render;

const MONITOR_LINES: usize = 14;
/// How often the list of MIDI ports is looked at to notice a controller being plugged in or pulled out.
const WATCH_EVERY: Duration = Duration::from_secs(2);

struct Link {
    handle: ControllerHandle,
    mapper: Mapper,
}

/// Everything about the controller that the application keeps.
pub(super) struct Controller {
    link: Option<Link>,
    queue: MessageQueue,
    /// The state of the connection, in words.
    message: String,
    /// Built-in presets, then the files in the controllers folder.
    presets: Vec<String>,
    /// What each of them speaks (Mackie Control or plain MIDI).
    kinds: BTreeMap<String, controller::PresetKind>,
    monitor: VecDeque<String>,
    /// The recent score the controller is on.
    cursor: usize,
    taps: Vec<Instant>,
    /// The port name (or part of it) the chosen preset looks for; empty if none is chosen.
    wanted: String,
    last_watch: Instant,
    /// The preset editor (`controls/editor.rs`).
    editor: editor::EditorState,
}

impl Controller {
    pub(super) fn new() -> Controller {
        Controller {
            link: None,
            queue: Arc::new(Mutex::new(Vec::new())),
            message: String::new(),
            presets: Vec::new(),
            kinds: BTreeMap::new(),
            monitor: VecDeque::new(),
            cursor: 0,
            taps: Vec::new(),
            wanted: String::new(),
            last_watch: Instant::now(),
            editor: editor::EditorState::default(),
        }
    }

    /// What the preset called `name` speaks, once the list has been read.
    pub(super) fn kind_of(&self, name: &str) -> Option<controller::PresetKind> {
        self.kinds.get(name).copied()
    }

    /// Puts an edited preset in place in the running connection, if there is one.
    pub(super) fn replace_preset(&mut self, preset: controller::Preset) {
        if let Some(link) = self.link.as_mut() {
            link.mapper.replace_preset(preset);
        }
    }

    /// The state of the connection, in words.
    pub(super) fn message(&self) -> &str {
        &self.message
    }

    pub(super) fn is_connected(&self) -> bool {
        self.link.is_some()
    }

    pub(super) fn preset_count(&self) -> usize {
        self.presets.len()
    }

    /// The preset in use and the layer the controller is on, while it is connected.
    pub(super) fn live(&self) -> Option<(&controller::Preset, Option<&str>)> {
        self.link.as_ref().map(|l| (l.mapper.preset(), l.mapper.layer_name()))
    }

    /// For the top bar, like the MIDI input indicator: the port and the layer
    /// in use, and whether the controller is connected. `None` if none is chosen.
    pub(super) fn label(&self, chosen: bool) -> Option<(String, bool)> {
        match self.link.as_ref() {
            Some(link) => Some((
                match link.mapper.layer_name() {
                    Some(layer) => format!("⏺ {} · layer {layer}", link.handle.port_name),
                    None => format!("⏺ {}", link.handle.port_name),
                },
                true,
            )),
            None => chosen.then(|| ("⏺ no controller".to_string(), false)),
        }
    }
}

#[derive(Debug, PartialEq)]
enum Watch {
    Keep,
    /// The connected controller is no longer among the ports.
    Lost,
    /// The controller we are waiting for is there now.
    Connect,
}

/// What to do about the controller given the MIDI input ports there are now.
/// `drum` is the drum module's port, which is never taken for the controller.
fn watch_step(connected: Option<&str>, ports: &[String], wanted: &str, drum: Option<&str>) -> Watch {
    match connected {
        Some(name) if ports.iter().any(|p| p == name) => Watch::Keep,
        Some(_) => Watch::Lost,
        None => match midi::find_port(ports, wanted) {
            Some(i) if drum != Some(ports[i].as_str()) => Watch::Connect,
            _ => Watch::Keep,
        },
    }
}

impl DrumApp {
    /// The names offered on the Controller page.
    pub(super) fn refresh_controllers(&mut self) {
        let mut names: Vec<String> = controller::BUILTIN.iter().map(|(n, _)| n.to_string()).collect();
        let custom = self.paths.controllers_dir().map(|d| controller::custom_presets(&d)).unwrap_or_default();
        for (name, _) in custom {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        let dir = self.paths.controllers_dir();
        self.controller.kinds = names.iter().filter_map(|n| controller::load_preset(n, dir.as_deref()).ok().map(|p| (n.clone(), p.kind))).collect();
        self.controller.presets = names;
    }

    /// Chooses a preset (`""`: none) as the one in use and connects its controller. The port
    /// is the preset's own again, and what the editor was doing with the old one is dropped.
    pub(super) fn select_preset(&mut self, name: String) {
        self.choose_preset(name, false);
    }

    /// `select_preset`, optionally keeping a port chosen by hand (a renamed preset keeps it).
    pub(super) fn choose_preset(&mut self, name: String, keep_port: bool) {
        self.cfg.controller_preset = name;
        if !keep_port {
            self.cfg.controller_port.clear();
        }
        self.controller.editor.start_over();
        self.connect_controller();
    }

    /// (Re)connects the chosen preset's controller; a failure is explained on the Controller page.
    pub(super) fn connect_controller(&mut self) {
        let c = &mut self.controller;
        c.link = None; // close the old port first (Windows ports are exclusive)
        c.wanted.clear();
        c.monitor.clear();
        if let Ok(mut q) = c.queue.lock() {
            q.clear();
        }
        if self.cfg.controller_preset.is_empty() {
            c.message = "No controller selected.".into();
            return;
        }
        let dir = self.paths.controllers_dir();
        let preset = match controller::load_preset(&self.cfg.controller_preset, dir.as_deref()) {
            Ok(p) => p,
            Err(e) => {
                c.message = e.to_string();
                return;
            }
        };
        let wanted = if self.cfg.controller_port.is_empty() { preset.port.clone() } else { self.cfg.controller_port.clone() };
        c.wanted = wanted.clone();
        match midi::connect_controller(&wanted, c.queue.clone()) {
            Ok(handle) if self.midi.input.as_ref().is_some_and(|h| h.port_name == handle.port_name) => {
                c.message = format!("\"{}\" is the drum input. Choose the controller's own port.", handle.port_name);
            }
            Ok(handle) => {
                c.message = if handle.has_output() {
                    format!("Connected to {}.", handle.port_name)
                } else {
                    format!("Connected to {}, but it has no output port that could be opened: the lights stay off.", handle.port_name)
                };
                self.view.status = format!("Controller: {}", handle.port_name);
                c.link = Some(Link { handle, mapper: Mapper::new(preset) });
            }
            Err(e) => c.message = format!("{e}. Switch the controller to Mackie Control mode, or choose its port below."),
        }
    }

    /// Runs the commands the controller sent since the last frame and brings its lights up to date.
    pub(super) fn update_controller(&mut self) {
        self.watch_controller();
        if self.controller.link.is_none() {
            return;
        }
        let messages = match self.controller.queue.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => Vec::new(),
        };
        for bytes in messages {
            let Some(msg) = controller::parse(&bytes) else { continue };
            // A control is being taught to the preset editor: it does nothing else.
            if self.learn_message(msg) {
                continue;
            }
            // The state is read for every message: a fader that moved the value a moment ago is already counted.
            let state = self.controller_state();
            let Some(link) = self.controller.link.as_mut() else { return };
            let bound = link.mapper.bound_to(msg);
            let commands = link.mapper.handle(msg, &state);
            let line = match bound {
                Some(action) => format!("{}  →  {action}", msg.text()),
                None => format!("{}  (not bound)", msg.text()),
            };
            self.controller.monitor.push_front(line);
            self.controller.monitor.truncate(MONITOR_LINES);
            for command in commands {
                self.apply_command(command);
            }
        }
        let state = self.controller_state();
        if let Some(link) = self.controller.link.as_mut() {
            for message in link.mapper.lights(&state) {
                link.handle.send(&message);
            }
        }
    }

    /// Notices a controller that was plugged in after the start or pulled out, and
    /// connects it or lets go of it. Not while a take records: it does not need the delay.
    fn watch_controller(&mut self) {
        let c = &self.controller;
        if c.wanted.is_empty() || c.last_watch.elapsed() < WATCH_EVERY || self.take_running() {
            return;
        }
        self.controller.last_watch = Instant::now();
        let ports = midi::list_inputs().unwrap_or_default();
        let connected = self.controller.link.as_ref().map(|l| l.handle.port_name.as_str());
        let drum = self.midi.input.as_ref().map(|h| h.port_name.as_str());
        match watch_step(connected, &ports, &self.controller.wanted, drum) {
            Watch::Keep => {}
            Watch::Lost => {
                self.controller.link = None;
                self.controller.message = "The controller was disconnected. It connects again when it is plugged in.".into();
                self.view.status = "Controller disconnected".into();
            }
            Watch::Connect => self.connect_controller(),
        }
    }

}
