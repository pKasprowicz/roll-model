//! The Results tab: the report of a take, the histogram and the CSV export.

use egui::{Color32, RichText};

use crate::render;

use super::DrumApp;

pub(super) fn grade_letter_color(overall: f64) -> Color32 {
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

pub(super) fn histogram(ui: &mut egui::Ui, dts: &[f64], window_ms: f64) {
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

impl DrumApp {
    pub(super) fn results_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let Some(take) = self.take.as_ref() else {
                ui.label("Record a take on the Score or Rudiments tab to see the analysis here.");
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
                        "Take {}{} · {}",
                        take.number,
                        if take.running { " (in progress)" } else { "" },
                        take.label
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

            if !r.hands.is_empty() {
                ui.add_space(8.0);
                ui.heading("Hands");
                egui::Grid::new("hands").num_columns(4).striped(true).show(ui, |ui| {
                    for h in ["Hand", "Strokes hit", "Offset", "Velocity (unaccented)"] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (hand, hs) in &r.hands {
                        ui.label(hand.label());
                        ui.label(hs.hit.to_string());
                        ui.label(if hs.hit > 0 { format!("{:+.0} ms", hs.mean_dt_ms) } else { "–".into() });
                        ui.label(if hs.plain > 0 { format!("{:.0}", hs.mean_velocity) } else { "–".into() });
                        ui.end_row();
                    }
                });
            }

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

    pub(super) fn export_csv(&mut self) {
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
            Ok(()) => self.view.status = format!("Saved {}", path.display()),
            Err(e) => self.view.error = Some(format!("Could not save: {e}")),
        }
    }
}
