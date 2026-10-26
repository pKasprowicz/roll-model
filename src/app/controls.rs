//! The hardware controller (a Mackie Control compatible surface such as the
//! Behringer X-Touch Mini): the connection, what its commands do to the
//! application, the lights, and the Controller page of Settings.
//! Which control sends which command is in the preset (`controller.rs`).

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::RichText;

use super::{DrumApp, Tab};
use crate::analysis::Report;
use crate::config::{Config, MAX_LANE_GAIN};
use crate::controller::{self, Action, Command, Mapper, State, GROUPS};
use crate::midi::{self, ControllerHandle, MessageQueue};
use crate::render;
use crate::score::Score;

const MONITOR_LINES: usize = 14;
/// Taps further apart than this start a new tap tempo.
const TAP_GAP: Duration = Duration::from_secs(2);
/// Taps that count towards the tempo.
const TAPS_KEPT: usize = 6;
const TEMPO_RANGE: (f32, f32) = (25.0, 150.0);
const METRO_RANGE: (f32, f32) = (20.0, 300.0);

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
    monitor: VecDeque<String>,
    /// The recent score the controller is on.
    cursor: usize,
    taps: Vec<Instant>,
}

impl Controller {
    pub(super) fn new() -> Controller {
        Controller {
            link: None,
            queue: Arc::new(Mutex::new(Vec::new())),
            message: String::new(),
            presets: Vec::new(),
            monitor: VecDeque::new(),
            cursor: 0,
            taps: Vec::new(),
        }
    }

    /// For the top bar.
    pub(super) fn label(&self) -> Option<String> {
        let link = self.link.as_ref()?;
        Some(match link.mapper.layer_name() {
            Some(layer) => format!("controller · layer {layer}"),
            None => "controller".into(),
        })
    }
}

/// Adds a tap and returns the tempo (beats per minute) they give, once there are two.
fn register_tap(taps: &mut Vec<Instant>, now: Instant) -> Option<f64> {
    if taps.last().is_some_and(|last| now.saturating_duration_since(*last) > TAP_GAP) {
        taps.clear();
    }
    taps.push(now);
    if taps.len() > TAPS_KEPT {
        taps.remove(0);
    }
    if taps.len() < 2 {
        return None;
    }
    let span = taps[taps.len() - 1].saturating_duration_since(taps[0]).as_secs_f64();
    (span > 0.0).then(|| 60.0 * (taps.len() - 1) as f64 / span)
}

/// The range of the same length before or after `from..=to` in a score of `n` measures.
fn shifted_range(from: usize, to: usize, n: usize, forward: bool) -> Option<(usize, usize)> {
    let len = to.saturating_sub(from) + 1;
    if forward {
        (to + 1 < n).then(|| (from + len, (to + len).min(n - 1)))
    } else {
        (from > 0).then(|| {
            let from = from.saturating_sub(len);
            (from, from + len - 1)
        })
    }
}

/// The measure the last take went worst in: the most missed, wrong or
/// sloppy notes and extra hits (`extra_in`: the measure of each). `None` if all was perfect.
fn worst_measure(score: &Score, report: &Report, extra_in: &[usize]) -> Option<usize> {
    let mut loss = vec![0.0_f64; score.measures.len()];
    for (e, r) in report.expected.iter().zip(&report.results) {
        if r.grade == crate::analysis::Grade::Pending {
            continue;
        }
        if let Some(slot) = score.notes.get(e.note_idx).and_then(|n| loss.get_mut(n.measure)) {
            *slot += 1.0 - r.grade.credit();
        }
    }
    for m in extra_in {
        if let Some(slot) = loss.get_mut(*m) {
            *slot += 1.0;
        }
    }
    let (worst, most) = loss.iter().copied().enumerate().fold((0, 0.0), |best, (i, l)| if l > best.1 { (i, l) } else { best });
    (most > 1e-9).then_some(worst)
}

impl DrumApp {
    /// The names offered on the Controller page.
    pub(super) fn refresh_controllers(&mut self) {
        let mut names: Vec<String> = controller::BUILTIN.iter().map(|(n, _)| n.to_string()).collect();
        let custom = Config::controllers_dir().map(|d| controller::custom_presets(&d)).unwrap_or_default();
        for (name, _) in custom {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        self.controller.presets = names;
    }

    /// (Re)connects the chosen preset's controller; a failure is explained on the Controller page.
    pub(super) fn connect_controller(&mut self) {
        let c = &mut self.controller;
        c.link = None; // close the old port first (Windows ports are exclusive)
        c.monitor.clear();
        if let Ok(mut q) = c.queue.lock() {
            q.clear();
        }
        if self.cfg.controller_preset.is_empty() {
            c.message = "No controller selected.".into();
            return;
        }
        let dir = Config::controllers_dir();
        let preset = match controller::load_preset(&self.cfg.controller_preset, dir.as_deref()) {
            Ok(p) => p,
            Err(e) => {
                c.message = e;
                return;
            }
        };
        let wanted = if self.cfg.controller_port.is_empty() { preset.port.clone() } else { self.cfg.controller_port.clone() };
        match midi::connect_controller(&wanted, c.queue.clone()) {
            Ok(handle) if self.midi_in.as_ref().is_some_and(|h| h.port_name == handle.port_name) => {
                c.message = format!("\"{}\" is the drum input. Choose the controller's own port.", handle.port_name);
            }
            Ok(handle) => {
                c.message = if handle.has_output() {
                    format!("Connected to {}.", handle.port_name)
                } else {
                    format!("Connected to {}, but it has no output port that could be opened: the lights stay off.", handle.port_name)
                };
                self.status = format!("Controller: {}", handle.port_name);
                c.link = Some(Link { handle, mapper: Mapper::new(preset) });
            }
            Err(e) => c.message = format!("{e}. Switch the controller to Mackie Control mode, or choose its port below."),
        }
    }

    /// Runs the commands the controller sent since the last frame and brings its lights up to date.
    pub(super) fn update_controller(&mut self) {
        if self.controller.link.is_none() {
            return;
        }
        let messages = match self.controller.queue.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => Vec::new(),
        };
        for bytes in messages {
            let Some(msg) = controller::parse(&bytes) else { continue };
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

    /// Tempo controls act on the metronome while its tab is open (or no score is loaded).
    fn tempo_is_metronome(&self) -> bool {
        self.tab == Tab::Metronome || self.score.is_none()
    }

    fn controller_state(&self) -> State {
        let c = &self.cfg;
        let measures = self.score.as_ref().map_or(0, |s| s.measures.len());
        let along = |i: usize| if measures > 1 { i as f32 / (measures - 1) as f32 } else { 0.0 };
        let tempo = if self.tempo_is_metronome() {
            (c.metro_bpm - METRO_RANGE.0) / (METRO_RANGE.1 - METRO_RANGE.0)
        } else {
            (self.tempo_pct - TEMPO_RANGE.0) / (TEMPO_RANGE.1 - TEMPO_RANGE.0)
        };
        let mut s = State {
            record: self.take.as_ref().is_some_and(|t| t.running),
            listen: self.listen.is_some(),
            click: c.metronome_during_take,
            looping: self.listen_loop,
            notes: self.listen_notes,
            metronome: self.metro.is_some(),
            tempo: tempo.clamp(0.0, 1.0),
            range_from: along(self.range_from),
            range_to: along(self.range_to),
            click_volume: c.click_volume,
            music_volume: c.playback_volume,
            count_in: c.count_in_bars as f32 / 2.0,
            bars_per_row: c.bars_per_row as f32 / 8.0,
            ..Default::default()
        };
        for i in 0..GROUPS {
            let lanes = controller::group_lanes(i);
            s.gain[i] = (c.gain_of(lanes[0]) / MAX_LANE_GAIN).clamp(0.0, 1.0);
            s.mute[i] = lanes.iter().all(|l| c.lane_mute.contains(l));
            s.solo[i] = lanes.iter().all(|l| self.solo.contains(l));
        }
        s
    }

    fn take_running(&self) -> bool {
        self.take.as_ref().is_some_and(|t| t.running)
    }

    /// Whether a change of tempo, range or score is allowed now; says why not.
    fn unlocked(&mut self) -> bool {
        if self.take_running() {
            self.status = "Locked while a take is recording.".into();
            return false;
        }
        true
    }

    pub(super) fn apply_command(&mut self, command: Command) {
        match command {
            Command::Press(action) => self.press(action),
            Command::Turn(action, steps) => self.turn(action, steps),
            Command::Set(action, x) => self.set(action, x),
        }
    }

    fn press(&mut self, action: Action) {
        let running = self.take_running();
        match action {
            Action::Record if running => self.stop_take(false),
            Action::Record if self.score.is_some() => self.start_take(),
            Action::Listen if !running && self.score.is_some() => {
                if self.listen.is_some() {
                    self.stop_listen();
                } else {
                    self.start_listen();
                }
            }
            Action::Record | Action::Listen => {
                if self.score.is_none() {
                    self.status = "Open a score first.".into();
                }
            }
            Action::Stop => {
                self.stop_take(false);
                self.stop_listen();
                self.stop_metro();
            }
            Action::Click => self.cfg.metronome_during_take = !self.cfg.metronome_during_take,
            Action::Loop if !running => self.listen_loop = !self.listen_loop,
            Action::Notes if !running => self.listen_notes = !self.listen_notes,
            Action::Metronome if !running => {
                if self.metro.is_some() {
                    self.stop_metro();
                } else {
                    self.start_metro();
                }
            }
            Action::NextTab => self.next_tab(),
            Action::LoopWorstBar if self.unlocked() => self.loop_worst_bar(),
            Action::RangeBack if self.unlocked() => self.move_range(false),
            Action::RangeForward if self.unlocked() => self.move_range(true),
            Action::TapTempo if self.unlocked() => self.tap_tempo(),
            Action::TempoDown if self.unlocked() => self.turn(Action::Tempo, -5),
            Action::TempoUp if self.unlocked() => self.turn(Action::Tempo, 5),
            Action::TempoReset if self.unlocked() => {
                if self.tempo_is_metronome() {
                    self.cfg.metro_bpm = Config::default().metro_bpm;
                } else {
                    self.tempo_pct = 100.0;
                }
                self.status = "Tempo reset.".into();
            }
            Action::DiscardTake if self.unlocked() => {
                if let Some(take) = self.take.take() {
                    self.history.retain(|h| h.number != take.number);
                    self.status = format!("Take {} discarded.", take.number);
                }
            }
            Action::OpenScore if self.unlocked() => {
                match self.cfg.recent_files.get(self.controller.cursor.min(self.cfg.recent_files.len().saturating_sub(1))).cloned() {
                    Some(path) => self.open_file(Path::new(&path)),
                    None => self.status = "There are no recent scores to open.".into(),
                }
            }
            Action::Mute(i) => self.toggle_mute(i as usize),
            Action::Solo(i) => self.toggle_solo(i as usize),
            Action::Audition(i) => self.audition(controller::group_lanes(i as usize)[0]),
            _ => {}
        }
    }

    fn turn(&mut self, action: Action, steps: i32) {
        let tempo_here = self.tempo_is_metronome();
        let measures = self.score.as_ref().map_or(0, |s| s.measures.len());
        let one = steps.signum();
        match action {
            Action::Tempo if self.unlocked() => {
                if tempo_here {
                    self.cfg.metro_bpm = (self.cfg.metro_bpm.round() + steps as f32).clamp(METRO_RANGE.0, METRO_RANGE.1);
                    self.status = format!("Metronome {:.0} BPM", self.cfg.metro_bpm);
                } else {
                    self.tempo_pct = (self.tempo_pct.round() + steps as f32).clamp(TEMPO_RANGE.0, TEMPO_RANGE.1);
                    self.status = format!("Tempo {:.0} %", self.tempo_pct);
                }
            }
            Action::RangeFrom if measures > 0 && self.unlocked() => {
                self.range_from = (self.range_from as i32 + steps).clamp(0, self.range_to as i32) as usize;
                self.status_range();
            }
            Action::RangeTo if measures > 0 && self.unlocked() => {
                self.range_to = (self.range_to as i32 + steps).clamp(self.range_from as i32, measures as i32 - 1) as usize;
                self.status_range();
            }
            Action::ClickVolume => self.cfg.click_volume = nudged_volume(self.cfg.click_volume, steps),
            Action::MusicVolume => self.cfg.playback_volume = nudged_volume(self.cfg.playback_volume, steps),
            Action::CountIn => self.cfg.count_in_bars = (self.cfg.count_in_bars as i32 + one).clamp(0, 2) as u32,
            Action::BarsPerRow => self.cfg.bars_per_row = (self.cfg.bars_per_row as i32 + one).clamp(0, 8) as usize,
            Action::Gain(i) => {
                let g = self.cfg.gain_of(controller::group_lanes(i as usize)[0]);
                self.set_group_gain(i as usize, (g + 0.05 * steps as f32).clamp(0.0, MAX_LANE_GAIN));
            }
            Action::Score if self.unlocked() => self.browse_scores(steps),
            _ => {}
        }
    }

    fn set(&mut self, action: Action, x: f32) {
        let x = x.clamp(0.0, 1.0);
        let measures = self.score.as_ref().map_or(0, |s| s.measures.len());
        let bar = |x: f32| (x * measures.saturating_sub(1) as f32).round() as usize;
        match action {
            Action::Tempo if self.unlocked() => {
                if self.tempo_is_metronome() {
                    self.cfg.metro_bpm = (METRO_RANGE.0 + x * (METRO_RANGE.1 - METRO_RANGE.0)).round();
                } else {
                    self.tempo_pct = (TEMPO_RANGE.0 + x * (TEMPO_RANGE.1 - TEMPO_RANGE.0)).round();
                }
            }
            Action::RangeFrom if measures > 0 && self.unlocked() => self.range_from = bar(x).min(self.range_to),
            Action::RangeTo if measures > 0 && self.unlocked() => self.range_to = bar(x).max(self.range_from),
            Action::ClickVolume => self.cfg.click_volume = x,
            Action::MusicVolume => self.cfg.playback_volume = x,
            Action::CountIn => self.cfg.count_in_bars = (x * 2.0).round() as u32,
            Action::BarsPerRow => self.cfg.bars_per_row = (x * 8.0).round() as usize,
            Action::Gain(i) => self.set_group_gain(i as usize, x * MAX_LANE_GAIN),
            _ => {}
        }
    }

    fn status_range(&mut self) {
        if let Some(score) = self.score.as_ref() {
            let number = |i: usize| score.measures.get(i).map(|m| m.number.clone()).unwrap_or_default();
            self.status = format!("Measures {}–{}", number(self.range_from), number(self.range_to));
        }
    }

    fn next_tab(&mut self) {
        let mut tabs = Vec::new();
        if self.score.is_some() {
            tabs.push(Tab::Score);
        }
        tabs.extend([Tab::Metronome, Tab::Results, Tab::Settings]);
        self.tab = match tabs.iter().position(|t| *t == self.tab) {
            Some(i) => tabs[(i + 1) % tabs.len()],
            None => tabs[0],
        };
    }

    fn tap_tempo(&mut self) {
        let Some(bpm) = register_tap(&mut self.controller.taps, Instant::now()) else {
            self.status = "Tap the tempo…".into();
            return;
        };
        if self.tempo_is_metronome() {
            self.cfg.metro_bpm = (bpm as f32).clamp(METRO_RANGE.0, METRO_RANGE.1);
            self.status = format!("Metronome {:.0} BPM (tapped)", self.cfg.metro_bpm);
        } else if let Some(score) = self.score.as_ref() {
            let written = score.tempo.bpm_at(score.measures[self.range_from.min(score.measures.len() - 1)].start_q);
            self.tempo_pct = (bpm / written * 100.0).clamp(TEMPO_RANGE.0 as f64, TEMPO_RANGE.1 as f64).round() as f32;
            self.status = format!("Tempo {:.0} % = {bpm:.0} BPM (tapped)", self.tempo_pct);
        }
    }

    fn move_range(&mut self, forward: bool) {
        let Some(n) = self.score.as_ref().map(|s| s.measures.len()) else { return };
        if let Some((from, to)) = shifted_range(self.range_from, self.range_to, n, forward) {
            self.range_from = from;
            self.range_to = to;
            self.status_range();
        }
    }

    /// Loops the measure the last take went worst in.
    fn loop_worst_bar(&mut self) {
        let worst = match (self.take.as_ref(), self.score.as_ref()) {
            (Some(take), Some(score)) => {
                let extra: Vec<usize> = take
                    .hits
                    .iter()
                    .zip(&take.report.hit_match)
                    .filter(|(_, matched)| matched.is_none())
                    .filter_map(|(h, _)| score.measure_at(score.tempo.sec_to_q(h.t + take.t0, take.tempo_scale)))
                    .collect();
                Some(worst_measure(score, &take.report, &extra))
            }
            _ => None,
        };
        match worst {
            None => self.status = "Record a take first.".into(),
            Some(None) => self.status = "The last take had no mistakes to practise.".into(),
            Some(Some(m)) => {
                self.range_from = m;
                self.range_to = m;
                self.listen_loop = true;
                self.status_range();
                self.start_listen();
            }
        }
    }

    fn browse_scores(&mut self, steps: i32) {
        let n = self.cfg.recent_files.len();
        if n == 0 {
            self.status = "There are no recent scores.".into();
            return;
        }
        let at = (self.controller.cursor.min(n - 1) as i32 + steps).clamp(0, n as i32 - 1) as usize;
        self.controller.cursor = at;
        let name = Path::new(&self.cfg.recent_files[at]).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        self.status = format!("Score {}/{n}: {name} — push the encoder to open it", at + 1);
    }

    fn set_group_gain(&mut self, group: usize, gain: f32) {
        for lane in controller::group_lanes(group) {
            // The default is not stored, as in the mixer.
            if (gain - 1.0).abs() < 0.01 {
                self.cfg.lane_gain.remove(lane);
            } else {
                self.cfg.lane_gain.insert(*lane, gain);
            }
        }
        self.status = format!("{}: volume {:.0} %", controller::group_label(group), gain * 100.0);
    }

    fn toggle_mute(&mut self, group: usize) {
        let lanes = controller::group_lanes(group);
        let muted = lanes.iter().all(|l| self.cfg.lane_mute.contains(l));
        for lane in lanes {
            if muted {
                self.cfg.lane_mute.remove(lane);
            } else {
                self.cfg.lane_mute.insert(*lane);
            }
        }
        self.status = format!("{} {}", controller::group_label(group), if muted { "unmuted" } else { "muted" });
    }

    fn toggle_solo(&mut self, group: usize) {
        let lanes = controller::group_lanes(group);
        let soloed = lanes.iter().all(|l| self.solo.contains(l));
        for lane in lanes {
            if soloed {
                self.solo.remove(lane);
            } else {
                self.solo.insert(*lane);
            }
        }
        self.status = format!("{} {}", controller::group_label(group), if soloed { "no longer solo" } else { "solo" });
    }

    // ------------------------------------------------------------ settings page

    pub(super) fn settings_controller(&mut self, ui: &mut egui::Ui) {
        ui.heading("Controller");
        ui.label(
            RichText::new("A control surface that speaks the Mackie Control protocol, such as the Behringer X-Touch Mini (switch it to MC mode) or X-Touch Compact. It works the transport, tempo, measures, click and mixer from the stool, and its lights follow the application. Close other programs that use its MIDI port.")
                .small()
                .weak(),
        );

        let mut preset: Option<String> = None;
        ui.horizontal(|ui| {
            ui.label("Preset");
            let current = if self.cfg.controller_preset.is_empty() { "— none —".to_string() } else { self.cfg.controller_preset.clone() };
            egui::ComboBox::from_id_salt("controller_preset").selected_text(current).width(260.0).show_ui(ui, |ui| {
                if ui.selectable_label(self.cfg.controller_preset.is_empty(), "— none —").clicked() {
                    preset = Some(String::new());
                }
                for name in &self.controller.presets {
                    let builtin = controller::builtin_text(name).is_some();
                    let label = if builtin { format!("{name} (built in)") } else { name.clone() };
                    if ui.selectable_label(&self.cfg.controller_preset == name, label).clicked() {
                        preset = Some(name.clone());
                    }
                }
            });
        });
        if let Some(name) = preset {
            self.cfg.controller_preset = name;
            self.cfg.controller_port.clear();
            self.connect_controller();
        }

        let mut port: Option<String> = None;
        ui.horizontal(|ui| {
            ui.label("Port");
            let current = match (&self.cfg.controller_port, self.controller.link.as_ref()) {
                (p, _) if !p.is_empty() => p.clone(),
                (_, Some(l)) => format!("automatic ({})", l.handle.port_name),
                _ => "automatic".to_string(),
            };
            egui::ComboBox::from_id_salt("controller_port").selected_text(current).width(260.0).show_ui(ui, |ui| {
                if ui.selectable_label(self.cfg.controller_port.is_empty(), "automatic (the preset's port)").clicked() {
                    port = Some(String::new());
                }
                for p in &self.midi_inputs {
                    if ui.selectable_label(&self.cfg.controller_port == p, p.as_str()).clicked() {
                        port = Some(p.clone());
                    }
                }
            });
            if ui.button("⟳ Refresh and reconnect").clicked() {
                self.refresh_ports();
                self.refresh_controllers();
                self.connect_controller();
            }
        });
        if let Some(p) = port {
            self.cfg.controller_port = p;
            self.connect_controller();
        }

        let (colour, text) = match self.controller.link.is_some() {
            true => (render::GREEN, self.controller.message.clone()),
            false => (render::AMBER, self.controller.message.clone()),
        };
        if !text.is_empty() {
            ui.label(RichText::new(text).color(colour));
        }

        if let Some(link) = self.controller.link.as_ref() {
            let preset = link.mapper.preset();
            for w in &preset.warnings {
                ui.label(RichText::new(format!("⚠ {w}")).small().color(render::AMBER));
            }
            ui.add_space(6.0);
            let layer = link.mapper.layer_name().map(|l| format!("layer {l}")).unwrap_or_else(|| "no layers".into());
            ui.label(format!("{} · {layer}{}", preset.name, if link.mapper.shift() { " · shift held" } else { "" }));
            ui.collapsing("Controls on this layer", |ui| {
                let layer = link.mapper.layer_name();
                egui::Grid::new("controller_binds").num_columns(2).striped(true).show(ui, |ui| {
                    for b in preset.binds.iter().filter(|b| b.layer.is_none() || b.layer.as_deref() == layer) {
                        let kind = if b.relative { " (turn)" } else { "" };
                        ui.label(format!("{}{kind}", b.src.text()));
                        ui.label(b.action.text());
                        ui.end_row();
                    }
                });
            });
        }

        ui.add_space(6.0);
        ui.label(RichText::new("Monitor (latest first): press and turn the controls to see what they send and what they are bound to").strong());
        for line in &self.controller.monitor {
            ui.label(RichText::new(line).monospace());
        }

        ui.add_space(10.0);
        ui.heading("Your own presets");
        match Config::controllers_dir() {
            Some(dir) => {
                ui.label(
                    RichText::new(format!(
                        "A preset is a text file in {} (any name ending in .txt); it then appears in the list above. Copy a built-in preset there to change it: the lines are explained at the top of each.",
                        dir.display()
                    ))
                    .small()
                    .weak(),
                );
                let builtin = controller::builtin_text(&self.cfg.controller_preset);
                if ui.add_enabled(builtin.is_some(), egui::Button::new("Copy the selected built-in preset there")).clicked() {
                    if let Some(text) = builtin {
                        match copy_preset(&dir, &self.cfg.controller_preset, text) {
                            Ok(path) => {
                                self.status = format!("Saved {}", path.display());
                                self.refresh_controllers();
                            }
                            Err(e) => self.error = Some(format!("Could not save the preset: {e}")),
                        }
                    }
                }
            }
            None => {
                ui.label("No settings folder is available on this system.");
            }
        }
    }
}

/// Steps of 2 % on a 0..1 volume, without the rounding errors adding up.
fn nudged_volume(v: f32, steps: i32) -> f32 {
    (((v + 0.02 * steps as f32) * 50.0).round() / 50.0).clamp(0.0, 1.0)
}

/// Writes `text` as "NAME (copy).txt" (or "(copy 2)", …) in `dir`; never overwrites.
fn copy_preset(dir: &Path, name: &str, text: &str) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut path = dir.join(format!("{name} (copy).txt"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{name} (copy {n}).txt"));
        n += 1;
    }
    std::fs::write(&path, text)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{analyze, expected_notes, Hit};
    use crate::config::Config;
    use crate::drums::Lane;
    use std::path::PathBuf;

    fn sample() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join("basic_rock_beat.musicxml")
    }

    fn app_with_score() -> DrumApp {
        let mut app = DrumApp::with_config(Config::default());
        app.open_file(&sample());
        assert!(app.score.is_some());
        app
    }

    #[test]
    fn tapping_gives_the_tempo_and_a_pause_starts_over() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut taps = Vec::new();
        assert_eq!(register_tap(&mut taps, at(0)), None);
        let bpm = register_tap(&mut taps, at(500)).unwrap();
        assert!((bpm - 120.0).abs() < 1e-6);
        assert!((register_tap(&mut taps, at(1000)).unwrap() - 120.0).abs() < 1e-6);
        // Slowing down: the average follows.
        let slower = register_tap(&mut taps, at(1700)).unwrap();
        assert!(slower < 120.0 && slower > 90.0, "{slower}");
        // A long pause starts a new tap tempo.
        assert_eq!(register_tap(&mut taps, at(10_000)), None);
        assert_eq!(taps.len(), 1);
        // Only the last few taps count.
        for i in 1..20 {
            register_tap(&mut taps, at(10_000 + i * 400));
        }
        assert_eq!(taps.len(), TAPS_KEPT);
    }

    #[test]
    fn the_range_moves_by_its_own_length_and_stops_at_the_ends() {
        assert_eq!(shifted_range(0, 3, 16, true), Some((4, 7)));
        assert_eq!(shifted_range(4, 7, 16, false), Some((0, 3)));
        assert_eq!(shifted_range(0, 3, 16, false), None);
        assert_eq!(shifted_range(12, 15, 16, true), None);
        assert_eq!(shifted_range(10, 13, 16, true), Some((14, 15)), "the last chunk may be shorter");
        assert_eq!(shifted_range(2, 5, 16, false), Some((0, 3)), "not below the first measure");
        assert_eq!(shifted_range(3, 3, 8, true), Some((4, 4)));
    }

    #[test]
    fn the_worst_measure_is_where_most_goes_wrong() {
        let app = app_with_score();
        let score = app.score.as_ref().unwrap();
        let last = score.measures.len() - 1;
        let expected = expected_notes(score, 0, last, 1.0);
        let tol = &app.cfg.tol;
        // Everything played perfectly, except that measure 2 (index 1) is left out.
        let hits: Vec<Hit> = expected
            .iter()
            .filter(|e| score.notes[e.note_idx].measure != 1)
            .map(|e| Hit { t: e.t, note: 36, velocity: 100, lane: e.lane })
            .collect();
        let report = analyze(&expected, &hits, tol, f64::INFINITY);
        assert_eq!(worst_measure(score, &report, &[]), Some(1));
        // Enough stray hits in measure 4 make it the worst one.
        let strays = vec![last; expected.len()];
        assert_eq!(worst_measure(score, &report, &strays), Some(last));
        // Nothing wrong, nothing to practise.
        let all: Vec<Hit> = expected.iter().map(|e| Hit { t: e.t, note: 36, velocity: 100, lane: e.lane }).collect();
        let perfect = analyze(&expected, &all, tol, f64::INFINITY);
        assert_eq!(worst_measure(score, &perfect, &[]), None);
    }

    #[test]
    fn the_tempo_encoder_and_fader_change_the_practice_tempo() {
        let mut app = app_with_score();
        app.apply_command(Command::Turn(Action::Tempo, 5));
        assert_eq!(app.tempo_pct, 105.0);
        app.apply_command(Command::Turn(Action::Tempo, -500));
        assert_eq!(app.tempo_pct, 25.0, "clamped");
        app.apply_command(Command::Set(Action::Tempo, 1.0));
        assert_eq!(app.tempo_pct, 150.0);
        app.apply_command(Command::Press(Action::TempoReset));
        assert_eq!(app.tempo_pct, 100.0);
        app.apply_command(Command::Press(Action::TempoDown));
        assert_eq!(app.tempo_pct, 95.0);
        assert_eq!(app.controller_state().tempo, (95.0 - 25.0) / 125.0);
    }

    #[test]
    fn on_the_metronome_tab_the_tempo_controls_set_its_bpm() {
        let mut app = app_with_score();
        app.tab = Tab::Metronome;
        app.apply_command(Command::Turn(Action::Tempo, 10));
        assert_eq!(app.cfg.metro_bpm, 110.0);
        assert_eq!(app.tempo_pct, 100.0, "the practice tempo is left alone");
        app.apply_command(Command::Set(Action::Tempo, 0.0));
        assert_eq!(app.cfg.metro_bpm, 20.0);
        // Without a score the metronome is the only tempo there is.
        let mut bare = DrumApp::with_config(Config::default());
        bare.apply_command(Command::Turn(Action::Tempo, -10));
        assert_eq!(bare.cfg.metro_bpm, 90.0);
    }

    #[test]
    fn range_encoders_keep_the_range_valid() {
        let mut app = app_with_score();
        let last = app.score.as_ref().unwrap().measures.len() - 1;
        assert_eq!((app.range_from, app.range_to), (0, last));
        app.apply_command(Command::Turn(Action::RangeFrom, 2));
        assert_eq!(app.range_from, 2.min(last));
        app.apply_command(Command::Turn(Action::RangeFrom, 100));
        assert_eq!(app.range_from, last, "not past the end");
        app.apply_command(Command::Turn(Action::RangeTo, -100));
        assert_eq!(app.range_to, app.range_from, "the end never goes before the start");
        app.apply_command(Command::Set(Action::RangeFrom, 0.0));
        assert_eq!(app.range_from, 0);
        app.apply_command(Command::Set(Action::RangeTo, 0.5));
        assert!(app.range_to >= app.range_from && app.range_to <= last);
        app.range_from = 0;
        app.range_to = 1;
        app.apply_command(Command::Press(Action::RangeForward));
        assert_eq!((app.range_from, app.range_to), (2, 3.min(last)));
        app.apply_command(Command::Press(Action::RangeBack));
        assert_eq!((app.range_from, app.range_to), (0, 1));
    }

    #[test]
    fn mixer_strips_control_groups_of_instruments() {
        let mut app = DrumApp::with_config(Config::default());
        app.apply_command(Command::Turn(Action::Gain(1), -4));
        for lane in [Lane::Snare, Lane::CrossStick] {
            assert!((app.cfg.gain_of(lane) - 0.8).abs() < 1e-5, "{lane:?}");
        }
        assert_eq!(app.cfg.gain_of(Lane::Kick), 1.0);
        assert!((app.controller_state().gain[1] - 0.8 / MAX_LANE_GAIN).abs() < 1e-5);
        app.apply_command(Command::Turn(Action::Gain(1), 4));
        assert!(app.cfg.lane_gain.is_empty(), "back at the default, nothing is stored");
        app.apply_command(Command::Set(Action::Gain(0), 1.0));
        assert_eq!(app.cfg.gain_of(Lane::Kick), MAX_LANE_GAIN);

        app.apply_command(Command::Press(Action::Mute(2)));
        for lane in [Lane::HiHatClosed, Lane::HiHatOpen, Lane::HiHatPedal] {
            assert!(app.cfg.lane_mute.contains(&lane), "{lane:?}");
        }
        assert!(app.controller_state().mute[2]);
        app.apply_command(Command::Press(Action::Mute(2)));
        assert!(app.cfg.lane_mute.is_empty());

        app.apply_command(Command::Press(Action::Solo(7)));
        assert!(app.solo.contains(&Lane::Ride) && app.controller_state().solo[7]);
        assert_eq!(app.cfg.effective_gain(&app.solo, Lane::Kick), 0.0);
        app.apply_command(Command::Press(Action::Solo(7)));
        assert!(app.solo.is_empty());
    }

    #[test]
    fn volume_encoders_move_in_small_steps_within_range() {
        let mut app = DrumApp::with_config(Config::default());
        app.cfg.click_volume = 0.5;
        app.apply_command(Command::Turn(Action::ClickVolume, 3));
        assert_eq!(app.cfg.click_volume, 0.56);
        app.apply_command(Command::Turn(Action::ClickVolume, 1000));
        assert_eq!(app.cfg.click_volume, 1.0);
        app.apply_command(Command::Set(Action::MusicVolume, 0.25));
        assert_eq!(app.cfg.playback_volume, 0.25);
        app.cfg.count_in_bars = 0;
        app.apply_command(Command::Turn(Action::CountIn, 5));
        assert_eq!(app.cfg.count_in_bars, 1, "one step at a time");
        app.apply_command(Command::Turn(Action::CountIn, 50));
        app.apply_command(Command::Turn(Action::CountIn, 50));
        assert_eq!(app.cfg.count_in_bars, 2);
        app.apply_command(Command::Turn(Action::BarsPerRow, 1));
        assert_eq!(app.cfg.bars_per_row, 1);
        app.apply_command(Command::Set(Action::BarsPerRow, 1.0));
        assert_eq!(app.cfg.bars_per_row, 8);
    }

    #[test]
    fn switches_and_tabs() {
        let mut app = app_with_score();
        let click = app.cfg.metronome_during_take;
        app.apply_command(Command::Press(Action::Click));
        assert_eq!(app.cfg.metronome_during_take, !click);
        assert!(app.listen_notes && !app.listen_loop);
        app.apply_command(Command::Press(Action::Loop));
        app.apply_command(Command::Press(Action::Notes));
        assert!(!app.listen_notes && app.listen_loop);
        let s = app.controller_state();
        assert!(s.looping && !s.notes && !s.record && !s.listen && !s.metronome);

        assert!(app.tab == Tab::Score);
        for expected in [Tab::Metronome, Tab::Results, Tab::Settings, Tab::Score] {
            app.apply_command(Command::Press(Action::NextTab));
            assert!(app.tab == expected);
        }
        let mut bare = DrumApp::with_config(Config::default());
        bare.apply_command(Command::Press(Action::NextTab));
        assert!(bare.tab == Tab::Metronome, "no Score tab without a score");
    }

    #[test]
    fn recent_scores_are_browsed_and_opened() {
        let mut app = DrumApp::with_config(Config::default());
        app.apply_command(Command::Turn(Action::Score, 1));
        assert!(app.status.contains("no recent"));
        let sample = sample().to_string_lossy().into_owned();
        app.cfg.recent_files = vec!["missing-1.musicxml".into(), sample, "missing-2.musicxml".into()];
        app.apply_command(Command::Turn(Action::Score, 1));
        assert_eq!(app.controller.cursor, 1);
        assert!(app.status.contains("basic_rock_beat") && app.status.contains("2/3"), "{}", app.status);
        app.apply_command(Command::Turn(Action::Score, 50));
        assert_eq!(app.controller.cursor, 2);
        app.apply_command(Command::Turn(Action::Score, -50));
        assert_eq!(app.controller.cursor, 0);
        app.apply_command(Command::Turn(Action::Score, 1));
        app.apply_command(Command::Press(Action::OpenScore));
        assert!(app.score.is_some() && app.tab == Tab::Score);
    }

    #[test]
    fn loop_worst_bar_and_discard_need_a_take() {
        let mut app = app_with_score();
        app.apply_command(Command::Press(Action::LoopWorstBar));
        assert_eq!(app.status, "Record a take first.");
        assert!(!app.listen_loop);
        app.apply_command(Command::Press(Action::DiscardTake));
        let mut closed = DrumApp::with_config(Config::default());
        closed.apply_command(Command::Press(Action::Record));
        assert_eq!(closed.status, "Open a score first.");
        assert!(closed.take.is_none() && closed.listen.is_none());
    }

    #[test]
    fn the_controller_page_draws_with_and_without_a_controller() {
        let mut app = DrumApp::with_config(Config::default());
        app.refresh_controllers();
        assert_eq!(app.controller.presets, ["Behringer X-Touch Mini", "Behringer X-Touch Compact"]);
        let ctx = egui::Context::default();
        for message in ["", "No controller selected."] {
            app.controller.message = message.into();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.settings_controller(ui));
            });
        }
        // Choosing a preset whose device is not there says so instead of failing.
        app.cfg.controller_preset = "Behringer X-Touch Mini".into();
        app.cfg.controller_port = "no such port".into();
        app.connect_controller();
        assert!(app.controller.link.is_none() && app.controller.message.contains("not found"), "{}", app.controller.message);
        app.cfg.controller_preset = "No such preset".into();
        app.connect_controller();
        assert!(app.controller.message.contains("not found"));
    }

    #[test]
    fn copies_of_a_preset_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("roll-model-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = copy_preset(&dir, "Pads", "one").unwrap();
        let b = copy_preset(&dir, "Pads", "two").unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "two");
        assert_eq!(controller::custom_presets(&dir).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
