//! Application state and UI (Score / Metronome / Results / Settings tabs).

mod audio_output;
mod controller_help;
mod controls;
mod home;
mod metro;
mod mixer;
mod results;
mod rudiment;
mod score_tab;
mod session;
mod settings;
mod startup;
mod trigger;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::RichText;

use crate::analysis::{Expected, Hit, Report};
use crate::config::{Config, Paths, RowLayout};
use crate::control::Control;
use crate::drums::Lane;
use crate::loader::load_score;
use crate::metronome::Metronome;
use crate::midi::{self, EventQueue, InputHandle, RawEvent};
use crate::musicxml::ParseOptions;
use crate::playback::{Player, Ramp};
use crate::render::{self};
use crate::score::Score;
use crate::widgets;
use session::signed_secs;
use settings::SettingsPage;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Home,
    Score,
    Rudiments,
    Metronome,
    Results,
    Settings,
}

/// How long a changed range or tempo must stay put before Listen restarts with it.
const LISTEN_RESTART_DELAY: Duration = Duration::from_millis(300);

/// Which practice source the practice fields of `DrumApp` (score, range, tempo, take, history)
/// belong to: the Score tab's file or the Rudiments tab's rudiment.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Score,
    Rudiments,
}

/// The practice fields of the source that is not shown: swapped with the ones in `DrumApp` by
/// `DrumApp::activate`, so each tab keeps its own score, range, tempo, take and history.
struct Parked {
    score: Option<Score>,
    score_path: Option<PathBuf>,
    range_from: usize,
    range_to: usize,
    tempo_pct: f32,
    take: Option<Take>,
    takes_started: usize,
    history: Vec<TakeSummary>,
}

impl Parked {
    fn new() -> Parked {
        Parked {
            score: None,
            score_path: None,
            range_from: 0,
            range_to: 0,
            tempo_pct: 100.0,
            take: None,
            takes_started: 0,
            history: Vec::new(),
        }
    }
}

/// Height of the line above the score that shows the count-in and the take's figures, with room
/// above and below the big count-in number.
const TAKE_LINE_HEIGHT: f32 = 42.0;

/// A take of a rudiment counts as clean from this overall score on.
const CLEAN_TAKE: f64 = 85.0;

/// One recorded attempt over a measure range.
struct Take {
    number: usize,
    /// What was played: the score's title, or the rudiment with its repetitions.
    label: String,
    /// Match hits to notes with a window that follows the spacing of the notes (rudiments).
    adaptive: bool,
    /// Score time zero = start of `from` measure.
    zero: Instant,
    from: usize,
    to: usize,
    tempo_scale: f64,
    /// Seconds from score start to the `from` measure (at `tempo_scale`).
    t0: f64,
    end_sec: f64,
    expected: Vec<Expected>,
    hits: Vec<Hit>,
    report: Report,
    running: bool,
    count_in_beat_sec: f64,
}

/// The score being played back to the user ("Listen"), for the playhead.
struct Listen {
    /// Score time zero = start of the first measure of the range.
    zero: Instant,
    tempo_scale: f64,
    /// Seconds from score start to the first measure of the range.
    t0: f64,
    end_sec: f64,
    /// Seconds per beat of the first measure, and the beats of the count-in.
    beat_sec: f64,
    count_in_beats: u32,
    /// The tempo ramp it plays with (none: `every` is 0).
    ramp: Ramp,
    /// What it was started with; a change of range or tempo (or of the score) restarts it.
    from: usize,
    to: usize,
    tempo_pct: i32,
    gen: u32,
}

/// What a take or a playback of the selected range needs: where time zero is
/// and the click track.
struct Plan {
    from: usize,
    to: usize,
    scale: f64,
    bpm: f64,
    beat_sec: f64,
    count_in_beats: u32,
    zero: Instant,
    t0: f64,
    end_sec: f64,
    /// Count-in and beat clicks, seconds from the first measure.
    clicks: Vec<(f64, bool)>,
}

struct TakeSummary {
    number: usize,
    range: String,
    bpm: f64,
    overall: f64,
    letter: &'static str,
    accuracy: f64,
    mean_dt: f64,
    std_dt: f64,
    extra: usize,
}

struct Calibration {
    zero: Instant,
    beats: Vec<f64>,
    hits: Vec<f64>,
}

/// What the window shows and says: the tab and Settings page, the status line, the last error.
struct ViewState {
    tab: Tab,
    settings_page: SettingsPage,
    status: String,
    error: Option<String>,
    /// A fixed bars-per-row that does not fit, as reported by the last drawing of the score.
    fit_bars: Option<usize>,
}

/// The drum module's MIDI input: the ports, the connection, the events waiting and what the
/// settings page shows of them.
struct MidiState {
    inputs: Vec<String>,
    outputs: Vec<String>,
    input: Option<InputHandle>,
    queue: EventQueue,
    monitor: VecDeque<(RawEvent, Option<Lane>)>,
    learn: Option<Lane>,
    lane_text: BTreeMap<Lane, String>,
}

/// Listen, and the instrument played from the mixer.
struct Playback {
    /// Set while the playhead runs; the player may outlive it while the last notes ring.
    listen: Option<Listen>,
    player: Option<Player>,
    /// Live switches of Listen (the click switch is `cfg.metronome_during_take`).
    notes: bool,
    looping: bool,
    /// Range and tempo changed while listening, and since when: restarts once it settles.
    pending: Option<((usize, usize, i32, u32), Instant)>,
    /// Shared with the sound threads of Listen and of takes.
    control: Arc<Control>,
    /// Instruments soloed in the mixer (not saved).
    solo: BTreeSet<Lane>,
    /// One instrument played from the mixer; it has its own control so it never disturbs Listen.
    audition: Option<Player>,
    audition_control: Arc<Control>,
}

/// The free metronome tab's run, its sound thread and the settings that thread shares.
struct MetroState {
    run: Option<metro::Run>,
    player: Option<Player>,
    control: Arc<Control>,
}

pub struct DrumApp {
    cfg: Config,
    saved_cfg: Config,
    /// Where the settings, sounds and controller presets are kept.
    paths: Paths,
    view: ViewState,
    /// Whose practice fields (`score` to `history`, below) are in use; the other's are in `parked`.
    source: Source,
    parked: Parked,
    /// Counts the scores put in place, so that Listen notices a new one.
    score_gen: u32,
    /// The Rudiments tab's own state (the chosen rudiment itself is in `cfg`).
    rud: rudiment::State,
    /// The start trigger: two hits in a row start a recording; `trigger_learn` waits for the
    /// hit that chooses one of its two instruments.
    trigger: crate::trigger::Detector,
    trigger_learn: Option<crate::trigger::Slot>,
    score: Option<Score>,
    score_path: Option<PathBuf>,

    range_from: usize,
    range_to: usize,
    tempo_pct: f32,
    follow: bool,

    midi: MidiState,

    take: Option<Take>,
    takes_started: usize,
    history: Vec<TakeSummary>,
    metronome: Option<Metronome>,
    calibration: Option<Calibration>,

    playback: Playback,
    audio_devices: Vec<String>,
    /// Name of the system's default sound output, for the settings page.
    audio_default: Option<String>,
    /// The open sound output (see `audio_output.rs`).
    audio: audio_output::AudioOutput,

    /// The free metronome tab.
    metro_run: MetroState,

    /// The Mackie Control surface (X-Touch).
    controller: controls::Controller,

    /// The help screen of the controller, opened from its indicator in the top bar.
    controller_help: Option<controller_help::ControllerHelp>,

    /// The splash: `Some` until the start-up steps are done.
    startup: Option<startup::Startup>,
}

const KEYBOARD_PADS: [(egui::Key, Lane, &str); 13] = [
    (egui::Key::Z, Lane::Kick, "Z"),
    (egui::Key::X, Lane::Snare, "X"),
    (egui::Key::G, Lane::CrossStick, "G"),
    (egui::Key::C, Lane::HiHatClosed, "C"),
    (egui::Key::V, Lane::HiHatOpen, "V"),
    (egui::Key::B, Lane::HiHatPedal, "B"),
    (egui::Key::A, Lane::Tom1, "A"),
    (egui::Key::S, Lane::Tom2, "S"),
    (egui::Key::D, Lane::Tom3, "D"),
    (egui::Key::F, Lane::Tom4, "F"),
    (egui::Key::Q, Lane::Crash1, "Q"),
    (egui::Key::W, Lane::Ride, "W"),
    (egui::Key::E, Lane::Crash2, "E"),
];

impl DrumApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> DrumApp {
        // The grey helper texts are the "small" style: half as big again as egui's default.
        cc.egui_ctx.all_styles_mut(|style| {
            if let Some(small) = style.text_styles.get_mut(&egui::TextStyle::Small) {
                small.size *= 1.5;
            }
        });
        // Everything slow is a step of the splash (`startup.rs`), so that the first frame is not late.
        let mut app = DrumApp::with_config(Config::default());
        app.paths = Paths::system();
        app.startup = Some(startup::Startup::new(initial_file, startup::scan_sound_outputs()));
        app
    }

    /// The app in its starting state, without touching MIDI or sound devices.
    fn with_config(cfg: Config) -> DrumApp {
        DrumApp {
            saved_cfg: cfg.clone(),
            paths: Paths::none(),
            cfg,
            view: ViewState { tab: Tab::Home, settings_page: SettingsPage::MidiInput, status: String::new(), error: None, fit_bars: None },
            source: Source::Score,
            parked: Parked::new(),
            score_gen: 0,
            rud: rudiment::State::default(),
            trigger: crate::trigger::Detector::default(),
            trigger_learn: None,
            score: None,
            score_path: None,
            range_from: 0,
            range_to: 0,
            tempo_pct: 100.0,
            follow: true,
            midi: MidiState {
                inputs: Vec::new(),
                outputs: Vec::new(),
                input: None,
                queue: Arc::new(Mutex::new(Vec::new())),
                monitor: VecDeque::new(),
                learn: None,
                lane_text: BTreeMap::new(),
            },
            take: None,
            takes_started: 0,
            history: Vec::new(),
            metronome: None,
            calibration: None,
            playback: Playback {
                listen: None,
                player: None,
                notes: true,
                looping: false,
                pending: None,
                control: Control::new(),
                solo: BTreeSet::new(),
                audition: None,
                audition_control: Control::new(),
            },
            audio_devices: Vec::new(),
            audio_default: None,
            audio: audio_output::AudioOutput::default(),
            metro_run: MetroState { run: None, player: None, control: Control::new() },
            controller: controls::Controller::new(),
            controller_help: None,
            startup: None,
        }
    }

    // ----------------------------------------------------------------- MIDI

    fn refresh_ports(&mut self) {
        self.midi.inputs = midi::list_inputs().unwrap_or_default();
        self.midi.outputs = midi::list_outputs().unwrap_or_default();
        self.audio_devices = crate::audio::output_devices();
        self.audio_default = crate::audio::default_output_device();
    }

    fn connect_midi(&mut self, name: Option<String>) {
        self.midi.input = None; // close the old port first (Windows ports are exclusive)
        let wanted = name.unwrap_or_else(|| self.cfg.midi_in_port.clone());
        match midi::connect_input(&wanted, self.midi.queue.clone()) {
            Ok(h) => {
                self.view.status = format!("MIDI input: {}", h.port_name);
                self.cfg.midi_in_port = h.port_name.clone();
                self.midi.input = Some(h);
            }
            Err(e) => {
                self.view.status = format!("{e}. Choose the drum module's port in Settings.");
            }
        }
    }

    fn drain_midi(&mut self) {
        let events: Vec<RawEvent> = match self.midi.queue.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => return,
        };
        for ev in events {
            self.handle_event(ev);
        }
    }

    fn handle_event(&mut self, ev: RawEvent) {
        self.learn_note(ev);
        let lane = self.cfg.note_map.lane(ev.note).map(|l| l.by_hihat_pedal(ev.hihat, self.cfg.hihat_cc_max));
        self.midi.monitor.push_front((ev, lane));
        self.midi.monitor.truncate(14);
        self.record_calibration_hit(ev);
        if let Some(lane) = lane {
            // A hit that belongs to the start trigger is no note of the take it starts.
            if self.handle_trigger(lane, ev.at) {
                return;
            }
            self.record_take_hit(ev, lane);
        }
    }

    /// MIDI-learn: the note now belongs to the lane that was waiting for one, and to no other.
    fn learn_note(&mut self, ev: RawEvent) {
        if let Some(lane) = self.midi.learn.take() {
            self.cfg.note_map.assign(ev.note, lane);
            self.midi.lane_text.clear();
            self.view.status = format!("Note {} ({}) mapped to {}", ev.note, midi::note_name(ev.note), lane.label());
        }
    }

    fn record_calibration_hit(&mut self, ev: RawEvent) {
        if let Some(cal) = self.calibration.as_mut() {
            cal.hits.push(signed_secs(ev.at, cal.zero));
        }
    }

    /// A hit of `lane` goes into the take that is running (from a little before its first beat on).
    fn record_take_hit(&mut self, ev: RawEvent, lane: Lane) {
        let latency = self.cfg.latency_ms / 1000.0;
        if let Some(take) = self.take.as_mut().filter(|t| t.running) {
            let t = signed_secs(ev.at, take.zero) - latency;
            if t > -take.count_in_beat_sec * 0.75 {
                take.hits.push(Hit { t, note: ev.note, velocity: ev.velocity, lane });
            }
        }
    }

    fn keyboard_pads(&mut self, ctx: &egui::Context) {
        if !self.cfg.keyboard_input || widgets::typing(ctx) {
            return;
        }
        let pressed: Vec<Lane> = ctx.input(|i| {
            KEYBOARD_PADS.iter().filter(|(k, _, _)| i.key_pressed(*k)).map(|(_, l, _)| *l).collect()
        });
        for lane in pressed {
            let note = self.cfg.note_map.notes_for(lane).first().copied().unwrap_or(0);
            self.handle_event(RawEvent { at: Instant::now(), note, velocity: 100, channel: 10, hihat: None });
        }
    }

    // ---------------------------------------------------------------- files

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("MusicXML", &["musicxml", "xml", "mxl"])
            .pick_file()
        {
            self.open_file(&p);
        }
    }

    /// Makes `source` the one the practice fields belong to. What is playing or recording stops;
    /// the other source's score, range, tempo, take and history are parked, not lost.
    fn activate(&mut self, source: Source) {
        if self.source == source {
            return;
        }
        self.stop_take(false);
        self.stop_listen();
        self.playback.pending = None;
        let Parked { score, score_path, range_from, range_to, tempo_pct, take, takes_started, history } = &mut self.parked;
        std::mem::swap(&mut self.score, score);
        std::mem::swap(&mut self.score_path, score_path);
        std::mem::swap(&mut self.range_from, range_from);
        std::mem::swap(&mut self.range_to, range_to);
        std::mem::swap(&mut self.tempo_pct, tempo_pct);
        std::mem::swap(&mut self.take, take);
        std::mem::swap(&mut self.takes_started, takes_started);
        std::mem::swap(&mut self.history, history);
        self.source = source;
        self.score_gen += 1;
    }

    /// Whether a score file is loaded, whichever tab is showing.
    fn has_main_score(&self) -> bool {
        match self.source {
            Source::Score => self.score.is_some(),
            Source::Rudiments => self.parked.score.is_some(),
        }
    }

    /// The file of the loaded score, whichever tab is showing.
    fn main_path(&self) -> Option<PathBuf> {
        match self.source {
            Source::Score => self.score_path.clone(),
            Source::Rudiments => self.parked.score_path.clone(),
        }
    }

    /// The file of the Score tab's score as the settings know it (an absolute path).
    fn score_key(&self) -> Option<String> {
        let path = self.main_path()?;
        Some(std::path::absolute(&path).unwrap_or(path).to_string_lossy().into_owned())
    }

    /// Puts the row layout kept for the score on screen. A score never laid out keeps what is set.
    fn restore_layout(&mut self) {
        let Some(layout) = self.score_key().and_then(|k| self.cfg.layout_of(&k)) else { return };
        self.cfg.bars_per_row = layout.bars_per_row;
        self.cfg.layout_align_bar = layout.align_bar;
    }

    /// Keeps the row layout on screen for the score, after the user changed it.
    fn keep_layout(&mut self) {
        if let Some(key) = self.score_key() {
            self.cfg.remember_layout(&key, RowLayout { bars_per_row: self.cfg.bars_per_row, align_bar: self.cfg.layout_align_bar });
        }
    }

    /// What the current practice is called: the score's title, or the rudiment and its repetitions.
    fn practice_label(&self) -> String {
        let Some(score) = self.score.as_ref() else { return String::new() };
        match self.source {
            Source::Score => score.title.clone(),
            Source::Rudiments => format!("{} ×{}", score.title, score.measures.len()),
        }
    }

    fn open_file(&mut self, path: &Path) {
        self.activate(Source::Score);
        self.stop_take(false);
        self.stop_listen();
        let opts = ParseOptions { expand_repeats: self.cfg.expand_repeats };
        match load_score(path, &opts) {
            Ok(score) => {
                self.view.status = format!(
                    "Loaded \"{}\": {} measures, {} notes, {:.0} BPM",
                    score.title,
                    score.measures.len(),
                    score.notes.len(),
                    score.tempo.bpm_at(0.0)
                );
                self.range_from = 0;
                self.range_to = score.measures.len().saturating_sub(1);
                self.score = Some(score);
                self.score_path = Some(path.to_path_buf());
                let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
                self.cfg.remember_recent(&absolute.to_string_lossy());
                self.restore_layout();
                self.take = None;
                self.view.error = None;
                self.score_gen += 1;
                self.view.tab = Tab::Score;
            }
            Err(e) => self.view.error = Some(format!("Could not load {}: {e}", path.display())),
        }
    }

    // ---------------------------------------------------------------- takes

    // -------------------------------------------------------------- listen

    // ---------------------------------------------------------------- views

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("📂 Open MusicXML…").clicked() {
                self.open_dialog();
            }
            if let Some(p) = self.main_path() {
                if ui.button("⟳ Reload").on_hover_text("Reload the file (e.g. after editing it)").clicked() {
                    self.open_file(&p);
                }
            }
            ui.separator();
            ui.selectable_value(&mut self.view.tab, Tab::Home, "🏠 Home");
            ui.add_enabled_ui(self.has_main_score(), |ui| {
                ui.selectable_value(&mut self.view.tab, Tab::Score, "🎼 Score");
            });
            ui.selectable_value(&mut self.view.tab, Tab::Rudiments, "🔁 Rudiments");
            ui.selectable_value(&mut self.view.tab, Tab::Metronome, "⏱ Metronome");
            ui.selectable_value(&mut self.view.tab, Tab::Results, "📊 Results");
            ui.selectable_value(&mut self.view.tab, Tab::Settings, "⚙ Settings");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match &self.midi.input {
                    Some(h) => ui.label(RichText::new(format!("⏺ {}", h.port_name)).color(render::GREEN)),
                    None => ui.label(RichText::new("⏺ no MIDI input").color(render::RED)),
                };
                if let Some((text, connected)) = self.controller.label(!self.cfg.controller_preset.is_empty()) {
                    let indicator = egui::Label::new(RichText::new(text).color(if connected { render::GREEN } else { render::RED })).sense(egui::Sense::click());
                    let response = ui.add(indicator).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Click to see what the controller's controls do");
                    if response.clicked() {
                        self.toggle_controller_help();
                    }
                }
                if let Some((ev, lane)) = self.midi.monitor.front() {
                    if ev.at.elapsed() < Duration::from_millis(1500) {
                        ui.label(format!(
                            "{} · note {} · vel {}",
                            lane.map(|l| l.label()).unwrap_or("unmapped"),
                            ev.note,
                            ev.velocity
                        ));
                    }
                }
            });
        });
    }

}

impl eframe::App for DrumApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // The splash window (`startup.rs`); the main window stays empty while the steps run.
        let mut key_taken = false;
        if self.startup.is_some() {
            match self.run_startup(ctx) {
                startup::Flow::Starting => {
                    egui::CentralPanel::default().show(ctx, |_| {});
                    return;
                }
                startup::Flow::KeyTaken => key_taken = true,
                startup::Flow::Go => {}
            }
        }
        // Files dropped onto the window
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if let Some(p) = dropped.first() {
            self.open_file(p);
        }

        self.update_state(ctx, key_taken);
        if !widgets::typing(ctx) && !key_taken {
            self.shortcut_keys(ctx);
        }
        self.draw(ctx);
        self.save_settings_if_changed();
        self.schedule_repaint(ctx);
    }
}

impl DrumApp {
    /// What happens every frame before anything is drawn: MIDI in, the controller, and the
    /// lifecycles of takes, Listen, the metronome, auditions and the calibration.
    fn update_state(&mut self, ctx: &egui::Context, key_taken: bool) {
        self.sync_source();
        self.drain_midi();
        self.update_controller();
        if !key_taken {
            self.keyboard_pads(ctx);
        }
        self.sync_control();
        self.update_take();
        self.update_listen();
        self.update_metro();
        self.update_audition();
        self.update_calibration();
        self.show_sound_errors();
    }

    /// R records, L listens, M starts the metronome, K switches the click, Esc stops.
    fn shortcut_keys(&mut self, ctx: &egui::Context) {
        let (rec, listen, metro, click, esc) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::R),
                i.key_pressed(egui::Key::L),
                i.key_pressed(egui::Key::M),
                i.key_pressed(egui::Key::K),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if click {
            self.cfg.metronome_during_take = !self.cfg.metronome_during_take;
        }
        let running = self.take.as_ref().is_some_and(|t| t.running);
        if esc && (running || self.playback.player.is_some() || self.metro_run.run.is_some()) {
            self.stop_take(false);
            self.stop_listen();
            self.stop_metro();
        } else if rec && !running && self.score.is_some() {
            self.start_take();
        } else if listen && !running && self.score.is_some() {
            if self.playback.listen.is_some() {
                self.stop_listen();
            } else {
                self.start_listen();
            }
        } else if metro && !running {
            if self.metro_run.run.is_some() {
                self.stop_metro();
            } else {
                self.start_metro();
            }
        }
    }

    /// The top bar, the status line, the tab that is shown and the windows of the controller.
    fn draw(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| self.top_bar(ui));
        // A tab clicked just now needs its practice source before it is drawn.
        self.sync_source();
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(e) = self.view.error.clone() {
                    ui.label(RichText::new(e).color(render::RED));
                    if ui.small_button("Dismiss").clicked() {
                        self.view.error = None;
                    }
                } else {
                    ui.label(RichText::new(&self.view.status).small());
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| match self.view.tab {
            Tab::Home => egui::ScrollArea::vertical().show(ui, |ui| self.welcome_panel(ui)).inner,
            Tab::Score => self.score_tab(ui),
            Tab::Rudiments => self.rudiments_tab(ui),
            Tab::Metronome => self.metronome_tab(ui),
            Tab::Results => self.results_tab(ui),
            Tab::Settings => self.settings_tab(ui),
        });

        self.controller_help_window(ctx);
        self.controller_editor_window(ctx);
    }

    fn save_settings_if_changed(&mut self) {
        if self.cfg != self.saved_cfg {
            if let Err(e) = self.cfg.save(&self.paths) {
                self.view.status = format!("Could not save settings: {e}");
            }
            self.saved_cfg = self.cfg.clone();
        }
    }

    /// Keep animating while recording, calibrating, listening or clicking; otherwise poll MIDI
    /// often enough for the monitor and MIDI-learn.
    fn schedule_repaint(&self, ctx: &egui::Context) {
        if self.take.as_ref().is_some_and(|t| t.running)
            || self.calibration.is_some()
            || self.playback.listen.is_some()
            || self.metro_run.run.is_some()
        {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join("basic_rock_beat.musicxml")
    }

    #[test]
    fn the_app_starts_on_home_with_no_score() {
        let app = DrumApp::with_config(Config::default());
        assert!(app.view.tab == Tab::Home);
        assert!(app.score.is_none(), "so the Score tab is greyed out");
    }

    #[test]
    fn a_score_gets_its_row_layout_back_when_it_is_opened_again() {
        let mut app = DrumApp::with_config(Config::default());
        app.open_file(&sample());
        assert_eq!((app.cfg.bars_per_row, app.cfg.layout_align_bar), (0, 0), "a score never laid out keeps what is set");
        app.cfg.bars_per_row = 3;
        app.cfg.layout_align_bar = 2;
        app.keep_layout();
        // Another layout is on screen for another score; this one comes back on opening.
        app.cfg.bars_per_row = 0;
        app.cfg.layout_align_bar = 0;
        app.open_file(&sample());
        assert_eq!((app.cfg.bars_per_row, app.cfg.layout_align_bar), (3, 2));
        // Back to automatic: forgotten, and a later score keeps what is on screen.
        app.cfg.bars_per_row = 0;
        app.cfg.layout_align_bar = 0;
        app.keep_layout();
        assert!(app.cfg.layouts.is_empty());
    }

    #[test]
    fn opening_a_score_goes_to_the_score_tab_and_a_failed_open_stays_put() {
        let mut app = DrumApp::with_config(Config::default());
        app.open_file(&sample());
        assert!(app.score.is_some());
        assert!(app.view.tab == Tab::Score);
        assert_eq!(app.cfg.recent_files.len(), 1, "remembered for the Home tab");

        app.view.tab = Tab::Home;
        app.open_file(Path::new("no/such/score.musicxml"));
        assert!(app.view.error.is_some());
        assert!(app.view.tab == Tab::Home, "a failed open does not leave Home");
        assert!(app.score.is_some(), "the loaded score stays");
        assert_eq!(app.cfg.recent_files.len(), 1, "a failed file is not remembered");
    }
}
