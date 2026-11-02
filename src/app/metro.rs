//! The Metronome tab: a click track that needs no score. It loops one bar of
//! clicks (an accent, normal beats, softer subdivisions) until it is stopped.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use egui::{pos2, vec2, RichText, Sense, Stroke};

use super::metro_state::{MetroKey, Run};
use super::session::{nothing_audible, Heard};
use super::{DrumApp, Source, RESTART_DELAY};
use crate::config::{limits, Subdivision};
use crate::controller::Action;
use crate::metronome::HAS_AUDIO;
use crate::playback::{self, PlayOptions, Player};
use crate::practice::Ramp;
use crate::render;
use crate::score::Score;
use crate::theme;
use crate::units::Quarters;
use crate::widgets;

impl DrumApp {
    fn metro_key(&self) -> MetroKey {
        MetroKey { tempo_tenths: (self.cfg.metro_bpm * 10.0).round() as i32, beats: self.cfg.metro_beats, subdivision: self.cfg.metro_sub }
    }

    /// The metronome gets `metro_ramp_bpm` faster every `metro_ramp_bars` bars.
    pub(super) fn metro_ramp(&self) -> Ramp {
        let c = &self.cfg;
        if c.metro_ramp_bpm == 0 || c.metro_bpm <= 0.0 {
            return Ramp::default();
        }
        Ramp { every: c.metro_ramp_bars, step: c.metro_ramp_bpm as f64 / c.metro_bpm as f64 }
    }

    /// The click goes where the Sound settings send it; there are no drums.
    fn metro_options(&self) -> PlayOptions {
        let c = &self.cfg;
        PlayOptions {
            audio: false,
            engine: self.audio.handle(),
            sounds_dir: None,
            click_audio: c.click_audio && HAS_AUDIO,
            midi_port: c.click_midi.then(|| c.midi_out_port.clone()),
            midi_drums: false,
            drum_channel: c.playback_channel,
            click_midi: c.click_midi,
            click_note: c.click_note,
            click_channel: c.click_channel,
            midi_delay: self.output_delay(),
            drum_notes: BTreeMap::new(),
            ramp: self.metro_ramp(),
        }
    }

    pub(super) fn start_metro(&mut self) {
        self.stop_all_sound();
        self.ensure_audio();
        let opts = self.metro_options();
        if let Some(message) = nothing_audible(&opts, Heard::Click) {
            self.view.error = Some(message);
            return;
        }
        let sub = self.cfg.metro_sub;
        let bpm = self.cfg.metro_bpm as f64;
        let (cues, bar_len) = playback::metronome_cues(bpm, self.cfg.metro_beats, sub);
        self.sync_control();
        // Half a second of lead: the sound device needs time to open.
        let zero = Instant::now() + Duration::from_millis(500);
        let player = Player::start(zero, cues, bar_len, opts, self.metro_run.control().clone());
        self.metro_run.begin(player, Run::new(zero, 60.0 / bpm, bar_len, self.metro_ramp(), self.metro_key()));
        self.view.status = format!("Metronome: {bpm:.0} BPM, {} beats per bar", self.cfg.metro_beats);
        let ramp = self.metro_ramp();
        if ramp.every > 0 {
            self.view.status.push_str(&format!(", {} BPM faster every {} bars", self.cfg.metro_ramp_bpm, ramp.every));
        }
    }

    pub(super) fn stop_metro(&mut self) {
        self.metro_run.stop();
    }

    /// Restarts a running metronome once a changed tempo, bar length or subdivision has settled.
    pub(super) fn update_metro(&mut self) {
        let polled = self.metro_run.poll(self.metro_key(), Instant::now(), RESTART_DELAY);
        if polled.restart {
            self.start_metro();
            return;
        }
        self.report_sound_errors(polled.errors);
    }

    pub(super) fn metronome_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.heading("Metronome");
        ui.label(
            RichText::new("A click track that needs no score. It plays through the click output chosen on the Sound page of Settings (the sound card and/or MIDI).")
                .small()
                .weak(),
        );
        ui.add_space(8.0);
        ui.scope(|ui| {
            widgets::roomy(ui);
            let running = self.metro_run.is_running();
            let label = if running { "⏹ Stop (M)" } else { "⏵ Start (M)" };
            if ui.button(RichText::new(label).strong().size(22.0)).clicked() {
                self.press(Action::Metronome);
            }
            ui.add_space(6.0);
            widgets::Slider::new("tempo", &mut self.cfg.metro_bpm, limits::METRO_BPM.range()).suffix(" BPM").show(ui);
            widgets::Slider::new("beats per bar", &mut self.cfg.metro_beats, 1..=12).show(ui);
            ui.horizontal(|ui| {
                ui.label("subdivision");
                for s in Subdivision::ALL {
                    ui.selectable_value(&mut self.cfg.metro_sub, s, s.label());
                }
            });
            widgets::Slider::new("volume", &mut self.cfg.click_volume, limits::CLICK_VOLUME.range()).step(0.02).show(ui);
            ui.horizontal(|ui| {
                widgets::Slider::new("faster by", &mut self.cfg.metro_ramp_bpm, limits::METRO_RAMP_BPM.range()).suffix(" BPM").zero_text("off").show(ui)
                    .on_hover_text("A tempo ramp: the metronome gets faster by this much every few bars, up to three times the starting tempo. Takes effect at the next start.");
                if self.cfg.metro_ramp_bpm > 0 {
                    widgets::Slider::new("every", &mut self.cfg.metro_ramp_bars, limits::METRO_RAMP_BARS.range()).suffix(" bars").show(ui);
                }
            });
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Changes while it runs take effect after a moment. The first beat of each bar is accented; subdivisions are softer.",
            )
            .small()
            .weak(),
        );
        ui.add_space(16.0);
        self.beat_lights(ui);
        self.rolling_preview(ui);
    }

    /// Where the rolling preview is, in quarters from the start of the score: the
    /// practice range repeats, and the metronome's first beat is its first note.
    fn metro_preview_pos(&self, start_q: Quarters, len_q: Quarters) -> f64 {
        match self.metro_run.beats_at(self.shown_now()) {
            Some(beats) => start_q.0 + beats.rem_euclid(len_q.0.max(1e-6)),
            None => start_q.0,
        }
    }

    /// The loaded score, or with none the metronome's own bar, scrolling past a
    /// fixed line at the metronome's tempo.
    fn rolling_preview(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        let click_bar = self.practice.score.is_none().then(|| Score::click_bar(self.cfg.metro_beats, self.cfg.metro_sub.parts()));
        let Some(score) = click_bar.as_ref().or(self.practice.score.as_ref()) else { return };
        let last = score.measures().len().saturating_sub(1);
        let (a, b) = match click_bar {
            Some(_) => (0, 0),
            None => (self.practice.range_from.min(last), self.practice.range_to.clamp(self.practice.range_from.min(last), last)),
        };
        let (start_q, len_q) = (score.measures()[a].start_q, score.measures()[b].end_q() - score.measures()[a].start_q);
        // A rudiment is practised at a tempo in BPM; the free metronome plays at that tempo.
        let tempo_factor = if self.source == Source::Rudiments { self.practice.tempo.scale(score.tempo.bpm_at(start_q)) } else { 1.0 };
        let (score_bpm, bar_q) = (score.tempo.bpm_at(start_q) * tempo_factor, score.measures()[a].len_q);
        let description = match click_bar {
            Some(_) => "no score loaded: the click pattern".to_string(),
            None => format!("\"{}\", bars {}–{} repeating", score.title, a + 1, b + 1),
        };
        let has_score = click_bar.is_none();

        let mut match_score = false;
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.cfg.metro_preview, "Show the notes");
            if self.cfg.metro_preview {
                widgets::hint(ui, description);
                if has_score {
                    match_score = ui
                        .button("Match score tempo and bar")
                        .on_hover_text("Set the tempo and beats per bar to the score's, so the click lines up with the bars")
                        .clicked();
                }
            }
        });
        if match_score {
            self.cfg.metro_bpm = (score_bpm as f32).clamp(20.0, 300.0);
            self.cfg.metro_beats = (bar_q.0.round() as u32).clamp(1, 12);
        }
        if !self.cfg.metro_preview {
            return;
        }
        let pos_q = self.metro_preview_pos(start_q, len_q);
        if let Some(score) = click_bar.as_ref().or(self.practice.score.as_ref()) {
            render::rolling(ui, &render::RollingView { score, range: (a, b), pos_q });
        }
        ui.label(
            RichText::new(if has_score {
                "The practice range of the Score tab (or the rudiment of the Rudiments tab) repeats at the metronome's tempo; the first note arrives on the first beat. Change the range on the Score tab."
            } else {
                "Every click of the bar rolls by: the first beat (accent), the other beats, and the softer subdivisions in brackets. Load a score to play its notes instead."
            })
            .small()
            .weak(),
        );
    }

    /// One circle per beat of the bar; the current beat lights up and fades.
    fn beat_lights(&self, ui: &mut egui::Ui) {
        let beats = self.cfg.metro_beats.max(1);
        let (radius, gap) = (22.0_f32, 14.0_f32);
        // Room for the outline of the first circle, which the painter would clip.
        let margin = 6.0_f32;
        let size = vec2(2.0 * margin + beats as f32 * (2.0 * radius + gap), 2.0 * radius + 8.0);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter_at(rect);
        let current = self.metro_run.beat_at(self.shown_now()).map(|(beat, into)| (beat as u64 % beats as u64, 1.0 - into));
        let outline = ui.visuals().widgets.noninteractive.fg_stroke.color.gamma_multiply(0.5);
        for b in 0..beats {
            let centre = pos2(rect.left() + margin + radius + b as f32 * (2.0 * radius + gap), rect.center().y);
            let colour = if b == 0 { theme::RED } else { theme::ACCENT };
            painter.circle_stroke(centre, radius, Stroke::new(1.5_f32, outline));
            if let Some((index, brightness)) = current {
                if index == b as u64 {
                    painter.circle_filled(centre, radius, colour.gamma_multiply(0.25 + 0.75 * brightness));
                }
            }
        }
    }
}
