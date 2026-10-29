//! The instrument mixer on the Sound page of Settings: a volume, mute and
//! solo for every instrument of Listen, a button that plays the instrument
//! on its own, and whether it uses a WAV file of the user's or the built-in sound.

use egui::RichText;

use super::DrumApp;
use crate::config::MAX_LANE_GAIN;
use crate::drums::Lane;
use crate::widgets;

impl DrumApp {
    pub(super) fn instrument_mixer(&mut self, ui: &mut egui::Ui) {
        ui.heading("Instruments");
        ui.label(
            RichText::new("Volume, mute and solo of each instrument in Listen; they change the sound while it plays. The play button sounds one instrument on its own, at its volume.")
                .small()
                .weak(),
        );
        let dir = self.paths.sounds_dir();
        let mut audition: Option<Lane> = None;
        egui::Grid::new("mixer").num_columns(6).spacing([14.0, 6.0]).striped(true).show(ui, |ui| {
            for heading in ["Instrument", "Volume", "Mute", "Solo", "", "Sound"] {
                ui.label(RichText::new(heading).strong());
            }
            ui.end_row();
            for lane in Lane::ALL {
                if lane == Lane::Other {
                    continue;
                }
                ui.label(lane.label());

                let mut gain = self.cfg.gain_of(lane);
                if widgets::slider(ui, "", &mut gain, 0.0..=MAX_LANE_GAIN, "", 0.05).changed() {
                    // The default is not stored, so it can be changed later without leaving traces.
                    if (gain - 1.0).abs() < 0.01 {
                        self.cfg.lane_gain.remove(&lane);
                    } else {
                        self.cfg.lane_gain.insert(lane, gain);
                    }
                }

                let mut muted = self.cfg.lane_mute.contains(&lane);
                if ui.checkbox(&mut muted, "").changed() {
                    if muted {
                        self.cfg.lane_mute.insert(lane);
                    } else {
                        self.cfg.lane_mute.remove(&lane);
                    }
                }

                let mut soloed = self.solo.contains(&lane);
                if ui.checkbox(&mut soloed, "").on_hover_text("Hear only the soloed instruments (not saved)").changed() {
                    if soloed {
                        self.solo.insert(lane);
                    } else {
                        self.solo.remove(&lane);
                    }
                }

                if ui.button("⏵").on_hover_text("Play this instrument").clicked() {
                    audition = Some(lane);
                }

                let file = format!("{}.wav", lane.name());
                if dir.as_ref().is_some_and(|d| d.join(&file).is_file()) {
                    ui.label("your WAV file").on_hover_text(format!("{file} in the sounds folder replaces the built-in sound"));
                } else {
                    ui.label(RichText::new("built-in").weak()).on_hover_text(format!("Put {file} in the sounds folder to use your own sound"));
                }
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            let changed = !self.cfg.lane_gain.is_empty() || !self.cfg.lane_mute.is_empty() || !self.solo.is_empty();
            if ui.add_enabled(changed, egui::Button::new("Reset volumes, mutes and solos")).clicked() {
                self.cfg.lane_gain.clear();
                self.cfg.lane_mute.clear();
                self.solo.clear();
            }
        });
        if let Some(lane) = audition {
            self.audition(lane);
        }
        if let Some(dir) = dir {
            ui.label(
                RichText::new(format!(
                    "To use your own sound for an instrument, put a WAV file named after it in {} (for example Kick.wav, HiHatClosed.wav, Crash1.wav). Files are read each time playback starts; an instrument without a file keeps its built-in sound.",
                    dir.display()
                ))
                .small()
                .weak(),
            );
        }
    }
}
