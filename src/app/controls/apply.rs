//! What a controller command does in the application: the state the lights show, and every action.

use super::super::{DrumApp, Source, Tab};
use crate::analysis::Report;
use crate::config::{limits, Config, MAX_LANE_GAIN};
use crate::controller::{self, Action, Command, State, GROUPS};
use crate::practice_tempo::PracticeTempo;
use crate::rudiments::{Lead, MAX_REPEATS};
use crate::score::Score;
use std::path::Path;
use std::time::{Duration, Instant};

/// Taps further apart than this start a new tap tempo.
pub(super) const TAP_GAP: Duration = Duration::from_secs(2);
/// Taps that count towards the tempo.
pub(super) const TAPS_KEPT: usize = 6;

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
        let Some(grade) = r.outcome.grade() else { continue };
        if let Some(slot) = score.notes.get(e.note_idx).and_then(|n| loss.get_mut(n.measure)) {
            *slot += 1.0 - grade.credit();
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

/// The actions that change the tempo, the range or the score: locked while a take records.
fn locked_by_recording(action: Action) -> bool {
    matches!(
        action,
        Action::LoopWorstBar
            | Action::RangeBack
            | Action::RangeForward
            | Action::TapTempo
            | Action::TempoDown
            | Action::TempoUp
            | Action::TempoReset
            | Action::PrevRudiment
            | Action::NextRudiment
            | Action::DiscardTake
            | Action::OpenScore
            | Action::Tempo
            | Action::RangeFrom
            | Action::RangeTo
            | Action::Score
            | Action::Repeats
            | Action::Lead
            | Action::Rudiment
    )
}

impl DrumApp {
    /// Tempo controls act on the metronome while its tab is open (or no score is loaded).
    pub(super) fn tempo_is_metronome(&self) -> bool {
        self.view.tab == Tab::Metronome || self.practice.score.is_none()
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
        let measures = self.practice.score.as_ref().map_or(0, |s| s.measures.len());
        let along = |i: usize| if measures > 1 { i as f32 / (measures - 1) as f32 } else { 0.0 };
        let tempo = if self.tempo_is_metronome() { limits::METRO_BPM.unit(c.metro_bpm) } else { self.practice.tempo.position() };
        let mut s = State {
            record: self.practice.take.as_ref().is_some_and(|t| t.running),
            listen: self.playback.listen.is_some(),
            click: c.metronome_during_take,
            looping: self.playback.looping,
            notes: self.playback.notes,
            metronome: self.metro_run.run.is_some(),
            tempo: tempo.clamp(0.0, 1.0),
            range_from: along(self.practice.range_from),
            range_to: along(self.practice.range_to),
            click_volume: c.click_volume,
            music_volume: c.playback_volume,
            count_in: limits::COUNT_IN_BARS.unit(c.count_in_bars),
            bars_per_row: limits::BARS_PER_ROW.unit(c.bars_per_row),
            repeats: (c.rudiment_repeats - 1) as f32 / (MAX_REPEATS - 1) as f32,
            lead: Lead::ALL.iter().position(|l| *l == self.rudiment_lead()).unwrap_or(0) as f32 / (Lead::ALL.len() - 1) as f32,
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
        self.practice.take.as_ref().is_some_and(|t| t.running)
    }

    /// Whether a change of tempo, range or score is allowed now; says why not.
    pub(super) fn unlocked(&mut self) -> bool {
        if self.take_running() {
            self.view.status = "Locked while a take is recording.".into();
            return false;
        }
        true
    }

    /// The one place that applies the lock of a recording: an action that changes the tempo, the
    /// range or the score is refused (with the reason on the status line) while a take records.
    /// `true` if the action may go on. It is asked once, before the action is looked at, so no
    /// match guard has to write the status line.
    fn allowed_now(&mut self, action: Action) -> bool {
        !locked_by_recording(action) || self.unlocked()
    }

    pub(super) fn apply_command(&mut self, command: Command) {
        match command {
            Command::Press(action) => self.press(self.in_context(action)),
            Command::Turn(action, steps) => self.turn(self.in_context(action), steps),
            Command::Set(action, x) => self.set(self.in_context(action), x),
        }
    }

    pub(in crate::app) fn press(&mut self, action: Action) {
        if !self.allowed_now(action) {
            return;
        }
        let running = self.take_running();
        match action {
            Action::Record if running => self.stop_take(false),
            Action::Record if self.practice.score.is_some() => self.start_take(),
            Action::Listen if !running && self.practice.score.is_some() => {
                if self.playback.listen.is_some() {
                    self.stop_listen();
                } else {
                    self.start_listen();
                }
            }
            Action::Record | Action::Listen => {
                if self.practice.score.is_none() {
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
            // Left alone while a take records (without a message: the buttons only toggle a mode).
            Action::Loop | Action::Notes | Action::Metronome => {}
            Action::NextTab => self.next_tab(),
            Action::LoopWorstBar => self.loop_worst_bar(),
            Action::RangeBack => self.move_range(false),
            Action::RangeForward => self.move_range(true),
            Action::TapTempo => self.tap_tempo(),
            Action::TempoDown => self.turn(Action::Tempo, -5),
            Action::TempoUp => self.turn(Action::Tempo, 5),
            Action::TempoReset => {
                if self.tempo_is_metronome() {
                    self.cfg.metro_bpm = Config::default().metro_bpm;
                } else if self.source == Source::Rudiments {
                    self.reset_rudiment_tempo();
                } else {
                    self.practice.tempo = PracticeTempo::Percent(100.0);
                }
                self.view.status = "Tempo reset.".into();
            }
            Action::PrevRudiment => self.step_rudiment(-1),
            Action::NextRudiment => self.step_rudiment(1),
            Action::DiscardTake => {
                if let Some(take) = self.practice.take.take() {
                    self.practice.history.retain(|h| h.number != take.number);
                    self.view.status = format!("Take {} discarded.", take.number);
                }
            }
            Action::OpenScore => {
                match self.cfg.recent_files.get(self.controller.cursor.min(self.cfg.recent_files.len().saturating_sub(1))).cloned() {
                    Some(path) => self.open_file(Path::new(&path)),
                    None => self.view.status = "There are no recent scores to open.".into(),
                }
            }
            Action::Mute(i) => self.toggle_mute(i as usize),
            Action::Solo(i) => self.toggle_solo(i as usize),
            Action::Audition(i) => self.audition(controller::group_lanes(i as usize)[0]),
            // Held down: the mapper reads it and makes the encoders turn five times as far.
            Action::Shift => {}
            // These are turned or set, not pressed.
            Action::Tempo
            | Action::RangeFrom
            | Action::RangeTo
            | Action::ClickVolume
            | Action::MusicVolume
            | Action::CountIn
            | Action::BarsPerRow
            | Action::Gain(_)
            | Action::Repeats
            | Action::Lead
            | Action::Score
            | Action::Rudiment => {}
        }
    }

    pub(super) fn turn(&mut self, action: Action, steps: i32) {
        if !self.allowed_now(action) {
            return;
        }
        let tempo_here = self.tempo_is_metronome();
        let measures = self.practice.score.as_ref().map_or(0, |s| s.measures.len());
        let one = steps.signum();
        match action {
            Action::Tempo => {
                if tempo_here {
                    self.cfg.metro_bpm = limits::METRO_BPM.clamp(self.cfg.metro_bpm.round() + steps as f32);
                    self.view.status = format!("Metronome {:.0} BPM", self.cfg.metro_bpm);
                } else {
                    self.practice.tempo = self.practice.tempo.stepped(steps);
                    self.view.status = format!("Tempo {}", self.practice.tempo.text());
                }
            }
            Action::RangeFrom if measures > 0 => {
                self.practice.range_from = (self.practice.range_from as i32 + steps).clamp(0, self.practice.range_to as i32) as usize;
                self.status_range();
            }
            Action::RangeTo if measures > 0 => {
                self.practice.range_to =
                    (self.practice.range_to as i32 + steps).clamp(self.practice.range_from as i32, measures as i32 - 1) as usize;
                self.status_range();
            }
            Action::ClickVolume => self.cfg.click_volume = nudged_volume(self.cfg.click_volume, steps),
            Action::MusicVolume => self.cfg.playback_volume = nudged_volume(self.cfg.playback_volume, steps),
            Action::CountIn => self.cfg.count_in_bars = limits::COUNT_IN_BARS.clamp((self.cfg.count_in_bars as i32 + one).max(0) as u32),
            Action::BarsPerRow => self.cfg.bars_per_row = limits::BARS_PER_ROW.clamp((self.cfg.bars_per_row as i32 + one).max(0) as usize),
            Action::Gain(i) => {
                let g = self.cfg.gain_of(controller::group_lanes(i as usize)[0]);
                self.set_group_gain(i as usize, (g + 0.05 * steps as f32).clamp(0.0, MAX_LANE_GAIN));
            }
            Action::Score => self.browse_scores(steps),
            Action::Repeats => {
                self.step_repeats(steps);
                self.view.status = format!("{} repeats", self.cfg.rudiment_repeats);
            }
            Action::Lead => {
                self.step_lead(one);
                self.view.status = format!("{} hand leads", self.rudiment_lead().label());
            }
            Action::Rudiment => self.step_rudiment(one),
            // A range with no measures has nothing to move.
            Action::RangeFrom | Action::RangeTo => {}
            // Buttons are pressed, not turned.
            Action::Record
            | Action::Listen
            | Action::Stop
            | Action::Click
            | Action::Loop
            | Action::Notes
            | Action::Metronome
            | Action::NextTab
            | Action::LoopWorstBar
            | Action::RangeBack
            | Action::RangeForward
            | Action::TapTempo
            | Action::TempoDown
            | Action::TempoUp
            | Action::DiscardTake
            | Action::TempoReset
            | Action::OpenScore
            | Action::PrevRudiment
            | Action::NextRudiment
            | Action::Shift
            | Action::Mute(_)
            | Action::Solo(_)
            | Action::Audition(_) => {}
        }
    }

    pub(super) fn set(&mut self, action: Action, x: f32) {
        if !self.allowed_now(action) {
            return;
        }
        let x = x.clamp(0.0, 1.0);
        let measures = self.practice.score.as_ref().map_or(0, |s| s.measures.len());
        let bar = |x: f32| (x * measures.saturating_sub(1) as f32).round() as usize;
        match action {
            Action::Tempo => {
                if self.tempo_is_metronome() {
                    self.cfg.metro_bpm = limits::METRO_BPM.at(x).round();
                } else {
                    self.practice.tempo = self.practice.tempo.at_position(x);
                }
            }
            Action::RangeFrom if measures > 0 => self.practice.range_from = bar(x).min(self.practice.range_to),
            Action::RangeTo if measures > 0 => self.practice.range_to = bar(x).max(self.practice.range_from),
            Action::ClickVolume => self.cfg.click_volume = x,
            Action::MusicVolume => self.cfg.playback_volume = x,
            Action::CountIn => self.cfg.count_in_bars = limits::COUNT_IN_BARS.at(x),
            Action::BarsPerRow => self.cfg.bars_per_row = limits::BARS_PER_ROW.at(x),
            Action::Repeats => {
                self.cfg.rudiment_repeats = 1 + (x * (MAX_REPEATS - 1) as f32).round() as usize;
                self.ensure_rudiment();
            }
            Action::Lead => {
                self.rud.lead = Some(Lead::ALL[(x * (Lead::ALL.len() - 1) as f32).round() as usize]);
                self.ensure_rudiment();
            }
            Action::Gain(i) => self.set_group_gain(i as usize, x * MAX_LANE_GAIN),
            // A range with no measures has nothing to set.
            Action::RangeFrom | Action::RangeTo => {}
            // Browsed by turning only, and the buttons are pressed.
            Action::Score
            | Action::Rudiment
            | Action::Record
            | Action::Listen
            | Action::Stop
            | Action::Click
            | Action::Loop
            | Action::Notes
            | Action::Metronome
            | Action::NextTab
            | Action::LoopWorstBar
            | Action::RangeBack
            | Action::RangeForward
            | Action::TapTempo
            | Action::TempoDown
            | Action::TempoUp
            | Action::DiscardTake
            | Action::TempoReset
            | Action::OpenScore
            | Action::PrevRudiment
            | Action::NextRudiment
            | Action::Shift
            | Action::Mute(_)
            | Action::Solo(_)
            | Action::Audition(_) => {}
        }
    }

    pub(super) fn status_range(&mut self) {
        if let Some(score) = self.practice.score.as_ref() {
            let number = |i: usize| score.measures.get(i).map(|m| m.number.clone()).unwrap_or_default();
            self.view.status = format!("Measures {}–{}", number(self.practice.range_from), number(self.practice.range_to));
        }
    }

    pub(super) fn next_tab(&mut self) {
        let mut tabs = Vec::new();
        if self.has_main_score() {
            tabs.push(Tab::Score);
        }
        tabs.extend([Tab::Rudiments, Tab::Metronome, Tab::Results, Tab::Settings]);
        let next = match tabs.iter().position(|t| *t == self.view.tab) {
            Some(i) => tabs[(i + 1) % tabs.len()],
            None => tabs[0],
        };
        self.show_tab(next);
    }

    pub(super) fn tap_tempo(&mut self) {
        let Some(bpm) = register_tap(&mut self.controller.taps, Instant::now()) else {
            self.view.status = "Tap the tempo…".into();
            return;
        };
        if self.tempo_is_metronome() {
            self.cfg.metro_bpm = limits::METRO_BPM.clamp(bpm as f32);
            self.view.status = format!("Metronome {:.0} BPM (tapped)", self.cfg.metro_bpm);
        } else if let Some(score) = self.practice.score.as_ref() {
            let written = score.tempo.bpm_at(score.measures[self.practice.range_from.min(score.measures.len() - 1)].start_q);
            self.practice.tempo = self.practice.tempo.from_bpm(bpm, written);
            let text = self.practice.tempo.text();
            self.view.status = match self.source {
                Source::Rudiments => format!("Tempo {text} (tapped)"),
                Source::Score => format!("Tempo {text} = {bpm:.0} BPM (tapped)"),
            };
        }
    }

    pub(super) fn move_range(&mut self, forward: bool) {
        let Some(n) = self.practice.score.as_ref().map(|s| s.measures.len()) else { return };
        if let Some((from, to)) = shifted_range(self.practice.range_from, self.practice.range_to, n, forward) {
            self.practice.range_from = from;
            self.practice.range_to = to;
            self.status_range();
        }
    }

    /// Loops the measure the last take went worst in.
    pub(super) fn loop_worst_bar(&mut self) {
        let worst = match (self.practice.take.as_ref(), self.practice.score.as_ref()) {
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
                self.practice.range_from = m;
                self.practice.range_to = m;
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
