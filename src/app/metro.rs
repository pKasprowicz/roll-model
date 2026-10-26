//! The Metronome tab: a click track that needs no score. It loops one bar of
//! clicks (an accent, normal beats, softer subdivisions) until it is stopped.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use egui::{pos2, vec2, RichText, Sense, Stroke};

use super::DrumApp;
use crate::metronome::HAS_AUDIO;
use crate::playback::{self, PlayOptions, Player, Subdivision};
use crate::render;
use crate::score::Score;
use crate::widgets;

/// How long a changed setting must stay put before a running metronome restarts with it.
const RESTART_DELAY: Duration = Duration::from_millis(300);

/// A running metronome and what it was started with.
pub(super) struct Run {
    zero: Instant,
    beat_sec: f64,
    /// Tenths of a BPM, beats per bar, subdivision: a change restarts it.
    signature: (i32, u32, usize),
    /// A changed signature and since when it has been stable.
    pending: Option<((i32, u32, usize), Instant)>,
}

impl DrumApp {
    fn metro_signature(&self) -> (i32, u32, usize) {
        ((self.cfg.metro_bpm * 10.0).round() as i32, self.cfg.metro_beats, self.cfg.metro_sub)
    }

    /// The click goes where the Sound settings send it; there are no drums.
    fn metro_options(&self) -> PlayOptions {
        let c = &self.cfg;
        PlayOptions {
            audio: false,
            audio_device: c.audio_output.clone(),
            sounds_dir: None,
            click_audio: c.click_audio && HAS_AUDIO,
            midi_port: c.click_midi.then(|| c.midi_out_port.clone()),
            midi_drums: false,
            drum_channel: c.playback_channel,
            click_midi: c.click_midi,
            click_note: c.click_note,
            click_channel: c.click_channel,
            drum_notes: BTreeMap::new(),
        }
    }

    pub(super) fn start_metro(&mut self) {
        self.stop_take(false);
        self.stop_listen();
        self.stop_metro();
        let opts = self.metro_options();
        if !opts.click_audio && !opts.click_midi {
            self.error = Some(if HAS_AUDIO {
                "The click is switched off: enable it under Metronome on the Sound page of Settings.".into()
            } else {
                "This build has no sound output: send the click to a MIDI output on the Sound page of Settings.".into()
            });
            return;
        }
        let sub = Subdivision::from_index(self.cfg.metro_sub);
        let bpm = self.cfg.metro_bpm as f64;
        let (cues, bar_len) = playback::metronome_cues(bpm, self.cfg.metro_beats, sub);
        self.sync_control();
        // Half a second of lead: the sound device needs time to open.
        let zero = Instant::now() + Duration::from_millis(500);
        self.metro_player = Some(Player::start(zero, cues, bar_len, opts, self.metro_control.clone()));
        self.metro = Some(Run { zero, beat_sec: 60.0 / bpm, signature: self.metro_signature(), pending: None });
        self.status = format!("Metronome: {bpm:.0} BPM, {} beats per bar", self.cfg.metro_beats);
    }

    pub(super) fn stop_metro(&mut self) {
        if let Some(mut p) = self.metro_player.take() {
            p.stop();
        }
        self.metro = None;
    }

    /// Restarts a running metronome once a changed tempo, bar length or subdivision has settled.
    pub(super) fn update_metro(&mut self) {
        let now_signature = self.metro_signature();
        let mut restart = false;
        if let Some(run) = self.metro.as_mut() {
            if run.signature == now_signature {
                run.pending = None;
            } else {
                match run.pending {
                    Some((sig, since)) if sig == now_signature => restart = since.elapsed() >= RESTART_DELAY,
                    _ => run.pending = Some((now_signature, Instant::now())),
                }
            }
        }
        if restart {
            self.start_metro();
        }
        if let Some(p) = self.metro_player.as_ref() {
            if let Some(e) = p.take_errors().into_iter().next() {
                self.error = Some(e);
            }
            if p.is_finished() {
                self.metro_player = None;
                self.metro = None;
            }
        }
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
            let running = self.metro.is_some();
            let label = if running { "⏹ Stop (M)" } else { "⏵ Start (M)" };
            if ui.button(RichText::new(label).strong().size(22.0)).clicked() {
                if running {
                    self.stop_metro();
                } else {
                    self.start_metro();
                }
            }
            ui.add_space(6.0);
            widgets::slider(ui, "tempo", &mut self.cfg.metro_bpm, 20.0..=300.0, " BPM", 1.0);
            widgets::slider(ui, "beats per bar", &mut self.cfg.metro_beats, 1..=12, "", 1.0);
            ui.horizontal(|ui| {
                ui.label("subdivision");
                for s in Subdivision::ALL {
                    ui.selectable_value(&mut self.cfg.metro_sub, s.index(), s.label());
                }
            });
            widgets::slider(ui, "volume", &mut self.cfg.click_volume, 0.0..=1.0, "", 0.02);
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new("Changes while it runs take effect after a moment. The first beat of each bar is accented; subdivisions are softer.")
                .small()
                .weak(),
        );
        ui.add_space(16.0);
        self.beat_lights(ui);
        self.rolling_preview(ui);
    }

    /// Where the rolling preview is, in quarters from the start of the score: the
    /// practice range repeats, and the metronome's first beat is its first note.
    fn metro_preview_pos(&self, start_q: f64, len_q: f64) -> f64 {
        let Some(run) = self.metro.as_ref() else { return start_q };
        let now = Instant::now();
        let elapsed = match now.checked_duration_since(run.zero) {
            Some(d) => d.as_secs_f64(),
            None => -run.zero.duration_since(now).as_secs_f64(),
        };
        start_q + (elapsed / run.beat_sec).rem_euclid(len_q.max(1e-6))
    }

    /// The loaded score, or with none the metronome's own bar, scrolling past a
    /// fixed line at the metronome's tempo.
    fn rolling_preview(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        let click_bar = self
            .score
            .is_none()
            .then(|| Score::click_bar(self.cfg.metro_beats, Subdivision::from_index(self.cfg.metro_sub).parts()));
        let Some(score) = click_bar.as_ref().or(self.score.as_ref()) else { return };
        let last = score.measures.len().saturating_sub(1);
        let (a, b) = match click_bar {
            Some(_) => (0, 0),
            None => (self.range_from.min(last), self.range_to.clamp(self.range_from.min(last), last)),
        };
        let (start_q, len_q) = (score.measures[a].start_q, score.measures[b].end_q() - score.measures[a].start_q);
        let (score_bpm, bar_q) = (score.tempo.bpm_at(start_q), score.measures[a].len_q);
        let description = match click_bar {
            Some(_) => "no score loaded: the click pattern".to_string(),
            None => format!("\"{}\", bars {}–{} repeating", score.title, a + 1, b + 1),
        };
        let has_score = click_bar.is_none();

        let mut match_score = false;
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.cfg.metro_preview, "Show the notes");
            if self.cfg.metro_preview {
                ui.label(RichText::new(description).small().weak());
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
            self.cfg.metro_beats = (bar_q.round() as u32).clamp(1, 12);
        }
        if !self.cfg.metro_preview {
            return;
        }
        let pos_q = self.metro_preview_pos(start_q, len_q);
        if let Some(score) = click_bar.as_ref().or(self.score.as_ref()) {
            render::rolling(ui, &render::RollingView { score, range: (a, b), pos_q });
        }
        ui.label(
            RichText::new(if has_score {
                "The practice range of the Score tab repeats at the metronome's tempo; the first note arrives on the first beat. Change the range on the Score tab."
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
        let current = self.metro.as_ref().and_then(|run| {
            let elapsed = Instant::now().checked_duration_since(run.zero)?.as_secs_f64();
            let beat = elapsed / run.beat_sec;
            Some((beat.floor() as u64 % beats as u64, 1.0 - beat.fract() as f32))
        });
        let outline = ui.visuals().widgets.noninteractive.fg_stroke.color.gamma_multiply(0.5);
        for b in 0..beats {
            let centre = pos2(rect.left() + margin + radius + b as f32 * (2.0 * radius + gap), rect.center().y);
            let colour = if b == 0 { render::RED } else { render::ACCENT };
            painter.circle_stroke(centre, radius, Stroke::new(1.5_f32, outline));
            if let Some((index, brightness)) = current {
                if index == b as u64 {
                    painter.circle_filled(centre, radius, colour.gamma_multiply(0.25 + 0.75 * brightness));
                }
            }
        }
    }
}
