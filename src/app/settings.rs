//! The Settings tab: its page list and the MIDI input, timing, sound and general pages (the controller page is in
//! `controls.rs`).

use egui::RichText;

use crate::drums::{format_note_list, parse_note_list, Lane};
use crate::metronome::HAS_AUDIO;
use crate::midi;
use crate::render;
use crate::widgets;

use super::{DrumApp, KEYBOARD_PADS};

/// The pages of the Settings tab.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsPage {
    MidiInput,
    Controller,
    Timing,
    Sound,
    General,
}

impl SettingsPage {
    pub(super) const ALL: [SettingsPage; 5] =
        [SettingsPage::MidiInput, SettingsPage::Controller, SettingsPage::Timing, SettingsPage::Sound, SettingsPage::General];

    pub(super) fn label(self) -> &'static str {
        match self {
            SettingsPage::MidiInput => "MIDI input",
            SettingsPage::Controller => "Controller",
            SettingsPage::Timing => "Timing",
            SettingsPage::Sound => "Sound",
            SettingsPage::General => "General",
        }
    }
}

impl DrumApp {
    /// Settings are split into pages so each one fits on a screen.
    pub(super) fn settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.scope(|ui| {
            widgets::roomy(ui);
            ui.horizontal(|ui| {
                for page in SettingsPage::ALL {
                    ui.selectable_value(&mut self.view.settings_page, page, page.label());
                }
            });
        });
        ui.separator();
        // One scroll position per page, so switching pages starts at the top.
        egui::ScrollArea::vertical()
            .id_salt(("settings", self.view.settings_page as u8))
            .auto_shrink([false, false])
            .show(ui, |ui| match self.view.settings_page {
                SettingsPage::MidiInput => self.settings_midi_input(ui),
                SettingsPage::Controller => self.settings_controller(ui),
                SettingsPage::Timing => self.settings_timing(ui),
                SettingsPage::Sound => self.settings_sound(ui),
                SettingsPage::General => self.settings_general(ui),
            });
    }

    pub(super) fn settings_midi_input(&mut self, ui: &mut egui::Ui) {
        ui.heading("MIDI input");
        ui.horizontal(|ui| {
            ui.label("Input port");
            let current = self.midi.input.as_ref().map(|h| h.port_name.clone()).unwrap_or_else(|| "— none —".into());
            let mut choice: Option<String> = None;
            egui::ComboBox::from_id_salt("midi_in_port")
                .selected_text(current)
                .width(260.0)
                .show_ui(ui, |ui| {
                    for p in &self.midi.inputs {
                        if ui.selectable_label(false, p.as_str()).clicked() {
                            choice = Some(p.clone());
                        }
                    }
                });
            if let Some(c) = choice {
                self.connect_midi(Some(c));
            }
            if ui.button("⟳ Refresh ports").clicked() {
                self.refresh_ports();
            }
            if self.midi.input.is_some() && ui.button("Disconnect").clicked() {
                self.midi.input = None;
            }
        });
        widgets::hint(ui, "Connect the V31 by USB (or a MIDI interface on MIDI OUT). Close other apps that use the port — on Windows a MIDI port can only be opened by one program.");

        ui.add_space(6.0);
        ui.label(RichText::new("Monitor (latest first)").strong());
        egui::Grid::new("monitor").num_columns(5).striped(true).show(ui, |ui| {
            for (ev, lane) in &self.midi.monitor {
                ui.label(format!("ch {}", ev.channel));
                ui.label(format!("note {} ({})", ev.note, midi::note_name(ev.note)));
                ui.label(format!("vel {}", ev.velocity));
                ui.label(ev.hihat.map(|p| format!("pedal {p}")).unwrap_or_default());
                ui.label(lane.map(|l| l.label()).unwrap_or("unmapped"));
                ui.end_row();
            }
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Hi-hat pedal (CC#4) value when closed");
            widgets::slider(ui, "", &mut self.cfg.hihat_cc_max, 1..=127, "", 1.0);
        });
        widgets::hint(ui, "The hi-hat counts as closed when the pedal is at least three quarters of the way to this value, otherwise open, so hits on the closed or open hi-hat note follow the pedal. Lower it if your pedal never reports up to 127: close the pedal, hit the hi-hat and read the pedal value in the monitor. Hits before any pedal message keep the note's own open/closed meaning.");

        ui.add_space(10.0);
        self.settings_trigger(ui);

        ui.add_space(10.0);
        ui.heading("Pad-to-instrument mapping");
        widgets::hint(ui, "Defaults follow Roland V-Drums factory note numbers. Click Learn and hit the pad/zone to assign it.");
        egui::Grid::new("mapping").num_columns(3).striped(true).show(ui, |ui| {
            for lane in Lane::ALL {
                if lane == Lane::Other {
                    continue;
                }
                ui.label(lane.label());
                let text = self
                    .midi
                    .lane_text
                    .entry(lane)
                    .or_insert_with(|| format_note_list(&self.cfg.note_map.notes_for(lane)));
                if ui.add(egui::TextEdit::singleline(text).desired_width(140.0)).changed() {
                    let notes = parse_note_list(text);
                    self.cfg.note_map.set_lane_notes(lane, &notes);
                }
                let learning = self.midi.learn == Some(lane);
                if ui.selectable_label(learning, if learning { "hit a pad…" } else { "Learn" }).clicked() {
                    self.midi.learn = if learning { None } else { Some(lane) };
                }
                ui.end_row();
            }
        });
        if ui.button("Reset to Roland V31 defaults").clicked() {
            self.cfg.note_map = crate::drums::NoteMap::roland_v31();
            self.midi.lane_text.clear();
        }
    }

    pub(super) fn settings_timing(&mut self, ui: &mut egui::Ui) {
        ui.heading("Timing");
        egui::Grid::new("timing").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label("Latency compensation");
            ui.horizontal(|ui| {
                let before = self.cfg.latency_ms;
                widgets::slider(ui, "", &mut self.cfg.latency_ms, -50.0..=200.0, " ms", 1.0);
                if self.cfg.latency_ms != before {
                    self.cfg.latency_engine = crate::audio::ENGINE_GENERATION; // set by hand with this engine
                }
                let calibrating = self.calibration.is_some();
                if ui.add_enabled(!calibrating, egui::Button::new("Calibrate…")).clicked() {
                    self.start_calibration();
                }
            });
            ui.end_row();
            ui.label("Picture and MIDI click");
            ui.checkbox(&mut self.cfg.sync_display, "Follow the sound: draw it, and send the MIDI click, as late as the sound is heard (the latency)");
            ui.end_row();
            ui.label("Perfect within");
            widgets::slider(ui, "", &mut self.cfg.tol.perfect_ms, 5.0..=50.0, " ms", 1.0);
            ui.end_row();
            ui.label("Good within");
            widgets::slider(ui, "", &mut self.cfg.tol.good_ms, 10.0..=100.0, " ms", 1.0);
            ui.end_row();
            ui.label("Hit window (beyond = extra/missed)");
            widgets::slider(ui, "", &mut self.cfg.tol.window_ms, 40.0..=250.0, " ms", 5.0);
            ui.end_row();
        });
        if self.cfg.latency_is_stale(crate::audio::ENGINE_GENERATION) {
            ui.horizontal(|ui| {
                ui.label(RichText::new("This latency was set with an earlier audio engine; the click now comes at another time. Calibrate again.").small().color(render::AMBER));
                if ui.small_button("Keep this value").clicked() {
                    self.cfg.latency_engine = crate::audio::ENGINE_GENERATION;
                }
            });
        }
        widgets::hint(ui, "Calibrate: play any pad exactly with 16 metronome clicks; the median offset becomes the latency compensation. Hover a slider and turn the mouse wheel to fine-tune it.");
        self.cfg.tol.good_ms = self.cfg.tol.good_ms.max(self.cfg.tol.perfect_ms);
        self.cfg.tol.window_ms = self.cfg.tol.window_ms.max(self.cfg.tol.good_ms);
    }

    /// What is known about the open sound output: which, how fast, and whether it is keeping up.
    fn audio_readout(&self, ui: &mut egui::Ui) {
        if !HAS_AUDIO {
            return;
        }
        let Some(info) = self.audio.info() else {
            widgets::hint(ui, "The output opens when the first sound starts.");
            return;
        };
        let calibrated = (self.cfg.latency_ms > 0.0 && !self.cfg.latency_is_stale(crate::audio::ENGINE_GENERATION)).then_some(self.cfg.latency_ms);
        let delay = delay_text(info.latency_ms, super::audio_output::SYSTEM_BUFFER_MS, calibrated);
        widgets::hint(ui, &format!("Open: {} at {} Hz. {delay}", info.device, info.rate));
        if info.late + info.dropped > 0 {
            ui.label(RichText::new(format!("Sounds that came too late: {} (the worst by {:.1} ms), dropped: {}. The computer may be too busy for the output.", info.late, info.worst_late_ms, info.dropped)).small().color(render::AMBER));
        }
        widgets::hint(ui, "Bluetooth headphones add 100-200 ms or more and cannot be used for timing: use a wired output, or send the click to the V31 (below).");
    }

    pub(super) fn settings_sound(&mut self, ui: &mut egui::Ui) {
        ui.heading("Audio output");
        ui.add_enabled_ui(HAS_AUDIO, |ui| {
            ui.horizontal(|ui| {
                ui.label("Output device");
                let default_label = match &self.audio_default {
                    Some(name) => format!("System default ({name})"),
                    None => "System default".to_string(),
                };
                let current = if self.cfg.audio_output.is_empty() { default_label.clone() } else { self.cfg.audio_output.clone() };
                egui::ComboBox::from_id_salt("audio_out").selected_text(current).width(360.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.cfg.audio_output, String::new(), default_label);
                    for d in &self.audio_devices {
                        ui.selectable_value(&mut self.cfg.audio_output, d.clone(), d.as_str());
                    }
                });
                if ui.button("⟳ Refresh").clicked() {
                    self.refresh_ports();
                }
            });
            if !self.cfg.audio_output.is_empty() && !self.audio_devices.contains(&self.cfg.audio_output) {
                ui.label(RichText::new("This device is not connected right now: the system default is used instead.").small().color(render::AMBER));
            }
        });
        ui.label(RichText::new(if HAS_AUDIO {
            "The sound card used for the metronome click, Listen and the calibration clicks. A change applies the next time sound starts."
        } else {
            "This build has no sound output; use MIDI output below."
        }).small().weak());

        self.audio_readout(ui);

        ui.add_space(10.0);
        ui.heading("MIDI output");
        ui.horizontal(|ui| {
            ui.label("Output port");
            egui::ComboBox::from_id_salt("midi_out_port")
                .selected_text(self.cfg.midi_out_port.clone())
                .width(260.0)
                .show_ui(ui, |ui| {
                    for p in &self.midi.outputs {
                        ui.selectable_value(&mut self.cfg.midi_out_port, p.clone(), p.as_str());
                    }
                });
            if ui.button("⟳ Refresh ports").clicked() {
                self.refresh_ports();
            }
        });
        widgets::hint(ui, "Where the metronome click and the notes of Listen go when they are sent over MIDI: the V31, or for example Windows' built-in \"Microsoft GS Wavetable Synth\".");

        ui.add_space(10.0);
        ui.heading("Metronome");
        ui.add_enabled_ui(HAS_AUDIO, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.cfg.click_audio, "Click through the computer's sound output");
                widgets::slider(ui, "volume", &mut self.cfg.click_volume, 0.0..=1.0, "", 0.02);
            });
        });
        ui.checkbox(&mut self.cfg.click_midi, "Send the click to the MIDI output");
        ui.horizontal(|ui| {
            widgets::slider(ui, "click note", &mut self.cfg.click_note, 0..=127, "", 1.0);
            widgets::slider(ui, "channel", &mut self.cfg.click_channel, 1..=16, "", 1.0);
        });
        widgets::hint(ui, "Sending the click to the V31 (channel 10, an unmapped note such as 76 = wood block) puts it in your headphones with no extra audio latency.");

        ui.add_space(10.0);
        ui.heading("Listening");
        widgets::hint(ui, "The Listen button on the Score tab plays the selected measures at the chosen tempo, with the count-in and click of a take.");
        ui.add_enabled_ui(HAS_AUDIO, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.cfg.playback_audio, "Play through the computer's sound output (built-in drum sounds)");
                widgets::slider(ui, "volume", &mut self.cfg.playback_volume, 0.0..=1.0, "", 0.02);
            });
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.cfg.playback_midi, "Send the notes to the MIDI output (the module's own sounds)");
            widgets::slider(ui, "channel", &mut self.cfg.playback_channel, 1..=16, "", 1.0);
        });
        widgets::hint(ui, "Each instrument is sent as its standard General MIDI / Roland note, or the note mapped to it on the MIDI input page, so the V31 plays its own kit.");

        ui.add_space(10.0);
        self.instrument_mixer(ui);
    }

    pub(super) fn settings_general(&mut self, ui: &mut egui::Ui) {
        ui.heading("Score");
        if ui.checkbox(&mut self.cfg.expand_repeats, "Expand repeats and 1st/2nd endings").changed() {
            if let Some(p) = self.main_path() {
                self.open_file(&p);
            }
        }

        ui.add_space(10.0);
        ui.heading("Recent scores");
        ui.horizontal(|ui| {
            ui.label(format!("{} remembered", self.cfg.recent_files.len()));
            if ui.add_enabled(!self.cfg.recent_files.is_empty(), egui::Button::new("Clear list")).clicked() {
                self.cfg.recent_files.clear();
            }
        });

        ui.add_space(10.0);
        ui.heading("Testing without a kit");
        ui.checkbox(&mut self.cfg.keyboard_input, "Computer keyboard plays pads");
        ui.label(
            RichText::new(KEYBOARD_PADS.iter().map(|(_, l, k)| format!("{k} = {}", l.label())).collect::<Vec<_>>().join(" · "))
                .small()
                .weak(),
        );

        ui.add_space(10.0);
        if let Some(p) = self.paths.config_file() {
            widgets::hint(ui, format!("Settings are saved automatically to {}", p.display()));
        }
    }
}

/// What the Sound page says about the delay. The driver's figure is the free space of one block; the
/// system's own buffer (`system_ms`, 22 ms on Windows) comes on top, so the sum is still a lower bound (45 ms
/// against 10 + 22 was measured on one machine, with the rest not accounted for).
fn delay_text(reported_ms: Option<f64>, system_ms: f64, calibrated_ms: Option<f64>) -> String {
    let mut s = match (reported_ms, system_ms > 0.0) {
        (Some(r), true) => format!("The driver reports about {r:.0} ms. The system adds a buffer of about {system_ms:.0} ms on top, so the delay is at least about {:.0} ms (one machine measured 45 ms end to end).", r + system_ms),
        (None, true) => format!("The driver reports no delay. The system adds a buffer of about {system_ms:.0} ms, so the delay is at least that."),
        (Some(r), false) => format!("The driver reports about {r:.0} ms, but that is only the least: the system's buffer comes on top, so the real delay is longer (one machine measured 45 ms where the driver said 10)."),
        (None, false) => "The driver reports no delay; the real one is not zero.".to_string(),
    };
    match calibrated_ms {
        Some(ms) => s.push_str(&format!(" Your calibrated latency is {ms:.0} ms (the whole path, and your own habit).")),
        None => s.push_str(" Settings → Timing → Calibrate measures it."),
    }
    s
}

#[cfg(test)]
mod delay_text_tests {
    use super::delay_text;

    #[test]
    fn the_reported_delay_is_called_a_lower_bound_and_calibration_is_offered_or_shown() {
        let t = delay_text(Some(10.0), 0.0, None);
        assert!(t.contains("about 10 ms") && t.contains("only the least") && t.contains("Calibrate"), "{t}");
        let t = delay_text(Some(10.0), 0.0, Some(45.0));
        assert!(t.contains("calibrated latency is 45 ms") && !t.contains("Calibrate measures"), "{t}");
        assert!(delay_text(None, 0.0, None).contains("not zero"));
    }

    #[test]
    fn the_system_buffer_is_added_to_what_the_driver_reports() {
        let t = delay_text(Some(10.0), 22.0, None);
        assert!(t.contains("about 10 ms") && t.contains("buffer of about 22 ms") && t.contains("at least about 32 ms"), "{t}");
        assert!(delay_text(None, 22.0, None).contains("at least that"));
    }
}
