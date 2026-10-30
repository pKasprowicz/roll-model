//! The start-up splash: its own window with the name, the copyright and what the program is
//! doing while it starts. The main window stays hidden until its first frame is drawn, so the
//! application builds itself in small steps instead of in `DrumApp::new`: each frame shows the
//! step that is about to run, the next frame runs it (reading the settings, looking for MIDI
//! ports, opening the drum module and the controller, a score given on the command line) and
//! the steps done so far stay listed with what they found. The splash stays open after the
//! start-up, so that what it says can be read, until a key or a mouse button is pressed.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use egui::{RichText, ViewportBuilder, ViewportClass, ViewportId};

use super::DrumApp;
use crate::config::Config;
use crate::metronome::{self, HAS_AUDIO};
use crate::midi;
use crate::render;
use crate::widgets;

const COPYRIGHT: &str = "Copyright © 2026 Piotr Kasprowicz";
const LICENCE: &str = "Licensed under the PolyForm Noncommercial License 1.0.0";
const TAGLINE: &str = "MusicXML drum scores · MIDI capture · performance grading";
/// Width of the line around the splash window.
const BORDER: f32 = 3.0;
const SIZE: egui::Vec2 = egui::vec2(640.0, 440.0);
/// Keys and clicks do not close the splash until this long after the start-up: the ones made
/// while the program was busy arrive in a heap, and nobody has read the splash yet.
const GRACE: Duration = Duration::from_millis(600);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Settings,
    MidiPorts,
    DrumInput,
    ControllerPresets,
    Controller,
    SoundOutputs,
    Score,
}

/// One finished step: what it found, and whether that is as hoped for.
struct Line {
    text: String,
    ok: bool,
}

/// The sound outputs and the default one; asking the sound system takes a while, so it
/// is done on a thread while the other steps run.
type SoundScan = (Vec<String>, Option<String>);

pub(super) fn scan_sound_outputs() -> Receiver<SoundScan> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send((metronome::output_devices(), metronome::default_output_device()));
    });
    rx
}

/// What the main window does on this frame.
#[derive(Debug, PartialEq)]
pub(super) enum Flow {
    /// The steps are running: the main window stays empty.
    Starting,
    /// The start-up is done (the splash may still be open): the main window works as usual.
    Go,
    /// As `Go`, but a key closed the splash on this frame and is not meant for the application.
    KeyTaken,
}

pub(super) struct Startup {
    steps: Vec<Step>,
    /// The step in progress; `steps.len()` when all are done.
    next: usize,
    /// Whether the splash showing that step has been drawn; the step runs on the frame after.
    drawn: bool,
    /// What the step in progress is doing, in words.
    label: String,
    log: Vec<Line>,
    /// When the step in progress was first run; its line says how long it took.
    began: Option<Instant>,
    sound: Receiver<SoundScan>,
    file: Option<PathBuf>,
    finished_at: Option<Instant>,
    frames: u32,
    /// Where the splash opens: the middle of the main window, worked out once.
    position: Option<egui::Pos2>,
}

impl Startup {
    pub(super) fn new(file: Option<PathBuf>, sound: Receiver<SoundScan>) -> Startup {
        let mut steps = vec![
            Step::Settings,
            Step::MidiPorts,
            Step::DrumInput,
            Step::ControllerPresets,
            Step::Controller,
            Step::SoundOutputs,
        ];
        if file.is_some() {
            steps.push(Step::Score);
        }
        let label = label(steps[0], &Config::default(), file.as_deref());
        Startup { steps, next: 0, drawn: false, label, log: Vec::new(), began: None, sound, file, finished_at: None, frames: 0, position: None }
    }

    fn current(&self) -> Option<Step> {
        self.steps.get(self.next).copied()
    }

    fn is_finished(&self) -> bool {
        self.finished_at.is_some()
    }

    /// The step to run on this frame: the one in progress, once the splash showing it is on screen.
    fn due(&self) -> Option<Step> {
        self.drawn.then(|| self.current()).flatten()
    }

    fn finish(&mut self, line: Line) {
        self.log.push(line);
        self.next += 1;
        self.drawn = false;
        if self.current().is_none() {
            self.finished_at = Some(Instant::now());
        }
    }

    /// Whether a key or a click may close the splash now.
    fn may_close(&self) -> bool {
        self.finished_at.is_some_and(|t| t.elapsed() >= GRACE)
    }

    fn progress(&self) -> f32 {
        self.next as f32 / self.steps.len() as f32
    }
}

/// What the step is doing while it runs.
fn label(step: Step, cfg: &Config, file: Option<&Path>) -> String {
    match step {
        Step::Settings => "Reading your settings…".into(),
        Step::MidiPorts => "Looking for MIDI ports…".into(),
        Step::DrumInput if cfg.midi_in_port.trim().is_empty() => "Looking for the drum module…".into(),
        Step::DrumInput => format!("Connecting to the drum module “{}”…", cfg.midi_in_port),
        Step::ControllerPresets => "Reading the controller presets…".into(),
        Step::Controller if cfg.controller_preset.is_empty() => "Checking the controller setting…".into(),
        Step::Controller => format!("Connecting the controller “{}”…", cfg.controller_preset),
        Step::SoundOutputs => "Listing the sound outputs…".into(),
        Step::Score => {
            let name = file.and_then(|f| f.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            format!("Opening {name}…")
        }
    }
}

fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// (a key was pressed, a mouse button was pressed) in the window `ctx` is drawing now.
/// Key repeats do not count (egui counts the first event of a key it has not seen go down as a press).
fn pressed(ctx: &egui::Context) -> (bool, bool) {
    ctx.input(|i| {
        let key = i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, repeat: false, .. }));
        let click = i.events.iter().any(|e| matches!(e, egui::Event::PointerButton { pressed: true, .. }));
        (key, click)
    })
}

impl DrumApp {
    /// Runs the next step of the start-up, draws the splash window and closes it when a key or
    /// a mouse button is pressed in the application (the main window or the splash itself).
    pub(super) fn run_startup(&mut self, ctx: &egui::Context) -> Flow {
        let Some(mut st) = self.startup.take() else { return Flow::Go };
        let was_finished = st.is_finished();
        if let Some(step) = st.due() {
            let began = *st.began.get_or_insert_with(Instant::now);
            if let Some(mut line) = self.run_step(step, &mut st) {
                line.text = format!("{} · {} ms", line.text, began.elapsed().as_millis());
                st.began = None;
                st.finish(line);
                if let Some(next) = st.current() {
                    st.label = label(next, &self.cfg, st.file.as_deref());
                }
            }
        }
        if st.position.is_none() {
            st.position = ctx.input(|i| i.viewport().outer_rect).map(|r| r.center() - SIZE / 2.0);
        }
        let used_in_splash = show(ctx, &st);
        // The main window is shown after the first frame and comes up in front of the splash:
        // bring the splash back, and again when the steps are done.
        st.frames += 1;
        if st.frames == 2 || (!was_finished && st.is_finished()) {
            ctx.send_viewport_cmd_to(splash_id(), egui::ViewportCommand::Focus);
        }
        let (key, click) = pressed(ctx);
        if st.may_close() && (used_in_splash || key || click) {
            // Dropped: the window is no longer drawn, so egui closes it.
            return if key { Flow::KeyTaken } else { Flow::Go };
        }
        let flow = if st.is_finished() { Flow::Go } else { Flow::Starting };
        st.drawn = true;
        ctx.request_repaint();
        self.startup = Some(st);
        flow
    }

    /// Does the work of `step`. `None` when it is not finished yet (the sound system is still answering).
    fn run_step(&mut self, step: Step, st: &mut Startup) -> Option<Line> {
        Some(match step {
            Step::Settings => {
                let cfg = Config::load_from(&self.paths);
                self.saved_cfg = cfg.clone();
                self.cfg = cfg;
                match self.paths.config_file().filter(|p| p.is_file()) {
                    Some(p) => Line { text: format!("Settings read from {}", p.display()), ok: true },
                    None => Line { text: "No settings file yet, starting with the defaults".into(), ok: true },
                }
            }
            Step::MidiPorts => {
                self.midi.inputs = midi::list_inputs().unwrap_or_default();
                self.midi.outputs = midi::list_outputs().unwrap_or_default();
                Line {
                    text: format!("Found {} and {}", count(self.midi.inputs.len(), "MIDI input"), count(self.midi.outputs.len(), "MIDI output")),
                    ok: true,
                }
            }
            Step::DrumInput => {
                let chosen = !self.cfg.midi_in_port.trim().is_empty();
                self.connect_midi(None);
                if chosen {
                    Line { text: self.view.status.clone(), ok: self.midi.input.is_some() }
                } else {
                    Line { text: "No drum module chosen yet: choose its port in Settings".into(), ok: false }
                }
            }
            Step::ControllerPresets => {
                self.refresh_controllers();
                Line { text: format!("{} available", count(self.controller.preset_count(), "controller preset")), ok: true }
            }
            Step::Controller => {
                self.connect_controller();
                let chosen = !self.cfg.controller_preset.is_empty();
                Line { text: self.controller.message().to_string(), ok: !chosen || self.controller.is_connected() }
            }
            Step::SoundOutputs => match st.sound.try_recv() {
                Ok((devices, default)) => {
                    let line = match (&default, HAS_AUDIO) {
                        (_, false) => Line { text: "This build has no sound output: the click can still go to the drum module".into(), ok: true },
                        (Some(d), true) => Line { text: format!("{}, playing on {d}", count(devices.len(), "sound output")), ok: true },
                        (None, true) => Line { text: "No sound output found: the click and Listen will be silent".into(), ok: false },
                    };
                    self.audio_devices = devices;
                    self.audio_default = default;
                    line
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => Line { text: "The sound system did not answer".into(), ok: false },
            },
            Step::Score => {
                if let Some(path) = st.file.clone() {
                    self.open_file(&path);
                }
                match &self.view.error {
                    Some(e) => Line { text: e.clone(), ok: false },
                    None => Line { text: self.view.status.clone(), ok: true },
                }
            }
        })
    }
}

fn splash_id() -> ViewportId {
    ViewportId::from_hash_of("splash")
}

/// Draws the splash in a window of its own (a window inside the main one where the platform
/// has no second window). True when it was used: a key, a click or a request to close.
fn show(ctx: &egui::Context, st: &Startup) -> bool {
    let mut window = ViewportBuilder::default()
        .with_title("Roll Model")
        .with_inner_size(SIZE)
        .with_decorations(false)
        .with_resizable(false)
        .with_taskbar(false);
    if let Some(at) = st.position {
        window = window.with_position(at);
    }
    ctx.show_viewport_immediate(splash_id(), window, |ctx, class| {
        if class == ViewportClass::Embedded {
            egui::Window::new("Roll Model").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| contents(ui, st));
        } else {
            let frame = egui::Frame::default().fill(ctx.style().visuals.panel_fill).inner_margin(10.0);
            egui::CentralPanel::default().frame(frame).show(ctx, |ui| contents(ui, st));
            // The window has no frame of its own and would blend with the main window behind it.
            let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("splash_border"));
            ctx.layer_painter(layer).rect_stroke(ctx.screen_rect().shrink(BORDER / 2.0), 0.0, egui::Stroke::new(BORDER, render::ACCENT));
        }
        let (key, click) = pressed(ctx);
        key || click || ctx.input(|i| i.viewport().close_requested())
    })
}

fn contents(ui: &mut egui::Ui, st: &Startup) {
    ui.set_min_size(egui::vec2(SIZE.x - 40.0, SIZE.y - 40.0));
    ui.vertical_centered(|ui| {
        ui.add_space(30.0);
        ui.label(RichText::new("Roll Model").size(44.0).strong());
        ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).weak());
        ui.add_space(6.0);
        ui.label(TAGLINE);
        ui.add_space(26.0);
        ui.add(egui::ProgressBar::new(st.progress()).desired_width(380.0));
        ui.add_space(12.0);
        if st.is_finished() {
            ui.label(RichText::new("Ready. Press any key or click to close this window.").strong());
        } else {
            ui.horizontal(|ui| {
                // Centred by hand: a spinner and the text as one unit.
                let font = egui::TextStyle::Body.resolve(ui.style());
                let text_width = ui.painter().layout_no_wrap(st.label.clone(), font, ui.visuals().text_color()).size().x;
                ui.add_space(((ui.available_width() - text_width - 28.0) / 2.0).max(0.0));
                ui.add(egui::Spinner::new());
                ui.label(&st.label);
            });
        }
        ui.add_space(14.0);
        for line in &st.log {
            let text = RichText::new(&line.text).small();
            ui.label(if line.ok { text.weak() } else { text.color(render::AMBER) });
        }
        ui.add_space((ui.available_height() - 48.0).max(8.0));
        ui.label(RichText::new(COPYRIGHT).weak());
        widgets::hint(ui, LICENCE);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join("basic_rock_beat.musicxml")
    }

    fn starting(file: Option<PathBuf>) -> DrumApp {
        let (tx, rx) = mpsc::channel();
        tx.send((vec!["Speakers".to_string()], Some("Speakers".to_string()))).unwrap();
        let mut app = DrumApp::with_config(Config::default());
        app.startup = Some(Startup::new(file, rx));
        app
    }

    fn key() -> egui::Event {
        egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }

    fn click() -> egui::Event {
        egui::Event::PointerButton { pos: egui::pos2(10.0, 10.0), button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }
    }

    /// One frame of the application, headless, with these input events.
    fn frame_with(app: &mut DrumApp, ctx: &egui::Context, events: Vec<egui::Event>) -> Flow {
        let mut flow = Flow::Go;
        let _ = ctx.run(egui::RawInput { events, ..Default::default() }, |ctx| flow = app.run_startup(ctx));
        flow
    }

    fn frame(app: &mut DrumApp, ctx: &egui::Context) -> Flow {
        frame_with(app, ctx, Vec::new())
    }

    /// Frames until the steps are done; the splash is then still open.
    fn run_steps(app: &mut DrumApp, ctx: &egui::Context) -> usize {
        let mut starting = 0;
        while frame(app, ctx) == Flow::Starting {
            starting += 1;
            assert!(starting < 50, "the start-up does not finish");
        }
        starting
    }

    /// The splash as if the start-up had finished `ago` ago.
    fn finished_ago(app: &mut DrumApp, ago: Duration) {
        app.startup.as_mut().unwrap().finished_at = Instant::now().checked_sub(ago);
    }

    #[test]
    fn the_steps_are_the_same_for_every_start_but_a_score_is_opened_last() {
        let (_tx, rx) = mpsc::channel();
        let plain = Startup::new(None, rx);
        assert_eq!(plain.steps.first(), Some(&Step::Settings), "the others need the settings");
        assert!(!plain.steps.contains(&Step::Score));
        let (_tx, rx) = mpsc::channel();
        let with_file = Startup::new(Some(sample()), rx);
        assert_eq!(with_file.steps.last(), Some(&Step::Score));
        assert_eq!(with_file.steps.len(), plain.steps.len() + 1);
    }

    #[test]
    fn a_step_runs_only_after_the_splash_that_names_it_was_drawn() {
        let (_tx, rx) = mpsc::channel();
        let mut st = Startup::new(None, rx);
        assert_eq!(st.due(), None, "nothing is shown yet");
        st.drawn = true;
        assert_eq!(st.due(), Some(Step::Settings));
        st.finish(Line { text: "done".into(), ok: true });
        assert_eq!(st.due(), None, "the next step has not been shown yet");
        assert_eq!(st.current(), Some(Step::MidiPorts));
        assert!(st.progress() > 0.0 && st.progress() < 1.0);
        assert!(!st.is_finished());
    }

    #[test]
    fn the_labels_say_what_is_being_connected() {
        let mut cfg = Config::default();
        cfg.midi_in_port = "V31".into();
        cfg.controller_preset = "Behringer X-Touch Mini".into();
        assert!(label(Step::DrumInput, &cfg, None).contains("V31"));
        assert!(label(Step::Controller, &cfg, None).contains("X-Touch Mini"));
        assert!(label(Step::Score, &cfg, Some(Path::new("D:/scores/rock.musicxml"))).contains("rock.musicxml"));
        cfg.midi_in_port.clear();
        assert!(!label(Step::DrumInput, &cfg, None).contains('“'), "no port to name");
    }

    #[test]
    fn the_first_frame_only_shows_the_splash_and_the_splash_stays_after_the_last_step() {
        let ctx = egui::Context::default();
        let mut app = starting(Some(sample()));
        assert!(app.score.is_none());
        assert_eq!(frame(&mut app, &ctx), Flow::Starting, "the first frame draws the splash");
        assert!(app.score.is_none(), "and runs nothing");
        let starting = 1 + run_steps(&mut app, &ctx);
        assert_eq!(starting, 7, "one frame to show each of the 7 steps");
        assert!(app.startup.as_ref().is_some_and(|s| s.is_finished()), "the splash is still there to be read");
        assert!(app.score.is_some(), "the score given on the command line is open");
        assert!(app.view.tab == super::super::Tab::Score);
        assert_eq!(app.audio_devices, ["Speakers"]);
        assert_eq!(app.startup.as_ref().unwrap().log.len(), 7, "every step left a line");
    }

    #[test]
    fn a_slow_sound_system_keeps_the_start_going_without_blocking() {
        let ctx = egui::Context::default();
        let (tx, rx) = mpsc::channel();
        let mut app = DrumApp::with_config(Config::default());
        app.startup = Some(Startup::new(None, rx));
        for _ in 0..12 {
            assert_eq!(frame(&mut app, &ctx), Flow::Starting, "waiting for the sound outputs");
        }
        assert_eq!(app.startup.as_ref().and_then(|s| s.current()), Some(Step::SoundOutputs));
        tx.send((Vec::new(), None)).unwrap();
        assert_eq!(frame(&mut app, &ctx), Flow::Go, "the answer finishes the last step");
        assert!(app.audio_default.is_none());
    }

    #[test]
    fn keys_and_clicks_do_not_close_the_splash_while_the_program_starts() {
        let ctx = egui::Context::default();
        let mut app = starting(None);
        frame(&mut app, &ctx);
        assert_eq!(frame_with(&mut app, &ctx, vec![key(), click()]), Flow::Starting);
        assert!(app.startup.is_some());
        assert!(!app.startup.as_ref().unwrap().is_finished());
    }

    #[test]
    fn what_was_pressed_while_the_program_was_busy_does_not_close_it_at_once() {
        let ctx = egui::Context::default();
        let mut app = starting(None);
        run_steps(&mut app, &ctx);
        assert_eq!(frame_with(&mut app, &ctx, vec![key()]), Flow::Go, "just finished: nobody has read it yet");
        assert!(app.startup.is_some());
        finished_ago(&mut app, GRACE * 2);
        assert_eq!(frame(&mut app, &ctx), Flow::Go, "nothing pressed, so it stays");
        assert!(app.startup.is_some());
    }

    #[test]
    fn a_key_closes_the_splash_and_is_not_used_by_the_application() {
        let ctx = egui::Context::default();
        let mut app = starting(None);
        run_steps(&mut app, &ctx);
        finished_ago(&mut app, GRACE * 2);
        assert_eq!(frame_with(&mut app, &ctx, vec![key()]), Flow::KeyTaken);
        assert!(app.startup.is_none(), "closed");
        assert_eq!(frame(&mut app, &ctx), Flow::Go, "and it does not come back");
        assert!(app.startup.is_none());
    }

    #[test]
    fn a_mouse_click_closes_the_splash_and_goes_on_to_the_application() {
        let ctx = egui::Context::default();
        let mut app = starting(None);
        run_steps(&mut app, &ctx);
        finished_ago(&mut app, GRACE * 2);
        assert_eq!(frame_with(&mut app, &ctx, vec![click()]), Flow::Go);
        assert!(app.startup.is_none());
    }
}
