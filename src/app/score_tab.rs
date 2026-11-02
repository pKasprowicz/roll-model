//! The Score tab: the transport row, the switches and the score itself.

use egui::{Color32, RichText};

use crate::config::limits;
use crate::controller::Action;
use crate::render::{self, LayoutOpts, ScoreView};
use crate::theme;
use crate::units::Seconds;
use crate::widgets;

use super::session::{score_q, signed_secs};
use super::{DrumApp, Source, TAKE_LINE_HEIGHT};

impl DrumApp {
    pub(super) fn score_tab(&mut self, ui: &mut egui::Ui) {
        let Some(score) = self.practice.score.as_ref() else {
            self.welcome_panel(ui);
            return;
        };
        let n_measures = score.measures.len();
        let running = self.take_running();
        let base_bpm = score.tempo.bpm_at(score.measures[self.practice.range_from.min(n_measures - 1)].start_q);
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
            ui.label(RichText::new(format!("⚠ {w}")).small().color(theme::AMBER));
        }
        // Three plain rows (sliders don't wrap inside `horizontal_wrapped`), drawn larger than the
        // rest of the UI because they are used with the mouse from a drum stool. Slider captions
        // come first, and the wheel tunes any slider under the pointer.
        ui.scope(|ui| {
            widgets::roomy(ui);
            self.transport_row(ui, running, n_measures);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!running, |ui| {
                    let mut from = self.practice.range_from + 1;
                    let mut to = self.practice.range_to + 1;
                    ui.label("Measures");
                    widgets::slider(ui, "from", &mut from, 1..=n_measures, "", 1.0);
                    widgets::slider(ui, "to", &mut to, 1..=n_measures, "", 1.0);
                    self.practice.range_from = from - 1;
                    self.practice.range_to = (to - 1).max(self.practice.range_from);
                });
            });
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!running, |ui| {
                    let (lo, hi) = self.practice.tempo.range();
                    let mut value = self.practice.tempo.value();
                    widgets::slider(ui, "tempo", &mut value, lo..=hi, self.practice.tempo.unit(), 1.0);
                    self.practice.tempo = self.practice.tempo.with_value(value);
                    ui.label(format!("= {:.0} BPM", self.practice.tempo.bpm(base_bpm)));
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
                    self.press(Action::Stop);
                }
            } else {
                let gradable = self.source == Source::Score || self.rudiment().gradable();
                let record = egui::Button::new(RichText::new("⏺ Record (R)").strong().color(theme::RED));
                if ui
                    .add_enabled(gradable, record)
                    .on_disabled_hover_text("A closed roll cannot be graded stroke by stroke: use Listen")
                    .clicked()
                {
                    self.press(Action::Record);
                }
            }
            if self.playback.listen.is_some() {
                if ui.button(RichText::new("⏹ Stop listening (L)").strong()).clicked() {
                    self.press(Action::Listen);
                }
            } else if ui
                .add_enabled(!running, egui::Button::new(RichText::new("⏵ Listen (L)").strong()))
                .on_hover_text("Hear the selected measures at the chosen tempo, with the count-in and click, before you play them")
                .clicked()
            {
                self.press(Action::Listen);
            }
            // Where Listen is in the music; a fixed width, so the controls after it stay put.
            let position = self.listen_position().unwrap_or_default();
            ui.add_sized([150.0, 30.0], egui::Label::new(RichText::new(position).weak()));
            ui.separator();
            // With a fixed number of bars per row the bars fill the width, so there is nothing to zoom.
            let auto_rows = self.cfg.bars_per_row == 0;
            ui.add_enabled_ui(auto_rows, |ui| {
                widgets::slider(ui, "zoom", &mut self.cfg.zoom, limits::ZOOM.range(), "", 0.05);
            })
            .response
            .on_disabled_hover_text("Zoom sets how wide a bar is drawn when the rows are automatic. With a fixed number of bars per row (Layout menu) the bars fill the width instead; set bars per row to auto to use the zoom.");
            ui.checkbox(&mut self.follow, "follow");
            ui.menu_button("Layout", |ui| {
                ui.set_min_width(320.0);
                let bars = widgets::slider_with_zero(ui, "bars per row", &mut self.cfg.bars_per_row, limits::BARS_PER_ROW.range(), "", 1.0, Some("auto"));
                let align = widgets::slider_with_zero(ui, "start a row at bar", &mut self.cfg.layout_align_bar, 0..=n_measures, "", 1.0, Some("off"));
                if bars.changed() || align.changed() {
                    self.keep_layout();
                }
                ui.checkbox(&mut self.cfg.break_at_pause, "new row after a pause")
                    .on_hover_text("A bar that ends with a rest (and no note in any voice during it) is the last of its row; the score goes on in the next row.");
                let printed = self
                    .practice
                    .score
                    .as_ref()
                    .and_then(|s| s.measures.get(self.cfg.layout_align_bar.wrapping_sub(1)))
                    .map(|m| m.number.clone());
                if let Some(number) = printed {
                    widgets::hint(ui, format!("Bar {} of the score as shown, printed as bar {number} in the file.", self.cfg.layout_align_bar));
                }
                widgets::hint(ui, "With a fixed number of bars per row every bar has the same width and the zoom no longer matters. Each score keeps its own layout.");
            });
            if self.is_midi_score() {
                self.midi_import_menu(ui);
            }
        });
    }

    /// The switches that work while the music plays, and the count-in.
    pub(super) fn playback_switches(&mut self, ui: &mut egui::Ui, running: bool) {
        // The click can be switched on and off while the music plays, in Listen and in a take.
        // The switches are the same commands as the keys and the controller's buttons: a click only
        // asks for the change, `press` makes it.
        let mut click = self.cfg.metronome_during_take;
        if ui
            .checkbox(&mut click, "click (K)")
            .on_hover_text("Switch the metronome on or off at any time, also while playing or recording. The count-in always sounds.")
            .changed()
        {
            self.press(Action::Click);
        }
        ui.add_enabled_ui(!running, |ui| {
            let mut notes = self.playback.notes;
            if ui
                .checkbox(&mut notes, "notes")
                .on_hover_text("Play the notes of the score in Listen. Switch them off to use Listen as a click track.")
                .changed()
            {
                self.press(Action::Notes);
            }
            let mut looping = self.playback.looping;
            if ui
                .checkbox(&mut looping, "loop")
                .on_hover_text("Repeat the selected measures in Listen until it is stopped. Switching it off lets the current round finish.")
                .changed()
            {
                self.press(Action::Loop);
            }
            widgets::slider(ui, "count-in bars", &mut self.cfg.count_in_bars, limits::COUNT_IN_BARS.range(), "", 1.0);
            ui.checkbox(&mut self.cfg.listen_count_in, "in Listen")
                .on_hover_text("Start Listen with the count-in too. A recording always has one; switch this off to hear the music at once.");
            widgets::slider_with_zero(ui, "ramp +", &mut self.cfg.ramp_percent, limits::RAMP_PERCENT.range(), " %", 1.0, Some("off"))
                .on_hover_text("With loop on, Listen gets faster by this many per cent of the written tempo every few passes (up to three times the starting tempo). Takes effect at the next start.");
            if self.cfg.ramp_percent > 0 {
                widgets::slider(ui, "every", &mut self.cfg.ramp_passes, limits::RAMP_PASSES.range(), " passes", 1.0);
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
            if let Some(take) = self.practice.take.as_ref() {
                let r = &take.report;
                if take.running {
                    let el = signed_secs(self.shown_now(), take.zero);
                    if el < Seconds::ZERO {
                        let beats_left = (-el / take.count_in_beat_sec).ceil() as i64;
                        ui.label(RichText::new(format!("Count-in… {beats_left}")).size(22.0).strong().color(theme::ACCENT));
                    } else {
                        ui.label(RichText::new("⏺ REC").strong().color(theme::RED));
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
            legend(ui, theme::GREEN, "perfect");
            legend(ui, theme::LIGHT_GREEN, "good");
            legend(ui, theme::AMBER, "ok");
            legend(ui, theme::ORANGE, "wrong pad");
            legend(ui, theme::RED, "missed / extra");
            widgets::hint(ui, "· click a measure to start there, shift+click to set the end · mouse wheel over a slider fine-tunes it");
            if let Some(fit) = self.view.fit_bars {
                let msg = format!("· {} bars per row do not fit at this width: showing {fit}", self.cfg.bars_per_row);
                ui.label(RichText::new(msg).small().color(theme::AMBER));
            }
        });
        // On a line of its own: it grows while the trigger waits for the second hit.
        if let Some((text, armed)) = self.trigger_hint() {
            let (color, text) =
                if armed { (theme::GREEN, format!("{text}: waiting for the second hit")) } else { (ui.visuals().weak_text_color(), text) };
            ui.label(RichText::new(text.trim_start_matches("· ")).small().color(color));
        }
        ui.separator();

        let played = self.played_notes();
        let grades = self.note_grades();
        let listen_q = self.listen_playhead();
        let score = self.practice.score.as_ref().unwrap();
        let playhead = match (self.practice.take.as_ref().filter(|t| t.running), listen_q) {
            (Some(t), _) => Some(score_q(score, t, signed_secs(self.shown_now(), t.zero))),
            (None, q) => q,
        };
        let view = ScoreView {
            score,
            grades: &grades,
            played: &played,
            playhead_q: playhead,
            range: (self.practice.range_from, self.practice.range_to),
            zoom: self.cfg.zoom,
            follow: self.follow && (running || self.playback.listen.is_some()),
            layout: LayoutOpts {
                bars_per_row: self.cfg.bars_per_row,
                align_bar: self.cfg.layout_align_bar,
                break_at_pause: self.cfg.break_at_pause,
            },
        };
        let out = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| render::show(ui, &view)).inner;
        self.view.fit_bars = out.fit_bars;
        if let (Some((mi, shift)), false) = (out.clicked_measure, running) {
            if shift {
                self.practice.range_to = mi.max(self.practice.range_from);
            } else {
                self.practice.range_from = mi;
                if self.practice.range_to < mi {
                    self.practice.range_to = n_measures - 1;
                }
            }
        }
    }
}
