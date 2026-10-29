//! The Controller page of Settings: the preset and port choice, the status, the monitor and the buttons.

use std::path::Path;
use egui::RichText;
use crate::controller::{self};
use crate::render;
use crate::widgets;
use super::super::DrumApp;

/// Writes `text` as "NAME (copy).txt" (or "(copy 2)", …) in `dir`; never overwrites.
pub(super) fn copy_preset(dir: &Path, name: &str, text: &str) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut path = dir.join(format!("{name} (copy).txt"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{name} (copy {n}).txt"));
        n += 1;
    }
    std::fs::write(&path, text)?;
    Ok(path)
}

impl DrumApp {
    pub(in crate::app) fn settings_controller(&mut self, ui: &mut egui::Ui) {
        ui.heading("Controller");
        ui.label(
            RichText::new("Control surfaces that run the program from the stool. A preset is either Mackie Control (the Behringer X-Touch Mini and Compact, in MC mode) or Generic MIDI (knobs, faders and buttons that send plain notes and cc, such as the Novation Launch Control XL, Korg nanoKONTROL2, Akai MPK Mini and Arturia BeatStep). Choose a preset, or use Manage to copy, rename or delete presets, start a new one and teach it by pressing the controls. Click the controller indicator at the top right of the window for a picture of it. Close other programs that use the controller's MIDI port.")
                .small()
                .weak(),
        );

        let mut preset: Option<String> = None;
        ui.horizontal(|ui| {
            ui.label("Preset");
            let current = if self.cfg.controller_preset.is_empty() { "— none —".to_string() } else { self.cfg.controller_preset.clone() };
            egui::ComboBox::from_id_salt("controller_preset").selected_text(current).width(260.0).show_ui(ui, |ui| {
                if ui.selectable_label(self.cfg.controller_preset.is_empty(), "— none —").clicked() {
                    preset = Some(String::new());
                }
                for kind in [controller::PresetKind::Mackie, controller::PresetKind::Midi] {
                    let of_kind: Vec<&String> = self.controller.presets.iter().filter(|n| self.controller.kinds.get(*n).copied().unwrap_or_default() == kind).collect();
                    if of_kind.is_empty() {
                        continue;
                    }
                    widgets::hint(ui, kind.label());
                    for name in of_kind {
                        let builtin = controller::builtin_text(name).is_some();
                        let label = if builtin { format!("{name} (built in)") } else { name.clone() };
                        if ui.selectable_label(&self.cfg.controller_preset == name, label).clicked() {
                            preset = Some(name.clone());
                        }
                    }
                }
            });
            if ui.button("Manage").on_hover_text("Create, copy, rename and delete presets, and change what their controls do").clicked() {
                self.controller.editor.view.open = true;
            }
        });
        if let Some(name) = preset {
            self.select_preset(name);
        }

        let mut port: Option<String> = None;
        ui.horizontal(|ui| {
            ui.label("Port");
            let current = match (&self.cfg.controller_port, self.controller.link.as_ref()) {
                (p, _) if !p.is_empty() => p.clone(),
                (_, Some(l)) => format!("automatic ({})", l.handle.port_name),
                _ => "automatic".to_string(),
            };
            egui::ComboBox::from_id_salt("controller_port").selected_text(current).width(260.0).show_ui(ui, |ui| {
                if ui.selectable_label(self.cfg.controller_port.is_empty(), "automatic (the preset's port)").clicked() {
                    port = Some(String::new());
                }
                for p in &self.midi.inputs {
                    if ui.selectable_label(&self.cfg.controller_port == p, p.as_str()).clicked() {
                        port = Some(p.clone());
                    }
                }
            });
            if ui.button("⟳ Refresh and reconnect").clicked() {
                self.refresh_ports();
                self.refresh_controllers();
                self.connect_controller();
            }
        });
        if let Some(p) = port {
            self.cfg.controller_port = p;
            self.connect_controller();
        }

        let (colour, text) = match self.controller.link.is_some() {
            true => (render::GREEN, self.controller.message.clone()),
            false => (render::AMBER, self.controller.message.clone()),
        };
        if !text.is_empty() {
            ui.label(RichText::new(text).color(colour));
        }
        if let Some(kind) = self.controller.kinds.get(&self.cfg.controller_preset) {
            widgets::hint(ui, format!("Type: {}", kind.label()));
        }

        if let Some(link) = self.controller.link.as_ref() {
            let preset = link.mapper.preset();
            for w in &preset.warnings {
                ui.label(RichText::new(format!("⚠ {w}")).small().color(render::AMBER));
            }
            ui.add_space(6.0);
            let layer = link.mapper.layer_name().map(|l| format!("layer {l}")).unwrap_or_else(|| "no layers".into());
            ui.label(format!("{} · {layer}{}", preset.name, if link.mapper.shift() { " · shift held" } else { "" }));
            ui.collapsing("Controls on this layer", |ui| {
                let layer = link.mapper.layer_name();
                egui::Grid::new("controller_binds").num_columns(2).striped(true).show(ui, |ui| {
                    for b in preset.binds.iter().filter(|b| b.layer.is_none() || b.layer.as_deref() == layer) {
                        let kind = if b.reading.is_relative() { " (turn)" } else { "" };
                        ui.label(format!("{}{kind}", b.input.src.text()));
                        ui.label(b.action.text());
                        ui.end_row();
                    }
                });
            });
        }

        ui.add_space(6.0);
        ui.label(RichText::new("Monitor (latest first): press and turn the controls to see what they send and what they are bound to").strong());
        for line in &self.controller.monitor {
            ui.label(RichText::new(line).monospace());
        }
    }
}
