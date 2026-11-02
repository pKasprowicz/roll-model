//! Application state and UI: the Home, Score, Rudiments, Metronome, Results and Settings tabs, and the windows of the controller.

mod audio_output;
mod controller_help;
mod controls;
mod debounce;
mod home;
mod import_menu;
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
use crate::controller::Action;
use crate::drums::Lane;
use crate::import::load_score;
use crate::metronome::Metronome;
use crate::midi::{self, EventQueue, InputHandle, RawEvent};
use crate::playback::{Player, Ramp};
use crate::practice_tempo::PracticeTempo;
use crate::score::Score;
use crate::theme;
use crate::units::{Millis, Seconds};
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

/// How long a changed range, tempo or metronome setting must stay put before what is playing restarts with it.
const RESTART_DELAY: Duration = Duration::from_millis(300);
/// Lines kept of the monitor of the drum module and of the controller.
const MONITOR_LINES: usize = 14;

/// Which practice source the practice fields of `DrumApp` (score, range, tempo, take, history)
/// belong to: the Score tab's file or the Rudiments tab's rudiment.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Score,
    Rudiments,
}

/// Everything that belongs to one source of practice (the Score tab's file or the Rudiments tab's
/// rudiment): its score, where it came from, the selected bars, the tempo, the take being played
/// and what was played. `DrumApp` holds two, the one in use and the one parked; `activate` swaps
/// them as a whole, so a field added here cannot be forgotten by the swap.
struct Practice {
    score: Option<Score>,
    score_path: Option<PathBuf>,
    /// What the import found out about the score, when it was read from a file.
    score_report: Option<crate::import::Report>,
    /// How a MIDI file is laid out; the Score tab's "MIDI import" menu changes it.
    midi_options: crate::import::MidiOptions,
    range_from: usize,
    range_to: usize,
    /// The practice tempo, with its unit: per cent of the written one for a score, BPM for a rudiment.
    tempo: PracticeTempo,
    take: Option<Take>,
    takes_started: usize,
    history: Vec<TakeSummary>,
}

impl Practice {
    fn new() -> Practice {
        Practice {
            score: None,
            score_path: None,
            score_report: None,
            midi_options: crate::import::MidiOptions::default(),
            range_from: 0,
            range_to: 0,
            tempo: PracticeTempo::default(),
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
    t0: Seconds,
    end_sec: Seconds,
    expected: Vec<Expected>,
    hits: Vec<Hit>,
    report: Report,
    running: bool,
    count_in_beat_sec: Seconds,
}

/// The score being played back to the user ("Listen"), for the playhead.
struct Listen {
    /// Score time zero = start of the first measure of the range.
    zero: Instant,
    tempo_scale: f64,
    /// Seconds from score start to the first measure of the range.
    t0: Seconds,
    end_sec: Seconds,
    /// Seconds per beat of the first measure, and the beats of the count-in.
    beat_sec: Seconds,
    count_in_beats: u32,
    /// The tempo ramp it plays with (none: `every` is 0).
    ramp: Ramp,
    /// What it was started with; a change of range or tempo (or of the score) restarts it.
    from: usize,
    to: usize,
    tempo_key: i32,
    gen: u32,
}

/// What a take or a playback of the selected range needs: where time zero is
/// and the click track.
struct Plan {
    from: usize,
    to: usize,
    scale: f64,
    bpm: f64,
    beat_sec: Seconds,
    count_in_beats: u32,
    zero: Instant,
    t0: Seconds,
    end_sec: Seconds,
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
    restart: debounce::Debounce<(usize, usize, i32, u32)>,
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
    /// What the sound threads reported, kept beside the settings.
    log: crate::log::Log,
    view: ViewState,
    /// Whose practice is the one in `practice`; the other's is in `parked`.
    source: Source,
    practice: Practice,
    parked: Practice,
    /// Counts the scores put in place, so that Listen notices a new one.
    score_gen: u32,
    /// The Rudiments tab's own state (the chosen rudiment itself is in `cfg`).
    rud: rudiment::State,
    /// The start trigger: two hits in a row start a recording; `trigger_learn` waits for the
    /// hit that chooses one of its two instruments.
    trigger: crate::trigger::Detector,
    trigger_learn: Option<crate::trigger::Slot>,
    /// A big file being read on another thread.
    loading: Option<import_menu::PendingLoad>,
    /// Since when the settings differ from the file; they are written a little later, not every frame.
    settings_dirty_since: Option<Instant>,
    /// Files over this many bytes are read on another thread, so the window does not freeze.
    background_bytes: u64,

    follow: bool,

    midi: MidiState,

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

/// How long changed settings wait before they are written.
const SETTINGS_SAVE_DELAY: Duration = Duration::from_millis(500);

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
        app.log = crate::log::Log::at(app.paths.log_file());
        app.startup = Some(startup::Startup::new(initial_file, startup::scan_sound_outputs()));
        app
    }

    /// The app in its starting state, without touching MIDI or sound devices.
    fn with_config(cfg: Config) -> DrumApp {
        DrumApp {
            saved_cfg: cfg.clone(),
            paths: Paths::none(),
            log: Default::default(),
            cfg,
            view: ViewState { tab: Tab::Home, settings_page: SettingsPage::MidiInput, status: String::new(), error: None, fit_bars: None },
            source: Source::Score,
            practice: Practice::new(),
            parked: Practice::new(),
            score_gen: 0,
            rud: rudiment::State::default(),
            trigger: crate::trigger::Detector::default(),
            trigger_learn: None,
            loading: None,
            settings_dirty_since: None,
            background_bytes: 1 << 20,
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
            metronome: None,
            calibration: None,
            playback: Playback {
                listen: None,
                player: None,
                notes: true,
                looping: false,
                restart: Default::default(),
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

    /// Connects the drum module's input; `Ok` holds the name of the port. Says nothing.
    fn connect_midi(&mut self, name: Option<String>) -> Result<String, midi::MidiError> {
        self.midi.input = None; // close the old port first (Windows ports are exclusive)
        let wanted = name.unwrap_or_else(|| self.cfg.midi_in_port.clone());
        let handle = midi::connect_input(&wanted, self.midi.queue.clone())?;
        let port = handle.port_name.clone();
        self.cfg.midi_in_port = port.clone();
        self.midi.input = Some(handle);
        Ok(port)
    }

    /// The line that tells how connecting the drum module went.
    fn midi_input_text(outcome: &Result<String, midi::MidiError>) -> String {
        match outcome {
            Ok(port) => format!("MIDI input: {port}"),
            Err(e) => format!("{e}. Choose the drum module's port in Settings."),
        }
    }

    /// Connects the drum module's input and shows how it went on the status line.
    fn connect_midi_and_say(&mut self, name: Option<String>) {
        let outcome = self.connect_midi(name);
        self.view.status = Self::midi_input_text(&outcome);
    }

    fn drain_midi(&mut self) {
        let events: Vec<RawEvent> = std::mem::take(&mut *crate::sync::locked(&self.midi.queue));
        for ev in events {
            self.handle_event(ev);
        }
    }

    fn handle_event(&mut self, ev: RawEvent) {
        self.learn_note(ev);
        let lane = self.cfg.note_map.lane(ev.note).map(|l| l.by_hihat_pedal(ev.hihat, self.cfg.hihat_cc_max));
        self.midi.monitor.push_front((ev, lane));
        self.midi.monitor.truncate(MONITOR_LINES);
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
            cal.hits.push(signed_secs(ev.at, cal.zero).0);
        }
    }

    /// A hit of `lane` goes into the take that is running (from a little before its first beat on).
    fn record_take_hit(&mut self, ev: RawEvent, lane: Lane) {
        let latency = Millis(self.cfg.latency_ms).secs();
        if let Some(take) = self.practice.take.as_mut().filter(|t| t.running) {
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
        let pressed: Vec<Lane> = ctx.input(|i| KEYBOARD_PADS.iter().filter(|(k, _, _)| i.key_pressed(*k)).map(|(_, l, _)| *l).collect());
        for lane in pressed {
            let note = self.cfg.note_map.notes_for(lane).first().copied().unwrap_or(0);
            self.handle_event(RawEvent { at: Instant::now(), note, velocity: 100, channel: 10, hihat: None });
        }
    }

    // ---------------------------------------------------------------- files

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Scores (MusicXML, MIDI)", crate::import::EXTENSIONS).pick_file() {
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
        self.playback.restart.cancel();
        std::mem::swap(&mut self.practice, &mut self.parked);
        self.source = source;
        self.score_gen += 1;
    }

    /// The Score tab's practice, whichever tab is showing.
    fn score_practice(&self) -> &Practice {
        match self.source {
            Source::Score => &self.practice,
            Source::Rudiments => &self.parked,
        }
    }

    /// Whether a score file is loaded, whichever tab is showing.
    fn has_main_score(&self) -> bool {
        self.score_practice().score.is_some()
    }

    /// The file of the loaded score, whichever tab is showing.
    fn main_path(&self) -> Option<PathBuf> {
        self.score_practice().score_path.clone()
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
        let Some(score) = self.practice.score.as_ref() else { return String::new() };
        match self.source {
            Source::Score => score.title.clone(),
            Source::Rudiments => format!("{} ×{}", score.title, score.measures.len()),
        }
    }

    /// Opens a score file and shows how it went on the status line (or as an error).
    fn open_file(&mut self, path: &Path) {
        match self.try_open(path) {
            Ok(status) => {
                self.view.status = status;
                self.view.error = None;
            }
            Err(error) => self.view.error = Some(error),
        }
    }

    /// Opens a score file and says nothing: `Ok` holds the line for the status bar, `Err` the
    /// reason it could not be opened. (A big file is read on another thread; the line then says it is loading.)
    fn try_open(&mut self, path: &Path) -> Result<String, String> {
        self.activate(Source::Score);
        self.stop_take(false);
        self.stop_listen();
        self.practice.midi_options = crate::import::MidiOptions::default();
        if std::fs::metadata(path).is_ok_and(|m| m.len() > self.background_bytes) {
            return Ok(self.start_loading(path));
        }
        self.loading = None;
        match load_score(path, &self.import_options()) {
            Ok(loaded) => Ok(self.install(loaded, path, false)),
            Err(e) => Err(format!("Could not load {}: {e}", path.display())),
        }
    }

    // ---------------------------------------------------------------- takes

    // -------------------------------------------------------------- listen

    // ---------------------------------------------------------------- views

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("📂 Open score…").clicked() {
                self.open_dialog();
            }
            if let Some(p) = self.main_path() {
                if ui.button("⟳ Reload").on_hover_text("Reload the file (e.g. after editing it)").clicked() {
                    self.open_file(&p);
                }
            }
            ui.separator();
            // The tab is chosen here and changed after the buttons: `show_tab` also puts its practice in place.
            let mut tab = self.view.tab;
            ui.selectable_value(&mut tab, Tab::Home, "🏠 Home");
            ui.add_enabled_ui(self.has_main_score(), |ui| {
                ui.selectable_value(&mut tab, Tab::Score, "🎼 Score");
            });
            ui.selectable_value(&mut tab, Tab::Rudiments, "🔁 Rudiments");
            ui.selectable_value(&mut tab, Tab::Metronome, "⏱ Metronome");
            ui.selectable_value(&mut tab, Tab::Results, "📊 Results");
            ui.selectable_value(&mut tab, Tab::Settings, "⚙ Settings");
            if tab != self.view.tab {
                self.show_tab(tab);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match &self.midi.input {
                    Some(h) => ui.label(RichText::new(format!("⏺ {}", h.port_name)).color(theme::GREEN)),
                    None => ui.label(RichText::new("⏺ no MIDI input").color(theme::RED)),
                };
                if let Some((text, connected)) = self.controller.label(!self.cfg.controller_preset.is_empty()) {
                    let indicator = egui::Label::new(RichText::new(text).color(if connected { theme::GREEN } else { theme::RED }))
                        .sense(egui::Sense::click());
                    let response = ui
                        .add(indicator)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("Click to see what the controller's controls do");
                    if response.clicked() {
                        self.toggle_controller_help();
                    }
                }
                if let Some((ev, lane)) = self.midi.monitor.front() {
                    if ev.at.elapsed() < Duration::from_millis(1500) {
                        ui.label(format!("{} · note {} · vel {}", lane.map(|l| l.label()).unwrap_or("unmapped"), ev.note, ev.velocity));
                    }
                }
            });
        });
    }
}

impl eframe::App for DrumApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush_draft();
        self.save_settings_now();
    }

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
        self.remember_tempo();
        self.poll_loading();
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
        // The keys are the same commands as the buttons of a controller, through the same function:
        // the same rules, the same lock, the same messages.
        let pressed = |key| ctx.input(|i| i.key_pressed(key));
        for (key, action) in [
            (egui::Key::K, Action::Click),
            (egui::Key::Escape, Action::Stop),
            (egui::Key::R, Action::Record),
            (egui::Key::L, Action::Listen),
            (egui::Key::M, Action::Metronome),
        ] {
            if pressed(key) {
                self.press(action);
            }
        }
    }

    /// The top bar, the status line, the tab that is shown and the windows of the controller.
    fn draw(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| self.top_bar(ui));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(e) = self.view.error.clone() {
                    ui.label(RichText::new(e).color(theme::RED));
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

    /// Writes the settings when they have been different from the file for [`SETTINGS_SAVE_DELAY`]
    /// (a slider being dragged changes them every frame), and once more on exit.
    fn save_settings_if_changed(&mut self) {
        if self.cfg == self.saved_cfg {
            self.settings_dirty_since = None;
            return;
        }
        let since = *self.settings_dirty_since.get_or_insert_with(Instant::now);
        if since.elapsed() >= SETTINGS_SAVE_DELAY {
            self.save_settings_now();
        }
    }

    fn save_settings_now(&mut self) {
        self.settings_dirty_since = None;
        if self.cfg == self.saved_cfg {
            return;
        }
        if let Err(e) = self.cfg.save(&self.paths) {
            self.view.status = format!("Could not save settings: {e}");
        }
        self.saved_cfg = self.cfg.clone();
    }

    /// Keep animating while recording, calibrating, listening or clicking; otherwise poll MIDI
    /// often enough for the monitor and MIDI-learn.
    fn schedule_repaint(&self, ctx: &egui::Context) {
        if self.take_running() || self.calibration.is_some() || self.playback.listen.is_some() || self.metro_run.run.is_some() {
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
        assert!(app.practice.score.is_none(), "so the Score tab is greyed out");
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
        assert!(app.practice.score.is_some());
        assert!(app.view.tab == Tab::Score);
        assert_eq!(app.cfg.recent_files.len(), 1, "remembered for the Home tab");

        app.view.tab = Tab::Home;
        app.open_file(Path::new("no/such/score.musicxml"));
        assert!(app.view.error.is_some());
        assert!(app.view.tab == Tab::Home, "a failed open does not leave Home");
        assert!(app.practice.score.is_some(), "the loaded score stays");
        assert_eq!(app.cfg.recent_files.len(), 1, "a failed file is not remembered");
    }

    #[test]
    fn a_midi_score_is_read_again_with_new_import_settings_and_keeps_its_place() {
        let mut app = DrumApp::with_config(Config::default());
        let midi = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join("basic_rock_beat.mid");
        app.open_file(&midi);
        assert!(app.is_midi_score(), "{:?}", app.view.error);
        assert_eq!(app.practice.score.as_ref().unwrap().measures.len(), 4);
        assert!(app.view.status.contains("sixteenths") || app.view.status.contains("eighths"), "{}", app.view.status);
        // The Score tab, with its MIDI menu, draws.
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.score_tab(ui));
            });
        }
        app.practice.range_from = 1;
        app.practice.range_to = 2;
        app.view.tab = Tab::Results;
        let gen = app.score_gen;

        app.practice.midi_options.first_bar_beats = 1;
        app.reimport();
        let score = app.practice.score.as_ref().unwrap();
        assert_eq!(score.measures[0].len_q, crate::units::Quarters(1.0), "a short first bar");
        assert_eq!((app.practice.range_from, app.practice.range_to), (1, 2), "the selected bars stay");
        assert!(app.view.tab == Tab::Results, "the tab stays");
        assert!(app.score_gen > gen);

        // Opening a file again starts from the default settings.
        app.open_file(&midi);
        assert_eq!(app.practice.midi_options, crate::import::MidiOptions::default());
        assert_eq!(app.practice.score.as_ref().unwrap().measures[0].len_q, crate::units::Quarters(4.0));

        // A MusicXML score has no such menu.
        app.open_file(&sample());
        assert!(!app.is_midi_score());
    }

    #[test]
    fn a_big_file_is_read_on_another_thread_and_arrives_later() {
        let mut app = DrumApp::with_config(Config::default());
        app.background_bytes = 0;
        app.open_file(&sample());
        assert!(app.practice.score.is_none(), "not there yet");
        assert!(app.loading.is_some());
        assert!(app.view.status.starts_with("Loading"), "{}", app.view.status);
        for _ in 0..500 {
            app.poll_loading();
            if app.loading.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(app.loading.is_none(), "finished");
        assert!(app.practice.score.is_some() && app.view.tab == Tab::Score);
        assert_eq!(app.cfg.recent_files.len(), 1);

        // A file that cannot be read says so, and the loaded score stays.
        let bad = std::env::temp_dir().join(format!("roll-model-bad-{}.mid", std::process::id()));
        std::fs::write(&bad, b"MThd not a midi file").unwrap();
        app.open_file(&bad);
        for _ in 0..500 {
            app.poll_loading();
            if app.loading.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = std::fs::remove_file(&bad);
        assert!(app.view.error.as_deref().is_some_and(|e| e.contains("Could not load")), "{:?}", app.view.error);
        assert!(app.practice.score.is_some(), "the loaded score stays");
    }

    #[test]
    fn opening_connecting_and_stopping_return_what_happened_and_say_nothing_themselves() {
        let mut app = DrumApp::with_config(Config::default());
        app.view.status = "before".into();

        let line = app.try_open(&sample()).unwrap();
        assert!(line.starts_with("Loaded \"Basic Rock Beat\""), "{line}");
        let reason = app.try_open(Path::new("no/such/score.musicxml")).unwrap_err();
        assert!(reason.starts_with("Could not load"), "{reason}");
        assert_eq!(app.view.status, "before", "the caller shows it");
        assert!(app.view.error.is_none());

        let outcome = app.connect_midi(Some("no such port".into()));
        assert!(outcome.is_err());
        assert!(DrumApp::midi_input_text(&outcome).ends_with("Choose the drum module's port in Settings."));
        assert_eq!(app.view.status, "before");
        app.connect_midi_and_say(Some("no such port".into()));
        assert!(app.view.status.contains("Choose the drum module"), "{}", app.view.status);

        assert!(matches!(app.finish_take(false), session::TakeOutcome::NoTake));
        assert!(app.view.status.contains("Choose the drum module"), "stopping nothing leaves the status alone");

        // The line of a clean take comes back from the function that decides it, not from the status.
        app.view.tab = Tab::Rudiments;
        app.sync_source();
        assert!(app.note_clean_rudiment(90.0).starts_with("Clean take at 90 BPM"));
    }

    #[test]
    fn every_sound_problem_goes_to_the_log_and_the_first_is_shown_with_the_count_of_the_rest() {
        let dir = std::env::temp_dir().join(format!("roll-model-sound-errors-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = DrumApp::with_config(Config::default());
        app.log = crate::log::Log::at(Some(dir.join("roll-model.log")));
        app.report_sound_errors(vec![]);
        assert!(app.view.error.is_none(), "nothing to say");
        app.report_sound_errors(vec!["one".into()]);
        assert_eq!(app.view.error.as_deref(), Some("one"));
        app.report_sound_errors(vec!["first".into(), "second".into(), "third".into()]);
        assert_eq!(app.view.error.as_deref(), Some("first (and 2 more problems, see the log)"));
        let log = std::fs::read_to_string(dir.join("roll-model.log")).unwrap();
        assert_eq!(log.lines().count(), 4);
        assert!(log.contains(" second") && log.contains(" third"), "{log}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn showing_a_tab_puts_its_practice_in_place_at_once_and_the_buttons_use_the_command_path() {
        let mut app = DrumApp::with_config(Config::default());
        app.show_tab(Tab::Rudiments);
        assert!(app.source == Source::Rudiments, "no frame has to pass");
        assert!(app.practice.score.is_some(), "the rudiment is built");
        app.show_tab(Tab::Metronome);
        assert!(app.source == Source::Rudiments, "the other tabs keep the source");
        // The buttons of the screen ask for the same actions as the keys: the same refusal.
        app.show_tab(Tab::Home);
        app.parked.score = None;
        app.practice.score = None;
        app.press(Action::Listen);
        assert_eq!(app.view.status, "Open a score first.");
        app.view.status.clear();
        app.press(Action::Click);
        assert!(app.cfg.metronome_during_take != Config::default().metronome_during_take);
    }

    #[test]
    fn a_scores_import_settings_go_with_it_when_the_tab_changes() {
        let mut app = DrumApp::with_config(Config::default());
        let midi = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join("basic_rock_beat.mid");
        app.open_file(&midi);
        app.practice.midi_options.first_bar_beats = 1;
        app.reimport();
        assert!(app.is_midi_score());

        app.view.tab = Tab::Rudiments;
        app.sync_source();
        assert!(!app.is_midi_score(), "the rudiment is not a MIDI file");
        assert_eq!(app.practice.midi_options, crate::import::MidiOptions::default(), "and has no settings of the file");
        assert!(app.has_main_score(), "the file is still loaded");
        assert_eq!(app.score_practice().midi_options.first_bar_beats, 1, "with its settings");
        assert_eq!(app.import_options().midi.first_bar_beats, 1, "which the next import uses");

        app.view.tab = Tab::Score;
        app.sync_source();
        assert!(app.is_midi_score());
        assert_eq!(app.practice.midi_options.first_bar_beats, 1);
    }

    #[test]
    fn a_file_that_arrives_while_the_rudiments_tab_shows_goes_to_the_score_tab() {
        let mut app = DrumApp::with_config(Config::default());
        app.background_bytes = 0;
        app.open_file(&sample());
        app.view.tab = Tab::Rudiments;
        app.sync_source();
        assert!(app.practice.score.as_ref().is_some_and(|s| s.title != "Basic Rock Beat"), "the rudiment is on screen");
        for _ in 0..500 {
            app.poll_loading();
            if app.loading.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(app.loading.is_none());
        app.sync_source();
        assert!(app.view.tab == Tab::Score, "the file is shown");
        assert_eq!(app.practice.score.as_ref().map(|s| s.title.as_str()), Some("Basic Rock Beat"));
        assert!(app.practice.score_path.is_some());

        // The rudiment is still there when the tab is opened again.
        app.view.tab = Tab::Rudiments;
        app.sync_source();
        assert!(app.practice.score.as_ref().is_some_and(|s| s.title != "Basic Rock Beat"));
        app.view.tab = Tab::Score;
        app.sync_source();
        assert_eq!(app.practice.score.as_ref().map(|s| s.title.as_str()), Some("Basic Rock Beat"), "and the file after that");
    }

    #[test]
    fn changed_settings_are_written_after_a_short_wait_and_on_exit_not_every_frame() {
        let dir = std::env::temp_dir().join(format!("roll-model-app-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = DrumApp::with_config(Config::default());
        app.paths = crate::config::Paths::in_dir(dir.clone());
        let file = dir.join("config.txt");

        app.cfg.zoom = 1.25;
        for _ in 0..30 {
            app.save_settings_if_changed(); // thirty frames of a slider being dragged
        }
        assert!(!file.exists(), "nothing is written while the value is still moving");
        assert!(app.settings_dirty_since.is_some());

        app.settings_dirty_since = Some(Instant::now() - SETTINGS_SAVE_DELAY);
        app.save_settings_if_changed();
        assert!(std::fs::read_to_string(&file).unwrap().contains("zoom = 1.25"));
        assert!(app.cfg == app.saved_cfg && app.settings_dirty_since.is_none());

        // Closing the window writes what is still waiting.
        app.cfg.zoom = 2.5;
        app.save_settings_if_changed();
        app.save_settings_now();
        assert!(std::fs::read_to_string(&file).unwrap().contains("zoom = 2.5"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_timing_and_sound_pages_draw_with_a_stale_latency_and_no_open_output() {
        let mut app = DrumApp::with_config(Config { latency_ms: 24.0, ..Config::default() });
        assert!(app.cfg.latency_is_stale(crate::audio::ENGINE_GENERATION), "a latency from before the engine says is stale");
        assert!(app.audio.info().is_none(), "no output is open until a sound starts");
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    app.settings_timing(ui);
                    app.settings_sound(ui);
                });
            });
        }
        app.cfg.latency_engine = crate::audio::ENGINE_GENERATION;
        assert!(!app.cfg.latency_is_stale(crate::audio::ENGINE_GENERATION));
    }
}
