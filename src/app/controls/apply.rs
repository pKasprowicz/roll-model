//! What a controller command does in the application: the state the lights show, and every action.

use std::path::Path;
use std::time::{Duration, Instant};
use crate::analysis::Report;
use crate::config::{Config, MAX_LANE_GAIN};
use crate::controller::{self, Action, Command, State, GROUPS};
use crate::rudiments::{Lead, MAX_REPEATS};
use crate::score::Score;
use super::super::{rudiment, DrumApp, Source, Tab};

/// Taps further apart than this start a new tap tempo.
pub(super) const TAP_GAP: Duration = Duration::from_secs(2);
/// Taps that count towards the tempo.
pub(super) const TAPS_KEPT: usize = 6;
pub(super) const TEMPO_RANGE: (f32, f32) = (25.0, 150.0);
pub(super) const METRO_RANGE: (f32, f32) = (20.0, 300.0);

/// Adds a tap and returns the tempo (beats per minute) they give, once there are two.
pub(super) fn register_tap(taps: &mut Vec<Instant>, now: Instant) -> Option<f64> {
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
pub(super) fn shifted_range(from: usize, to: usize, n: usize, forward: bool) -> Option<(usize, usize)> {
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
pub(super) fn worst_measure(score: &Score, report: &Report, extra_in: &[usize]) -> Option<usize> {
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

/// Steps of 2 % on a 0..1 volume, without the rounding errors adding up.
pub(super) fn nudged_volume(v: f32, steps: i32) -> f32 {
    (((v + 0.02 * steps as f32) * 50.0).round() / 50.0).clamp(0.0, 1.0)
}

impl DrumApp {
    /// Tempo controls act on the metronome while its tab is open (or no score is loaded).
    pub(super) fn tempo_is_metronome(&self) -> bool {
        self.view.tab == Tab::Metronome || self.score.is_none()
    }

    /// The range of the practice tempo: percent of the written tempo, or BPM for a rudiment.
    pub(super) fn tempo_range(&self) -> (f32, f32) {
        match self.source {
            Source::Rudiments => rudiment::BPM_RANGE,
            Source::Score => TEMPO_RANGE,
        }
    }

    /// What a control means for the practice at hand. With a rudiment, the controls that choose
    /// the measures choose its repetitions and lead instead, the range buttons step through the
    /// rudiments, and the encoder that browses recent scores browses rudiments.
    pub(super) fn in_context(&self, action: Action) -> Action {
        if self.source != Source::Rudiments {
            return action;
        }
        match action {
            Action::RangeFrom => Action::Repeats,
            Action::RangeTo => Action::Lead,
            Action::RangeBack => Action::PrevRudiment,
            Action::RangeForward => Action::NextRudiment,
            Action::Score => Action::Rudiment,
            other => other,
        }
    }

    pub(super) fn controller_state(&self) -> State {
        let c = &self.cfg;
        let measures = self.score.as_ref().map_or(0, |s| s.measures.len());
        let along = |i: usize| if measures > 1 { i as f32 / (measures - 1) as f32 } else { 0.0 };
        let range = self.tempo_range();
        let tempo = if self.tempo_is_metronome() {
            (c.metro_bpm - METRO_RANGE.0) / (METRO_RANGE.1 - METRO_RANGE.0)
        } else {
            (self.tempo_pct - range.0) / (range.1 - range.0)
        };
        let mut s = State {
            record: self.take.as_ref().is_some_and(|t| t.running),
            listen: self.playback.listen.is_some(),
            click: c.metronome_during_take,
            looping: self.playback.looping,
            notes: self.playback.notes,
            metronome: self.metro_run.run.is_some(),
            tempo: tempo.clamp(0.0, 1.0),
            range_from: along(self.range_from),
            range_to: along(self.range_to),
            click_volume: c.click_volume,
            music_volume: c.playback_volume,
            count_in: c.count_in_bars as f32 / 2.0,
            bars_per_row: c.bars_per_row as f32 / 8.0,
            repeats: (c.rudiment_repeats - 1) as f32 / (MAX_REPEATS - 1) as f32,
            lead: Lead::ALL.iter().position(|l| *l == self.rudiment_lead()).unwrap_or(0) as f32 / 2.0,
            ..Default::default()
        };
        if self.source == Source::Rudiments {
            // The range encoders' rings show the repetitions and the lead.
            s.range_from = s.repeats;
            s.range_to = s.lead;
        }
        for i in 0..GROUPS {
            let lanes = controller::group_lanes(i);
            s.gain[i] = (c.gain_of(lanes[0]) / MAX_LANE_GAIN).clamp(0.0, 1.0);
            s.mute[i] = lanes.iter().all(|l| c.lane_mute.contains(l));
            s.solo[i] = lanes.iter().all(|l| self.playback.solo.contains(l));
        }
        s
    }

    pub(super) fn take_running(&self) -> bool {
        self.take.as_ref().is_some_and(|t| t.running)
    }

    /// Whether a change of tempo, range or score is allowed now; says why not.
    pub(super) fn unlocked(&mut self) -> bool {
        if self.take_running() {
            self.view.status = "Locked while a take is recording.".into();
            return false;
        }
        true
    }

    pub(super) fn apply_command(&mut self, command: Command) {
        match command {
            Command::Press(action) => self.press(self.in_context(action)),
            Command::Turn(action, steps) => self.turn(self.in_context(action), steps),
            Command::Set(action, x) => self.set(self.in_context(action), x),
        }
    }

    pub(super) fn press(&mut self, action: Action) {
        let running = self.take_running();
        match action {
            Action::Record if running => self.stop_take(false),
            Action::Record if self.score.is_some() => self.start_take(),
            Action::Listen if !running && self.score.is_some() => {
                if self.playback.listen.is_some() {
                    self.stop_listen();
                } else {
                    self.start_listen();
                }
            }
            Action::Record | Action::Listen => {
                if self.score.is_none() {
                    self.view.status = "Open a score first.".into();
                }
            }
            Action::Stop => {
                self.stop_take(false);
                self.stop_listen();
                self.stop_metro();
            }
            Action::Click => self.cfg.metronome_during_take = !self.cfg.metronome_during_take,
            Action::Loop if !running => self.playback.looping = !self.playback.looping,
            Action::Notes if !running => self.playback.notes = !self.playback.notes,
            Action::Metronome if !running => {
                if self.metro_run.run.is_some() {
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
                } else if self.source == Source::Rudiments {
                    self.reset_rudiment_tempo();
                } else {
                    self.tempo_pct = 100.0;
                }
                self.view.status = "Tempo reset.".into();
            }
            Action::PrevRudiment if self.unlocked() => self.step_rudiment(-1),
            Action::NextRudiment if self.unlocked() => self.step_rudiment(1),
            Action::DiscardTake if self.unlocked() => {
                if let Some(take) = self.take.take() {
                    self.history.retain(|h| h.number != take.number);
                    self.view.status = format!("Take {} discarded.", take.number);
                }
            }
            Action::OpenScore if self.unlocked() => {
                match self.cfg.recent_files.get(self.controller.cursor.min(self.cfg.recent_files.len().saturating_sub(1))).cloned() {
                    Some(path) => self.open_file(Path::new(&path)),
                    None => self.view.status = "There are no recent scores to open.".into(),
                }
            }
            Action::Mute(i) => self.toggle_mute(i as usize),
            Action::Solo(i) => self.toggle_solo(i as usize),
            Action::Audition(i) => self.audition(controller::group_lanes(i as usize)[0]),
            _ => {}
        }
    }

    pub(super) fn turn(&mut self, action: Action, steps: i32) {
        let tempo_here = self.tempo_is_metronome();
        let measures = self.score.as_ref().map_or(0, |s| s.measures.len());
        let one = steps.signum();
        match action {
            Action::Tempo if self.unlocked() => {
                if tempo_here {
                    self.cfg.metro_bpm = (self.cfg.metro_bpm.round() + steps as f32).clamp(METRO_RANGE.0, METRO_RANGE.1);
                    self.view.status = format!("Metronome {:.0} BPM", self.cfg.metro_bpm);
                } else {
                    let range = self.tempo_range();
                    self.tempo_pct = (self.tempo_pct.round() + steps as f32).clamp(range.0, range.1);
                    self.view.status = match self.source {
                        Source::Rudiments => format!("Tempo {:.0} BPM", self.tempo_pct),
                        Source::Score => format!("Tempo {:.0} %", self.tempo_pct),
                    };
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
            Action::Repeats if self.unlocked() => {
                self.step_repeats(steps);
                self.view.status = format!("{} repeats", self.cfg.rudiment_repeats);
            }
            Action::Lead if self.unlocked() => {
                self.step_lead(one);
                self.view.status = format!("{} hand leads", self.rudiment_lead().label());
            }
            Action::Rudiment if self.unlocked() => self.step_rudiment(one),
            _ => {}
        }
    }

    pub(super) fn set(&mut self, action: Action, x: f32) {
        let x = x.clamp(0.0, 1.0);
        let measures = self.score.as_ref().map_or(0, |s| s.measures.len());
        let bar = |x: f32| (x * measures.saturating_sub(1) as f32).round() as usize;
        match action {
            Action::Tempo if self.unlocked() => {
                if self.tempo_is_metronome() {
                    self.cfg.metro_bpm = (METRO_RANGE.0 + x * (METRO_RANGE.1 - METRO_RANGE.0)).round();
                } else {
                    let range = self.tempo_range();
                    self.tempo_pct = (range.0 + x * (range.1 - range.0)).round();
                }
            }
            Action::RangeFrom if measures > 0 && self.unlocked() => self.range_from = bar(x).min(self.range_to),
            Action::RangeTo if measures > 0 && self.unlocked() => self.range_to = bar(x).max(self.range_from),
            Action::ClickVolume => self.cfg.click_volume = x,
            Action::MusicVolume => self.cfg.playback_volume = x,
            Action::CountIn => self.cfg.count_in_bars = (x * 2.0).round() as u32,
            Action::BarsPerRow => self.cfg.bars_per_row = (x * 8.0).round() as usize,
            Action::Repeats if self.unlocked() => {
                self.cfg.rudiment_repeats = 1 + (x * (MAX_REPEATS - 1) as f32).round() as usize;
                self.ensure_rudiment();
            }
            Action::Lead if self.unlocked() => {
                self.rud.lead = Some(Lead::ALL[(x * 2.0).round() as usize]);
                self.ensure_rudiment();
            }
            Action::Gain(i) => self.set_group_gain(i as usize, x * MAX_LANE_GAIN),
            _ => {}
        }
    }

    pub(super) fn status_range(&mut self) {
        if let Some(score) = self.score.as_ref() {
            let number = |i: usize| score.measures.get(i).map(|m| m.number.clone()).unwrap_or_default();
            self.view.status = format!("Measures {}–{}", number(self.range_from), number(self.range_to));
        }
    }

    pub(super) fn next_tab(&mut self) {
        let mut tabs = Vec::new();
        if self.has_main_score() {
            tabs.push(Tab::Score);
        }
        tabs.extend([Tab::Rudiments, Tab::Metronome, Tab::Results, Tab::Settings]);
        self.view.tab = match tabs.iter().position(|t| *t == self.view.tab) {
            Some(i) => tabs[(i + 1) % tabs.len()],
            None => tabs[0],
        };
        self.sync_source();
    }

    pub(super) fn tap_tempo(&mut self) {
        let Some(bpm) = register_tap(&mut self.controller.taps, Instant::now()) else {
            self.view.status = "Tap the tempo…".into();
            return;
        };
        if self.tempo_is_metronome() {
            self.cfg.metro_bpm = (bpm as f32).clamp(METRO_RANGE.0, METRO_RANGE.1);
            self.view.status = format!("Metronome {:.0} BPM (tapped)", self.cfg.metro_bpm);
        } else if let Some(score) = self.score.as_ref() {
            let written = score.tempo.bpm_at(score.measures[self.range_from.min(score.measures.len() - 1)].start_q);
            let range = self.tempo_range();
            self.tempo_pct = (bpm / written * 100.0).clamp(f64::from(range.0), f64::from(range.1)).round() as f32;
            self.view.status = match self.source {
                Source::Rudiments => format!("Tempo {:.0} BPM (tapped)", self.tempo_pct),
                Source::Score => format!("Tempo {:.0} % = {bpm:.0} BPM (tapped)", self.tempo_pct),
            };
        }
    }

    pub(super) fn move_range(&mut self, forward: bool) {
        let Some(n) = self.score.as_ref().map(|s| s.measures.len()) else { return };
        if let Some((from, to)) = shifted_range(self.range_from, self.range_to, n, forward) {
            self.range_from = from;
            self.range_to = to;
            self.status_range();
        }
    }

    /// Loops the measure the last take went worst in.
    pub(super) fn loop_worst_bar(&mut self) {
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
            None => self.view.status = "Record a take first.".into(),
            Some(None) => self.view.status = "The last take had no mistakes to practise.".into(),
            Some(Some(m)) => {
                self.range_from = m;
                self.range_to = m;
                self.playback.looping = true;
                self.status_range();
                self.start_listen();
            }
        }
    }

    pub(super) fn browse_scores(&mut self, steps: i32) {
        let n = self.cfg.recent_files.len();
        if n == 0 {
            self.view.status = "There are no recent scores.".into();
            return;
        }
        let at = (self.controller.cursor.min(n - 1) as i32 + steps).clamp(0, n as i32 - 1) as usize;
        self.controller.cursor = at;
        let name = Path::new(&self.cfg.recent_files[at]).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        self.view.status = format!("Score {}/{n}: {name} — push the encoder to open it", at + 1);
    }

    pub(super) fn set_group_gain(&mut self, group: usize, gain: f32) {
        for lane in controller::group_lanes(group) {
            // The default is not stored, as in the mixer.
            if (gain - 1.0).abs() < 0.01 {
                self.cfg.lane_gain.remove(lane);
            } else {
                self.cfg.lane_gain.insert(*lane, gain);
            }
        }
        self.view.status = format!("{}: volume {:.0} %", controller::group_label(group), gain * 100.0);
    }

    pub(super) fn toggle_mute(&mut self, group: usize) {
        let lanes = controller::group_lanes(group);
        let muted = lanes.iter().all(|l| self.cfg.lane_mute.contains(l));
        for lane in lanes {
            if muted {
                self.cfg.lane_mute.remove(lane);
            } else {
                self.cfg.lane_mute.insert(*lane);
            }
        }
        self.view.status = format!("{} {}", controller::group_label(group), if muted { "unmuted" } else { "muted" });
    }

    pub(super) fn toggle_solo(&mut self, group: usize) {
        let lanes = controller::group_lanes(group);
        let soloed = lanes.iter().all(|l| self.playback.solo.contains(l));
        for lane in lanes {
            if soloed {
                self.playback.solo.remove(lane);
            } else {
                self.playback.solo.insert(*lane);
            }
        }
        self.view.status = format!("{} {}", controller::group_label(group), if soloed { "no longer solo" } else { "solo" });
    }
}
