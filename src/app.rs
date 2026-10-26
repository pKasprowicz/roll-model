//! Application state and UI (Score / Metronome / Results / Settings tabs).

mod controls;
mod metro;
mod mixer;
mod rudiment;
mod trigger;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Color32, RichText};

use crate::analysis::{analyze_with, expected_notes, Expected, Grade, Hit, Report};
use crate::config::Config;
use crate::control::Control;
use crate::drums::{format_note_list, parse_note_list, Lane};
use crate::loader::load_score;
use crate::metronome::{self, ClickOptions, Metronome, HAS_AUDIO};
use crate::midi::{self, EventQueue, InputHandle, RawEvent};
use crate::musicxml::ParseOptions;
use crate::playback::{self, PlayOptions, Player};
use crate::render::{self, HitStatus, LayoutOpts, PlayedNote, ScoreView};
use crate::score::Score;
use crate::widgets;

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

/// The pages of the Settings tab.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsPage {
    MidiInput,
    Controller,
    Timing,
    Sound,
    General,
}

impl SettingsPage {
    const ALL: [SettingsPage; 5] =
        [SettingsPage::MidiInput, SettingsPage::Controller, SettingsPage::Timing, SettingsPage::Sound, SettingsPage::General];

    fn label(self) -> &'static str {
        match self {
            SettingsPage::MidiInput => "MIDI input",
            SettingsPage::Controller => "Controller",
            SettingsPage::Timing => "Timing",
            SettingsPage::Sound => "Sound",
            SettingsPage::General => "General",
        }
    }
}

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

pub struct DrumApp {
    cfg: Config,
    saved_cfg: Config,
    tab: Tab,
    settings_page: SettingsPage,
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
    status: String,
    error: Option<String>,

    range_from: usize,
    range_to: usize,
    tempo_pct: f32,
    follow: bool,

    midi_inputs: Vec<String>,
    midi_outputs: Vec<String>,
    midi_in: Option<InputHandle>,
    queue: EventQueue,
    monitor: VecDeque<(RawEvent, Option<Lane>)>,
    learn: Option<Lane>,
    lane_text: BTreeMap<Lane, String>,

    take: Option<Take>,
    takes_started: usize,
    history: Vec<TakeSummary>,
    metronome: Option<Metronome>,
    calibration: Option<Calibration>,

    /// Set while the playhead runs; the player may outlive it while the last notes ring.
    listen: Option<Listen>,
    player: Option<Player>,
    /// Live switches of Listen (the click switch is `cfg.metronome_during_take`).
    listen_notes: bool,
    listen_loop: bool,
    /// Shared with the sound threads of Listen and of takes.
    control: Arc<Control>,

    /// Instruments soloed in the mixer (not saved).
    solo: BTreeSet<Lane>,
    /// One instrument played from the mixer; it has its own control so it never disturbs Listen.
    audition: Option<Player>,
    audition_control: Arc<Control>,
    audio_devices: Vec<String>,
    /// Name of the system's default sound output, for the settings page.
    audio_default: Option<String>,
    /// A fixed bars-per-row that does not fit, as reported by the last drawing of the score.
    fit_bars: Option<usize>,
    /// Range and tempo changed while listening, and since when: restarts once it settles.
    listen_pending: Option<((usize, usize, i32, u32), Instant)>,

    /// The free metronome tab.
    metro: Option<metro::Run>,
    metro_player: Option<Player>,
    metro_control: Arc<Control>,

    /// The Mackie Control surface (X-Touch).
    controller: controls::Controller,
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
        let mut app = DrumApp::with_config(Config::load());
        app.refresh_ports();
        app.connect_midi(None);
        app.refresh_controllers();
        app.connect_controller();
        if let Some(p) = initial_file {
            app.open_file(&p);
        }
        app
    }

    /// The app in its starting state, without touching MIDI or sound devices.
    fn with_config(cfg: Config) -> DrumApp {
        DrumApp {
            saved_cfg: cfg.clone(),
            cfg,
            tab: Tab::Home,
            settings_page: SettingsPage::MidiInput,
            source: Source::Score,
            parked: Parked::new(),
            score_gen: 0,
            rud: rudiment::State::default(),
            trigger: crate::trigger::Detector::default(),
            trigger_learn: None,
            score: None,
            score_path: None,
            status: String::new(),
            error: None,
            range_from: 0,
            range_to: 0,
            tempo_pct: 100.0,
            follow: true,
            midi_inputs: Vec::new(),
            midi_outputs: Vec::new(),
            midi_in: None,
            queue: Arc::new(Mutex::new(Vec::new())),
            monitor: VecDeque::new(),
            learn: None,
            lane_text: BTreeMap::new(),
            take: None,
            takes_started: 0,
            history: Vec::new(),
            metronome: None,
            calibration: None,
            listen: None,
            player: None,
            listen_notes: true,
            listen_loop: false,
            control: Control::new(),
            solo: BTreeSet::new(),
            audition: None,
            audition_control: Control::new(),
            audio_devices: Vec::new(),
            audio_default: None,
            fit_bars: None,
            listen_pending: None,
            metro: None,
            metro_player: None,
            metro_control: Control::new(),
            controller: controls::Controller::new(),
        }
    }

    // ----------------------------------------------------------------- MIDI

    fn refresh_ports(&mut self) {
        self.midi_inputs = midi::list_inputs().unwrap_or_default();
        self.midi_outputs = midi::list_outputs().unwrap_or_default();
        self.audio_devices = metronome::output_devices();
        self.audio_default = metronome::default_output_device();
    }

    fn connect_midi(&mut self, name: Option<String>) {
        self.midi_in = None; // close the old port first (Windows ports are exclusive)
        let wanted = name.unwrap_or_else(|| self.cfg.midi_in_port.clone());
        match midi::connect_input(&wanted, self.queue.clone()) {
            Ok(h) => {
                self.status = format!("MIDI input: {}", h.port_name);
                self.cfg.midi_in_port = h.port_name.clone();
                self.midi_in = Some(h);
            }
            Err(e) => {
                self.status = format!("{e}. Choose the drum module's port in Settings.");
            }
        }
    }

    fn drain_midi(&mut self) {
        let events: Vec<RawEvent> = match self.queue.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => return,
        };
        for ev in events {
            self.handle_event(ev);
        }
    }

    fn handle_event(&mut self, ev: RawEvent) {
        if let Some(lane) = self.learn.take() {
            // MIDI-learn: the note now belongs to this lane only.
            self.cfg.note_map.assign(ev.note, lane);
            self.lane_text.clear();
            self.status = format!("Note {} ({}) mapped to {}", ev.note, midi::note_name(ev.note), lane.label());
        }
        let lane = self.cfg.note_map.lane(ev.note).map(|l| l.by_hihat_pedal(ev.hihat, self.cfg.hihat_cc_max));
        self.monitor.push_front((ev, lane));
        self.monitor.truncate(14);

        if let Some(cal) = self.calibration.as_mut() {
            cal.hits.push(signed_secs(ev.at, cal.zero));
        }
        // A hit that belongs to the start trigger is no note of the take it starts.
        if let Some(lane) = lane {
            if self.handle_trigger(lane, ev.at) {
                return;
            }
        }
        let latency = self.cfg.latency_ms / 1000.0;
        if let (Some(take), Some(lane)) = (self.take.as_mut(), lane) {
            if take.running {
                let t = signed_secs(ev.at, take.zero) - latency;
                if t > -take.count_in_beat_sec * 0.75 {
                    take.hits.push(Hit { t, note: ev.note, velocity: ev.velocity, lane });
                }
            }
        }
    }

    fn keyboard_pads(&mut self, ctx: &egui::Context) {
        if !self.cfg.keyboard_input || ctx.wants_keyboard_input() {
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
        self.listen_pending = None;
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
                self.status = format!(
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
                self.take = None;
                self.error = None;
                self.score_gen += 1;
                self.tab = Tab::Score;
            }
            Err(e) => self.error = Some(format!("Could not load {}: {e}", path.display())),
        }
    }

    // ---------------------------------------------------------------- takes

    fn click_options(&self) -> ClickOptions {
        ClickOptions {
            audio: self.cfg.click_audio && HAS_AUDIO,
            audio_device: self.cfg.audio_output.clone(),
            volume: self.cfg.click_volume,
            midi_port: self.cfg.click_midi.then(|| self.cfg.midi_out_port.clone()),
            midi_note: self.cfg.click_note,
            midi_channel: self.cfg.click_channel,
        }
    }

    /// Copies the settings that can change while sound is playing into the
    /// controls the sound threads read.
    fn sync_control(&mut self) {
        let c = &self.control;
        c.set_notes(self.listen_notes);
        c.set_click(self.cfg.metronome_during_take);
        c.set_looping(self.listen_loop);
        c.set_volume(self.cfg.playback_volume);
        c.set_click_volume(self.cfg.click_volume);
        for lane in Lane::ALL {
            c.set_lane_gain(lane, self.cfg.effective_gain(&self.solo, lane));
        }
        // The free metronome always clicks and loops.
        let m = &self.metro_control;
        m.set_click(true);
        m.set_looping(true);
        m.set_click_volume(self.cfg.click_volume);
        // An instrument played from the mixer sounds at its own gain even when muted there.
        let a = &self.audition_control;
        a.set_volume(self.cfg.playback_volume);
        for lane in Lane::ALL {
            a.set_lane_gain(lane, self.cfg.gain_of(lane));
        }
    }

    /// Timing for the selected measures at the chosen tempo, starting now:
    /// half a second of lead (the sound device needs time to open), then the
    /// count-in if wanted, then the music. The click track always has the
    /// beat clicks of the music; the click switch decides whether they sound.
    fn plan_run(&self, count_in: bool) -> Option<Plan> {
        let score = self.score.as_ref()?;
        if score.measures.is_empty() {
            return None;
        }
        let from = self.range_from.min(score.measures.len() - 1);
        let to = self.range_to.clamp(from, score.measures.len() - 1);
        let scale = self.tempo_pct as f64 / 100.0;
        let m0 = &score.measures[from];
        let bpm = score.tempo.bpm_at(m0.start_q) * scale;
        let beat_sec = 60.0 / bpm * m0.beat_q();
        let count_in_beats = if count_in { self.cfg.count_in_bars * m0.beats } else { 0 };
        let lead = 0.5;
        let zero = Instant::now() + Duration::from_secs_f64(lead + count_in_beats as f64 * beat_sec);
        let t0 = score.tempo.q_to_sec(m0.start_q, scale);
        let end_sec = score.tempo.q_to_sec(score.measures[to].end_q(), scale) - t0;
        let clicks = score.click_track(from, to, scale, count_in_beats, true);
        Some(Plan { from, to, scale, bpm, beat_sec, count_in_beats, zero, t0, end_sec, clicks })
    }

    fn start_take(&mut self) {
        self.stop_take(false);
        self.stop_listen();
        self.stop_metro();
        self.sync_control();
        if self.source == Source::Rudiments && !self.rudiment().gradable() {
            self.status = format!("{} cannot be graded stroke by stroke: use Listen.", self.rudiment().name);
            return;
        }
        let label = self.practice_label();
        let adaptive = self.source == Source::Rudiments;
        let Some(plan) = self.plan_run(true) else { return };
        let Some(score) = self.score.as_ref() else { return };
        let expected = expected_notes(score, plan.from, plan.to, plan.scale);
        self.status = format!(
            "Recording measures {}–{} at {:.0} BPM",
            score.measures[plan.from].number,
            score.measures[plan.to].number,
            plan.bpm
        );
        self.metronome = Some(Metronome::start(plan.zero, plan.clicks, self.click_options(), Some(self.control.clone())));
        self.takes_started += 1;
        self.take = Some(Take {
            number: self.takes_started,
            zero: plan.zero,
            from: plan.from,
            to: plan.to,
            tempo_scale: plan.scale,
            t0: plan.t0,
            end_sec: plan.end_sec,
            report: analyze_with(&expected, &[], &self.cfg.tol, f64::NEG_INFINITY, adaptive),
            label,
            adaptive,
            expected,
            hits: Vec::new(),
            running: true,
            count_in_beat_sec: plan.beat_sec,
        });
    }

    // -------------------------------------------------------------- listen

    fn play_options(&self) -> PlayOptions {
        let c = &self.cfg;
        PlayOptions {
            audio: c.playback_audio && HAS_AUDIO,
            audio_device: c.audio_output.clone(),
            sounds_dir: Config::sounds_dir(),
            click_audio: c.click_audio && HAS_AUDIO,
            midi_port: (c.playback_midi || c.click_midi).then(|| c.midi_out_port.clone()),
            midi_drums: c.playback_midi,
            drum_channel: c.playback_channel,
            click_midi: c.click_midi,
            click_note: c.click_note,
            click_channel: c.click_channel,
            drum_notes: Lane::ALL.iter().filter_map(|l| c.note_map.playback_note(*l).map(|n| (*l, n))).collect(),
        }
    }

    /// The message to show when no output could possibly make a sound.
    fn nothing_audible(opts: &PlayOptions) -> Option<String> {
        if opts.audio || opts.midi_drums || opts.click_audio || opts.click_midi {
            return None;
        }
        Some(if HAS_AUDIO {
            "Playback is switched off: enable a sound output on the Sound page of Settings.".into()
        } else {
            "This build has no sound output: send playback to a MIDI output on the Sound page of Settings.".into()
        })
    }

    /// Plays the selected measures at the chosen tempo, with the count-in and
    /// click a take would have, so the drummer can hear them first.
    fn start_listen(&mut self) {
        self.begin_listen(true);
    }

    fn begin_listen(&mut self, count_in: bool) {
        self.stop_take(false);
        self.stop_listen();
        self.stop_metro();
        self.listen_pending = None;
        let opts = self.play_options();
        if let Some(message) = Self::nothing_audible(&opts) {
            self.error = Some(message);
            return;
        }
        self.sync_control();
        let Some(plan) = self.plan_run(count_in) else { return };
        let Some(score) = self.score.as_ref() else { return };
        let expected = expected_notes(score, plan.from, plan.to, plan.scale);
        self.status = if expected.is_empty() {
            "There are no notes in the selected measures; only the click plays.".into()
        } else {
            format!(
                "Listening to measures {}–{} at {:.0} BPM",
                score.measures[plan.from].number,
                score.measures[plan.to].number,
                plan.bpm
            )
        };
        let cues = playback::cues(&expected, &plan.clicks);
        self.player = Some(Player::start(plan.zero, cues, plan.end_sec, opts, self.control.clone()));
        self.listen = Some(Listen {
            zero: plan.zero,
            tempo_scale: plan.scale,
            t0: plan.t0,
            end_sec: plan.end_sec,
            beat_sec: plan.beat_sec,
            count_in_beats: plan.count_in_beats,
            from: plan.from,
            to: plan.to,
            tempo_pct: self.tempo_pct.round() as i32,
            gen: self.score_gen,
        });
    }

    /// Stops at once, also cutting off cymbals that are still ringing.
    fn stop_listen(&mut self) {
        if let Some(mut p) = self.player.take() {
            p.stop();
        }
        self.listen = None;
    }

    /// The range and tempo Listen would start with now.
    fn listen_signature(&self) -> Option<(usize, usize, i32, u32)> {
        let n = self.score.as_ref()?.measures.len();
        if n == 0 {
            return None;
        }
        let from = self.range_from.min(n - 1);
        Some((from, self.range_to.clamp(from, n - 1), self.tempo_pct.round() as i32, self.score_gen))
    }

    fn update_listen(&mut self) {
        if self.listen.is_some() && self.control.music_done() {
            self.listen = None; // the music is over; the last notes may still ring
        }
        // Changing the range or the tempo while listening restarts it, without the count-in, once the
        // change has settled (a slider being dragged changes it on every frame).
        let wanted = match (self.listen.as_ref(), self.listen_signature()) {
            (Some(l), Some(sig)) if (l.from, l.to, l.tempo_pct, l.gen) != sig => Some(sig),
            _ => None,
        };
        match (wanted, self.listen_pending) {
            (None, _) => self.listen_pending = None,
            (Some(sig), Some((pending, since))) if pending == sig => {
                if since.elapsed() >= LISTEN_RESTART_DELAY {
                    self.begin_listen(false);
                }
            }
            (Some(sig), _) => self.listen_pending = Some((sig, Instant::now())),
        }
        let Some(player) = self.player.as_ref() else { return };
        if let Some(e) = player.take_errors().into_iter().next() {
            self.error = Some(e);
        }
        if player.is_finished() {
            self.listen = None;
            self.player = None;
        }
    }

    /// Shows the first problem the metronome thread of a take or a calibration ran into.
    fn show_sound_errors(&mut self) {
        if let Some(m) = self.metronome.as_ref() {
            if let Some(e) = m.take_errors().into_iter().next() {
                self.error = Some(e);
            }
        }
    }

    /// "bar 3 · beat 2", or the count-in, for the label next to the Listen button.
    fn listen_position(&self) -> Option<String> {
        let l = self.listen.as_ref()?;
        let score = self.score.as_ref()?;
        let elapsed = signed_secs(Instant::now(), l.zero);
        if elapsed < 0.0 {
            let left = (-elapsed / l.beat_sec).ceil() as u32;
            return Some(if left > l.count_in_beats { "starting…".into() } else { format!("count-in {left}") });
        }
        let e = if elapsed >= l.end_sec { elapsed.rem_euclid(l.end_sec) } else { elapsed };
        let q = score.tempo.sec_to_q(e + l.t0, l.tempo_scale);
        let m = &score.measures[score.measure_at(q)?];
        let beat = (((q - m.start_q) / m.beat_q()).floor() as u32 + 1).min(m.beats);
        Some(format!("bar {} · beat {}", m.number, beat))
    }

    /// The playhead of Listen, in quarter notes; it wraps around while looping.
    fn listen_playhead(&self) -> Option<f64> {
        let l = self.listen.as_ref()?;
        let score = self.score.as_ref()?;
        let elapsed = signed_secs(Instant::now(), l.zero);
        let e = if elapsed >= l.end_sec { elapsed.rem_euclid(l.end_sec) } else { elapsed };
        Some(score.tempo.sec_to_q(e + l.t0, l.tempo_scale))
    }

    /// Plays one instrument on its own so it can be heard (and tuned) without a score.
    fn audition(&mut self, lane: Lane) {
        let opts = self.play_options();
        if !opts.audio && !opts.midi_drums {
            self.error = Some(Self::nothing_audible(&PlayOptions { click_audio: false, click_midi: false, ..opts }).unwrap_or_else(|| {
                "Switch on a sound output for Listen on the Sound page of Settings to hear the instruments.".into()
            }));
            return;
        }
        if let Some(mut p) = self.audition.take() {
            p.stop();
        }
        self.sync_control();
        let cue = playback::Cue { t: 0.0, sound: playback::Sound::Drum { lane, velocity: 100 } };
        let start = Instant::now() + Duration::from_millis(300);
        self.audition = Some(Player::start(start, vec![cue], 0.01, opts, self.audition_control.clone()));
    }

    fn update_audition(&mut self) {
        let Some(p) = self.audition.as_ref() else { return };
        if let Some(e) = p.take_errors().into_iter().next() {
            self.error = Some(e);
        }
        if p.is_finished() {
            self.audition = None;
        }
    }

    /// `completed`: the whole range was played (otherwise notes after the
    /// stop point are left ungraded).
    fn stop_take(&mut self, completed: bool) {
        if let Some(mut m) = self.metronome.take() {
            m.stop();
        }
        let Some(take) = self.take.as_mut() else { return };
        if !take.running {
            return;
        }
        take.running = false;
        let elapsed = if completed { f64::INFINITY } else { signed_secs(Instant::now(), take.zero) };
        take.report = analyze_with(&take.expected, &take.hits, &self.cfg.tol, elapsed, take.adaptive);
        let r = &take.report;
        if r.evaluated == 0 {
            self.status = "Take stopped before any notes were due — nothing to grade.".into();
            return;
        }
        let bpm = self
            .score
            .as_ref()
            .map(|s| s.tempo.bpm_at(s.measures[take.from].start_q) * take.tempo_scale)
            .unwrap_or(0.0);
        let range = if take.adaptive {
            take.label.clone()
        } else {
            self.score
                .as_ref()
                .map(|s| format!("{}–{}", s.measures[take.from].number, s.measures[take.to].number))
                .unwrap_or_default()
        };
        let clean = completed && take.adaptive && r.overall >= CLEAN_TAKE;
        self.history.push(TakeSummary {
            number: take.number,
            range,
            bpm,
            overall: r.overall,
            letter: r.letter(),
            accuracy: r.accuracy,
            mean_dt: r.mean_dt_ms,
            std_dt: r.std_dt_ms,
            extra: r.extra,
        });
        self.status = format!(
            "Take {}: {:.0} ({}) — accuracy {:.0}%, timing {:+.0} ± {:.0} ms, {} missed, {} extra",
            take.number,
            r.overall,
            r.letter(),
            r.accuracy * 100.0,
            r.mean_dt_ms,
            r.std_dt_ms,
            r.missed,
            r.extra
        );
        if clean {
            self.note_clean_rudiment(bpm);
        }
    }

    fn update_take(&mut self) {
        let Some(take) = self.take.as_mut() else { return };
        if !take.running {
            return;
        }
        let elapsed = signed_secs(Instant::now(), take.zero);
        take.report = analyze_with(&take.expected, &take.hits, &self.cfg.tol, elapsed, take.adaptive);
        if elapsed > take.end_sec + 1.0 {
            self.stop_take(true);
        }
    }

    fn start_calibration(&mut self) {
        self.stop_take(false);
        self.stop_listen();
        self.stop_metro();
        let beat = 0.6; // 100 BPM
        let zero = Instant::now() + Duration::from_secs_f64(1.0);
        let beats: Vec<f64> = (0..16).map(|i| i as f64 * beat).collect();
        let clicks = beats.iter().map(|t| (*t, false)).collect();
        // No control: the calibration clicks must sound whatever the click switch says.
        self.metronome = Some(Metronome::start(zero, clicks, self.click_options(), None));
        self.calibration = Some(Calibration { zero, beats, hits: Vec::new() });
        self.status = "Calibrating: hit any pad together with the 16 clicks…".into();
    }

    fn update_calibration(&mut self) {
        let Some(cal) = self.calibration.as_ref() else { return };
        let now = signed_secs(Instant::now(), cal.zero);
        if now < cal.beats.last().copied().unwrap_or(0.0) + 0.6 {
            return;
        }
        // Skip the first 4 clicks (getting into the groove).
        let mut offsets: Vec<f64> = cal.beats[4..]
            .iter()
            .filter_map(|b| {
                cal.hits
                    .iter()
                    .map(|h| h - b)
                    .filter(|d| d.abs() < 0.25)
                    .min_by(|x, y| x.abs().total_cmp(&y.abs()))
            })
            .collect();
        self.calibration = None;
        self.metronome = None;
        if offsets.len() < 6 {
            self.status = "Calibration failed: not enough hits near the clicks. Is the metronome audible?".into();
            return;
        }
        offsets.sort_by(|a, b| a.total_cmp(b));
        let median = offsets[offsets.len() / 2] * 1000.0;
        self.cfg.latency_ms = median.round();
        self.status = format!("Latency calibrated: {:.0} ms ({} hits)", median, offsets.len());
    }

    // ---------------------------------------------------------------- views

    fn played_notes(&self) -> Vec<PlayedNote> {
        let (Some(take), Some(score)) = (self.take.as_ref(), self.score.as_ref()) else { return Vec::new() };
        take.hits
            .iter()
            .enumerate()
            .map(|(hi, h)| {
                let matched = take.report.hit_match.get(hi).copied().flatten();
                let (status, dt) = match matched {
                    Some(ei) => {
                        let r = take.report.results[ei];
                        let s = match r.grade {
                            Grade::Perfect => HitStatus::Perfect,
                            Grade::Good => HitStatus::Good,
                            Grade::Ok => HitStatus::Ok,
                            Grade::WrongPad => HitStatus::WrongPad,
                            _ => HitStatus::Neutral,
                        };
                        (s, r.dt_ms)
                    }
                    None if h.t > take.end_sec + 0.2 || h.t < -0.2 => (HitStatus::Neutral, None),
                    None => (HitStatus::Extra, None),
                };
                PlayedNote {
                    q: score.tempo.sec_to_q(h.t + take.t0, take.tempo_scale),
                    lane: h.lane,
                    velocity: h.velocity,
                    status,
                    dt_ms: dt,
                }
            })
            .collect()
    }

    fn note_grades(&self) -> Vec<Option<Grade>> {
        let n = self.score.as_ref().map(|s| s.notes.len()).unwrap_or(0);
        let mut g = vec![None; n];
        if let Some(take) = self.take.as_ref() {
            for (e, r) in take.expected.iter().zip(&take.report.results) {
                if let Some(slot) = g.get_mut(e.note_idx) {
                    *slot = Some(r.grade);
                }
            }
        }
        g
    }

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
            ui.selectable_value(&mut self.tab, Tab::Home, "🏠 Home");
            ui.add_enabled_ui(self.has_main_score(), |ui| {
                ui.selectable_value(&mut self.tab, Tab::Score, "🎼 Score");
            });
            ui.selectable_value(&mut self.tab, Tab::Rudiments, "🔁 Rudiments");
            ui.selectable_value(&mut self.tab, Tab::Metronome, "⏱ Metronome");
            ui.selectable_value(&mut self.tab, Tab::Results, "📊 Results");
            ui.selectable_value(&mut self.tab, Tab::Settings, "⚙ Settings");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match &self.midi_in {
                    Some(h) => ui.label(RichText::new(format!("⏺ {}", h.port_name)).color(render::GREEN)),
                    None => ui.label(RichText::new("⏺ no MIDI input").color(render::RED)),
                };
                if let Some((text, connected)) = self.controller.label(!self.cfg.controller_preset.is_empty()) {
                    ui.label(RichText::new(text).color(if connected { render::GREEN } else { render::RED }));
                }
                if let Some((ev, lane)) = self.monitor.front() {
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

    /// Shown instead of the score until one is loaded: how to start, and the
    /// scores opened before.
    fn welcome_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(36.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Roll Model").size(34.0).strong());
            ui.add_space(4.0);
            ui.label("Load a MusicXML drum score, listen to it, then play it on your kit and see how you did.");
            ui.add_space(18.0);
            let open = egui::Button::new(RichText::new("📂 Open MusicXML…").size(18.0)).min_size(egui::vec2(260.0, 46.0));
            if ui.add(open).clicked() {
                self.open_dialog();
            }
            ui.add_space(6.0);
            ui.label(RichText::new("or drop a file onto the window — .musicxml, .xml or .mxl, exported from MuseScore, Guitar Pro, Sibelius, Dorico…").small().weak());
            ui.add_space(14.0);
            let rudiments = egui::Button::new(RichText::new("Practise the 40 rudiments").size(16.0)).min_size(egui::vec2(260.0, 38.0));
            if ui.add(rudiments).on_hover_text("Singles, rolls, paradiddles, flams and drags: pick one, repeat it and play it along with the click").clicked() {
                self.tab = Tab::Rudiments;
            }
        });
        ui.add_space(28.0);

        let width = ui.available_width().min(640.0);
        let mut open: Option<PathBuf> = None;
        let mut forget: Option<usize> = None;
        let mut clear = false;
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - width) / 2.0);
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.heading("Recent scores");
                    if !self.cfg.recent_files.is_empty() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            clear = ui.small_button("Clear list").clicked();
                        });
                    }
                });
                if self.cfg.recent_files.is_empty() {
                    ui.label(RichText::new("Scores you open are listed here so you can come back to them.").weak());
                }
                for (i, path) in self.cfg.recent_files.iter().enumerate() {
                    let (clicked, remove) = recent_row(ui, path, width);
                    ui.add_space(6.0);
                    if remove {
                        forget = Some(i);
                    } else if clicked {
                        open = Some(PathBuf::from(path));
                    }
                }
            });
        });
        if clear {
            self.cfg.recent_files.clear();
        } else if let Some(i) = forget {
            self.cfg.recent_files.remove(i);
        }
        if let Some(path) = open {
            self.open_file(&path);
        }
    }

    fn score_tab(&mut self, ui: &mut egui::Ui) {
        let Some(score) = self.score.as_ref() else {
            self.welcome_panel(ui);
            return;
        };
        let n_measures = score.measures.len();
        let running = self.take.as_ref().is_some_and(|t| t.running);
        let base_bpm = score.tempo.bpm_at(score.measures[self.range_from.min(n_measures - 1)].start_q);
        let title = score.title.clone();
        let part = score.part_name.clone();
        let warnings = score.warnings.clone();

        ui.horizontal_wrapped(|ui| {
            ui.heading(title.as_str());
            if !part.is_empty() {
                ui.label(RichText::new(&part).weak());
            }
        });
        for w in &warnings {
            ui.label(RichText::new(format!("⚠ {w}")).small().color(render::AMBER));
        }
        // Three plain rows (sliders don't wrap inside `horizontal_wrapped`), drawn larger than the
        // rest of the UI because they are used with the mouse from a drum stool. Slider captions
        // come first, and the wheel tunes any slider under the pointer.
        ui.scope(|ui| {
            widgets::roomy(ui);
            self.transport_row(ui, running, n_measures);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!running, |ui| {
                    let mut from = self.range_from + 1;
                    let mut to = self.range_to + 1;
                    ui.label("Measures");
                    widgets::slider(ui, "from", &mut from, 1..=n_measures, "", 1.0);
                    widgets::slider(ui, "to", &mut to, 1..=n_measures, "", 1.0);
                    self.range_from = from - 1;
                    self.range_to = (to - 1).max(self.range_from);
                });
            });
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!running, |ui| {
                    widgets::slider(ui, "tempo", &mut self.tempo_pct, 25.0..=150.0, "%", 1.0);
                    ui.label(format!("= {:.0} BPM", base_bpm * self.tempo_pct as f64 / 100.0));
                });
                self.playback_switches(ui, running);
            });
        });
        self.score_body(ui, running, n_measures);
    }

    /// The first row of the Score and Rudiments tabs: record, listen, zoom, follow, layout.
    fn transport_row(&mut self, ui: &mut egui::Ui, running: bool, n_measures: usize) {
        ui.horizontal(|ui| {
            if running {
                if ui.button(RichText::new("⏹ Stop (Esc)").strong()).clicked() {
                    self.stop_take(false);
                }
            } else {
                let gradable = self.source == Source::Score || self.rudiment().gradable();
                let record = egui::Button::new(RichText::new("⏺ Record (R)").strong().color(render::RED));
                if ui
                    .add_enabled(gradable, record)
                    .on_disabled_hover_text("A closed roll cannot be graded stroke by stroke: use Listen")
                    .clicked()
                {
                    self.start_take();
                }
            }
            if self.listen.is_some() {
                if ui.button(RichText::new("⏹ Stop listening (L)").strong()).clicked() {
                    self.stop_listen();
                }
            } else if ui
                .add_enabled(!running, egui::Button::new(RichText::new("⏵ Listen (L)").strong()))
                .on_hover_text("Hear the selected measures at the chosen tempo, with the count-in and click, before you play them")
                .clicked()
            {
                self.start_listen();
            }
            // Where Listen is in the music; a fixed width, so the controls after it stay put.
            let position = self.listen_position().unwrap_or_default();
            ui.add_sized([150.0, 30.0], egui::Label::new(RichText::new(position).weak()));
            ui.separator();
            widgets::slider(ui, "zoom", &mut self.cfg.zoom, 0.5..=3.0, "", 0.05);
            ui.checkbox(&mut self.follow, "follow");
            ui.menu_button("Layout", |ui| {
                ui.set_min_width(320.0);
                widgets::slider_with_zero(ui, "bars per row", &mut self.cfg.bars_per_row, 0..=8, "", 1.0, Some("auto"));
                widgets::slider_with_zero(ui, "start a row at bar", &mut self.cfg.layout_align_bar, 0..=n_measures, "", 1.0, Some("off"));
                let printed = self
                    .score
                    .as_ref()
                    .and_then(|s| s.measures.get(self.cfg.layout_align_bar.wrapping_sub(1)))
                    .map(|m| m.number.clone());
                if let Some(number) = printed {
                    ui.label(RichText::new(format!("Bar {} of the score as shown, printed as bar {number} in the file.", self.cfg.layout_align_bar)).small().weak());
                }
                ui.label(RichText::new("With a fixed number of bars per row every bar has the same width and the zoom no longer matters.").small().weak());
            });
        });
    }

    /// The switches that work while the music plays, and the count-in.
    fn playback_switches(&mut self, ui: &mut egui::Ui, running: bool) {
        // The click can be switched on and off while the music plays, in Listen and in a take.
        ui.checkbox(&mut self.cfg.metronome_during_take, "click (K)")
            .on_hover_text("Switch the metronome on or off at any time, also while playing or recording. The count-in always sounds.");
        ui.add_enabled_ui(!running, |ui| {
            ui.checkbox(&mut self.listen_notes, "notes")
                .on_hover_text("Play the notes of the score in Listen. Switch them off to use Listen as a click track.");
            ui.checkbox(&mut self.listen_loop, "loop")
                .on_hover_text("Repeat the selected measures in Listen until it is stopped. Switching it off lets the current round finish.");
            widgets::slider(ui, "count-in bars", &mut self.cfg.count_in_bars, 0..=2, "", 1.0);
        });
    }

    /// The take's live figures, the colour legend and the score with the notes played, shared by
    /// the Score and Rudiments tabs.
    fn score_body(&mut self, ui: &mut egui::Ui, running: bool, n_measures: usize) {
        // Count-in / live stats line. Its height is reserved (the big count-in number is taller
        // than the rest), also with no take, so nothing below it moves when a take starts or ends.
        ui.horizontal(|ui| {
            ui.set_min_height(TAKE_LINE_HEIGHT);
            if let Some(take) = self.take.as_ref() {
                let r = &take.report;
                if take.running {
                    let el = signed_secs(Instant::now(), take.zero);
                    if el < 0.0 {
                        let beats_left = (-el / take.count_in_beat_sec).ceil() as i64;
                        ui.label(RichText::new(format!("Count-in… {beats_left}")).size(22.0).strong().color(render::ACCENT));
                    } else {
                        ui.label(RichText::new("⏺ REC").strong().color(render::RED));
                    }
                } else {
                    ui.label(RichText::new(format!("Take {}", take.number)).strong());
                }
                if r.evaluated > 0 {
                    ui.label(RichText::new(format!("{:.0} {}", r.overall, r.letter())).size(18.0).strong());
                    ui.label(format!(
                        "accuracy {:.0}% · {} perfect · {} good · {} ok · {} wrong pad · {} missed · {} extra · timing {:+.0} ± {:.0} ms",
                        r.accuracy * 100.0,
                        r.perfect,
                        r.good,
                        r.ok,
                        r.wrong_pad,
                        r.missed,
                        r.extra,
                        r.mean_dt_ms,
                        r.std_dt_ms
                    ));
                }
            }
        });
        ui.horizontal(|ui| {
            let legend = |ui: &mut egui::Ui, c: Color32, t: &str| {
                ui.label(RichText::new("■").color(c));
                ui.label(RichText::new(t).small());
            };
            legend(ui, render::GREEN, "perfect");
            legend(ui, render::LIGHT_GREEN, "good");
            legend(ui, render::AMBER, "ok");
            legend(ui, render::ORANGE, "wrong pad");
            legend(ui, render::RED, "missed / extra");
            ui.label(RichText::new("· click a measure to start there, shift+click to set the end · mouse wheel over a slider fine-tunes it").small().weak());
            if let Some(fit) = self.fit_bars {
                let msg = format!("· {} bars per row do not fit at this width: showing {fit}", self.cfg.bars_per_row);
                ui.label(RichText::new(msg).small().color(render::AMBER));
            }
        });
        // On a line of its own: it grows while the trigger waits for the second hit.
        if let Some((text, armed)) = self.trigger_hint() {
            let (color, text) = if armed { (render::GREEN, format!("{text}: waiting for the second hit")) } else { (ui.visuals().weak_text_color(), text) };
            ui.label(RichText::new(text.trim_start_matches("· ")).small().color(color));
        }
        ui.separator();

        let played = self.played_notes();
        let grades = self.note_grades();
        let listen_q = self.listen_playhead();
        let score = self.score.as_ref().unwrap();
        let playhead = match (self.take.as_ref().filter(|t| t.running), listen_q) {
            (Some(t), _) => Some(score_q(score, t, signed_secs(Instant::now(), t.zero))),
            (None, q) => q,
        };
        let view = ScoreView {
            score,
            grades: &grades,
            played: &played,
            playhead_q: playhead,
            range: (self.range_from, self.range_to),
            zoom: self.cfg.zoom,
            follow: self.follow && (running || self.listen.is_some()),
            layout: LayoutOpts { bars_per_row: self.cfg.bars_per_row, align_bar: self.cfg.layout_align_bar },
        };
        let out = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| render::show(ui, &view))
            .inner;
        self.fit_bars = out.fit_bars;
        if let (Some((mi, shift)), false) = (out.clicked_measure, running) {
            if shift {
                self.range_to = mi.max(self.range_from);
            } else {
                self.range_from = mi;
                if self.range_to < mi {
                    self.range_to = n_measures - 1;
                }
            }
        }
    }

    fn results_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let Some(take) = self.take.as_ref() else {
                ui.label("Record a take on the Score or Rudiments tab to see the analysis here.");
                return;
            };
            let r = &take.report;
            if r.evaluated == 0 {
                ui.label("No notes have been graded yet.");
                return;
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new(r.letter()).size(48.0).strong().color(grade_letter_color(r.overall)));
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("{:.1} / 100", r.overall)).size(24.0).strong());
                    ui.label(format!(
                        "Take {}{} · {}",
                        take.number,
                        if take.running { " (in progress)" } else { "" },
                        take.label
                    ));
                });
            });
            ui.add_space(6.0);
            egui::Grid::new("metrics").num_columns(2).striped(true).show(ui, |ui| {
                let mut row = |k: &str, v: String| {
                    ui.label(k);
                    ui.label(RichText::new(v).strong());
                    ui.end_row();
                };
                row("Note accuracy", format!("{:.1}%", r.accuracy * 100.0));
                row("Notes graded", r.evaluated.to_string());
                row("Perfect / Good / OK", format!("{} / {} / {}", r.perfect, r.good, r.ok));
                row("Wrong pad / Missed / Extra", format!("{} / {} / {}", r.wrong_pad, r.missed, r.extra));
                row(
                    "Average offset",
                    format!(
                        "{:+.1} ms ({})",
                        r.mean_dt_ms,
                        if r.mean_dt_ms > 3.0 { "behind the beat" } else if r.mean_dt_ms < -3.0 { "ahead of the beat" } else { "centred" }
                    ),
                );
                row("Average |offset|", format!("{:.1} ms", r.mean_abs_dt_ms));
                row("Consistency (std. dev.)", format!("{:.1} ms", r.std_dt_ms));
                row("Tempo drift", format!("{:+.0} ms/min", r.drift_ms_per_min));
                row("Timing score", format!("{:.0}%", r.timing_score * 100.0));
                row(
                    "Dynamics (accents/ghosts)",
                    r.dynamics_score.map(|d| format!("{:.0}%", d * 100.0)).unwrap_or_else(|| "n/a".into()),
                );
            });

            ui.add_space(8.0);
            ui.heading("Feedback");
            for f in &r.feedback {
                ui.label(format!("• {f}"));
            }

            ui.add_space(8.0);
            ui.heading("Timing distribution");
            let dts: Vec<f64> = r.results.iter().filter_map(|x| x.dt_ms).collect();
            histogram(ui, &dts, self.cfg.tol.window_ms);

            ui.add_space(8.0);
            ui.heading("Per instrument");
            egui::Grid::new("lanes").num_columns(8).striped(true).show(ui, |ui| {
                for h in ["Instrument", "Expected", "Hit", "Missed", "Wrong pad", "Extra", "Offset", "Velocity"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (lane, ls) in &r.lanes {
                    ui.label(lane.label());
                    ui.label(ls.expected.to_string());
                    ui.label(ls.hit.to_string());
                    ui.label(ls.missed.to_string());
                    ui.label(ls.wrong_pad.to_string());
                    ui.label(ls.extra.to_string());
                    ui.label(if ls.hit > 0 { format!("{:+.0} ± {:.0} ms", ls.mean_dt_ms, ls.std_dt_ms) } else { "–".into() });
                    ui.label(if ls.hit > 0 { format!("{:.0}", ls.mean_velocity) } else { "–".into() });
                    ui.end_row();
                }
            });

            if !r.hands.is_empty() {
                ui.add_space(8.0);
                ui.heading("Hands");
                egui::Grid::new("hands").num_columns(4).striped(true).show(ui, |ui| {
                    for h in ["Hand", "Strokes hit", "Offset", "Velocity (unaccented)"] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (hand, hs) in &r.hands {
                        ui.label(hand.label());
                        ui.label(hs.hit.to_string());
                        ui.label(if hs.hit > 0 { format!("{:+.0} ms", hs.mean_dt_ms) } else { "–".into() });
                        ui.label(if hs.plain > 0 { format!("{:.0}", hs.mean_velocity) } else { "–".into() });
                        ui.end_row();
                    }
                });
            }

            ui.add_space(8.0);
            if ui.button("💾 Export take as CSV…").clicked() {
                self.export_csv();
            }

            if !self.history.is_empty() {
                ui.add_space(8.0);
                ui.heading("Session history");
                egui::Grid::new("history").num_columns(7).striped(true).show(ui, |ui| {
                    for h in ["Take", "Measures", "BPM", "Score", "Accuracy", "Timing", "Extra"] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for t in self.history.iter().rev() {
                        ui.label(t.number.to_string());
                        ui.label(t.range.as_str());
                        ui.label(format!("{:.0}", t.bpm));
                        ui.label(RichText::new(format!("{:.0} {}", t.overall, t.letter)).color(grade_letter_color(t.overall)));
                        ui.label(format!("{:.0}%", t.accuracy * 100.0));
                        ui.label(format!("{:+.0} ± {:.0} ms", t.mean_dt, t.std_dt));
                        ui.label(t.extra.to_string());
                        ui.end_row();
                    }
                });
            }
        });
    }

    fn export_csv(&mut self) {
        let (Some(take), Some(score)) = (self.take.as_ref(), self.score.as_ref()) else { return };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("CSV", &["csv"])
            .set_file_name(format!("take_{}.csv", take.number))
            .save_file()
        else {
            return;
        };
        let mut s = String::from("measure,beat,instrument,expected_ms,played_ms,offset_ms,midi_note,velocity,grade\n");
        for (e, r) in take.expected.iter().zip(&take.report.results) {
            let n = &score.notes[e.note_idx];
            let m = &score.measures[n.measure];
            let beat = (n.q - m.start_q) / m.beat_q() + 1.0;
            let (played, note, vel) = r
                .hit
                .map(|h| {
                    let h = &take.hits[h];
                    (format!("{:.1}", h.t * 1000.0), h.note.to_string(), h.velocity.to_string())
                })
                .unwrap_or_default();
            s.push_str(&format!(
                "{},{:.3},{},{:.1},{},{},{},{},{}\n",
                m.number,
                beat,
                e.lane.label(),
                e.t * 1000.0,
                played,
                r.dt_ms.map(|d| format!("{d:.1}")).unwrap_or_default(),
                note,
                vel,
                r.grade.label()
            ));
        }
        for (hi, h) in take.hits.iter().enumerate() {
            if take.report.hit_match[hi].is_none() {
                s.push_str(&format!(",,{},,{:.1},,{},{},Extra\n", h.lane.label(), h.t * 1000.0, h.note, h.velocity));
            }
        }
        match std::fs::write(&path, s) {
            Ok(()) => self.status = format!("Saved {}", path.display()),
            Err(e) => self.error = Some(format!("Could not save: {e}")),
        }
    }

    /// Settings are split into pages so each one fits on a screen.
    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.scope(|ui| {
            widgets::roomy(ui);
            ui.horizontal(|ui| {
                for page in SettingsPage::ALL {
                    ui.selectable_value(&mut self.settings_page, page, page.label());
                }
            });
        });
        ui.separator();
        // One scroll position per page, so switching pages starts at the top.
        egui::ScrollArea::vertical()
            .id_salt(("settings", self.settings_page as u8))
            .auto_shrink([false, false])
            .show(ui, |ui| match self.settings_page {
                SettingsPage::MidiInput => self.settings_midi_input(ui),
                SettingsPage::Controller => self.settings_controller(ui),
                SettingsPage::Timing => self.settings_timing(ui),
                SettingsPage::Sound => self.settings_sound(ui),
                SettingsPage::General => self.settings_general(ui),
            });
    }

    fn settings_midi_input(&mut self, ui: &mut egui::Ui) {
        ui.heading("MIDI input");
        ui.horizontal(|ui| {
            ui.label("Input port");
            let current = self.midi_in.as_ref().map(|h| h.port_name.clone()).unwrap_or_else(|| "— none —".into());
            let mut choice: Option<String> = None;
            egui::ComboBox::from_id_salt("midi_in_port")
                .selected_text(current)
                .width(260.0)
                .show_ui(ui, |ui| {
                    for p in &self.midi_inputs {
                        if ui.selectable_label(false, p.as_str()).clicked() {
                            choice = Some(p.clone());
                        }
                    }
                });
            if let Some(c) = choice {
                self.connect_midi(Some(c));
            }
            if ui.button("⟳ Refresh ports").clicked() {
                self.refresh_ports();
            }
            if self.midi_in.is_some() && ui.button("Disconnect").clicked() {
                self.midi_in = None;
            }
        });
        ui.label(RichText::new("Connect the V31 by USB (or a MIDI interface on MIDI OUT). Close other apps that use the port — on Windows a MIDI port can only be opened by one program.").small().weak());

        ui.add_space(6.0);
        ui.label(RichText::new("Monitor (latest first)").strong());
        egui::Grid::new("monitor").num_columns(5).striped(true).show(ui, |ui| {
            for (ev, lane) in &self.monitor {
                ui.label(format!("ch {}", ev.channel));
                ui.label(format!("note {} ({})", ev.note, midi::note_name(ev.note)));
                ui.label(format!("vel {}", ev.velocity));
                ui.label(ev.hihat.map(|p| format!("pedal {p}")).unwrap_or_default());
                ui.label(lane.map(|l| l.label()).unwrap_or("unmapped"));
                ui.end_row();
            }
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Hi-hat pedal (CC#4) value when closed");
            widgets::slider(ui, "", &mut self.cfg.hihat_cc_max, 1..=127, "", 1.0);
        });
        ui.label(RichText::new("The hi-hat counts as closed when the pedal is at least three quarters of the way to this value, otherwise open, so hits on the closed or open hi-hat note follow the pedal. Lower it if your pedal never reports up to 127: close the pedal, hit the hi-hat and read the pedal value in the monitor. Hits before any pedal message keep the note's own open/closed meaning.").small().weak());

        ui.add_space(10.0);
        self.settings_trigger(ui);

        ui.add_space(10.0);
        ui.heading("Pad-to-instrument mapping");
        ui.label(RichText::new("Defaults follow Roland V-Drums factory note numbers. Click Learn and hit the pad/zone to assign it.").small().weak());
        egui::Grid::new("mapping").num_columns(3).striped(true).show(ui, |ui| {
            for lane in Lane::ALL {
                if lane == Lane::Other {
                    continue;
                }
                ui.label(lane.label());
                let text = self
                    .lane_text
                    .entry(lane)
                    .or_insert_with(|| format_note_list(&self.cfg.note_map.notes_for(lane)));
                if ui.add(egui::TextEdit::singleline(text).desired_width(140.0)).changed() {
                    let notes = parse_note_list(text);
                    self.cfg.note_map.set_lane_notes(lane, &notes);
                }
                let learning = self.learn == Some(lane);
                if ui.selectable_label(learning, if learning { "hit a pad…" } else { "Learn" }).clicked() {
                    self.learn = if learning { None } else { Some(lane) };
                }
                ui.end_row();
            }
        });
        if ui.button("Reset to Roland V31 defaults").clicked() {
            self.cfg.note_map = crate::drums::NoteMap::roland_v31();
            self.lane_text.clear();
        }
    }

    fn settings_timing(&mut self, ui: &mut egui::Ui) {
        ui.heading("Timing");
        egui::Grid::new("timing").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label("Latency compensation");
            ui.horizontal(|ui| {
                widgets::slider(ui, "", &mut self.cfg.latency_ms, -50.0..=200.0, " ms", 1.0);
                let calibrating = self.calibration.is_some();
                if ui.add_enabled(!calibrating, egui::Button::new("Calibrate…")).clicked() {
                    self.start_calibration();
                }
            });
            ui.end_row();
            ui.label("Perfect within");
            widgets::slider(ui, "", &mut self.cfg.tol.perfect_ms, 5.0..=50.0, " ms", 1.0);
            ui.end_row();
            ui.label("Good within");
            widgets::slider(ui, "", &mut self.cfg.tol.good_ms, 10.0..=100.0, " ms", 1.0);
            ui.end_row();
            ui.label("Hit window (beyond = extra/missed)");
            widgets::slider(ui, "", &mut self.cfg.tol.window_ms, 40.0..=250.0, " ms", 5.0);
            ui.end_row();
        });
        ui.label(RichText::new("Calibrate: play any pad exactly with 16 metronome clicks; the median offset becomes the latency compensation. Hover a slider and turn the mouse wheel to fine-tune it.").small().weak());
        self.cfg.tol.good_ms = self.cfg.tol.good_ms.max(self.cfg.tol.perfect_ms);
        self.cfg.tol.window_ms = self.cfg.tol.window_ms.max(self.cfg.tol.good_ms);
    }

    fn settings_sound(&mut self, ui: &mut egui::Ui) {
        ui.heading("Audio output");
        ui.add_enabled_ui(HAS_AUDIO, |ui| {
            ui.horizontal(|ui| {
                ui.label("Output device");
                let default_label = match &self.audio_default {
                    Some(name) => format!("System default ({name})"),
                    None => "System default".to_string(),
                };
                let current = if self.cfg.audio_output.is_empty() { default_label.clone() } else { self.cfg.audio_output.clone() };
                egui::ComboBox::from_id_salt("audio_out").selected_text(current).width(360.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.cfg.audio_output, String::new(), default_label);
                    for d in &self.audio_devices {
                        ui.selectable_value(&mut self.cfg.audio_output, d.clone(), d.as_str());
                    }
                });
                if ui.button("⟳ Refresh").clicked() {
                    self.refresh_ports();
                }
            });
            if !self.cfg.audio_output.is_empty() && !self.audio_devices.contains(&self.cfg.audio_output) {
                ui.label(RichText::new("This device is not connected right now: the system default is used instead.").small().color(render::AMBER));
            }
        });
        ui.label(RichText::new(if HAS_AUDIO {
            "The sound card used for the metronome click, Listen and the calibration clicks. A change applies the next time sound starts."
        } else {
            "This build has no sound output; use MIDI output below."
        }).small().weak());

        ui.add_space(10.0);
        ui.heading("MIDI output");
        ui.horizontal(|ui| {
            ui.label("Output port");
            egui::ComboBox::from_id_salt("midi_out_port")
                .selected_text(self.cfg.midi_out_port.clone())
                .width(260.0)
                .show_ui(ui, |ui| {
                    for p in &self.midi_outputs {
                        ui.selectable_value(&mut self.cfg.midi_out_port, p.clone(), p.as_str());
                    }
                });
            if ui.button("⟳ Refresh ports").clicked() {
                self.refresh_ports();
            }
        });
        ui.label(RichText::new("Where the metronome click and the notes of Listen go when they are sent over MIDI: the V31, or for example Windows' built-in \"Microsoft GS Wavetable Synth\".").small().weak());

        ui.add_space(10.0);
        ui.heading("Metronome");
        ui.add_enabled_ui(HAS_AUDIO, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.cfg.click_audio, "Click through the computer's sound output");
                widgets::slider(ui, "volume", &mut self.cfg.click_volume, 0.0..=1.0, "", 0.02);
            });
        });
        ui.checkbox(&mut self.cfg.click_midi, "Send the click to the MIDI output");
        ui.horizontal(|ui| {
            widgets::slider(ui, "click note", &mut self.cfg.click_note, 0..=127, "", 1.0);
            widgets::slider(ui, "channel", &mut self.cfg.click_channel, 1..=16, "", 1.0);
        });
        ui.label(RichText::new("Sending the click to the V31 (channel 10, an unmapped note such as 76 = wood block) puts it in your headphones with no extra audio latency.").small().weak());

        ui.add_space(10.0);
        ui.heading("Listening");
        ui.label(RichText::new("The Listen button on the Score tab plays the selected measures at the chosen tempo, with the count-in and click of a take.").small().weak());
        ui.add_enabled_ui(HAS_AUDIO, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.cfg.playback_audio, "Play through the computer's sound output (built-in drum sounds)");
                widgets::slider(ui, "volume", &mut self.cfg.playback_volume, 0.0..=1.0, "", 0.02);
            });
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.cfg.playback_midi, "Send the notes to the MIDI output (the module's own sounds)");
            widgets::slider(ui, "channel", &mut self.cfg.playback_channel, 1..=16, "", 1.0);
        });
        ui.label(RichText::new("Each instrument is sent as its standard General MIDI / Roland note, or the note mapped to it on the MIDI input page, so the V31 plays its own kit.").small().weak());

        ui.add_space(10.0);
        self.instrument_mixer(ui);
    }

    fn settings_general(&mut self, ui: &mut egui::Ui) {
        ui.heading("Score");
        if ui.checkbox(&mut self.cfg.expand_repeats, "Expand repeats and 1st/2nd endings").changed() {
            if let Some(p) = self.main_path() {
                self.open_file(&p);
            }
        }

        ui.add_space(10.0);
        ui.heading("Recent scores");
        ui.horizontal(|ui| {
            ui.label(format!("{} remembered", self.cfg.recent_files.len()));
            if ui.add_enabled(!self.cfg.recent_files.is_empty(), egui::Button::new("Clear list")).clicked() {
                self.cfg.recent_files.clear();
            }
        });

        ui.add_space(10.0);
        ui.heading("Testing without a kit");
        ui.checkbox(&mut self.cfg.keyboard_input, "Computer keyboard plays pads");
        ui.label(
            RichText::new(KEYBOARD_PADS.iter().map(|(_, l, k)| format!("{k} = {}", l.label())).collect::<Vec<_>>().join(" · "))
                .small()
                .weak(),
        );

        ui.add_space(10.0);
        if let Some(p) = Config::path() {
            ui.label(RichText::new(format!("Settings are saved automatically to {}", p.display())).small().weak());
        }
    }
}

/// `s` cut to its last `max` characters, with "…" in front when it was longer.
fn shorten_start(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        format!("…{}", s.chars().skip(n - max + 1).collect::<String>())
    }
}

/// One entry of the recent-scores list: the file name over its folder, a
/// remove button on the right. Returns (row clicked, remove clicked).
fn recent_row(ui: &mut egui::Ui, path: &str, width: f32) -> (bool, bool) {
    let file = Path::new(path);
    let name = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string());
    let folder = shorten_start(&file.parent().map(|d| d.display().to_string()).unwrap_or_default(), 78);
    let exists = file.is_file();

    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 52.0), egui::Sense::click());
    // Clip to the row (a little wider, so the border is not cut in half).
    let painter = ui.painter_at(rect.expand(2.0));
    let visuals = ui.style().interact(&response);
    let (fill, stroke) = if response.has_focus() {
        // Reached with the keyboard: a tinted row with the selection outline, text stays readable.
        (ui.visuals().selection.bg_fill.gamma_multiply(0.3), ui.visuals().selection.stroke)
    } else if response.hovered() {
        (visuals.bg_fill, visuals.bg_stroke)
    } else {
        (ui.visuals().faint_bg_color, ui.visuals().widgets.noninteractive.bg_stroke)
    };
    painter.rect(rect, 6.0, fill, stroke);
    let (title_color, sub_color) = if exists {
        (ui.visuals().strong_text_color(), ui.visuals().weak_text_color())
    } else {
        (ui.visuals().weak_text_color(), render::RED)
    };
    painter.text(rect.left_top() + egui::vec2(14.0, 8.0), egui::Align2::LEFT_TOP, &name, egui::FontId::proportional(17.0), title_color);
    let sub = if exists { folder } else { format!("{folder}  — file not found") };
    painter.text(rect.left_top() + egui::vec2(14.0, 31.0), egui::Align2::LEFT_TOP, sub, egui::FontId::proportional(12.0), sub_color);

    // Drawn after the row, so it gets the click when the pointer is over it.
    let button = egui::Rect::from_center_size(egui::pos2(rect.right() - 26.0, rect.center().y), egui::vec2(30.0, 30.0));
    let remove = ui.put(button, egui::Button::new("×")).on_hover_text("Remove from the list").clicked();
    let clicked = response.on_hover_text(path).clicked();
    (clicked, remove)
}

fn signed_secs(t: Instant, zero: Instant) -> f64 {
    if t >= zero {
        (t - zero).as_secs_f64()
    } else {
        -(zero - t).as_secs_f64()
    }
}

fn score_q(score: &Score, take: &Take, elapsed: f64) -> f64 {
    score.tempo.sec_to_q(elapsed + take.t0, take.tempo_scale)
}

fn grade_letter_color(overall: f64) -> Color32 {
    if overall >= 90.0 {
        render::GREEN
    } else if overall >= 75.0 {
        render::LIGHT_GREEN
    } else if overall >= 60.0 {
        render::AMBER
    } else {
        render::RED
    }
}

fn histogram(ui: &mut egui::Ui, dts: &[f64], window_ms: f64) {
    let bins = 21usize;
    let half = window_ms.max(20.0);
    let bw = 2.0 * half / bins as f64;
    let mut counts = vec![0usize; bins];
    for d in dts {
        let i = ((d + half) / bw).floor();
        if i >= 0.0 && (i as usize) < bins {
            counts[i as usize] += 1;
        }
    }
    let max = counts.iter().copied().max().unwrap_or(0).max(1);
    let size = egui::vec2(ui.available_width().min(640.0), 120.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    let weak = ui.visuals().weak_text_color();
    let plot = rect.shrink2(egui::vec2(4.0, 16.0));
    let col_w = plot.width() / bins as f32;
    for (i, c) in counts.iter().enumerate() {
        let center = -half + (i as f64 + 0.5) * bw;
        let h = plot.height() * *c as f32 / max as f32;
        let x0 = plot.left() + i as f32 * col_w;
        let r = egui::Rect::from_min_max(egui::pos2(x0 + 1.0, plot.bottom() - h), egui::pos2(x0 + col_w - 1.0, plot.bottom()));
        let color = if center.abs() <= 20.0 { render::GREEN } else if center.abs() <= 45.0 { render::LIGHT_GREEN } else { render::AMBER };
        p.rect_filled(r, 2.0, color);
    }
    let mid = plot.center().x;
    p.line_segment([egui::pos2(mid, plot.top()), egui::pos2(mid, plot.bottom())], egui::Stroke::new(1.0_f32, weak));
    let f = egui::FontId::proportional(10.0);
    p.text(egui::pos2(plot.left(), rect.bottom()), egui::Align2::LEFT_BOTTOM, format!("early −{half:.0} ms"), f.clone(), weak);
    p.text(egui::pos2(mid, rect.bottom()), egui::Align2::CENTER_BOTTOM, "0", f.clone(), weak);
    p.text(egui::pos2(plot.right(), rect.bottom()), egui::Align2::RIGHT_BOTTOM, format!("late +{half:.0} ms"), f, weak);
}

impl eframe::App for DrumApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Files dropped onto the window
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if let Some(p) = dropped.first() {
            self.open_file(p);
        }

        self.sync_source();
        self.drain_midi();
        self.update_controller();
        self.keyboard_pads(ctx);
        self.sync_control();
        self.update_take();
        self.update_listen();
        self.update_metro();
        self.update_audition();
        self.update_calibration();
        self.show_sound_errors();

        if !ctx.wants_keyboard_input() {
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
            if esc && (running || self.player.is_some() || self.metro.is_some()) {
                self.stop_take(false);
                self.stop_listen();
                self.stop_metro();
            } else if rec && !running && self.score.is_some() {
                self.start_take();
            } else if listen && !running && self.score.is_some() {
                if self.listen.is_some() {
                    self.stop_listen();
                } else {
                    self.start_listen();
                }
            } else if metro && !running {
                if self.metro.is_some() {
                    self.stop_metro();
                } else {
                    self.start_metro();
                }
            }
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| self.top_bar(ui));
        // A tab clicked just now needs its practice source before it is drawn.
        self.sync_source();
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(e) = self.error.clone() {
                    ui.label(RichText::new(e).color(render::RED));
                    if ui.small_button("Dismiss").clicked() {
                        self.error = None;
                    }
                } else {
                    ui.label(RichText::new(&self.status).small());
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Home => egui::ScrollArea::vertical().show(ui, |ui| self.welcome_panel(ui)).inner,
            Tab::Score => self.score_tab(ui),
            Tab::Rudiments => self.rudiments_tab(ui),
            Tab::Metronome => self.metronome_tab(ui),
            Tab::Results => self.results_tab(ui),
            Tab::Settings => self.settings_tab(ui),
        });

        if self.cfg != self.saved_cfg {
            if let Err(e) = self.cfg.save() {
                self.status = format!("Could not save settings: {e}");
            }
            self.saved_cfg = self.cfg.clone();
        }

        // Keep animating while recording/calibrating; otherwise poll MIDI
        // often enough for the monitor and MIDI-learn.
        if self.take.as_ref().is_some_and(|t| t.running)
            || self.calibration.is_some()
            || self.listen.is_some()
            || self.metro.is_some()
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

    #[test]
    fn long_folders_keep_their_end() {
        assert_eq!(shorten_start("D:\\scores", 20), "D:\\scores");
        assert_eq!(shorten_start("C:\\Users\\someone\\Documents\\Drum scores\\Rock", 16), "…rum scores\\Rock");
        assert_eq!(shorten_start("żółć", 4), "żółć", "counts characters, not bytes");
        assert_eq!(shorten_start("żółćżółć", 4), "…ółć");
    }

    fn sample() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join("basic_rock_beat.musicxml")
    }

    #[test]
    fn the_app_starts_on_home_with_no_score() {
        let app = DrumApp::with_config(Config::default());
        assert!(app.tab == Tab::Home);
        assert!(app.score.is_none(), "so the Score tab is greyed out");
    }

    #[test]
    fn opening_a_score_goes_to_the_score_tab_and_a_failed_open_stays_put() {
        let mut app = DrumApp::with_config(Config::default());
        app.open_file(&sample());
        assert!(app.score.is_some());
        assert!(app.tab == Tab::Score);
        assert_eq!(app.cfg.recent_files.len(), 1, "remembered for the Home tab");

        app.tab = Tab::Home;
        app.open_file(Path::new("no/such/score.musicxml"));
        assert!(app.error.is_some());
        assert!(app.tab == Tab::Home, "a failed open does not leave Home");
        assert!(app.score.is_some(), "the loaded score stays");
        assert_eq!(app.cfg.recent_files.len(), 1, "a failed file is not remembered");
    }
}
