//! The Manage window of the Controller page: creating, copying, renaming and deleting presets,
//! and below them a table of the program's actions with the control
//! each is bound to, and *Learn* (press or turn a control on the device to bind it). The preset
//! being edited is a file in the controllers folder; every change is written at once and put in
//! place in the running controller. The built-in presets are not edited: copy one first, or start
//! a new preset. The editing itself (`assign`, `clear`, `to_text`) is in `controller.rs`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{RichText, ViewportBuilder, ViewportClass, ViewportId};

use super::page::copy_preset;
use super::*;
use crate::controller::{Binding, Encoding, Light, Preset, Reading};
use crate::widgets;

/// What the Manage window remembers between frames: widget state, not behaviour.
#[derive(Default)]
pub(super) struct ManageView {
    /// The layer being edited (an index of the preset's layers).
    pub(super) layer: usize,
    /// The new name typed for the chosen preset, and the preset it was typed for.
    pub(super) rename_to: String,
    pub(super) rename_for: String,
    /// "Delete" was clicked and waits for a second click.
    pub(super) confirm_delete: bool,
    /// The Manage window is open.
    pub(super) open: bool,
    /// A new layer is being set up: its name as typed so far.
    pub(super) adding_layer: bool,
    pub(super) new_layer: String,
    /// "Delete layer" was clicked and waits for a second click.
    pub(super) confirm_layer_delete: bool,
}

/// How many clicks to each side tell an encoder's encoding.
const DETECT_CLICKS: usize = 4;

/// The values an encoder sent while its encoding is being found out.
pub(super) struct Detect {
    pub(super) action: Action,
    /// The cc and channel of the first message; others are ignored.
    cc: Option<(u8, u8)>,
    clockwise: Vec<u8>,
    counter: Vec<u8>,
}

impl Detect {
    /// Turn to the right first, then to the left.
    fn prompt(&self) -> String {
        if self.clockwise.len() < DETECT_CLICKS {
            format!("Turn the encoder for \"{}\" slowly to the RIGHT, {} clicks ({} so far)…", self.action.title(), DETECT_CLICKS, self.clockwise.len())
        } else {
            format!("Now slowly to the LEFT, {} clicks ({} so far)…", DETECT_CLICKS, self.counter.len())
        }
    }
}

/// What the editor remembers between frames: the learn state machine and its choices.
pub(super) struct EditorState {
    /// The action that waits for the next control the device sends.
    pub(super) learning: Option<Action>,
    /// An encoder is being turned right and then left to find out how it counts.
    pub(super) detecting: Option<Detect>,
    /// The layer that waits for the button that selects it (the name it will have).
    pub(super) learning_layer: Option<String>,
    /// After a control is learned, wait for one for the next action in the list.
    pub(super) keep_going: bool,
    /// Limit a learned control to the MIDI channel it sent on.
    pub(super) remember_channel: bool,
    /// How a value's cc is read, chosen before it is learned (by the action's name).
    pub(super) kinds: BTreeMap<String, Reading>,
    pub(super) notice: String,
    /// What the Manage window remembers about itself.
    pub(super) view: ManageView,
}

impl EditorState {
    /// Says something to the user in the window.
    pub(super) fn say(&mut self, text: impl Into<String>) {
        self.notice = text.into();
    }

    /// Waits for the next control the device sends, for `action`.
    pub(super) fn start_learning(&mut self, action: Action) {
        self.learning = Some(action);
        self.learning_layer = None;
        self.detecting = None;
        self.notice.clear();
    }

    pub(super) fn stop_learning(&mut self) {
        self.learning = None;
        self.learning_layer = None;
        self.detecting = None;
    }

    /// How a value's cc is read for `action` when nothing is bound yet: the choice made in the
    /// table, or (absolute, except for the lists, which are only turned).
    pub(super) fn chosen_kind(&self, action: Action) -> Reading {
        self.kinds.get(&action.text()).copied().unwrap_or(if action.is_browse() { Reading::Relative(Encoding::Mackie) } else { Reading::Absolute })
    }

    pub(super) fn choose_kind(&mut self, action: Action, reading: Reading) {
        self.kinds.insert(action.text(), reading);
    }

    /// What the editor was doing belongs to the preset that was chosen: drop it when another is.
    pub(super) fn start_over(&mut self) {
        self.stop_learning();
        self.view.confirm_delete = false;
        self.view.confirm_layer_delete = false;
        self.view.adding_layer = false;
        self.view.layer = 0;
        self.notice.clear();
    }
}

impl Default for EditorState {
    fn default() -> EditorState {
        EditorState { learning: None, detecting: None, learning_layer: None, keep_going: true, remember_channel: false, kinds: BTreeMap::new(), notice: String::new(), view: ManageView::default() }
    }
}

/// What a click on the page asks for; done after the table is drawn.
enum Op {
    Learn(Action),
    Cancel,
    Clear(Action),
    Kind(Action, Reading),
    NewPreset,
    CopyToEdit,
    Rename,
    AskDelete(bool),
    Delete,
    /// Give the action a light (false) or a ring (true) on the control it is bound to.
    AddLight(Action, bool),
    SetLight(Light),
    ClearLight(Action, bool),
    Detect(Action),
    AddLayer,
    CloseAddLayer,
    LearnLayer,
    AskDeleteLayer(bool),
    DeleteLayer(String),
}

const KINDS: [Reading; 5] = [
    Reading::Absolute,
    Reading::Relative(Encoding::Mackie),
    Reading::Relative(Encoding::Twos),
    Reading::Relative(Encoding::Offset),
    Reading::Relative(Encoding::SignBit2),
];

fn kind_text(reading: Reading) -> String {
    match reading {
        Reading::Relative(encoding) => format!("relative, {}", encoding.word()),
        Reading::Absolute => "absolute".to_string(),
    }
}

fn control_text(b: &Binding) -> String {
    match b.input.channel {
        Some(ch) => format!("{} ch {ch}", b.input.src.text()),
        None => b.input.src.text(),
    }
}

/// The actions in the order the editor lists them.
fn action_order() -> Vec<Action> {
    Action::sections().into_iter().flat_map(|(_, actions)| actions).collect()
}

impl DrumApp {
    fn preset_dir(&self) -> Option<PathBuf> {
        self.paths.controllers_dir()
    }

    /// The file of the chosen preset if it is one the user may edit: not a built-in one.
    pub(super) fn editable_path(&self) -> Option<PathBuf> {
        let dir = self.preset_dir()?;
        match controller::preset_source(&self.cfg.controller_preset, Some(&dir))? {
            controller::PresetSource::File(path) => Some(path),
            controller::PresetSource::BuiltIn(_) => None,
        }
    }

    /// The chosen preset as it is now, for the table: the one in use, or its file.
    fn preset_being_edited(&self) -> Option<Preset> {
        if let Some((preset, _)) = self.controller.live() {
            return Some(preset.clone());
        }
        let dir = self.preset_dir()?;
        controller::load_preset(&self.cfg.controller_preset, Some(&dir)).ok()
    }

    /// Changes the chosen (custom) preset: reads its file, applies `change`, writes the file and
    /// puts the result in place in the running controller.
    fn edit_preset<R>(&mut self, change: impl FnOnce(&mut Preset) -> R) -> Option<R> {
        let path = self.editable_path()?;
        let dir = self.preset_dir()?;
        let mut preset = match controller::load_preset(&self.cfg.controller_preset, Some(&dir)) {
            Ok(p) => p,
            Err(e) => {
                self.view.error = Some(e.to_string());
                return None;
            }
        };
        let result = change(&mut preset);
        if let Err(e) = std::fs::write(&path, preset.to_text()) {
            self.view.error = Some(format!("Could not save {}: {e}", path.display()));
            return None;
        }
        self.controller.replace_preset(preset);
        Some(result)
    }

    /// The layer the table edits: `None` for a preset without layers.
    fn editor_layer(&self, preset: &Preset) -> Option<String> {
        preset.layers.get(self.controller.editor.view.layer).map(|l| l.0.clone())
    }

    /// While an action waits for its control, the next message from the device is taken for it
    /// (and never acts). True if the message was used up. A message that cannot be the control
    /// of that action (a release, a fader for a button) is used up and ignored.
    pub(super) fn learn_message(&mut self, msg: controller::Msg) -> bool {
        if self.controller.editor.detecting.is_some() {
            return self.detect_message(msg);
        }
        if let Some(name) = self.controller.editor.learning_layer.clone() {
            return self.learn_layer_button(&name, msg);
        }
        let Some(action) = self.controller.editor.learning else { return false };
        let Some(preset) = self.preset_being_edited() else {
            self.controller.editor.stop_learning();
            return false;
        };
        let layer = self.editor_layer(&preset);
        let existing = preset.bindings_of(layer.as_deref(), action).into_iter().find(|b| matches!(b.input.src, controller::Src::Cc(_))).map(|b| b.reading);
        let reading = existing.unwrap_or_else(|| self.controller.editor.chosen_kind(action));
        let Some(found) = controller::learn(msg, action, reading, self.controller.editor.remember_channel) else { return true };
        let taken_from = self.edit_preset(|p| p.assign(layer.as_deref(), action, found)).flatten();
        let mut notice = format!("{}: {}", action.title(), control_text(&Binding { layer: None, input: found.input, reading: found.reading, action }));
        if let Some(other) = taken_from {
            notice.push_str(&format!(" (it was {})", other.title()));
        }
        let editor = &mut self.controller.editor;
        editor.notice = notice;
        editor.learning = None;
        if editor.keep_going {
            let order = action_order();
            editor.learning = order.iter().position(|a| *a == action).and_then(|i| order.get(i + 1)).copied();
        }
        true
    }

    /// While an encoder is being turned right and then left: collects its values (the first cc
    /// that arrives is the encoder, other messages are used up and ignored), and when there are
    /// enough in both directions sets the kind of the action to the encoding that fits.
    fn detect_message(&mut self, msg: controller::Msg) -> bool {
        let controller::Msg::Cc { ch, cc, val } = msg else { return true };
        let Some(d) = self.controller.editor.detecting.as_mut() else { return false };
        if *d.cc.get_or_insert((cc, ch)) != (cc, ch) || val == 0 {
            return true;
        }
        if d.clockwise.len() < DETECT_CLICKS {
            d.clockwise.push(val);
        } else {
            d.counter.push(val);
        }
        if d.counter.len() < DETECT_CLICKS {
            return true;
        }
        let Some(d) = self.controller.editor.detecting.take() else { return true };
        match Encoding::guess(&d.clockwise, &d.counter) {
            Some(encoding) => {
                let reading = Reading::Relative(encoding);
                self.controller.editor.choose_kind(d.action, reading);
                if let Some(preset) = self.preset_being_edited() {
                    let layer = self.editor_layer(&preset);
                    self.edit_preset(|p| p.set_reading(layer.as_deref(), d.action, reading));
                }
                self.controller.editor.say(format!("{} counts {}.", d.action.title(), kind_text(reading)));
            }
            None => {
                let seen = |v: &[u8]| v.iter().map(u8::to_string).collect::<Vec<_>>().join(" ");
                self.controller.editor.say(format!(
                    "Could not tell how it counts (right: {}; left: {}). Is it a knob that sends positions? Choose absolute then.",
                    seen(&d.clockwise),
                    seen(&d.counter)
                ));
            }
        }
        true
    }

    /// While a layer waits for its button: the next press from the device is the button that
    /// selects it. True if the message was used up (releases and faders are, and ignored).
    fn learn_layer_button(&mut self, name: &str, msg: controller::Msg) -> bool {
        // Any button press will do; the action only decides what kind of message counts.
        let Some(found) = controller::learn(msg, Action::Stop, Reading::Absolute, false) else { return true };
        let src = found.input.src;
        match self.edit_preset(|p| p.add_layer(name, src).map(|()| p.layers.len() - 1)) {
            Some(Ok(index)) => {
                let editor = &mut self.controller.editor;
                editor.say(format!("Layer {name}: {}", src.text()));
                editor.view.layer = index;
                editor.view.adding_layer = false;
                editor.view.new_layer.clear();
                editor.stop_learning();
            }
            Some(Err(e)) => {
                // The same layer waits for another press.
                self.controller.editor.say(e.to_string());
            }
            None => self.controller.editor.stop_learning(),
        }
        true
    }

    /// A new, empty preset for the chosen port, to be taught by pressing the controls.
    fn new_learned_preset(&mut self) {
        let drum = self.midi.input.as_ref().map(|h| h.port_name.clone());
        let port = if !self.cfg.controller_port.is_empty() {
            Some(self.cfg.controller_port.clone())
        } else {
            self.midi.inputs.iter().find(|p| Some(*p) != drum.as_ref()).cloned()
        };
        let Some(port) = port else {
            self.controller.editor.say("No MIDI input to use. Plug the controller in, then use Refresh and reconnect.");
            return;
        };
        let Some(dir) = self.preset_dir() else {
            self.view.error = Some("No settings folder is available on this system.".into());
            return;
        };
        let text = format!("name = My controller\nport = {port}\n");
        match unused_path(&dir, "My controller").and_then(|path| std::fs::write(&path, text).map(|_| path)) {
            Ok(path) => self.use_new_preset(&path, &format!("Saved {}", path.display())),
            Err(e) => self.view.error = Some(format!("Could not create the preset: {e}")),
        }
    }

    /// A copy of the chosen preset (built-in or not) that can be edited.
    fn copy_to_edit(&mut self) {
        let name = self.cfg.controller_preset.clone();
        let Some(dir) = self.preset_dir() else {
            self.view.error = Some("No settings folder is available on this system.".into());
            return;
        };
        let text = match controller::preset_source(&name, Some(&dir)) {
            Some(controller::PresetSource::BuiltIn(text)) => Ok(text.to_string()),
            Some(controller::PresetSource::File(path)) => std::fs::read_to_string(path),
            None => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such preset")),
        };
        let Ok(text) = text else {
            self.view.error = Some(format!("Could not read the preset \"{name}\"."));
            return;
        };
        match copy_preset(&dir, &name, &text) {
            Ok(path) => self.use_new_preset(&path, &format!("Copied to {}", path.display())),
            Err(e) => self.view.error = Some(format!("Could not copy the preset: {e}")),
        }
    }

    /// Chooses the preset in `path` (just created) and connects it.
    fn use_new_preset(&mut self, path: &std::path::Path, status: &str) {
        self.refresh_controllers();
        if let Some(stem) = path.file_stem() {
            self.select_preset(stem.to_string_lossy().into_owned());
        }
        self.view.status = status.to_string();
        self.controller.editor.view.open = true;
    }

    /// The editor window, while it is open: a window of its own (inside the main window where the
    /// platform has no second window).
    pub(in crate::app) fn controller_editor_window(&mut self, ctx: &egui::Context) {
        if !self.controller.editor.view.open {
            return;
        }
        let title = "Manage controller presets".to_string();
        let builder = ViewportBuilder::default().with_title(title.clone()).with_inner_size([960.0, 780.0]).with_min_inner_size([640.0, 360.0]);
        let closed = ctx.show_viewport_immediate(ViewportId::from_hash_of("controller_editor"), builder, |ctx, class| {
            if class == ViewportClass::Embedded {
                let mut open = true;
                egui::Window::new(title.clone()).id(egui::Id::new("controller_editor_window")).open(&mut open).default_size([860.0, 560.0]).resizable(true).show(ctx, |ui| self.manage_ui(ui));
                !open
            } else {
                egui::CentralPanel::default().show(ctx, |ui| self.manage_ui(ui));
                ctx.input(|i| i.viewport().close_requested())
            }
        });
        if closed {
            self.controller.editor.view.open = false;
            self.controller.editor.stop_learning();
        }
    }

    /// The contents of the window: the list of presets on the left, the chosen preset's
    /// name and assignments on the right.
    fn manage_ui(&mut self, ui: &mut egui::Ui) {
        egui::SidePanel::left("controller_preset_list").resizable(true).default_width(290.0).min_width(190.0).show_inside(ui, |ui| self.preset_list(ui));
        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                self.preset_header(ui);
                self.assignments_body(ui);
            });
        });
    }

    /// All presets, grouped by what they speak, with Add, Copy and Delete under the list.
    fn preset_list(&mut self, ui: &mut egui::Ui) {
        let chosen = self.cfg.controller_preset.clone();
        let custom = self.editable_path().is_some();
        let mut op: Option<Op> = None;
        let mut choose: Option<String> = None;
        egui::TopBottomPanel::bottom("controller_preset_buttons").show_inside(ui, |ui| {
            ui.add_space(4.0);
            if self.controller.editor.view.confirm_delete {
                ui.label(RichText::new(format!("Delete \"{chosen}\"? Its file is removed.")).color(render::AMBER));
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        op = Some(Op::Delete);
                    }
                    if ui.button("Keep").clicked() {
                        op = Some(Op::AskDelete(false));
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    if ui.button("Add").on_hover_text("A new, empty preset for the controller's port; press the controls to bind them").clicked() {
                        op = Some(Op::NewPreset);
                    }
                    if ui.add_enabled(!chosen.is_empty(), egui::Button::new("Copy")).on_hover_text("A copy of the chosen preset that can be changed").clicked() {
                        op = Some(Op::CopyToEdit);
                    }
                    if ui.add_enabled(custom, egui::Button::new("Delete…")).on_hover_text("Remove the chosen preset (built-in presets cannot be removed)").clicked() {
                        op = Some(Op::AskDelete(true));
                    }
                });
            }
            ui.add_space(2.0);
        });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if ui.selectable_label(chosen.is_empty(), "— none —").clicked() {
                    choose = Some(String::new());
                }
                for kind in [controller::PresetKind::Mackie, controller::PresetKind::Midi] {
                    let of_kind: Vec<&String> = self.controller.presets.iter().filter(|n| self.controller.kind_of(n).unwrap_or_default() == kind).collect();
                    if of_kind.is_empty() {
                        continue;
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new(kind.label()).strong());
                    for name in of_kind {
                        let label = if controller::builtin_text(name).is_some() { format!("{name}  (built in)") } else { name.clone() };
                        if ui.selectable_label(&chosen == name, label).clicked() {
                            choose = Some(name.clone());
                        }
                    }
                }
            });
        });
        if let Some(name) = choose {
            if name != chosen {
                self.select_preset(name);
            }
        }
        self.run_op(op);
    }

    /// The chosen preset's name and type, renaming it, and what cannot be done with a built-in one.
    fn preset_header(&mut self, ui: &mut egui::Ui) {
        let chosen = self.cfg.controller_preset.clone();
        if self.controller.editor.view.rename_for != chosen {
            self.controller.editor.view.rename_for = chosen.clone();
            self.controller.editor.view.rename_to = chosen.clone();
        }
        if chosen.is_empty() {
            ui.label(RichText::new("No preset is chosen. Pick one in the list, or Add a new one.").weak());
            return;
        }
        let custom = self.editable_path().is_some();
        let mut op: Option<Op> = None;
        ui.horizontal(|ui| {
            ui.heading(chosen.as_str());
            if let Some(kind) = self.controller.kind_of(&chosen) {
                widgets::hint(ui, format!("{}{}", kind.label(), if custom { "" } else { ", built in" }));
            }
        });
        if custom {
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut self.controller.editor.view.rename_to).desired_width(240.0));
                if ui.button("Rename").clicked() {
                    op = Some(Op::Rename);
                }
            });
        } else {
            widgets::hint(ui, "A built-in preset cannot be renamed, deleted or changed. Copy it to make one of your own.");
            if let Some(dir) = self.preset_dir() {
                widgets::hint(ui, format!("Your presets are text files in {} (any name ending in .txt).", dir.display()));
            }
        }
        if !custom && !self.controller.editor.notice.is_empty() {
            ui.label(RichText::new(self.controller.editor.notice.clone()).small().color(render::AMBER));
        }
        self.run_op(op);
    }

    /// A new name for the chosen preset of the user's: its file is renamed and the `name` line changed.
    fn rename_preset(&mut self) {
        let Some(old) = self.editable_path() else { return };
        let new = self.controller.editor.view.rename_to.trim().to_string();
        let notice = match check_preset_name(&new) {
            _ if new == self.cfg.controller_preset => Some("It already has that name.".to_string()),
            Err(why) => Some(why.to_string()),
            Ok(()) => None,
        };
        if let Some(notice) = notice {
            self.controller.editor.say(notice);
            return;
        }
        let Some(dir) = self.preset_dir() else { return };
        let target = dir.join(format!("{new}.txt"));
        if target.exists() {
            self.controller.editor.say("A preset with that name exists already.");
            return;
        }
        let result = std::fs::rename(&old, &target);
        match result {
            Ok(()) => {
                self.refresh_controllers();
                self.choose_preset(new.clone(), true);
                self.controller.editor.say(format!("Renamed to {new}."));
            }
            Err(e) => self.view.error = Some(format!("Could not rename the preset: {e}")),
        }
    }

    /// Deletes the chosen preset of the user's (its file).
    fn delete_preset(&mut self) {
        self.controller.editor.view.confirm_delete = false;
        let Some(path) = self.editable_path() else { return };
        let name = self.cfg.controller_preset.clone();
        match std::fs::remove_file(&path) {
            Ok(()) => {
                self.refresh_controllers();
                self.select_preset(String::new());
                self.controller.editor.say(format!("Deleted {name}."));
            }
            Err(e) => self.view.error = Some(format!("Could not delete {}: {e}", path.display())),
        }
    }

    /// The table of actions.
    fn assignments_body(&mut self, ui: &mut egui::Ui) {
        let mut op: Option<Op> = None;
        let Some(path) = self.editable_path() else {
            ui.separator();
            widgets::hint(ui, "Nothing to edit: a built-in preset cannot be changed. Copy it above, or start a new preset, then teach it your controls by pressing them.");
            return;
        };
        let Some(preset) = self.preset_being_edited() else { return };
        ui.separator();
        let connected = self.controller.is_connected();
        widgets::hint(ui, format!("Editing {}. Every change is saved at once.", path.display()));
        ui.checkbox(&mut self.controller.editor.keep_going, "After learning a control, go on to the next action");
        ui.checkbox(&mut self.controller.editor.remember_channel, "Limit a learned control to the MIDI channel it sends on");
        ui.horizontal(|ui| {
            ui.label("Layer");
            for (i, (name, _)) in preset.layers.iter().enumerate() {
                ui.selectable_value(&mut self.controller.editor.view.layer, i, name.as_str());
            }
            if preset.layers.is_empty() {
                ui.label(RichText::new("none: every control works on its own").weak());
            }
            if !self.controller.editor.view.adding_layer && ui.small_button("Add layer…").on_hover_text("A layer is a set of assignments that a button of the device switches to").clicked() {
                op = Some(Op::AddLayer);
            }
            if let Some(name) = self.editor_layer(&preset) {
                if self.controller.editor.view.confirm_layer_delete {
                    if ui.small_button(format!("Really delete layer {name} and what it does?")).clicked() {
                        op = Some(Op::DeleteLayer(name));
                    }
                    if ui.small_button("Keep").clicked() {
                        op = Some(Op::AskDeleteLayer(false));
                    }
                } else if ui.small_button("Delete layer").clicked() {
                    op = Some(Op::AskDeleteLayer(true));
                }
            }
        });
        if self.controller.editor.view.adding_layer {
            ui.horizontal(|ui| {
                ui.label("New layer's name");
                ui.add(egui::TextEdit::singleline(&mut self.controller.editor.view.new_layer).desired_width(80.0).char_limit(24));
                let waiting = self.controller.editor.learning_layer.is_some();
                if waiting {
                    if ui.button("Cancel").clicked() {
                        op = Some(Op::Cancel);
                    }
                } else if ui.add_enabled(connected && !self.controller.editor.view.new_layer.trim().is_empty(), egui::Button::new("Learn its button")).clicked() {
                    op = Some(Op::LearnLayer);
                }
                if !waiting && ui.small_button("Close").clicked() {
                    op = Some(Op::CloseAddLayer);
                }
            });
            if let Some(name) = self.controller.editor.learning_layer.clone() {
                ui.label(RichText::new(format!("Press the button that selects layer \"{name}\" on the device…")).strong().color(render::ACCENT));
            }
        }
        let layer = self.editor_layer(&preset);
        if !connected {
            ui.label(RichText::new("The controller is not connected: connect it to learn controls.").small().color(render::AMBER));
        }
        if let Some(d) = self.controller.editor.detecting.as_ref() {
            ui.label(RichText::new(d.prompt()).strong().color(render::ACCENT));
        } else if let Some(action) = self.controller.editor.learning {
            ui.label(RichText::new(format!("Press or turn the control for \"{}\" on the device…", action.title())).strong().color(render::ACCENT));
        } else if !self.controller.editor.notice.is_empty() {
            ui.label(RichText::new(self.controller.editor.notice.clone()).small());
        }

        for (section, (title, actions)) in Action::sections().into_iter().enumerate() {
            egui::CollapsingHeader::new(title).default_open(section == 0).id_salt(("controller_editor_section", section)).show(ui, |ui| {
                egui::Grid::new(("controller_editor_grid", section)).num_columns(4).striped(true).show(ui, |ui| {
                    for action in actions {
                        let bound = preset.bindings_of(layer.as_deref(), action);
                        let learning = self.controller.editor.learning == Some(action);
                        ui.label(if learning { RichText::new(action.title()).strong() } else { RichText::new(action.title()) }).on_hover_text(action.help());
                        let controls = if bound.is_empty() { "—".to_string() } else { bound.iter().map(|b| control_text(b)).collect::<Vec<_>>().join(", ") };
                        ui.label(RichText::new(controls).monospace());
                        // How a value's cc is read: its binding's, or the choice made before learning it.
                        if action.is_level() || action.is_browse() {
                            let current = bound
                                .iter()
                                .find(|b| matches!(b.input.src, controller::Src::Cc(_)))
                                .map(|b| b.reading)
                                .unwrap_or_else(|| self.controller.editor.chosen_kind(action));
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt(("controller_kind", action.text())).selected_text(kind_text(current)).width(150.0).show_ui(ui, |ui| {
                                    for reading in KINDS {
                                        if action.is_browse() && !reading.is_relative() {
                                            continue;
                                        }
                                        if ui.selectable_label(current == reading, kind_text(reading)).clicked() {
                                            op = Some(Op::Kind(action, reading));
                                        }
                                    }
                                });
                                if ui.small_button("Detect").on_hover_text("Turn the encoder to the right and then to the left to find out how it counts").clicked() {
                                    op = Some(Op::Detect(action));
                                }
                            });
                        } else {
                            ui.label("");
                        }
                        ui.horizontal(|ui| {
                            if learning {
                                if ui.button("Cancel").clicked() {
                                    op = Some(Op::Cancel);
                                }
                            } else if ui.button("Learn").clicked() {
                                op = Some(Op::Learn(action));
                            }
                            if !bound.is_empty() && ui.small_button("Clear").clicked() {
                                op = Some(Op::Clear(action));
                            }
                        });
                        ui.end_row();
                    }
                });
            });
        }
        egui::CollapsingHeader::new("Lights and rings").default_open(!preset.lights.is_empty()).id_salt("controller_editor_lights").show(ui, |ui| {
            widgets::hint(ui, "A light shows an action that is on (a button's LED); a ring shows a value around an encoder. Only actions that have a control are listed.");
            egui::Grid::new("controller_editor_lights_grid").num_columns(2).striped(true).show(ui, |ui| {
                for action in action_order() {
                    if !(action.has_light() || action.is_level()) {
                        continue;
                    }
                    let bound = preset.bindings_of(layer.as_deref(), action);
                    let shown = preset.lights_of(layer.as_deref(), action, false).is_some() || preset.lights_of(layer.as_deref(), action, true).is_some();
                    if bound.is_empty() && !shown {
                        continue;
                    }
                    ui.label(action.title());
                    if let Some(light_op) = light_cell(ui, &preset, layer.as_deref(), action, &bound) {
                        op = Some(light_op);
                    }
                    ui.end_row();
                }
            });
        });
        self.run_op(op);
    }

    fn run_op(&mut self, op: Option<Op>) {
        let Some(op) = op else { return };
        match op {
            Op::Learn(action) => {
                if !self.controller.is_connected() {
                    self.controller.editor.say("Connect the controller first (Refresh and reconnect, above).");
                } else {
                    self.controller.editor.start_learning(action);
                }
            }
            Op::Cancel => self.controller.editor.stop_learning(),
            Op::Detect(action) => {
                if !self.controller.is_connected() {
                    self.controller.editor.say("Connect the controller first (Refresh and reconnect, above).");
                } else {
                    let editor = &mut self.controller.editor;
                    editor.stop_learning();
                    editor.notice.clear();
                    editor.detecting = Some(Detect { action, cc: None, clockwise: Vec::new(), counter: Vec::new() });
                }
            }
            Op::AddLight(action, ring) => {
                let Some(preset) = self.preset_being_edited() else { return };
                let layer = self.editor_layer(&preset);
                let wanted = |b: &&Binding| if ring { matches!(b.input.src, controller::Src::Cc(_)) } else { !matches!(b.input.src, controller::Src::Bend(_)) };
                if let Some(b) = preset.bindings_of(layer.as_deref(), action).into_iter().find(wanted) {
                    let out = match b.input.src {
                        controller::Src::Cc(n) => controller::Out::Cc(n),
                        controller::Src::Note(n) | controller::Src::Bend(n) => controller::Out::Note(n),
                    };
                    let mut light = Light::new(layer.as_deref(), action, out, ring);
                    light.channel = b.input.channel.unwrap_or(1);
                    self.edit_preset(|p| p.set_light(light));
                }
            }
            Op::SetLight(light) => {
                self.edit_preset(|p| p.set_light(light));
            }
            Op::ClearLight(action, ring) => {
                if let Some(preset) = self.preset_being_edited() {
                    let layer = self.editor_layer(&preset);
                    self.edit_preset(|p| p.clear_light(layer.as_deref(), action, ring));
                }
            }
            Op::AddLayer => {
                self.controller.editor.view.adding_layer = true;
                self.controller.editor.view.confirm_layer_delete = false;
            }
            Op::CloseAddLayer => self.controller.editor.view.adding_layer = false,
            Op::LearnLayer => {
                let name = self.controller.editor.view.new_layer.trim().to_string();
                if !self.controller.is_connected() {
                    self.controller.editor.say("Connect the controller first (Refresh and reconnect, above).");
                } else {
                    let editor = &mut self.controller.editor;
                    editor.stop_learning();
                    editor.notice.clear();
                    editor.learning_layer = Some(name);
                }
            }
            Op::AskDeleteLayer(ask) => self.controller.editor.view.confirm_layer_delete = ask,
            Op::DeleteLayer(name) => {
                self.edit_preset(|p| p.remove_layer(&name));
                let editor = &mut self.controller.editor;
                editor.view.confirm_layer_delete = false;
                editor.view.layer = 0;
                editor.say(format!("Layer {name} deleted."));
            }
            Op::Clear(action) => {
                if let Some(preset) = self.preset_being_edited() {
                    let layer = self.editor_layer(&preset);
                    self.edit_preset(|p| p.clear(layer.as_deref(), action));
                }
            }
            Op::Kind(action, reading) => {
                self.controller.editor.choose_kind(action, reading);
                if let Some(preset) = self.preset_being_edited() {
                    let layer = self.editor_layer(&preset);
                    self.edit_preset(|p| p.set_reading(layer.as_deref(), action, reading));
                }
            }
            Op::NewPreset => self.new_learned_preset(),
            Op::CopyToEdit => self.copy_to_edit(),
            Op::Rename => self.rename_preset(),
            Op::AskDelete(ask) => self.controller.editor.view.confirm_delete = ask,
            Op::Delete => self.delete_preset(),
        }
    }
}

/// The light and the ring of an action in the table: what is there (with its numbers, which can be
/// changed), or a button to add one on the control the action is bound to. A light is for a button
/// (set on the same note or cc), a ring for a knob or encoder (set on the same cc, which many devices
/// echo; the Mackie ring cc is another number, and is typed over).
fn light_cell(ui: &mut egui::Ui, preset: &Preset, layer: Option<&str>, action: Action, bound: &[&Binding]) -> Option<Op> {
    let mut op = None;
    ui.horizontal(|ui| {
        for ring in [false, true] {
            if (ring && !action.is_level()) || (!ring && !action.has_light()) {
                continue;
            }
            let kind = if ring { "ring" } else { "light" };
            match preset.lights_of(layer, action, ring) {
                Some(found) => {
                    let mut light = found.clone();
                    let mut changed = false;
                    ui.label(RichText::new(format!("{kind}: {}", out_text(light.out))).monospace().small());
                    let mut number = match light.out {
                        controller::Out::Note(n) | controller::Out::Cc(n) => n,
                    };
                    if ui.add(egui::DragValue::new(&mut number).range(0..=127).prefix("no. ")).on_hover_text("The note or cc the device is told to light").changed() {
                        light.out = match light.out {
                            controller::Out::Note(_) => controller::Out::Note(number),
                            controller::Out::Cc(_) => controller::Out::Cc(number),
                        };
                        changed = true;
                    }
                    if !ring {
                        changed |= ui.add(egui::DragValue::new(&mut light.on).range(0..=127).prefix("on ")).on_hover_text("The value that lights it (often the colour)").changed();
                        changed |= ui.add(egui::DragValue::new(&mut light.off).range(0..=127).prefix("off ")).on_hover_text("The value that darkens it").changed();
                    }
                    if changed {
                        op = Some(Op::SetLight(light));
                    }
                    if ui.small_button("×").on_hover_text(format!("Remove the {kind}")).clicked() {
                        op = Some(Op::ClearLight(action, ring));
                    }
                }
                None => {
                    let possible = bound.iter().any(|b| if ring { matches!(b.input.src, controller::Src::Cc(_)) } else { !matches!(b.input.src, controller::Src::Bend(_)) });
                    let hint = if ring { "Show the value in an LED ring on the same cc as the knob; change the number if the device uses another" } else { "Light the button on the same note or cc it sends; change the number if the device uses another" };
                    if ui.add_enabled(possible, egui::Button::new(format!("Add {kind}")).small()).on_hover_text(hint).clicked() {
                        op = Some(Op::AddLight(action, ring));
                    }
                }
            }
        }
    });
    op
}

fn out_text(out: controller::Out) -> String {
    match out {
        controller::Out::Note(n) => format!("note {n}"),
        controller::Out::Cc(n) => format!("cc {n}"),
    }
}

/// Why `name` cannot be the name of a preset file of the user's, if it cannot.
fn check_preset_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        Err("Type a name.")
    } else if name.chars().any(|c| "\\/:*?\"<>|".contains(c) || c.is_control()) || name.starts_with('.') || name.ends_with('.') {
        Err("A name cannot contain \\ / : * ? \" < > | or start or end with a dot.")
    } else if controller::builtin_text(name).is_some() {
        Err("That is the name of a built-in preset.")
    } else {
        Ok(())
    }
}

/// "NAME.txt" in `dir`, or "NAME 2.txt", … if that exists; the folder is created.
fn unused_path(dir: &std::path::Path, name: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut path = dir.join(format!("{name}.txt"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{name} {n}.txt"));
        n += 1;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Paths};

    #[test]
    fn new_presets_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("roll-model-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = unused_path(&dir, "My controller").unwrap();
        std::fs::write(&a, "x").unwrap();
        let b = unused_path(&dir, "My controller").unwrap();
        assert_ne!(a, b);
        assert!(b.to_string_lossy().ends_with("My controller 2.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kinds_are_named_and_the_lists_have_no_absolute_kind() {
        assert_eq!(kind_text(Reading::Absolute), "absolute");
        assert_eq!(kind_text(Reading::Relative(Encoding::Twos)), "relative, twos");
        assert_eq!(KINDS.iter().filter(|r| !r.is_relative()).count(), 1);
    }

    #[test]
    fn the_actions_are_listed_in_one_order_for_going_on_to_the_next() {
        let order = action_order();
        assert_eq!(order.first(), Some(&Action::Record));
        assert_eq!(order.last(), Some(&Action::Audition(7)));
        assert!(order.iter().position(|a| *a == Action::Tempo) < order.iter().position(|a| *a == Action::Gain(0)));
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("roll-model-editor-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dir = dir.join("settings");
        let controllers = dir.join("controllers");
        std::fs::create_dir_all(&controllers).unwrap();
        controllers
    }

    fn pressed(note: u8) -> controller::Msg {
        controller::Msg::NoteOn { ch: 1, note, vel: 100 }
    }

    #[test]
    fn a_light_and_a_ring_are_added_on_the_control_of_the_action_and_edited() {
        let dir = temp_dir("lights");
        std::fs::write(dir.join("Pads.txt"), "name = Pads
port = PADS
kind = midi
bind = note 36 ch 3 -> record
bind = cc 13 abs -> tempo
").unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        let saved = || controller::Preset::parse(&std::fs::read_to_string(dir.join("Pads.txt")).unwrap());

        app.run_op(Some(Op::AddLight(Action::Record, false)));
        app.run_op(Some(Op::AddLight(Action::Tempo, true)));
        let light = saved().lights_of(None, Action::Record, false).cloned().unwrap();
        assert_eq!((light.out, light.channel, light.on, light.off), (controller::Out::Note(36), 3, 127, 0), "on the control of the action, on its channel");
        assert_eq!(saved().lights_of(None, Action::Tempo, true).unwrap().out, controller::Out::Cc(13));
        // An action without a control of the right kind gets nothing.
        app.run_op(Some(Op::AddLight(Action::Stop, false)));
        app.run_op(Some(Op::AddLight(Action::Record, true)));
        assert_eq!(saved().lights.len(), 2);
        // Editing replaces, clearing removes.
        let mut colour = light.clone();
        colour.on = 60;
        colour.out = controller::Out::Note(40);
        app.run_op(Some(Op::SetLight(colour)));
        let kept = saved().lights_of(None, Action::Record, false).cloned().unwrap();
        assert_eq!((kept.out, kept.on), (controller::Out::Note(40), 60));
        app.run_op(Some(Op::ClearLight(Action::Record, false)));
        assert!(saved().lights_of(None, Action::Record, false).is_none());
        assert!(saved().warnings.is_empty(), "{:?}", saved().warnings);
        // The table draws with a light, a ring, and the buttons to add one.
        let ctx = egui::Context::default();
        app.controller.editor.view.open = true;
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.controller_editor_window(ctx));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_encoder_is_turned_right_and_left_to_find_out_how_it_counts() {
        let dir = temp_dir("detect");
        std::fs::write(dir.join("Pads.txt"), "name = Pads
port = PADS
kind = midi
bind = cc 13 rel -> tempo
").unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        let saved = || controller::Preset::parse(&std::fs::read_to_string(dir.join("Pads.txt")).unwrap());
        let cc = |cc: u8, val: u8| controller::Msg::Cc { ch: 1, cc, val };
        let start = |app: &mut DrumApp| app.controller.editor.detecting = Some(Detect { action: Action::Tempo, cc: None, clockwise: Vec::new(), counter: Vec::new() });

        start(&mut app);
        assert!(app.controller.editor.detecting.as_ref().unwrap().prompt().contains("RIGHT"));
        // Other controls are used up and ignored, also the first cc that fixed the encoder.
        assert!(app.learn_message(pressed(36)));
        for v in [1, 1, 2] {
            assert!(app.learn_message(cc(13, v)));
        }
        assert!(app.learn_message(cc(14, 99)), "another knob does not count");
        assert!(app.learn_message(cc(13, 1)));
        assert!(app.controller.editor.detecting.as_ref().unwrap().prompt().contains("LEFT"));
        for v in [127, 127, 126, 127] {
            assert!(app.learn_message(cc(13, v)));
        }
        assert!(app.controller.editor.detecting.is_none());
        assert_eq!(saved().binds[0].reading, Reading::Relative(Encoding::Twos));
        assert_eq!(app.controller.editor.chosen_kind(Action::Tempo), Reading::Relative(Encoding::Twos));
        assert!(app.controller.editor.notice.contains("twos"), "{}", app.controller.editor.notice);

        // A knob that sends positions is not guessed at: nothing changes, and the numbers are shown.
        start(&mut app);
        for v in [10, 40, 90, 100, 80, 20, 5, 60] {
            app.learn_message(cc(13, v));
        }
        assert!(app.controller.editor.detecting.is_none());
        assert_eq!(saved().binds[0].reading, Reading::Relative(Encoding::Twos));
        assert!(app.controller.editor.notice.contains("Could not tell") && app.controller.editor.notice.contains("10 40 90 100"), "{}", app.controller.editor.notice);
        // Cancelling stops it.
        start(&mut app);
        app.run_op(Some(Op::Cancel));
        assert!(app.controller.editor.detecting.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_layer_is_added_by_pressing_its_button_and_removed_with_what_it_does() {
        let dir = temp_dir("layers");
        std::fs::write(dir.join("Pads.txt"), "name = Pads
port = PADS
bind = note 36 -> record
").unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        let saved = || controller::Preset::parse(&std::fs::read_to_string(dir.join("Pads.txt")).unwrap());

        // The layer waits for a button: a release and a fader are used up and change nothing.
        app.controller.editor.learning_layer = Some("B".into());
        assert!(app.learn_message(controller::Msg::NoteOn { ch: 1, note: 85, vel: 0 }));
        assert!(app.learn_message(controller::Msg::Bend { ch: 1, value: 100 }));
        assert!(saved().layers.is_empty() && app.controller.editor.learning_layer.is_some());
        // A button that does something cannot select a layer: it says why and waits for another.
        assert!(app.learn_message(pressed(36)));
        assert!(app.controller.editor.notice.contains("Record"), "{}", app.controller.editor.notice);
        assert!(saved().layers.is_empty() && app.controller.editor.learning_layer.is_some());
        // A free button does, and the new layer is the one shown.
        assert!(app.learn_message(pressed(85)));
        assert_eq!(saved().layers, vec![("B".to_string(), controller::Src::Note(85))]);
        assert_eq!((app.controller.editor.learning_layer.clone(), app.controller.editor.view.layer), (None, 0));
        // Bindings on the layer go with it.
        app.controller.editor.learning = Some(Action::Stop);
        assert!(app.learn_message(pressed(40)));
        assert_eq!(saved().bindings_of(Some("B"), Action::Stop).len(), 1);
        app.run_op(Some(Op::DeleteLayer("B".into())));
        assert!(saved().layers.is_empty() && saved().bindings_of(Some("B"), Action::Stop).is_empty());
        assert_eq!(saved().bindings_of(None, Action::Record).len(), 1, "what works on every layer stays");
        assert!(saved().warnings.is_empty(), "{:?}", saved().warnings);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn learning_binds_the_controls_one_after_the_other_and_saves_the_preset() {
        let dir = temp_dir("learn");
        std::fs::write(dir.join("Pads.txt"), "name = Pads\nport = PADS\n").unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        assert!(app.editable_path().is_some());
        let saved = || controller::Preset::parse(&std::fs::read_to_string(dir.join("Pads.txt")).unwrap());

        // Nothing waits for a control: messages are not used up.
        assert!(!app.learn_message(pressed(36)));
        // Record waits; the first press binds it and the editor goes on to Listen.
        app.controller.editor.learning = Some(Action::Record);
        assert!(app.learn_message(pressed(36)));
        assert_eq!(app.controller.editor.learning, Some(Action::Listen));
        assert_eq!(saved().bindings_of(None, Action::Record)[0].input.src, controller::Src::Note(36));
        // A release is used up and changes nothing; the next press binds Listen.
        assert!(app.learn_message(controller::Msg::NoteOn { ch: 1, note: 36, vel: 0 }));
        assert_eq!(app.controller.editor.learning, Some(Action::Listen));
        assert!(app.learn_message(pressed(37)));
        assert_eq!(saved().bindings_of(None, Action::Listen)[0].input.src, controller::Src::Note(37));
        // Pressing a used pad takes it from its action, and says so.
        app.controller.editor.learning = Some(Action::Stop);
        assert!(app.learn_message(pressed(36)));
        assert!(app.controller.editor.notice.contains("it was Record"), "{}", app.controller.editor.notice);
        assert!(saved().bindings_of(None, Action::Record).is_empty());
        // Without "go on", learning ends after one control.
        app.controller.editor.keep_going = false;
        app.controller.editor.learning = Some(Action::Tempo);
        assert!(app.learn_message(controller::Msg::Cc { ch: 2, cc: 13, val: 40 }));
        assert_eq!(app.controller.editor.learning, None);
        let tempo = saved().binds.iter().find(|b| b.action == Action::Tempo).cloned().unwrap();
        assert_eq!((tempo.input.src, tempo.input.channel, tempo.reading), (controller::Src::Cc(13), None, Reading::Absolute));
        // The channel is remembered when asked for, and a relative choice is kept for the knob.
        app.controller.editor.remember_channel = true;
        app.controller.editor.kinds.insert(Action::RangeFrom.text(), Reading::Relative(Encoding::Twos));
        app.controller.editor.learning = Some(Action::RangeFrom);
        assert!(app.learn_message(controller::Msg::Cc { ch: 2, cc: 14, val: 1 }));
        let from = saved().binds.iter().find(|b| b.action == Action::RangeFrom).cloned().unwrap();
        assert_eq!((from.input.channel, from.reading), (Some(2), Reading::Relative(Encoding::Twos)));
        // The file is a normal preset: it parses without complaint and says what was learned.
        assert!(saved().warnings.is_empty(), "{:?}", saved().warnings);
        assert_eq!(saved().port, "PADS");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_preset_is_made_for_a_port_and_a_built_in_one_is_copied_to_be_edited() {
        let dir = temp_dir("new");
        let mut app = DrumApp::with_config(Config { controller_preset: "Behringer X-Touch Mini".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        app.midi.inputs = vec!["Some Pad Controller".into()];
        assert!(app.editable_path().is_none(), "a built-in preset is not edited");
        app.new_learned_preset();
        assert_eq!(app.cfg.controller_preset, "My controller");
        let text = std::fs::read_to_string(dir.join("My controller.txt")).unwrap();
        assert!(text.contains("port = Some Pad Controller"), "{text}");
        assert!(app.editable_path().is_some());
        assert_eq!(app.paths.controllers_dir(), Some(dir.clone()), "the folder is kept");
        // A copy of a built-in preset has all its assignments and is editable.
        app.cfg.controller_preset = "Korg nanoKONTROL2".into();
        app.copy_to_edit();
        assert_eq!(app.cfg.controller_preset, "Korg nanoKONTROL2 (copy)");
        let copy = controller::Preset::parse(&std::fs::read_to_string(dir.join("Korg nanoKONTROL2 (copy).txt")).unwrap());
        assert_eq!(copy, controller::Preset::parse(controller::builtin_text("Korg nanoKONTROL2").unwrap()));
        // Editing the copy changes the copy only.
        app.controller.editor.learning = Some(Action::Record);
        app.learn_message(controller::Msg::Cc { ch: 1, cc: 99, val: 127 });
        let edited = controller::Preset::parse(&std::fs::read_to_string(dir.join("Korg nanoKONTROL2 (copy).txt")).unwrap());
        assert_eq!(edited.bindings_of(None, Action::Record)[0].input.src, controller::Src::Cc(99));
        let original = controller::Preset::parse(controller::builtin_text("Korg nanoKONTROL2").unwrap());
        assert_eq!(original.bindings_of(None, Action::Record)[0].input.src, controller::Src::Cc(45), "the built-in one is untouched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_are_checked() {
        assert!(check_preset_name("My pads 2").is_ok());
        for bad in ["", "a/b", "a\\b", "what?", "a:b", ".hidden", "dot.", "Korg nanoKONTROL2", "x\ty"] {
            assert!(check_preset_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_preset_can_be_renamed_and_deleted_and_a_built_in_one_cannot() {
        let dir = temp_dir("rename");
        std::fs::write(dir.join("Pads.txt"), "# my notes\nname = Pads\nport = PADS\nbind = note 1 -> record\n").unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        // A bad name changes nothing and says why.
        app.controller.editor.view.rename_to = "a/b".into();
        app.rename_preset();
        assert!(dir.join("Pads.txt").exists() && app.controller.editor.notice.contains("cannot contain"), "{}", app.controller.editor.notice);
        app.controller.editor.view.rename_to = "Pads".into();
        app.rename_preset();
        assert!(app.controller.editor.notice.contains("already"));
        // A name in use is refused.
        std::fs::write(dir.join("Other.txt"), "name = Other\n").unwrap();
        app.controller.editor.view.rename_to = "Other".into();
        app.rename_preset();
        assert!(app.controller.editor.notice.contains("exists"));
        // A good name renames the file, keeps the content and the comments, and follows the choice.
        app.controller.editor.view.rename_to = "Stage pads".into();
        app.rename_preset();
        assert!(!dir.join("Pads.txt").exists() && dir.join("Stage pads.txt").exists());
        assert_eq!(app.cfg.controller_preset, "Stage pads");
        let text = std::fs::read_to_string(dir.join("Stage pads.txt")).unwrap();
        assert!(text.contains("# my notes") && text.contains("bind = note 1 -> record"), "the file is moved as it is: {text}");
        assert_eq!(controller::load_preset("Stage pads", Some(&dir)).unwrap().name, "Stage pads", "and is known by its new name");
        // Deleting asks first in the window (a flag), then removes the file and the choice.
        app.controller.editor.view.confirm_delete = true;
        app.delete_preset();
        assert!(!dir.join("Stage pads.txt").exists() && dir.join("Other.txt").exists());
        assert_eq!(app.cfg.controller_preset, "");
        assert!(!app.controller.editor.view.confirm_delete);
        // A built-in preset is neither renamed nor deleted.
        app.cfg.controller_preset = "Korg nanoKONTROL2".into();
        app.controller.editor.view.rename_to = "Mine".into();
        app.rename_preset();
        app.delete_preset();
        assert_eq!(app.cfg.controller_preset, "Korg nanoKONTROL2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clicking_a_preset_in_the_list_chooses_it_and_shows_its_assignments() {
        let dir = temp_dir("choose");
        std::fs::write(dir.join("Pads.txt"), "name = Pads\nport = PADS\nbind = note 1 -> record\n").unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Korg nanoKONTROL2".into(), controller_port: "X".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        app.controller.editor.learning = Some(Action::Record);
        app.controller.editor.view.confirm_delete = true;
        app.select_preset("Pads".into());
        assert_eq!(app.cfg.controller_preset, "Pads");
        assert!(app.cfg.controller_port.is_empty(), "the preset's own port is used");
        assert!(app.controller.editor.learning.is_none() && !app.controller.editor.view.confirm_delete);
        assert!(app.editable_path().is_some(), "and it is one that can be edited");
        // The window draws with the list on the left for a built-in and for an own preset, and for none.
        app.refresh_controllers();
        let ctx = egui::Context::default();
        for name in ["Pads", "Korg nanoKONTROL2", ""] {
            app.select_preset(name.into());
            for _ in 0..2 {
                let _ = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.manage_ui(ui));
                });
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_preset_needs_a_port() {
        let dir = temp_dir("noport");
        let mut app = DrumApp::with_config(Config::default());
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        app.new_learned_preset();
        assert!(app.controller.editor.notice.contains("No MIDI input"), "{}", app.controller.editor.notice);
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none(), "nothing was written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_editor_is_a_window_that_opens_from_the_page_and_closes_again() {
        let ctx = egui::Context::default();
        let mut app = DrumApp::with_config(Config::default());
        let frame = |app: &mut DrumApp| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.controller_editor_window(ctx));
        };
        frame(&mut app);
        assert!(!app.controller.editor.view.open, "closed until asked for");
        app.controller.editor.view.open = true;
        frame(&mut app);
        frame(&mut app);
        assert!(app.controller.editor.view.open, "it stays open");
        // Making a preset opens the editor on it.
        let dir = temp_dir("window");
        app.controller.editor.view.open = false;
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        app.midi.inputs = vec!["Some Pad Controller".into()];
        app.new_learned_preset();
        assert!(app.controller.editor.view.open, "a new preset opens the editor");
        frame(&mut app);
        // The layer rows draw while a layer is being added, waited for and deleted.
        app.controller.editor.view.adding_layer = true;
        app.controller.editor.view.new_layer = "B".into();
        frame(&mut app);
        app.controller.editor.learning_layer = Some("B".into());
        frame(&mut app);
        app.controller.editor.stop_learning();
        app.edit_preset(|p| p.add_layer("B", controller::Src::Note(85)).unwrap());
        app.controller.editor.view.confirm_layer_delete = true;
        frame(&mut app);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_table_draws_for_a_preset_that_cannot_be_edited() {
        let mut app = DrumApp::with_config(Config::default());
        app.refresh_controllers();
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.manage_ui(ui));
            });
        }
        assert!(app.editable_path().is_none(), "the Mini is built in");
        app.cfg.controller_preset = String::new();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.manage_ui(ui));
        });
        // A message while nothing is being learned is not used up.
        assert!(!app.learn_message(controller::Msg::NoteOn { ch: 1, note: 1, vel: 100 }));
    }
}
