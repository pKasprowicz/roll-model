//! The Metronome tab: a click track that needs no score. It loops one bar of
//! clicks (an accent, normal beats, softer subdivisions) until it is stopped.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use egui::{pos2, vec2, RichText, Sense, Stroke};

use super::metro_state::{MetroKey, Run};
use super::session::{nothing_audible, Heard};
use super::{DrumApp, RESTART_DELAY};
use crate::audio::HAS_AUDIO;
use crate::config::{limits, Subdivision};
use crate::controller::Action;
use crate::playback::{self, PlayOptions, Player};
use crate::practice::Ramp;
use crate::theme;
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
            midi_port: (c.click_midi || c.midi_clock).then(|| c.midi_out_port.clone()),
            midi_drums: false,
            drum_channel: c.playback_channel,
            click_midi: c.click_midi,
            click_note: c.click_note,
            click_channel: c.click_channel,
            midi_delay: self.output_delay(),
            drum_notes: BTreeMap::new(),
            ramp: self.metro_ramp(),
            clock: self.clock_options(),
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
        let (mut cues, bar_len) = playback::metronome_cues(bpm, self.cfg.metro_beats, sub);
        if opts.clock.is_some() {
            cues = playback::with_clock(cues, playback::metronome_clock(bpm, self.cfg.metro_beats));
        }
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
