//! The lifecycles of a take, Listen, an audition and the latency calibration: starting them, following them
//! every frame, and stopping them.

use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::analysis::{expected_notes, Window};
use crate::config::Passes;
use crate::drums::Lane;
use crate::metronome::{ClickOptions, Metronome, HAS_AUDIO};
use crate::playback::{self, ClockOptions, PlayOptions, Player};
use crate::practice::Ramp;
use crate::score::Score;
use crate::score::{Click, Clicks};
use crate::units::{Quarters, Seconds};

use super::take::{signed_secs, Plan, Shown, StopReason, Take};
use super::{Calibration, DrumApp, Listen, ListenKey, Source, TakeSummary, CLEAN_TAKE, RESTART_DELAY};

/// Whether Listen begins with the count-in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CountIn {
    With,
    Without,
}

pub(super) fn score_q(score: &Score, take: &Take, elapsed: Seconds) -> f64 {
    take.q_at(score, elapsed).0
}

/// What stopping a take came to.
pub(super) enum TakeOutcome {
    /// No take was running.
    NoTake,
    /// It was stopped before a note was due, so there is nothing to grade.
    NothingDue,
    /// Graded: the line that sums it up, and the tempo of a clean take of a rudiment.
    Graded { summary: String, clean_bpm: Option<f64> },
}

/// What a start is to make heard.
#[derive(Clone, Copy)]
pub(super) enum Heard {
    /// Listen: the music and its click.
    Playback,
    /// The metronome.
    Click,
    /// One instrument on its own.
    Instruments,
}

/// What to tell the player when nothing is switched on to make `what` audible.
pub(super) fn nothing_audible(opts: &PlayOptions, what: Heard) -> Option<String> {
    // A device that follows the clock is reason enough to start the click or the music.
    let clicks = opts.click_audio || opts.click_midi || opts.clock.is_some();
    let instruments = opts.audio || opts.midi_drums;
    let audible = match what {
        Heard::Playback => instruments || clicks,
        Heard::Click => clicks,
        Heard::Instruments => instruments,
    };
    if audible {
        return None;
    }
    let (switched_off, no_output) = match what {
        Heard::Playback => (
            "Playback is switched off: enable a sound output on the Sound page of Settings.",
            "This build has no sound output: send playback to a MIDI output on the Sound page of Settings.",
        ),
        Heard::Click => (
            "The click is switched off: enable it under Metronome on the Sound page of Settings.",
            "This build has no sound output: send the click to a MIDI output on the Sound page of Settings.",
        ),
        Heard::Instruments => (
            "Switch on a sound output for Listen on the Sound page of Settings to hear the instruments.",
            "This build has no sound output: send the instruments to a MIDI output on the Sound page of Settings.",
        ),
    };
    Some(if HAS_AUDIO { switched_off } else { no_output }.into())
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
            midi_delay: self.output_delay(),
        }
    }

    /// How long the sound takes through the computer's output: the latency compensation, when it is meant
    /// for this engine (see `Config::display_delay`), and only while the output is open. The screen and the
    /// MIDI notes that sound together with the audio are shifted by it.
    pub(super) fn output_delay(&self) -> Duration {
        if !self.audio.is_open() {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(self.cfg.display_delay(crate::audio::ENGINE_GENERATION))
    }

    /// Copies the settings that can change while sound is playing into the
    /// controls the sound threads read.
    pub(super) fn sync_control(&mut self) {
        let c = self.playback.control();
        c.set_notes(self.playback.notes());
        c.set_click(self.cfg.metronome_during_take);
        c.set_passes(self.cfg.passes);
        c.set_volume(self.cfg.playback_volume);
        c.set_click_volume(self.cfg.click_volume);
        for lane in Lane::ALL {
            c.set_lane_gain(lane, self.cfg.effective_gain(self.playback.solo(), lane));
        }
        // The free metronome always clicks and goes on until stopped.
        let m = self.metro_run.control();
        m.set_click(true);
        m.set_passes(Passes::Unlimited);
        m.set_click_volume(self.cfg.click_volume);
        // An instrument played from the mixer sounds at its own gain even when muted there.
        let a = self.playback.audition_control();
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
        let score = self.practice.score.as_ref()?;
        if score.measures().is_empty() {
            return None;
        }
        let from = self.practice.range_from.min(score.measures().len() - 1);
        let to = self.practice.range_to.clamp(from, score.measures().len() - 1);
        let m0 = &score.measures()[from];
        let written = score.tempo.bpm_at(m0.start_q);
        let scale = self.practice.tempo.scale(written);
        let bpm = written * scale;
        let beat_sec = Seconds(60.0 / bpm * m0.beat_q().0);
        let count_in_beats = if count_in { self.cfg.count_in_bars * m0.beats } else { 0 };
        let lead = 0.5;
        let zero = Instant::now() + Duration::from_secs_f64(lead + count_in_beats as f64 * beat_sec.0);
        let t0 = score.tempo.q_to_sec(m0.start_q, scale);
        let end_sec = score.tempo.q_to_sec(score.measures()[to].end_q(), scale) - t0;
        let clicks = score.click_track(from, to, scale, count_in_beats, Clicks::WithBeats);
        Some(Plan { from, to, scale, bpm, beat_sec, count_in_beats, zero, t0, end_sec, clicks })
    }

    /// Stops everything that makes sound by itself: a take, Listen and the metronome.
    pub(super) fn stop_all_sound(&mut self) {
        self.stop_take(StopReason::Aborted);
        self.stop_listen();
        self.stop_metro();
    }

    pub(super) fn start_take(&mut self) {
        self.stop_all_sound();
        self.ensure_audio();
        self.sync_control();
        if self.source == Source::Rudiments && !self.rudiment().gradable() {
            self.view.status = format!("{} cannot be graded stroke by stroke: use Listen.", self.rudiment().name);
            return;
        }
        let label = self.practice_label();
        let window = if self.source == Source::Rudiments { Window::Adaptive } else { Window::Fixed };
        let Some(plan) = self.plan_run(true) else { return };
        let Some(score) = self.practice.score.as_ref() else { return };
        let expected = expected_notes(score, plan.from, plan.to, plan.scale);
        self.view.status = format!(
            "Recording measures {}–{} at {:.0} BPM",
            score.measures()[plan.from].number,
            score.measures()[plan.to].number,
            plan.bpm
        );
        let take = Take::start(self.practice.takes_started + 1, label, window, &plan, expected, &self.cfg.tol);
        self.metronome = Some(Metronome::start(plan.zero, plan.clicks, self.click_options(), Some(self.playback.control().clone())));
        self.practice.takes_started += 1;
        self.practice.take = Some(take);
    }

    pub(super) fn play_options(&self) -> PlayOptions {
        let c = &self.cfg;
        PlayOptions {
            audio: c.playback_audio && HAS_AUDIO,
            engine: self.audio.handle(),
            sounds_dir: self.paths.sounds_dir(),
            click_audio: c.click_audio && HAS_AUDIO,
            midi_port: (c.playback_midi || c.click_midi || c.midi_clock).then(|| c.midi_out_port.clone()),
            midi_drums: c.playback_midi,
            drum_channel: c.playback_channel,
            click_midi: c.click_midi,
            click_note: c.click_note,
            click_channel: c.click_channel,
            midi_delay: self.output_delay(),
            drum_notes: Lane::ALL.iter().filter_map(|l| c.note_map.playback_note(*l).map(|n| (*l, n))).collect(),
            ramp: self.listen_ramp(),
            clock: self.clock_options(),
        }
    }

    /// The MIDI clock, when the settings ask for it.
    pub(super) fn clock_options(&self) -> Option<ClockOptions> {
        let c = &self.cfg;
        c.midi_clock.then_some(ClockOptions { start: c.midi_clock_start, position: c.midi_clock_position })
    }

    /// The tempo of the practice as a factor of the written tempo at the start of the selected bars.
    pub(super) fn practice_scale(&self) -> f64 {
        let written = self
            .practice
            .score
            .as_ref()
            .filter(|s| !s.measures().is_empty())
            .map_or(100.0, |s| s.tempo.bpm_at(s.measures()[self.practice.range_from.min(s.measures().len() - 1)].start_q));
        self.practice.tempo.scale(written)
    }

    /// The tempo ramp of Listen: `ramp_percent` points of the written tempo every `ramp_passes` passes.
    pub(super) fn listen_ramp(&self) -> Ramp {
        let c = &self.cfg;
        // The tempo as per cent of the written one, whatever unit it is set in.
        let percent = self.practice_scale() * 100.0;
        if c.ramp_percent == 0 || percent <= 0.0 {
            return Ramp::default();
        }
        Ramp { every: c.ramp_passes, step: c.ramp_percent as f64 / percent }
    }

    /// Plays the selected measures at the chosen tempo, with the count-in and
    /// click a take would have, so the drummer can hear them first.
    pub(super) fn start_listen(&mut self) {
        self.begin_listen(if self.cfg.listen_count_in { CountIn::With } else { CountIn::Without });
    }

    pub(super) fn begin_listen(&mut self, count_in: CountIn) {
        self.stop_all_sound();
        self.ensure_audio();
        self.playback.cancel_restart();
        let opts = self.play_options();
        if let Some(message) = nothing_audible(&opts, Heard::Playback) {
            self.view.error = Some(message);
            return;
        }
        let ramp = opts.ramp;
        self.sync_control();
        let Some(plan) = self.plan_run(count_in == CountIn::With) else { return };
        let Some(score) = self.practice.score.as_ref() else { return };
        let expected = expected_notes(score, plan.from, plan.to, plan.scale);
        self.view.status = if expected.is_empty() {
            "There are no notes in the selected measures; only the click plays.".into()
        } else {
            format!(
                "Listening to measures {}–{} at {:.0} BPM",
                score.measures()[plan.from].number,
                score.measures()[plan.to].number,
                plan.bpm
            )
        };
        let mut cues = playback::cues(&expected, &plan.clicks);
        if let Some(clock) = opts.clock.as_ref() {
            let first = &score.measures()[plan.from];
            let count_in = Quarters(first.beat_q().0 * plan.count_in_beats as f64);
            let clock = playback::clock_cues(&score.tempo, first.start_q, score.measures()[plan.to].end_q(), plan.scale, count_in, clock);
            cues = playback::with_clock(cues, clock);
        }
        let player = Player::start(plan.zero, cues, plan.end_sec.0, opts, self.playback.control().clone());
        let listen = Listen {
            zero: plan.zero,
            tempo_scale: plan.scale,
            t0: plan.t0,
            end_sec: plan.end_sec,
            beat_sec: plan.beat_sec,
            count_in_beats: plan.count_in_beats,
            ramp,
            key: ListenKey { from: plan.from, to: plan.to, tempo: self.practice.tempo.value().round() as i32, score_gen: self.score_gen },
        };
        self.playback.begin(player, listen);
    }

    /// Stops at once, also cutting off cymbals that are still ringing.
    pub(super) fn stop_listen(&mut self) {
        self.playback.stop();
    }

    /// The range and tempo Listen would start with now.
    pub(super) fn listen_signature(&self) -> Option<ListenKey> {
        let n = self.practice.score.as_ref()?.measures().len();
        if n == 0 {
            return None;
        }
        let from = self.practice.range_from.min(n - 1);
        Some(ListenKey {
            from,
            to: self.practice.range_to.clamp(from, n - 1),
            tempo: self.practice.tempo.value().round() as i32,
            score_gen: self.score_gen,
        })
    }

    pub(super) fn update_listen(&mut self) {
        let polled = self.playback.poll(self.listen_signature(), Instant::now(), RESTART_DELAY);
        if polled.restart {
            self.begin_listen(CountIn::Without);
            return;
        }
        if polled.finished {
            self.view.status = match self.cfg.passes {
                Passes::Count(n) if n > 1 => format!("Listen finished: {n} passes"),
                _ => "Listen finished".to_string(),
            };
        }
        self.report_sound_errors(polled.errors);
    }

    /// Shows the problems the sound threads ran into: all of them go to the log, and the first one
    /// is on the screen, with how many more there are.
    pub(super) fn report_sound_errors(&mut self, errors: Vec<String>) {
        for e in &errors {
            self.log.line(e);
        }
        let mut errors = errors.into_iter();
        if let Some(first) = errors.next() {
            let more = errors.count();
            self.view.error = Some(if more == 0 { first } else { format!("{first} (and {more} more problems, see the log)") });
        }
    }

    /// Shows the problems the engine and the metronome thread of a take or a calibration ran into.
    pub(super) fn show_sound_errors(&mut self) {
        let mut errors = self.audio.take_errors();
        if let Some(m) = self.metronome.as_ref() {
            errors.extend(m.take_errors());
        }
        self.report_sound_errors(errors);
    }

    /// "bar 3 · beat 2", or the count-in, for the label next to the Listen button.
    pub(super) fn listen_position(&self) -> Option<String> {
        let l = self.playback.listen()?;
        let score = self.practice.score.as_ref()?;
        let elapsed = signed_secs(self.shown_now(), l.zero);
        if elapsed < Seconds::ZERO {
            let left = (-elapsed / l.beat_sec).ceil() as u32;
            return Some(if left > l.count_in_beats { "starting…".into() } else { format!("count-in {left}") });
        }
        let (pass, e) = l.ramp.locate(l.end_sec.0, elapsed.0);
        let q = score.tempo.sec_to_q(Seconds(e) + l.t0, l.tempo_scale);
        let m = &score.measures()[score.measure_at(q)?];
        let beat = (((q - m.start_q) / m.beat_q()).floor() as u32 + 1).min(m.beats);
        let mut text = format!("bar {} · beat {}", m.number, beat);
        if l.ramp.every > 0 {
            text.push_str(&format!(" · {:.0} %", l.tempo_scale * 100.0 * l.ramp.factor(pass)));
        }
        Some(text)
    }

    /// The moment the screen shows: the playhead, the beat lights and the count-in are drawn for it. With
    /// the sound coming through the computer it is one output delay (the latency compensation) behind
    /// the clock, so the picture and the sound agree; grading always uses the clock itself.
    pub(super) fn shown_now(&self) -> Instant {
        let now = Instant::now();
        now.checked_sub(self.output_delay()).unwrap_or(now)
    }

    /// The playhead of Listen, in quarter notes; it wraps around while looping.
    pub(super) fn listen_playhead(&self) -> Option<f64> {
        let l = self.playback.listen()?;
        let score = self.practice.score.as_ref()?;
        let elapsed = signed_secs(self.shown_now(), l.zero);
        let (_, e) = l.ramp.locate(l.end_sec.0, elapsed.0);
        Some(score.tempo.sec_to_q(Seconds(e) + l.t0, l.tempo_scale).0)
    }

    /// Plays one instrument on its own so it can be heard (and tuned) without a score.
    pub(super) fn audition(&mut self, lane: Lane) {
        self.ensure_audio();
        let opts = self.play_options();
        if let Some(message) = nothing_audible(&opts, Heard::Instruments) {
            self.view.error = Some(message);
            return;
        }
        self.playback.stop_audition();
        self.sync_control();
        let cue = playback::Cue { t: 0.0, sound: playback::Sound::Drum { lane, velocity: 100 } };
        let start = Instant::now() + Duration::from_millis(300);
        let player = Player::start(start, vec![cue], 0.01, opts, self.playback.audition_control().clone());
        self.playback.begin_audition(player);
    }

    pub(super) fn update_audition(&mut self) {
        let errors = self.playback.poll_audition();
        self.report_sound_errors(errors);
    }

    /// `completed`: the whole range was played (otherwise notes after the
    /// stop point are left ungraded).
    /// Stops the take, and shows how it went on the status line.
    pub(super) fn stop_take(&mut self, reason: StopReason) {
        match self.finish_take(reason) {
            TakeOutcome::NoTake => {}
            TakeOutcome::NothingDue => self.view.status = "Take stopped before any notes were due — nothing to grade.".into(),
            TakeOutcome::Graded { summary, clean_bpm } => {
                self.view.status = match clean_bpm {
                    Some(bpm) => format!("{summary} · {}", self.note_clean_rudiment(bpm)),
                    None => summary,
                };
            }
        }
    }

    /// Stops the metronome and the take that records, grades the take and puts it in the history;
    /// says nothing, and returns what came of it.
    pub(super) fn finish_take(&mut self, reason: StopReason) -> TakeOutcome {
        if let Some(mut m) = self.metronome.take() {
            m.stop();
        }
        let Some(take) = self.practice.take.as_mut() else { return TakeOutcome::NoTake };
        if !take.finish(reason, &self.cfg.tol, Instant::now()) {
            return TakeOutcome::NoTake;
        }
        let take = &*take;
        let r = take.report();
        if r.evaluated == 0 {
            return TakeOutcome::NothingDue;
        }
        let score = self.practice.score.as_ref();
        let bpm = score.map_or(0.0, |s| take.bpm(s));
        let range = if take.window() == Window::Adaptive {
            take.label().to_string()
        } else {
            score.map(|s| take.range_text(s)).unwrap_or_default()
        };
        let clean = reason == StopReason::Completed && take.window() == Window::Adaptive && r.overall >= CLEAN_TAKE;
        self.practice.history.push(TakeSummary {
            number: take.number(),
            range,
            bpm,
            overall: r.overall,
            letter: r.letter(),
            accuracy: r.accuracy,
            mean_dt: r.mean_dt_ms,
            std_dt: r.std_dt_ms,
            extra: r.extra,
        });
        let summary = format!(
            "Take {}: {:.0} ({}) — accuracy {:.0}%, timing {:+.0} ± {:.0} ms, {} missed, {} extra",
            take.number(),
            r.overall,
            r.letter(),
            r.accuracy * 100.0,
            r.mean_dt_ms,
            r.std_dt_ms,
            r.missed,
            r.extra
        );
        TakeOutcome::Graded { summary, clean_bpm: clean.then_some(bpm) }
    }

    pub(super) fn update_take(&mut self) {
        let Some(take) = self.practice.take.as_mut() else { return };
        if take.update(&self.cfg.tol, Instant::now()) {
            self.stop_take(StopReason::Completed);
        }
    }

    pub(super) fn start_calibration(&mut self) {
        self.stop_all_sound();
        self.ensure_audio();
        let beat = 0.6; // 100 BPM
        let zero = Instant::now() + Duration::from_secs_f64(1.0);
        let beats: Vec<f64> = (0..16).map(|i| i as f64 * beat).collect();
        let clicks = beats.iter().map(|t| Click { t: Seconds(*t), accent: false }).collect();
        // No control: the calibration clicks must sound whatever the click switch says.
        // The calibration measures the whole delay, so its MIDI click is not moved.
        let opts = ClickOptions { midi_delay: Duration::ZERO, ..self.click_options() };
        self.metronome = Some(Metronome::start(zero, clicks, opts, None));
        self.calibration = Some(Calibration { zero, beats, hits: Vec::new() });
        self.view.status = "Calibrating: hit any pad together with the 16 clicks…".into();
    }

    pub(super) fn update_calibration(&mut self) {
        let Some(cal) = self.calibration.as_ref() else { return };
        let now = signed_secs(Instant::now(), cal.zero).0;
        if now < cal.beats.last().copied().unwrap_or(0.0) + 0.6 {
            return;
        }
        // Skip the first 4 clicks (getting into the groove).
        let mut offsets: Vec<f64> = cal.beats[4..]
            .iter()
            .filter_map(|b| cal.hits.iter().map(|h| h - b).filter(|d| d.abs() < 0.25).min_by(|x, y| x.abs().total_cmp(&y.abs())))
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

    /// What the take shows on the score: the notes played and the grades, as of the last hit or grading.
    pub(super) fn take_shown(&self) -> Option<Rc<Shown>> {
        Some(self.practice.take.as_ref()?.shown(self.practice.score.as_ref()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn options(playback_midi: bool, click_midi: bool) -> PlayOptions {
        let cfg = Config { playback_audio: false, click_audio: false, playback_midi, click_midi, ..Config::default() };
        DrumApp::with_config(cfg).play_options()
    }

    #[test]
    fn what_cannot_be_heard_is_told_in_one_place_for_the_music_the_click_and_the_instruments() {
        let nothing = options(false, false);
        for what in [Heard::Playback, Heard::Click, Heard::Instruments] {
            let message = nothing_audible(&nothing, what).expect("nothing is switched on");
            assert!(message.contains("Sound page of Settings"), "{message}");
        }
        let click_only = options(false, true);
        assert!(nothing_audible(&click_only, Heard::Playback).is_none(), "the click is heard with the music");
        assert!(nothing_audible(&click_only, Heard::Click).is_none());
        assert!(nothing_audible(&click_only, Heard::Instruments).is_some(), "but no instrument sounds");
        let drums_only = options(true, false);
        assert!(nothing_audible(&drums_only, Heard::Playback).is_none() && nothing_audible(&drums_only, Heard::Instruments).is_none());
        assert!(nothing_audible(&drums_only, Heard::Click).is_some());
    }

    #[test]
    fn the_midi_clock_follows_the_settings_and_opens_the_midi_output_by_itself() {
        use crate::config::ClockStart;
        let off = DrumApp::with_config(Config { playback_midi: false, click_midi: false, ..Config::default() });
        assert!(off.play_options().clock.is_none() && off.play_options().midi_port.is_none());
        let cfg = Config {
            midi_clock: true,
            midi_clock_start: ClockStart::Downbeat,
            midi_clock_position: true,
            playback_audio: false,
            click_audio: false,
            playback_midi: false,
            click_midi: false,
            midi_out_port: "V31".into(),
            ..Config::default()
        };
        let app = DrumApp::with_config(cfg);
        let opts = app.play_options();
        assert_eq!(opts.clock, Some(ClockOptions { start: ClockStart::Downbeat, position: true }));
        assert_eq!(opts.midi_port.as_deref(), Some("V31"), "the output is opened for the clock alone");
        assert!(nothing_audible(&opts, Heard::Playback).is_none() && nothing_audible(&opts, Heard::Click).is_none());
        assert!(nothing_audible(&opts, Heard::Instruments).is_some(), "an instrument is not made heard by a clock");
    }
}
