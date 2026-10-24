//! Application state and UI (Score / Results / Settings tabs).

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Color32, RichText};

use crate::analysis::{analyze, expected_notes, Expected, Grade, Hit, Report};
use crate::config::Config;
use crate::drums::{format_note_list, parse_note_list, Lane};
use crate::loader::load_score;
use crate::metronome::{ClickOptions, Metronome, HAS_AUDIO};
use crate::midi::{self, EventQueue, InputHandle, RawEvent};
use crate::musicxml::ParseOptions;
use crate::playback::{self, PlayOptions, Player};
use crate::render::{self, HitStatus, PlayedNote, ScoreView};
use crate::score::Score;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Score,
    Results,
    Settings,
}

/// One recorded attempt over a measure range.
struct Take {
    number: usize,
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
}

/// What a take or a playback of the selected range needs: where time zero is
/// and the click track.
struct Plan {
    from: usize,
    to: usize,
    scale: f64,
    bpm: f64,
    beat_sec: f64,
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
    pub fn new(_cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> DrumApp {
        let cfg = Config::load();
        let mut app = DrumApp {
            saved_cfg: cfg.clone(),
            cfg,
            tab: Tab::Score,
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
        };
        app.refresh_ports();
        app.connect_midi(None);
        if let Some(p) = initial_file {
            app.open_file(&p);
        }
        app
    }

    // ----------------------------------------------------------------- MIDI

    fn refresh_ports(&mut self) {
        self.midi_inputs = midi::list_inputs().unwrap_or_default();
        self.midi_outputs = midi::list_outputs().unwrap_or_default();
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
        let lane = self.cfg.note_map.lane(ev.note);
        self.monitor.push_front((ev, lane));
        self.monitor.truncate(14);

        if let Some(cal) = self.calibration.as_mut() {
            cal.hits.push(signed_secs(ev.at, cal.zero));
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
            self.handle_event(RawEvent { at: Instant::now(), note, velocity: 100, channel: 10 });
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

    fn open_file(&mut self, path: &Path) {
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
                self.take = None;
                self.error = None;
                self.tab = Tab::Score;
            }
            Err(e) => self.error = Some(format!("Could not load {}: {e}", path.display())),
        }
    }

    // ---------------------------------------------------------------- takes

    fn click_options(&self) -> ClickOptions {
        ClickOptions {
            audio: self.cfg.click_audio && HAS_AUDIO,
            volume: self.cfg.click_volume,
            midi_port: self.cfg.click_midi.then(|| self.cfg.midi_out_port.clone()),
            midi_note: self.cfg.click_note,
            midi_channel: self.cfg.click_channel,
        }
    }

    /// Timing for the selected measures at the chosen tempo, starting now:
    /// half a second of lead (the sound device needs time to open), then the
    /// count-in, then the music.
    fn plan_run(&self) -> Option<Plan> {
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
        let count_in_beats = self.cfg.count_in_bars * m0.beats;
        let lead = 0.5;
        let zero = Instant::now() + Duration::from_secs_f64(lead + count_in_beats as f64 * beat_sec);
        let t0 = score.tempo.q_to_sec(m0.start_q, scale);
        let end_sec = score.tempo.q_to_sec(score.measures[to].end_q(), scale) - t0;
        let clicks = score.click_track(from, to, scale, count_in_beats, self.cfg.metronome_during_take);
        Some(Plan { from, to, scale, bpm, beat_sec, zero, t0, end_sec, clicks })
    }

    fn start_take(&mut self) {
        self.stop_take(false);
        self.stop_listen();
        let Some(plan) = self.plan_run() else { return };
        let Some(score) = self.score.as_ref() else { return };
        let expected = expected_notes(score, plan.from, plan.to, plan.scale);
        self.status = format!(
            "Recording measures {}–{} at {:.0} BPM",
            score.measures[plan.from].number,
            score.measures[plan.to].number,
            plan.bpm
        );
        self.metronome = Some(Metronome::start(plan.zero, plan.clicks, self.click_options()));
        self.takes_started += 1;
        self.take = Some(Take {
            number: self.takes_started,
            zero: plan.zero,
            from: plan.from,
            to: plan.to,
            tempo_scale: plan.scale,
            t0: plan.t0,
            end_sec: plan.end_sec,
            report: analyze(&expected, &[], &self.cfg.tol, f64::NEG_INFINITY),
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
            volume: c.playback_volume,
            click_audio: c.click_audio && HAS_AUDIO,
            click_volume: c.click_volume,
            midi_port: (c.playback_midi || c.click_midi).then(|| c.midi_out_port.clone()),
            midi_drums: c.playback_midi,
            drum_channel: c.playback_channel,
            click_midi: c.click_midi,
            click_note: c.click_note,
            click_channel: c.click_channel,
            drum_notes: Lane::ALL.iter().filter_map(|l| c.note_map.playback_note(*l).map(|n| (*l, n))).collect(),
        }
    }

    /// Plays the selected measures at the chosen tempo, with the count-in and
    /// click a take would have, so the drummer can hear them first.
    fn start_listen(&mut self) {
        self.stop_take(false);
        self.stop_listen();
        let opts = self.play_options();
        if !opts.audio && !opts.midi_drums {
            self.error = Some(if HAS_AUDIO {
                "Playback is switched off: enable it in the Listening section of Settings.".into()
            } else {
                "This build has no sound output: send playback to a MIDI output in the Listening section of Settings.".into()
            });
            return;
        }
        let Some(plan) = self.plan_run() else { return };
        let Some(score) = self.score.as_ref() else { return };
        let expected = expected_notes(score, plan.from, plan.to, plan.scale);
        if expected.is_empty() {
            self.status = "There are no notes in the selected measures.".into();
            return;
        }
        self.status = format!(
            "Listening to measures {}–{} at {:.0} BPM",
            score.measures[plan.from].number,
            score.measures[plan.to].number,
            plan.bpm
        );
        let cues = playback::cues(&expected, &plan.clicks);
        self.player = Some(Player::start(plan.zero, cues, opts));
        self.listen = Some(Listen { zero: plan.zero, tempo_scale: plan.scale, t0: plan.t0, end_sec: plan.end_sec });
    }

    /// Stops at once, also cutting off cymbals that are still ringing.
    fn stop_listen(&mut self) {
        if let Some(mut p) = self.player.take() {
            p.stop();
        }
        self.listen = None;
    }

    fn update_listen(&mut self) {
        if self.listen.as_ref().is_some_and(|l| signed_secs(Instant::now(), l.zero) > l.end_sec + 0.25) {
            self.listen = None; // the music is over; the last notes may still ring
        }
        let Some(player) = self.player.as_ref() else { return };
        if let Some(e) = player.take_errors().into_iter().next() {
            self.error = Some(e);
        }
        if self.listen.is_none() && player.is_finished() {
            self.player = None;
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
        take.report = analyze(&take.expected, &take.hits, &self.cfg.tol, elapsed);
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
        let range = self
            .score
            .as_ref()
            .map(|s| format!("{}–{}", s.measures[take.from].number, s.measures[take.to].number))
            .unwrap_or_default();
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
    }

    fn update_take(&mut self) {
        let Some(take) = self.take.as_mut() else { return };
        if !take.running {
            return;
        }
        let elapsed = signed_secs(Instant::now(), take.zero);
        take.report = analyze(&take.expected, &take.hits, &self.cfg.tol, elapsed);
        if elapsed > take.end_sec + 1.0 {
            self.stop_take(true);
        }
    }

    fn start_calibration(&mut self) {
        self.stop_take(false);
        self.stop_listen();
        let beat = 0.6; // 100 BPM
        let zero = Instant::now() + Duration::from_secs_f64(1.0);
        let beats: Vec<f64> = (0..16).map(|i| i as f64 * beat).collect();
        let clicks = beats.iter().map(|t| (*t, false)).collect();
        self.metronome = Some(Metronome::start(zero, clicks, self.click_options()));
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
            if let Some(p) = self.score_path.clone() {
                if ui.button("⟳ Reload").on_hover_text("Reload the file (e.g. after editing it)").clicked() {
                    self.open_file(&p);
                }
            }
            ui.separator();
            ui.selectable_value(&mut self.tab, Tab::Score, "🎼 Score");
            ui.selectable_value(&mut self.tab, Tab::Results, "📊 Results");
            ui.selectable_value(&mut self.tab, Tab::Settings, "⚙ Settings");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match &self.midi_in {
                    Some(h) => ui.label(RichText::new(format!("⏺ {}", h.port_name)).color(render::GREEN)),
                    None => ui.label(RichText::new("⏺ no MIDI input").color(render::RED)),
                };
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

    fn score_tab(&mut self, ui: &mut egui::Ui) {
        let Some(score) = self.score.as_ref() else {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.heading("Open a MusicXML drum score to start");
                ui.label("Export from MuseScore, Guitar Pro, Sibelius, Dorico… as .musicxml or .mxl — or drop the file here.");
                ui.add_space(10.0);
                if ui.button("📂 Open MusicXML…").clicked() {
                    self.open_dialog();
                }
            });
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
        // Two plain rows: sliders don't wrap inside `horizontal_wrapped`, so one long row ran off-screen.
        ui.horizontal(|ui| {
            if running {
                if ui.button(RichText::new("⏹ Stop (Esc)").strong()).clicked() {
                    self.stop_take(false);
                }
            } else if ui.button(RichText::new("⏺ Record (R)").strong().color(render::RED)).clicked() {
                self.start_take();
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
            ui.add_enabled_ui(!running, |ui| {
                let mut from = self.range_from + 1;
                let mut to = self.range_to + 1;
                ui.label("Measures");
                ui.add(egui::Slider::new(&mut from, 1..=n_measures).text("from"));
                ui.add(egui::Slider::new(&mut to, 1..=n_measures).text("to"));
                self.range_from = from - 1;
                self.range_to = (to - 1).max(self.range_from);
            });
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!running, |ui| {
                ui.add(egui::Slider::new(&mut self.tempo_pct, 25.0..=150.0).suffix("%").text("tempo"));
                ui.label(format!("= {:.0} BPM", base_bpm * self.tempo_pct as f64 / 100.0));
                ui.checkbox(&mut self.cfg.metronome_during_take, "click");
                ui.add(egui::Slider::new(&mut self.cfg.count_in_bars, 0..=2).text("count-in bars"));
            });
            ui.checkbox(&mut self.follow, "follow");
            ui.add(egui::Slider::new(&mut self.cfg.zoom, 0.5..=3.0).text("zoom"));
        });

        // Count-in / live stats line
        if let Some(take) = self.take.as_ref() {
            let r = &take.report;
            ui.horizontal(|ui| {
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
            });
        }
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
            ui.label(RichText::new("· click a measure to start there, shift+click to set the end").small().weak());
        });
        ui.separator();

        let played = self.played_notes();
        let grades = self.note_grades();
        let score = self.score.as_ref().unwrap();
        let playhead = match (self.take.as_ref().filter(|t| t.running), self.listen.as_ref()) {
            (Some(t), _) => Some(score_q(score, t, signed_secs(Instant::now(), t.zero))),
            (None, Some(l)) => Some(score.tempo.sec_to_q(signed_secs(Instant::now(), l.zero) + l.t0, l.tempo_scale)),
            (None, None) => None,
        };
        let view = ScoreView {
            score,
            grades: &grades,
            played: &played,
            playhead_q: playhead,
            range: (self.range_from, self.range_to),
            zoom: self.cfg.zoom,
            follow: self.follow && (running || self.listen.is_some()),
        };
        let out = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| render::show(ui, &view))
            .inner;
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
                ui.label("Record a take on the Score tab to see the analysis here.");
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
                        "Take {}{}",
                        take.number,
                        if take.running { " (in progress)" } else { "" }
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

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.heading("MIDI input");
            ui.horizontal(|ui| {
                let current = self.midi_in.as_ref().map(|h| h.port_name.clone()).unwrap_or_else(|| "— none —".into());
                let mut choice: Option<String> = None;
                egui::ComboBox::from_label("input port")
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
            egui::Grid::new("monitor").num_columns(4).striped(true).show(ui, |ui| {
                for (ev, lane) in &self.monitor {
                    ui.label(format!("ch {}", ev.channel));
                    ui.label(format!("note {} ({})", ev.note, midi::note_name(ev.note)));
                    ui.label(format!("vel {}", ev.velocity));
                    ui.label(lane.map(|l| l.label()).unwrap_or("unmapped"));
                    ui.end_row();
                }
            });

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

            ui.add_space(10.0);
            ui.heading("Timing");
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.cfg.latency_ms, -50.0..=200.0).suffix(" ms").text("latency compensation"));
                let calibrating = self.calibration.is_some();
                if ui.add_enabled(!calibrating, egui::Button::new("Calibrate…")).clicked() {
                    self.start_calibration();
                }
            });
            ui.label(RichText::new("Calibrate: play any pad exactly with 16 metronome clicks; the median offset becomes the compensation.").small().weak());
            ui.add(egui::Slider::new(&mut self.cfg.tol.perfect_ms, 5.0..=50.0).suffix(" ms").text("perfect within"));
            ui.add(egui::Slider::new(&mut self.cfg.tol.good_ms, 10.0..=100.0).suffix(" ms").text("good within"));
            ui.add(egui::Slider::new(&mut self.cfg.tol.window_ms, 40.0..=250.0).suffix(" ms").text("hit window (beyond = extra/missed)"));
            self.cfg.tol.good_ms = self.cfg.tol.good_ms.max(self.cfg.tol.perfect_ms);
            self.cfg.tol.window_ms = self.cfg.tol.window_ms.max(self.cfg.tol.good_ms);

            ui.add_space(10.0);
            ui.heading("Metronome");
            ui.add_enabled_ui(HAS_AUDIO, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.cfg.click_audio, "Click through the computer's sound output");
                    ui.add(egui::Slider::new(&mut self.cfg.click_volume, 0.0..=1.0).text("volume"));
                });
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.cfg.click_midi, "Send click to MIDI output");
                egui::ComboBox::from_label("output port")
                    .selected_text(self.cfg.midi_out_port.clone())
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for p in &self.midi_outputs {
                            ui.selectable_value(&mut self.cfg.midi_out_port, p.clone(), p.as_str());
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.cfg.click_note, 0..=127).text("click note"));
                ui.add(egui::Slider::new(&mut self.cfg.click_channel, 1..=16).text("channel"));
            });
            ui.label(RichText::new("Sending the click to the V31 (channel 10, an unmapped note such as 76 = wood block) puts it in your headphones with no extra audio latency.").small().weak());

            ui.add_space(10.0);
            ui.heading("Listening");
            ui.label(RichText::new("The Listen button on the Score tab plays the selected measures at the chosen tempo, with the count-in and click of a take.").small().weak());
            ui.add_enabled_ui(HAS_AUDIO, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.cfg.playback_audio, "Play through the computer's sound output (built-in drum sounds)");
                    ui.add(egui::Slider::new(&mut self.cfg.playback_volume, 0.0..=1.0).text("volume"));
                });
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.cfg.playback_midi, "Send the notes to the MIDI output (the module's own sounds)");
                ui.add(egui::Slider::new(&mut self.cfg.playback_channel, 1..=16).text("channel"));
            });
            ui.label(RichText::new("Uses the output port chosen under Metronome. Each instrument is sent as its standard General MIDI / Roland note, or the note mapped to it above, so the V31 plays its own kit. Windows' built-in \"Microsoft GS Wavetable Synth\" works as an output too.").small().weak());

            ui.add_space(10.0);
            ui.heading("Score");
            if ui.checkbox(&mut self.cfg.expand_repeats, "Expand repeats and 1st/2nd endings").changed() {
                if let Some(p) = self.score_path.clone() {
                    self.open_file(&p);
                }
            }

            ui.add_space(10.0);
            ui.heading("Testing without a kit");
            ui.checkbox(&mut self.cfg.keyboard_input, "Computer keyboard plays pads");
            ui.label(
                RichText::new(
                    KEYBOARD_PADS.iter().map(|(_, l, k)| format!("{k} = {}", l.label())).collect::<Vec<_>>().join(" · "),
                )
                .small()
                .weak(),
            );

            ui.add_space(10.0);
            if let Some(p) = Config::path() {
                ui.label(RichText::new(format!("Settings are saved automatically to {}", p.display())).small().weak());
            }
        });
    }
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

        self.drain_midi();
        self.keyboard_pads(ctx);
        self.update_take();
        self.update_listen();
        self.update_calibration();

        if !ctx.wants_keyboard_input() {
            let (rec, listen, esc) = ctx.input(|i| {
                (i.key_pressed(egui::Key::R), i.key_pressed(egui::Key::L), i.key_pressed(egui::Key::Escape))
            });
            let running = self.take.as_ref().is_some_and(|t| t.running);
            if esc && (running || self.player.is_some()) {
                self.stop_take(false);
                self.stop_listen();
            } else if rec && !running && self.score.is_some() {
                self.start_take();
            } else if listen && !running && self.score.is_some() {
                if self.listen.is_some() {
                    self.stop_listen();
                } else {
                    self.start_listen();
                }
            }
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| self.top_bar(ui));
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
            Tab::Score => self.score_tab(ui),
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
        if self.take.as_ref().is_some_and(|t| t.running) || self.calibration.is_some() || self.listen.is_some() {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}
