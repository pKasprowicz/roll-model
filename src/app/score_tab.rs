//! The Score tab: the transport row, the switches and the score itself.

use std::time::Instant;

use egui::{Color32, RichText};

use crate::render::{self, LayoutOpts, ScoreView};
use crate::widgets;

use super::session::{score_q, signed_secs};
use super::{DrumApp, Source, TAKE_LINE_HEIGHT};

impl DrumApp {
    pub(super) fn score_tab(&mut self, ui: &mut egui::Ui) {
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
    pub(super) fn transport_row(&mut self, ui: &mut egui::Ui, running: bool, n_measures: usize) {
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
            if self.playback.listen.is_some() {
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
                let bars = widgets::slider_with_zero(ui, "bars per row", &mut self.cfg.bars_per_row, 0..=8, "", 1.0, Some("auto"));
                let align = widgets::slider_with_zero(ui, "start a row at bar", &mut self.cfg.layout_align_bar, 0..=n_measures, "", 1.0, Some("off"));
                if bars.changed() || align.changed() {
                    self.keep_layout();
                }
                ui.checkbox(&mut self.cfg.break_at_pause, "new row after a pause")
                    .on_hover_text("A bar that ends with a rest (and no note in any voice during it) is the last of its row; the score goes on in the next row.");
                let printed = self
                    .score
                    .as_ref()
                    .and_then(|s| s.measures.get(self.cfg.layout_align_bar.wrapping_sub(1)))
                    .map(|m| m.number.clone());
                if let Some(number) = printed {
                    widgets::hint(ui, format!("Bar {} of the score as shown, printed as bar {number} in the file.", self.cfg.layout_align_bar));
                }
                widgets::hint(ui, "With a fixed number of bars per row every bar has the same width and the zoom no longer matters. Each score keeps its own layout.");
            });
        });
    }

    /// The switches that work while the music plays, and the count-in.
    pub(super) fn playback_switches(&mut self, ui: &mut egui::Ui, running: bool) {
        // The click can be switched on and off while the music plays, in Listen and in a take.
        ui.checkbox(&mut self.cfg.metronome_during_take, "click (K)")
            .on_hover_text("Switch the metronome on or off at any time, also while playing or recording. The count-in always sounds.");
        ui.add_enabled_ui(!running, |ui| {
            ui.checkbox(&mut self.playback.notes, "notes")
                .on_hover_text("Play the notes of the score in Listen. Switch them off to use Listen as a click track.");
            ui.checkbox(&mut self.playback.looping, "loop")
                .on_hover_text("Repeat the selected measures in Listen until it is stopped. Switching it off lets the current round finish.");
            widgets::slider(ui, "count-in bars", &mut self.cfg.count_in_bars, 0..=2, "", 1.0);
            ui.checkbox(&mut self.cfg.listen_count_in, "in Listen")
                .on_hover_text("Start Listen with the count-in too. A recording always has one; switch this off to hear the music at once.");
            widgets::slider_with_zero(ui, "ramp +", &mut self.cfg.ramp_percent, 0..=20, " %", 1.0, Some("off"))
                .on_hover_text("With loop on, Listen gets faster by this many per cent of the written tempo every few passes (up to three times the starting tempo). Takes effect at the next start.");
            if self.cfg.ramp_percent > 0 {
                widgets::slider(ui, "every", &mut self.cfg.ramp_passes, 1..=16, " passes", 1.0);
            }
        });
    }

    /// The take's live figures, the colour legend and the score with the notes played, shared by
    /// the Score and Rudiments tabs.
    pub(super) fn score_body(&mut self, ui: &mut egui::Ui, running: bool, n_measures: usize) {
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
            widgets::hint(ui, "· click a measure to start there, shift+click to set the end · mouse wheel over a slider fine-tunes it");
            if let Some(fit) = self.view.fit_bars {
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
            follow: self.follow && (running || self.playback.listen.is_some()),
            layout: LayoutOpts { bars_per_row: self.cfg.bars_per_row, align_bar: self.cfg.layout_align_bar, break_at_pause: self.cfg.break_at_pause },
        };
        let out = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| render::show(ui, &view))
            .inner;
        self.view.fit_bars = out.fit_bars;
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
}
