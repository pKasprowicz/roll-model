//! The lifecycles of a take, Listen, an audition and the latency calibration: starting them, following them
//! every frame, and stopping them.

use std::time::{Duration, Instant};

use crate::analysis::{analyze_with, expected_notes, Grade};
use crate::drums::Lane;
use crate::metronome::{ClickOptions, Metronome, HAS_AUDIO};
use crate::playback::{self, PlayOptions, Player, Ramp};
use crate::render::{HitStatus, PlayedNote};
use crate::score::Score;

use super::{Calibration, DrumApp, Listen, Plan, Source, Take, TakeSummary, CLEAN_TAKE, LISTEN_RESTART_DELAY};

pub(super) fn signed_secs(t: Instant, zero: Instant) -> f64 {
    if t >= zero {
        (t - zero).as_secs_f64()
    } else {
        -(zero - t).as_secs_f64()
    }
}

pub(super) fn score_q(score: &Score, take: &Take, elapsed: f64) -> f64 {
    score.tempo.sec_to_q(elapsed + take.t0, take.tempo_scale)
}

impl DrumApp {
    pub(super) fn click_options(&self) -> ClickOptions {
        ClickOptions {
            audio: self.cfg.click_audio && HAS_AUDIO,
            engine: self.audio.handle(),
            volume: self.cfg.click_volume,
            midi_port: self.cfg.click_midi.then(|| self.cfg.midi_out_port.clone()),
            midi_note: self.cfg.click_note,
            midi_channel: self.cfg.click_channel,
        }
    }

    /// Copies the settings that can change while sound is playing into the
    /// controls the sound threads read.
    pub(super) fn sync_control(&mut self) {
        let c = &self.playback.control;
        c.set_notes(self.playback.notes);
        c.set_click(self.cfg.metronome_during_take);
        c.set_looping(self.playback.looping);
        c.set_volume(self.cfg.playback_volume);
        c.set_click_volume(self.cfg.click_volume);
        for lane in Lane::ALL {
            c.set_lane_gain(lane, self.cfg.effective_gain(&self.playback.solo, lane));
        }
        // The free metronome always clicks and loops.
        let m = &self.metro_run.control;
        m.set_click(true);
        m.set_looping(true);
        m.set_click_volume(self.cfg.click_volume);
        // An instrument played from the mixer sounds at its own gain even when muted there.
        let a = &self.playback.audition_control;
        a.set_volume(self.cfg.playback_volume);
        for lane in Lane::ALL {
            a.set_lane_gain(lane, self.cfg.gain_of(lane));
        }
    }

    /// Timing for the selected measures at the chosen tempo, starting now:
    /// half a second of lead (the sound device needs time to open), then the
    /// count-in if wanted, then the music. The click track always has the
    /// beat clicks of the music; the click switch decides whether they sound.
    pub(super) fn plan_run(&self, count_in: bool) -> Option<Plan> {
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

    pub(super) fn start_take(&mut self) {
        self.stop_take(false);
        self.stop_listen();
        self.stop_metro();
        self.ensure_audio();
        self.sync_control();
        if self.source == Source::Rudiments && !self.rudiment().gradable() {
            self.view.status = format!("{} cannot be graded stroke by stroke: use Listen.", self.rudiment().name);
            return;
        }
        let label = self.practice_label();
        let adaptive = self.source == Source::Rudiments;
        let Some(plan) = self.plan_run(true) else { return };
        let Some(score) = self.score.as_ref() else { return };
        let expected = expected_notes(score, plan.from, plan.to, plan.scale);
        self.view.status = format!(
            "Recording measures {}–{} at {:.0} BPM",
            score.measures[plan.from].number,
            score.measures[plan.to].number,
            plan.bpm
        );
        self.metronome = Some(Metronome::start(plan.zero, plan.clicks, self.click_options(), Some(self.playback.control.clone())));
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

    pub(super) fn play_options(&self) -> PlayOptions {
        let c = &self.cfg;
        PlayOptions {
            audio: c.playback_audio && HAS_AUDIO,
            engine: self.audio.handle(),
            sounds_dir: self.paths.sounds_dir(),
            click_audio: c.click_audio && HAS_AUDIO,
            midi_port: (c.playback_midi || c.click_midi).then(|| c.midi_out_port.clone()),
            midi_drums: c.playback_midi,
            drum_channel: c.playback_channel,
            click_midi: c.click_midi,
            click_note: c.click_note,
            click_channel: c.click_channel,
            drum_notes: Lane::ALL.iter().filter_map(|l| c.note_map.playback_note(*l).map(|n| (*l, n))).collect(),
            ramp: self.listen_ramp(),
        }
    }

    /// The tempo ramp of Listen: `ramp_percent` points of the written tempo every `ramp_passes` passes.
    pub(super) fn listen_ramp(&self) -> Ramp {
        let c = &self.cfg;
        if c.ramp_percent == 0 || self.tempo_pct <= 0.0 {
            return Ramp::default();
        }
        Ramp { every: c.ramp_passes, step: c.ramp_percent as f64 / self.tempo_pct as f64 }
    }

    /// The message to show when no output could possibly make a sound.
    pub(super) fn nothing_audible(opts: &PlayOptions) -> Option<String> {
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
    pub(super) fn start_listen(&mut self) {
        self.begin_listen(self.cfg.listen_count_in);
    }

    pub(super) fn begin_listen(&mut self, count_in: bool) {
        self.stop_take(false);
        self.stop_listen();
        self.stop_metro();
        self.ensure_audio();
        self.playback.pending = None;
        let opts = self.play_options();
        if let Some(message) = Self::nothing_audible(&opts) {
            self.view.error = Some(message);
            return;
        }
        let ramp = opts.ramp;
        self.sync_control();
        let Some(plan) = self.plan_run(count_in) else { return };
        let Some(score) = self.score.as_ref() else { return };
        let expected = expected_notes(score, plan.from, plan.to, plan.scale);
        self.view.status = if expected.is_empty() {
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
        self.playback.player = Some(Player::start(plan.zero, cues, plan.end_sec, opts, self.playback.control.clone()));
        self.playback.listen = Some(Listen {
            zero: plan.zero,
            tempo_scale: plan.scale,
            t0: plan.t0,
            end_sec: plan.end_sec,
            beat_sec: plan.beat_sec,
            count_in_beats: plan.count_in_beats,
            ramp,
            from: plan.from,
            to: plan.to,
            tempo_pct: self.tempo_pct.round() as i32,
            gen: self.score_gen,
        });
    }

    /// Stops at once, also cutting off cymbals that are still ringing.
    pub(super) fn stop_listen(&mut self) {
        if let Some(mut p) = self.playback.player.take() {
            p.stop();
        }
        self.playback.listen = None;
    }

    /// The range and tempo Listen would start with now.
    pub(super) fn listen_signature(&self) -> Option<(usize, usize, i32, u32)> {
        let n = self.score.as_ref()?.measures.len();
        if n == 0 {
            return None;
        }
        let from = self.range_from.min(n - 1);
        Some((from, self.range_to.clamp(from, n - 1), self.tempo_pct.round() as i32, self.score_gen))
    }

    pub(super) fn update_listen(&mut self) {
        if self.playback.listen.is_some() && self.playback.control.music_done() {
            self.playback.listen = None; // the music is over; the last notes may still ring
        }
        // Changing the range or the tempo while listening restarts it, without the count-in, once the
        // change has settled (a slider being dragged changes it on every frame).
        let wanted = match (self.playback.listen.as_ref(), self.listen_signature()) {
            (Some(l), Some(sig)) if (l.from, l.to, l.tempo_pct, l.gen) != sig => Some(sig),
            _ => None,
        };
        match (wanted, self.playback.pending) {
            (None, _) => self.playback.pending = None,
            (Some(sig), Some((pending, since))) if pending == sig => {
                if since.elapsed() >= LISTEN_RESTART_DELAY {
                    self.begin_listen(false);
                }
            }
            (Some(sig), _) => self.playback.pending = Some((sig, Instant::now())),
        }
        let Some(player) = self.playback.player.as_ref() else { return };
        if let Some(e) = player.take_errors().into_iter().next() {
            self.view.error = Some(e);
        }
        if player.is_finished() {
            self.playback.listen = None;
            self.playback.player = None;
        }
    }

    /// Shows the first problem the metronome thread of a take or a calibration ran into.
    pub(super) fn show_sound_errors(&mut self) {
        if let Some(e) = self.audio.take_errors().into_iter().next() {
            self.view.error = Some(e);
        }
        if let Some(m) = self.metronome.as_ref() {
            if let Some(e) = m.take_errors().into_iter().next() {
                self.view.error = Some(e);
            }
        }
    }

    /// "bar 3 · beat 2", or the count-in, for the label next to the Listen button.
    pub(super) fn listen_position(&self) -> Option<String> {
        let l = self.playback.listen.as_ref()?;
        let score = self.score.as_ref()?;
        let elapsed = signed_secs(Instant::now(), l.zero);
        if elapsed < 0.0 {
            let left = (-elapsed / l.beat_sec).ceil() as u32;
            return Some(if left > l.count_in_beats { "starting…".into() } else { format!("count-in {left}") });
        }
        let (pass, e) = l.ramp.locate(l.end_sec, elapsed);
        let q = score.tempo.sec_to_q(e + l.t0, l.tempo_scale);
        let m = &score.measures[score.measure_at(q)?];
        let beat = (((q - m.start_q) / m.beat_q()).floor() as u32 + 1).min(m.beats);
        let mut text = format!("bar {} · beat {}", m.number, beat);
        if l.ramp.every > 0 {
            text.push_str(&format!(" · {:.0} %", l.tempo_pct as f64 * l.ramp.factor(pass)));
        }
        Some(text)
    }

    /// The playhead of Listen, in quarter notes; it wraps around while looping.
    pub(super) fn listen_playhead(&self) -> Option<f64> {
        let l = self.playback.listen.as_ref()?;
        let score = self.score.as_ref()?;
        let elapsed = signed_secs(Instant::now(), l.zero);
        let (_, e) = l.ramp.locate(l.end_sec, elapsed);
        Some(score.tempo.sec_to_q(e + l.t0, l.tempo_scale))
    }

    /// Plays one instrument on its own so it can be heard (and tuned) without a score.
    pub(super) fn audition(&mut self, lane: Lane) {
        self.ensure_audio();
        let opts = self.play_options();
        if !opts.audio && !opts.midi_drums {
            self.view.error = Some(Self::nothing_audible(&PlayOptions { click_audio: false, click_midi: false, ..opts }).unwrap_or_else(|| {
                "Switch on a sound output for Listen on the Sound page of Settings to hear the instruments.".into()
            }));
            return;
        }
        if let Some(mut p) = self.playback.audition.take() {
            p.stop();
        }
        self.sync_control();
        let cue = playback::Cue { t: 0.0, sound: playback::Sound::Drum { lane, velocity: 100 } };
        let start = Instant::now() + Duration::from_millis(300);
        self.playback.audition = Some(Player::start(start, vec![cue], 0.01, opts, self.playback.audition_control.clone()));
    }

    pub(super) fn update_audition(&mut self) {
        let Some(p) = self.playback.audition.as_ref() else { return };
        if let Some(e) = p.take_errors().into_iter().next() {
            self.view.error = Some(e);
        }
        if p.is_finished() {
            self.playback.audition = None;
        }
    }

    /// `completed`: the whole range was played (otherwise notes after the
    /// stop point are left ungraded).
    pub(super) fn stop_take(&mut self, completed: bool) {
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
            self.view.status = "Take stopped before any notes were due — nothing to grade.".into();
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
        self.view.status = format!(
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

    pub(super) fn update_take(&mut self) {
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

    pub(super) fn start_calibration(&mut self) {
        self.stop_take(false);
        self.stop_listen();
        self.stop_metro();
        self.ensure_audio();
        let beat = 0.6; // 100 BPM
        let zero = Instant::now() + Duration::from_secs_f64(1.0);
        let beats: Vec<f64> = (0..16).map(|i| i as f64 * beat).collect();
        let clicks = beats.iter().map(|t| (*t, false)).collect();
        // No control: the calibration clicks must sound whatever the click switch says.
        self.metronome = Some(Metronome::start(zero, clicks, self.click_options(), None));
        self.calibration = Some(Calibration { zero, beats, hits: Vec::new() });
        self.view.status = "Calibrating: hit any pad together with the 16 clicks…".into();
    }

    pub(super) fn update_calibration(&mut self) {
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
            self.view.status = "Calibration failed: not enough hits near the clicks. Is the metronome audible?".into();
            return;
        }
        offsets.sort_by(|a, b| a.total_cmp(b));
        let median = offsets[offsets.len() / 2] * 1000.0;
        self.cfg.latency_ms = median.round();
        self.cfg.latency_engine = crate::audio::ENGINE_GENERATION;
        self.view.status = format!("Latency calibrated: {:.0} ms ({} hits)", median, offsets.len());
    }

    pub(super) fn played_notes(&self) -> Vec<PlayedNote> {
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

    pub(super) fn note_grades(&self) -> Vec<Option<Grade>> {
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
}
