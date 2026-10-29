//! The Home tab: the welcome panel with the recent scores.

use std::path::{Path, PathBuf};

use egui::RichText;

use crate::render;
use crate::widgets;

use super::{DrumApp, Tab};

/// `s` cut to its last `max` characters, with "…" in front when it was longer.
pub(super) fn shorten_start(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        format!("…{}", s.chars().skip(n - max + 1).collect::<String>())
    }
}

/// One entry of the recent-scores list: the file name over its folder, a
/// remove button on the right. Returns (row clicked, remove clicked).
pub(super) fn recent_row(ui: &mut egui::Ui, path: &str, width: f32) -> (bool, bool) {
    let file = Path::new(path);
    let name = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string());
    let folder = shorten_start(&file.parent().map(|d| d.display().to_string()).unwrap_or_default(), 78);
    let exists = file.is_file();

    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 52.0), egui::Sense::click());
    // Clip to the row (a little wider, so the border is not cut in half).
    let painter = ui.painter_at(rect.expand(2.0));
    let visuals = ui.style().interact(&response);
    let (fill, stroke) = if response.has_focus() {
        // Reached with the keyboard: a tinted row with the selection outline, text stays readable.
        (ui.visuals().selection.bg_fill.gamma_multiply(0.3), ui.visuals().selection.stroke)
    } else if response.hovered() {
        (visuals.bg_fill, visuals.bg_stroke)
    } else {
        (ui.visuals().faint_bg_color, ui.visuals().widgets.noninteractive.bg_stroke)
    };
    painter.rect(rect, 6.0, fill, stroke);
    let (title_color, sub_color) = if exists {
        (ui.visuals().strong_text_color(), ui.visuals().weak_text_color())
    } else {
        (ui.visuals().weak_text_color(), render::RED)
    };
    painter.text(rect.left_top() + egui::vec2(14.0, 8.0), egui::Align2::LEFT_TOP, &name, egui::FontId::proportional(17.0), title_color);
    let sub = if exists { folder } else { format!("{folder}  — file not found") };
    painter.text(rect.left_top() + egui::vec2(14.0, 31.0), egui::Align2::LEFT_TOP, sub, egui::FontId::proportional(12.0), sub_color);

    // Drawn after the row, so it gets the click when the pointer is over it.
    let button = egui::Rect::from_center_size(egui::pos2(rect.right() - 26.0, rect.center().y), egui::vec2(30.0, 30.0));
    let remove = ui.put(button, egui::Button::new("×")).on_hover_text("Remove from the list").clicked();
    let clicked = response.on_hover_text(path).clicked();
    (clicked, remove)
}

impl DrumApp {
    /// Shown instead of the score until one is loaded: how to start, and the
    /// scores opened before.
    pub(super) fn welcome_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(36.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Roll Model").size(34.0).strong());
            ui.add_space(4.0);
            ui.label("Load a MusicXML drum score, listen to it, then play it on your kit and see how you did.");
            ui.add_space(18.0);
            let open = egui::Button::new(RichText::new("📂 Open MusicXML…").size(18.0)).min_size(egui::vec2(260.0, 46.0));
            if ui.add(open).clicked() {
                self.open_dialog();
            }
            ui.add_space(6.0);
            widgets::hint(ui, "or drop a file onto the window — .musicxml, .xml or .mxl, exported from MuseScore, Guitar Pro, Sibelius, Dorico…");
            ui.add_space(14.0);
            let rudiments = egui::Button::new(RichText::new("Practise the 40 rudiments").size(16.0)).min_size(egui::vec2(260.0, 38.0));
            if ui.add(rudiments).on_hover_text("Singles, rolls, paradiddles, flams and drags: pick one, repeat it and play it along with the click").clicked() {
                self.view.tab = Tab::Rudiments;
            }
        });
        ui.add_space(28.0);

        let width = ui.available_width().min(640.0);
        let mut open: Option<PathBuf> = None;
        let mut forget: Option<usize> = None;
        let mut clear = false;
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - width) / 2.0);
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.heading("Recent scores");
                    if !self.cfg.recent_files.is_empty() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            clear = ui.small_button("Clear list").clicked();
                        });
                    }
                });
                if self.cfg.recent_files.is_empty() {
                    ui.label(RichText::new("Scores you open are listed here so you can come back to them.").weak());
                }
                for (i, path) in self.cfg.recent_files.iter().enumerate() {
                    let (clicked, remove) = recent_row(ui, path, width);
                    ui.add_space(6.0);
                    if remove {
                        forget = Some(i);
                    } else if clicked {
                        open = Some(PathBuf::from(path));
                    }
                }
            });
        });
        if clear {
            self.cfg.recent_files.clear();
        } else if let Some(i) = forget {
            self.cfg.recent_files.remove(i);
        }
        if let Some(path) = open {
            self.open_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_folders_keep_their_end() {
        assert_eq!(shorten_start("D:\\scores", 20), "D:\\scores");
        assert_eq!(shorten_start("C:\\Users\\someone\\Documents\\Drum scores\\Rock", 16), "…rum scores\\Rock");
        assert_eq!(shorten_start("żółć", 4), "żółć", "counts characters, not bytes");
        assert_eq!(shorten_start("żółćżółć", 4), "…ółć");
    }
}
