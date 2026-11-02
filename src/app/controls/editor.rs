//! The Manage window of the Controller page: creating, copying, renaming and deleting presets,
//! and below them a table of the program's actions with the control
//! each is bound to, and *Learn* (press or turn a control on the device to bind it). The preset
//! being edited is a file in the controllers folder; every change is written at once and put in
//! place in the running controller. The built-in presets are not edited: copy one first, or start
//! a new preset. The editing itself (`assign`, `clear`, `to_text`) is in `controller.rs`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use egui::{RichText, ViewportBuilder, ViewportClass, ViewportId};

use super::learner::{action_order, Heard, Learner};
use super::{controller, Action, DrumApp};
use crate::controller::{Binding, Encoding, Light, Preset, Reading};
use crate::theme;
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

/// What the Manage window says when a control is to be learned and there is no controller.
const CONNECT_FIRST: &str = "Connect the controller first (Refresh and reconnect, above).";

/// What the editor remembers between frames: the learn state machine and its choices.
pub(super) struct EditorState {
    /// What waits for the next control the device sends, and the choices that go with it.
    pub(super) learner: Learner,
    pub(super) notice: String,
    /// A preset whose lights are being changed (a number dragged): the file it belongs to and the
    /// preset as it is now. The file is written when the drag ends, not on every frame of it.
    pub(super) draft: Option<(PathBuf, Preset)>,
    /// What the Manage window remembers about itself.
    pub(super) view: ManageView,
    /// The folder looked at once for where the file of the chosen preset is, until Refresh.
    file_of: RefCell<Option<(String, PathBuf, Option<PathBuf>)>>,
    /// The chosen preset as it was read, for as long as its file is as it was.
    read: RefCell<Option<Read>>,
    /// How many times a preset was read from its file.
    #[cfg(test)]
    reads: std::cell::Cell<usize>,
}

/// A preset read from its file (or from the built-in text) and what the file looked like then.
struct Read {
    name: String,
    file: Option<PathBuf>,
    stamp: Option<(SystemTime, u64)>,
    preset: Preset,
}

fn stamp_of(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

impl EditorState {
    /// Says something to the user in the window.
    pub(super) fn say(&mut self, text: impl Into<String>) {
        self.notice = text.into();
    }

    /// Waits for the next control the device sends, for `action`.
    pub(super) fn start_learning(&mut self, action: Action) {
        self.learner.wait_for_control(action);
        self.notice.clear();
    }

    /// Waits for the clicks of the encoder of `action`.
    pub(super) fn start_detecting(&mut self, action: Action) {
        self.learner.wait_for_clicks(action);
        self.notice.clear();
    }

    /// Waits for the button of the layer to be called `name`.
    pub(super) fn start_learning_layer(&mut self, name: String) {
        self.learner.wait_for_layer(name);
        self.notice.clear();
    }

    pub(super) fn stop_learning(&mut self) {
        self.learner.stop();
    }

    /// The files of the presets are looked at again: one was made, renamed, removed or changed outside.
    pub(super) fn forget_files(&self) {
        *self.file_of.borrow_mut() = None;
        *self.read.borrow_mut() = None;
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
        EditorState {
            learner: Learner::new(),
            notice: String::new(),
            draft: None,
            view: ManageView::default(),
            file_of: RefCell::new(None),
            read: RefCell::new(None),
            #[cfg(test)]
            reads: std::cell::Cell::new(0),
        }
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
    /// Change a light; `true` when the number stopped changing (the drag ended, or the typing).
    SetLight(Light, bool),
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

/// The preset could not be read or written; the reason is already on screen (`view.error`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EditFailed;

impl DrumApp {
    fn preset_dir(&self) -> Option<PathBuf> {
        self.paths.controllers_dir()
    }

    /// The file of the chosen preset if it is one the user may edit: not a built-in one.
    pub(super) fn editable_path(&self) -> Option<PathBuf> {
        let dir = self.preset_dir()?;
        let name = &self.cfg.controller_preset;
        let mut known = self.controller.editor.file_of.borrow_mut();
        if let Some((n, d, file)) = known.as_ref() {
            if n == name && *d == dir {
                return file.clone();
            }
        }
        let file = match controller::preset_source(name, Some(&dir)) {
            Some(controller::PresetSource::File(path)) => Some(path),
            _ => None,
        };
        *known = Some((name.clone(), dir, file.clone()));
        file
    }

    /// The chosen preset as it is now, for the table: the one in use, or its file.
    fn preset_being_edited(&self) -> Option<Preset> {
        if let Some((path, preset)) = &self.controller.editor.draft {
            if self.editable_path().as_ref() == Some(path) {
                return Some(preset.clone());
            }
        }
        if let Some((preset, _)) = self.controller.live() {
            return Some(preset.clone());
        }
        self.read_preset()
    }

    /// The chosen preset read from its file, once for as long as the file is as it was: the table
    /// asks for it on every frame.
    fn read_preset(&self) -> Option<Preset> {
        let dir = self.preset_dir()?;
        let name = &self.cfg.controller_preset;
        let file = self.editable_path();
        let stamp = file.as_deref().and_then(stamp_of);
        let mut read = self.controller.editor.read.borrow_mut();
        if let Some(r) = read.as_ref() {
            if r.name == *name && r.file == file && r.stamp == stamp {
                return Some(r.preset.clone());
            }
        }
        #[cfg(test)]
        self.controller.editor.reads.set(self.controller.editor.reads.get() + 1);
        let preset = controller::load_preset(name, Some(&dir)).ok()?;
        *read = Some(Read { name: name.clone(), file, stamp, preset: preset.clone() });
        Some(preset)
    }

    /// Changes the chosen (custom) preset: reads its file, applies `change`, writes the file and
    /// puts the result in place in the running controller.
    fn edit_preset<R>(&mut self, change: impl FnOnce(&mut Preset) -> R) -> Result<R, EditFailed> {
        self.flush_draft();
        let path = self.editable_path().ok_or(EditFailed)?;
        let dir = self.preset_dir().ok_or(EditFailed)?;
        let mut preset = match controller::load_preset(&self.cfg.controller_preset, Some(&dir)) {
            Ok(p) => p,
            Err(e) => {
                self.view.error = Some(e.to_string());
                return Err(EditFailed);
            }
        };
        let result = change(&mut preset);
        if let Err(e) = crate::config::write_atomically(&path, preset.to_text().as_bytes()) {
            self.view.error = Some(format!("Could not save {}: {e}", path.display()));
            return Err(EditFailed);
        }
        self.controller.replace_preset(preset);
        Ok(result)
    }

    /// Writes the preset whose lights were being dragged, if there is one; before anything else
    /// reads the file, and when the program closes.
    pub(in crate::app) fn flush_draft(&mut self) {
        let Some((path, preset)) = self.controller.editor.draft.take() else { return };
        if self.editable_path().as_ref() != Some(&path) {
            return; // another preset was chosen meanwhile
        }
        if let Err(e) = crate::config::write_atomically(&path, preset.to_text().as_bytes()) {
            self.view.error = Some(format!("Could not save {}: {e}", path.display()));
        }
    }

    /// Changes a light. While a number is still being dragged (`settled` is false) the running
    /// controller follows at once and the file waits; it is written when the drag ends.
    fn set_light(&mut self, light: Light, settled: bool) {
        let Some(path) = self.editable_path() else { return };
        let Some(mut preset) = self.preset_being_edited() else { return };
        preset.set_light(light);
        self.controller.replace_preset(preset.clone());
        self.controller.editor.draft = Some((path, preset));
        if settled {
            self.flush_draft();
        }
    }

    /// The layer the table edits: `None` for a preset without layers.
    fn editor_layer(&self, preset: &Preset) -> Option<String> {
        preset.layers.get(self.controller.editor.view.layer).map(|l| l.0.clone())
    }

    /// While something waits for a control, the next message from the device is taken for it (and
    /// never acts). True if the message was used up: a message that cannot be the control that
    /// waits for it (a release, a fader for a button) is used up and ignored.
    pub(super) fn learn_message(&mut self, msg: controller::Msg) -> bool {
        let preset = if self.controller.editor.learner.control_waited_for().is_some() {
            let Some(preset) = self.preset_being_edited() else {
                self.controller.editor.stop_learning();
                return false;
            };
            Some(preset)
        } else {
            None
        };
        let layer = preset.as_ref().and_then(|p| self.editor_layer(p));
        let existing = |action| {
            let bound = preset.as_ref()?.bindings_of(layer.as_deref(), action);
            bound.into_iter().find(|b| matches!(b.input.src, controller::Src::Cc(_))).map(|b| b.reading)
        };
        match self.controller.editor.learner.feed(msg, existing) {
            Heard::Unused => return false,
            Heard::Ignored => {}
            Heard::Control { action, found } => self.bind_learned(action, layer.as_deref(), found),
            Heard::Layer { name, src } => self.add_learned_layer(&name, src),
            Heard::Detected { action, encoding, clockwise, counter } => self.say_detected(action, encoding, &clockwise, &counter),
        }
        true
    }

    /// Saves the control that was learned for `action`, and says which it is.
    fn bind_learned(&mut self, action: Action, layer: Option<&str>, found: controller::Learned) {
        let Ok(taken_from) = self.edit_preset(|p| p.assign(layer, action, found)) else {
            // Not saved: say so, and do not go on to the next action as if it had been.
            let editor = &mut self.controller.editor;
            editor.stop_learning();
            editor.notice = format!("{}: not saved, see the message above", action.title());
            return;
        };
        let mut notice =
            format!("{}: {}", action.title(), control_text(&Binding { layer: None, input: found.input, reading: found.reading, action }));
        if let Some(other) = taken_from {
            notice.push_str(&format!(" (it was {})", other.title()));
        }
        let editor = &mut self.controller.editor;
        editor.notice = notice;
        editor.learner.bound(action);
    }

    /// Saves the encoding that the clicks of an encoder showed, and says what they showed.
    fn say_detected(&mut self, action: Action, encoding: Option<Encoding>, clockwise: &[u8], counter: &[u8]) {
        let Some(encoding) = encoding else {
            let seen = |v: &[u8]| v.iter().map(u8::to_string).collect::<Vec<_>>().join(" ");
            self.controller.editor.say(format!(
                "Could not tell how it counts (right: {}; left: {}). Is it a knob that sends positions? Choose absolute then.",
                seen(clockwise),
                seen(counter)
            ));
            return;
        };
        let reading = Reading::Relative(encoding);
        if let Some(preset) = self.preset_being_edited() {
            let layer = self.editor_layer(&preset);
            if self.edit_preset(|p| p.set_reading(layer.as_deref(), action, reading)).is_err() {
                self.controller.editor.say(format!("{} counts {}, but this could not be saved.", action.title(), kind_text(reading)));
                return;
            }
        }
        self.controller.editor.say(format!("{} counts {}.", action.title(), kind_text(reading)));
    }

    /// Adds the layer `name` to the preset, selected by the button `src`.
    fn add_learned_layer(&mut self, name: &str, src: controller::Src) {
        match self.edit_preset(|p| p.add_layer(name, src).map(|()| p.layers.len() - 1)) {
            Ok(Ok(index)) => {
                let editor = &mut self.controller.editor;
                editor.say(format!("Layer {name}: {}", src.text()));
                editor.view.layer = index;
                editor.view.adding_layer = false;
                editor.view.new_layer.clear();
                editor.stop_learning();
            }
            Ok(Err(e)) => {
                // The same layer waits for another press.
                self.controller.editor.say(e.to_string());
            }
            Err(EditFailed) => self.controller.editor.stop_learning(),
        }
    }

    /// A new, empty preset for the chosen port, to be taught by pressing the controls.
    fn new_learned_preset(&mut self) {
        let drum = self.midi.connected_port().map(str::to_string);
        let port = if !self.cfg.controller_port.is_empty() {
            Some(self.cfg.controller_port.clone())
        } else {
            self.midi.inputs().iter().find(|p| Some(*p) != drum.as_ref()).cloned()
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
        match controller::PresetStore::new(dir).create("My controller", &text) {
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
        match controller::PresetStore::new(dir).copy(&name, &text) {
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
        let builder =
            ViewportBuilder::default().with_title(title.clone()).with_inner_size([960.0, 780.0]).with_min_inner_size([640.0, 360.0]);
        let closed = ctx.show_viewport_immediate(ViewportId::from_hash_of("controller_editor"), builder, |ctx, class| {
            if class == ViewportClass::Embedded {
                let mut open = true;
                egui::Window::new(title.clone())
                    .id(egui::Id::new("controller_editor_window"))
                    .open(&mut open)
                    .default_size([860.0, 560.0])
                    .resizable(true)
                    .show(ctx, |ui| self.manage_ui(ui));
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
        egui::SidePanel::left("controller_preset_list")
            .resizable(true)
            .default_width(290.0)
            .min_width(190.0)
            .show_inside(ui, |ui| self.preset_list(ui));
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
                ui.label(RichText::new(format!("Delete \"{chosen}\"? Its file is removed.")).color(theme::AMBER));
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
                    if ui
                        .button("Add")
                        .on_hover_text("A new, empty preset for the controller's port; press the controls to bind them")
                        .clicked()
                    {
                        op = Some(Op::NewPreset);
                    }
                    if ui
                        .add_enabled(!chosen.is_empty(), egui::Button::new("Copy"))
                        .on_hover_text("A copy of the chosen preset that can be changed")
                        .clicked()
                    {
                        op = Some(Op::CopyToEdit);
                    }
                    if ui
                        .add_enabled(custom, egui::Button::new("Delete…"))
                        .on_hover_text("Remove the chosen preset (built-in presets cannot be removed)")
                        .clicked()
                    {
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
                    let of_kind: Vec<&String> =
                        self.controller.presets.iter().filter(|n| self.controller.kind_of(n).unwrap_or_default() == kind).collect();
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
            ui.label(RichText::new(self.controller.editor.notice.clone()).small().color(theme::AMBER));
        }
        self.run_op(op);
    }

    /// A new name for the chosen preset of the user's: its file is renamed and the `name` line changed.
    fn rename_preset(&mut self) {
        let Some(old) = self.editable_path() else { return };
        let new = self.controller.editor.view.rename_to.trim().to_string();
        if new == self.cfg.controller_preset {
            self.controller.editor.say("It already has that name.");
            return;
        }
        let Some(dir) = self.preset_dir() else { return };
        match controller::PresetStore::new(dir).rename(&old, &new) {
            Ok(_) => {
                self.refresh_controllers();
                self.choose_preset(new.clone(), true);
                self.controller.editor.say(format!("Renamed to {new}."));
            }
            Err(e @ (controller::RenameError::Name(_) | controller::RenameError::Exists)) => self.controller.editor.say(e.to_string()),
            Err(controller::RenameError::Io(e)) => self.view.error = Some(format!("Could not rename the preset: {e}")),
        }
    }

    /// Deletes the chosen preset of the user's (its file).
    fn delete_preset(&mut self) {
        self.controller.editor.view.confirm_delete = false;
        let Some(path) = self.editable_path() else { return };
        let name = self.cfg.controller_preset.clone();
        match controller::PresetStore::new(path.parent().map(|p| p.to_path_buf()).unwrap_or_default()).delete(&path) {
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
        let Some(path) = self.editable_path() else {
            ui.separator();
            widgets::hint(ui, "Nothing to edit: a built-in preset cannot be changed. Copy it above, or start a new preset, then teach it your controls by pressing them.");
            return;
        };
        let Some(preset) = self.preset_being_edited() else { return };
        ui.separator();
        widgets::hint(ui, format!("Editing {}. Every change is saved at once.", path.display()));
        let mut op = self.layer_rows(ui, &preset);
        let layer = self.editor_layer(&preset);
        if !self.controller.is_connected() {
            ui.label(RichText::new("The controller is not connected: connect it to learn controls.").small().color(theme::AMBER));
        }
        self.waiting_banner(ui);

        let learner = &self.controller.editor.learner;
        let waiting_for = learner.control_waited_for();
        for (section, (title, actions)) in Action::sections().into_iter().enumerate() {
            egui::CollapsingHeader::new(title).default_open(section == 0).id_salt(("controller_editor_section", section)).show(ui, |ui| {
                egui::Grid::new(("controller_editor_grid", section)).num_columns(4).striped(true).show(ui, |ui| {
                    for action in actions {
                        let bound = preset.bindings_of(layer.as_deref(), action);
                        let row = ActionRow {
                            action,
                            bound: &bound,
                            learning: waiting_for == Some(action),
                            unbound_kind: learner.chosen_kind(action),
                        };
                        if let Some(clicked) = action_row(ui, &row) {
                            op = Some(clicked);
                        }
                    }
                });
            });
        }
        if let Some(clicked) = lights_section(ui, &preset, layer.as_deref()) {
            op = Some(clicked);
        }
        self.run_op(op);
    }

    /// The two choices that go with learning, then the layers: which one the table edits, adding one
    /// (named, then its button learned) and deleting one. What was clicked, if anything.
    fn layer_rows(&mut self, ui: &mut egui::Ui, preset: &Preset) -> Option<Op> {
        let mut op = None;
        let connected = self.controller.is_connected();
        let editor = &mut self.controller.editor;
        ui.checkbox(&mut editor.learner.keep_going, "After learning a control, go on to the next action");
        ui.checkbox(&mut editor.learner.remember_channel, "Limit a learned control to the MIDI channel it sends on");
        ui.horizontal(|ui| {
            ui.label("Layer");
            for (i, (name, _)) in preset.layers.iter().enumerate() {
                ui.selectable_value(&mut editor.view.layer, i, name.as_str());
            }
            if preset.layers.is_empty() {
                ui.label(RichText::new("none: every control works on its own").weak());
            }
            if !editor.view.adding_layer
                && ui
                    .small_button("Add layer…")
                    .on_hover_text("A layer is a set of assignments that a button of the device switches to")
                    .clicked()
            {
                op = Some(Op::AddLayer);
            }
            if let Some(name) = preset.layers.get(editor.view.layer).map(|l| l.0.clone()) {
                if editor.view.confirm_layer_delete {
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
        if !editor.view.adding_layer {
            return op;
        }
        let waiting = editor.learner.layer_waited_for().map(str::to_string);
        ui.horizontal(|ui| {
            ui.label("New layer's name");
            ui.add(egui::TextEdit::singleline(&mut editor.view.new_layer).desired_width(80.0).char_limit(24));
            if waiting.is_some() {
                if ui.button("Cancel").clicked() {
                    op = Some(Op::Cancel);
                }
            } else {
                let named = !editor.view.new_layer.trim().is_empty();
                if ui.add_enabled(connected && named, egui::Button::new("Learn its button")).clicked() {
                    op = Some(Op::LearnLayer);
                }
                if ui.small_button("Close").clicked() {
                    op = Some(Op::CloseAddLayer);
                }
            }
        });
        if let Some(name) = waiting {
            ui.label(RichText::new(format!("Press the button that selects layer \"{name}\" on the device…")).strong().color(theme::ACCENT));
        }
        op
    }

    /// What the window asks of the user now: the control to press or the encoder to turn, or else
    /// what it said last.
    fn waiting_banner(&self, ui: &mut egui::Ui) {
        let editor = &self.controller.editor;
        if let Some(prompt) = editor.learner.prompt() {
            ui.label(RichText::new(prompt).strong().color(theme::ACCENT));
        } else if let Some(action) = editor.learner.control_waited_for() {
            ui.label(
                RichText::new(format!("Press or turn the control for \"{}\" on the device…", action.title())).strong().color(theme::ACCENT),
            );
        } else if !editor.notice.is_empty() {
            ui.label(RichText::new(editor.notice.clone()).small());
        }
    }

    fn run_op(&mut self, op: Option<Op>) {
        let Some(op) = op else { return };
        match op {
            Op::Learn(action) => {
                if !self.controller.is_connected() {
                    self.controller.editor.say(CONNECT_FIRST);
                } else {
                    self.controller.editor.start_learning(action);
                }
            }
            Op::Cancel => self.controller.editor.stop_learning(),
            Op::Detect(action) => {
                if !self.controller.is_connected() {
                    self.controller.editor.say(CONNECT_FIRST);
                } else {
                    self.controller.editor.start_detecting(action);
                }
            }
            Op::AddLight(action, ring) => {
                let Some(preset) = self.preset_being_edited() else { return };
                let layer = self.editor_layer(&preset);
                let target = preset
                    .bindings_of(layer.as_deref(), action)
                    .into_iter()
                    .find_map(|b| b.input.src.light_target(ring).map(|out| (b, out)));
                if let Some((b, out)) = target {
                    let mut light = Light::new(layer.as_deref(), action, out, ring);
                    light.channel = b.input.channel.unwrap_or(1);
                    let _ = self.edit_preset(|p| p.set_light(light));
                }
            }
            Op::SetLight(light, settled) => self.set_light(light, settled),
            Op::ClearLight(action, ring) => {
                if let Some(preset) = self.preset_being_edited() {
                    let layer = self.editor_layer(&preset);
                    let _ = self.edit_preset(|p| p.clear_light(layer.as_deref(), action, ring));
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
                    self.controller.editor.say(CONNECT_FIRST);
                } else {
                    self.controller.editor.start_learning_layer(name);
                }
            }
            Op::AskDeleteLayer(ask) => self.controller.editor.view.confirm_layer_delete = ask,
            Op::DeleteLayer(name) => {
                let deleted = self.edit_preset(|p| p.remove_layer(&name)).is_ok();
                let editor = &mut self.controller.editor;
                editor.view.confirm_layer_delete = false;
                if deleted {
                    editor.view.layer = 0;
                    editor.say(format!("Layer {name} deleted."));
                }
            }
            Op::Clear(action) => {
                if let Some(preset) = self.preset_being_edited() {
                    let layer = self.editor_layer(&preset);
                    let _ = self.edit_preset(|p| p.clear(layer.as_deref(), action));
                }
            }
            Op::Kind(action, reading) => {
                self.controller.editor.learner.choose_kind(action, reading);
                if let Some(preset) = self.preset_being_edited() {
                    let layer = self.editor_layer(&preset);
                    let _ = self.edit_preset(|p| p.set_reading(layer.as_deref(), action, reading));
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

/// One action of the table: what it is bound to, and how what it is bound to is read.
struct ActionRow<'a> {
    action: Action,
    bound: &'a [&'a Binding],
    /// The action waits for its control.
    learning: bool,
    /// How a value's cc is read when nothing is bound yet.
    unbound_kind: Reading,
}

/// A row of the table of actions: the title, the controls, how a value is read (for the actions
/// that have one) and the buttons. What was clicked, if anything.
fn action_row(ui: &mut egui::Ui, row: &ActionRow) -> Option<Op> {
    let ActionRow { action, bound, learning, unbound_kind } = *row;
    let mut op = None;
    ui.label(if learning { RichText::new(action.title()).strong() } else { RichText::new(action.title()) }).on_hover_text(action.help());
    let controls = if bound.is_empty() { "—".to_string() } else { bound.iter().map(|b| control_text(b)).collect::<Vec<_>>().join(", ") };
    ui.label(RichText::new(controls).monospace());
    if action.is_level() || action.is_browse() {
        let current = bound.iter().find(|b| matches!(b.input.src, controller::Src::Cc(_))).map(|b| b.reading).unwrap_or(unbound_kind);
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(("controller_kind", action.text())).selected_text(kind_text(current)).width(150.0).show_ui(
                ui,
                |ui| {
                    for reading in KINDS.into_iter().filter(|r| !action.is_browse() || r.is_relative()) {
                        if ui.selectable_label(current == reading, kind_text(reading)).clicked() {
                            op = Some(Op::Kind(action, reading));
                        }
                    }
                },
            );
            if ui
                .small_button("Detect")
                .on_hover_text("Turn the encoder to the right and then to the left to find out how it counts")
                .clicked()
            {
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
    op
}

/// The section of the lights and rings: a row for each action that can have one and has a control.
/// What was clicked, if anything.
fn lights_section(ui: &mut egui::Ui, preset: &Preset, layer: Option<&str>) -> Option<Op> {
    let mut op = None;
    egui::CollapsingHeader::new("Lights and rings").default_open(!preset.lights.is_empty()).id_salt("controller_editor_lights").show(ui, |ui| {
        widgets::hint(ui, "A light shows an action that is on (a button's LED); a ring shows a value around an encoder. Only actions that have a control are listed.");
        egui::Grid::new("controller_editor_lights_grid").num_columns(2).striped(true).show(ui, |ui| {
            for action in action_order() {
                if !(action.has_light() || action.is_level()) {
                    continue;
                }
                let bound = preset.bindings_of(layer, action);
                let shown = preset.lights_of(layer, action, false).is_some() || preset.lights_of(layer, action, true).is_some();
                if bound.is_empty() && !shown {
                    continue;
                }
                ui.label(action.title());
                if let Some(light_op) = light_cell(ui, preset, layer, action, &bound) {
                    op = Some(light_op);
                }
                ui.end_row();
            }
        });
    });
    op
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
                    let mut settled = false;
                    ui.label(RichText::new(format!("{kind}: {}", out_text(light.out))).monospace().small());
                    let mut number = match light.out {
                        controller::Out::Note(n) | controller::Out::Cc(n) => n,
                    };
                    let response = ui
                        .add(egui::DragValue::new(&mut number).range(0..=127).prefix("no. "))
                        .on_hover_text("The note or cc the device is told to light");
                    settled |= response.drag_stopped() || response.lost_focus();
                    if response.changed() {
                        light.out = match light.out {
                            controller::Out::Note(_) => controller::Out::Note(number),
                            controller::Out::Cc(_) => controller::Out::Cc(number),
                        };
                        changed = true;
                    }
                    if !ring {
                        let on = ui
                            .add(egui::DragValue::new(&mut light.on).range(0..=127).prefix("on "))
                            .on_hover_text("The value that lights it (often the colour)");
                        let off = ui
                            .add(egui::DragValue::new(&mut light.off).range(0..=127).prefix("off "))
                            .on_hover_text("The value that darkens it");
                        changed |= on.changed() || off.changed();
                        settled |= on.drag_stopped() || on.lost_focus() || off.drag_stopped() || off.lost_focus();
                    }
                    if changed || settled {
                        op = Some(Op::SetLight(light, settled));
                    }
                    if ui.small_button("×").on_hover_text(format!("Remove the {kind}")).clicked() {
                        op = Some(Op::ClearLight(action, ring));
                    }
                }
                None => {
                    let possible = bound.iter().any(|b| b.input.src.light_target(ring).is_some());
                    let hint = if ring {
                        "Show the value in an LED ring on the same cc as the knob; change the number if the device uses another"
                    } else {
                        "Light the button on the same note or cc it sends; change the number if the device uses another"
                    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Paths};

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

    /// The `controllers` folder of a settings folder that is removed when the guard is dropped.
    fn temp_dir(tag: &str) -> (roll_model_core::testing::TempDir, PathBuf) {
        let guard = roll_model_core::testing::TempDir::new(&format!("editor-{tag}"));
        let controllers = guard.path().join("settings").join("controllers");
        std::fs::create_dir_all(&controllers).unwrap();
        (guard, controllers)
    }

    fn pressed(note: u8) -> controller::Msg {
        controller::Msg::NoteOn { ch: 1, note, vel: 100 }
    }

    #[test]
    fn a_light_and_a_ring_are_added_on_the_control_of_the_action_and_edited() {
        let (_guard, dir) = temp_dir("lights");
        std::fs::write(
            dir.join("Pads.txt"),
            "name = Pads
port = PADS
kind = midi
bind = note 36 ch 3 -> record
bind = cc 13 abs -> tempo
",
        )
        .unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        let saved = || controller::Preset::parse(&std::fs::read_to_string(dir.join("Pads.txt")).unwrap());

        app.run_op(Some(Op::AddLight(Action::Record, false)));
        app.run_op(Some(Op::AddLight(Action::Tempo, true)));
        let light = saved().lights_of(None, Action::Record, false).cloned().unwrap();
        assert_eq!(
            (light.out, light.channel, light.on, light.off),
            (controller::Out::Note(36), 3, 127, 0),
            "on the control of the action, on its channel"
        );
        assert_eq!(saved().lights_of(None, Action::Tempo, true).unwrap().out, controller::Out::Cc(13));
        // An action without a control of the right kind gets nothing.
        app.run_op(Some(Op::AddLight(Action::Stop, false)));
        app.run_op(Some(Op::AddLight(Action::Record, true)));
        assert_eq!(saved().lights.len(), 2);
        // Editing replaces, clearing removes.
        let mut colour = light.clone();
        colour.on = 60;
        colour.out = controller::Out::Note(40);
        app.run_op(Some(Op::SetLight(colour, true)));
        let kept = saved().lights_of(None, Action::Record, false).cloned().unwrap();
        assert_eq!((kept.out, kept.on), (controller::Out::Note(40), 60));
        app.run_op(Some(Op::ClearLight(Action::Record, false)));
        assert!(saved().lights_of(None, Action::Record, false).is_none());
        assert!(saved().warnings.is_empty(), "{:?}", saved().warnings);
        // The table draws with a light, a ring, and the buttons to add one.
        let ctx = egui::Context::default();
        app.controller.editor.view.open = true;
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.controller_editor_window(ctx));
    }

    #[test]
    fn the_preset_is_read_from_its_file_once_for_as_long_as_the_file_is_as_it_was() {
        let (_guard, dir) = temp_dir("read-once");
        let file = dir.join("Pads.txt");
        std::fs::write(
            &file,
            "name = Pads
port = PADS
bind = note 1 -> record
",
        )
        .unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        let ctx = egui::Context::default();
        let frame = |app: &mut DrumApp| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.manage_ui(ui));
            });
        };
        for _ in 0..5 {
            frame(&mut app);
        }
        assert_eq!(app.controller.editor.reads.get(), 1, "five frames, one read");

        // A change made by the window is seen at once, and is read again once.
        app.run_op(Some(Op::Clear(Action::Record)));
        assert!(app.preset_being_edited().unwrap().bindings_of(None, Action::Record).is_empty());
        let reads = app.controller.editor.reads.get();
        for _ in 0..3 {
            frame(&mut app);
        }
        assert_eq!(app.controller.editor.reads.get(), reads, "the window's own change is what the table shows");

        // A change made to the file from outside is seen once the file looks different.
        std::fs::write(
            &file,
            "name = Pads
port = PADS
bind = note 1 -> record
bind = note 2 -> stop
",
        )
        .unwrap();
        assert_eq!(app.preset_being_edited().unwrap().bindings_of(None, Action::Stop).len(), 1);
        assert!(app.controller.editor.reads.get() > reads);
    }

    #[test]
    fn a_light_dragged_through_many_values_is_written_once_when_the_drag_ends() {
        let (_guard, dir) = temp_dir("light-drag");
        std::fs::write(
            dir.join("Pads.txt"),
            "name = Pads
port = PADS
bind = note 36 -> record
",
        )
        .unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        let file = dir.join("Pads.txt");
        let on_disk = || controller::Preset::parse(&std::fs::read_to_string(&file).unwrap());
        app.run_op(Some(Op::AddLight(Action::Record, false)));
        let mut light = on_disk().lights_of(None, Action::Record, false).cloned().unwrap();
        let written = std::fs::read_to_string(&file).unwrap();

        for on in 1..=60 {
            light.on = on;
            app.run_op(Some(Op::SetLight(light.clone(), false))); // sixty frames of a drag
        }
        assert_eq!(std::fs::read_to_string(&file).unwrap(), written, "the file is not touched while the number moves");
        let shown = app.preset_being_edited().unwrap();
        assert_eq!(shown.lights_of(None, Action::Record, false).unwrap().on, 60, "the table shows the value now");

        app.run_op(Some(Op::SetLight(light.clone(), true)));
        assert_eq!(on_disk().lights_of(None, Action::Record, false).unwrap().on, 60, "written when the drag ends");
        assert!(app.controller.editor.draft.is_none());

        // Closing the program in the middle of a drag keeps what was dragged to.
        light.on = 7;
        app.run_op(Some(Op::SetLight(light, false)));
        app.flush_draft();
        assert_eq!(on_disk().lights_of(None, Action::Record, false).unwrap().on, 7);
        // Another change while a drag is pending does not lose it.
        let mut again = on_disk().lights_of(None, Action::Record, false).cloned().unwrap();
        again.on = 9;
        app.run_op(Some(Op::SetLight(again, false)));
        app.run_op(Some(Op::Clear(Action::Stop)));
        assert_eq!(on_disk().lights_of(None, Action::Record, false).unwrap().on, 9);
    }

    #[test]
    fn learning_stops_and_says_so_when_the_preset_cannot_be_saved() {
        let (_guard, dir) = temp_dir("learn-fails");
        std::fs::write(
            dir.join("Pads.txt"),
            "name = Pads
port = PADS
bind = note 1 -> stop
",
        )
        .unwrap();
        std::fs::create_dir(dir.join("Pads.txt.tmp")).unwrap(); // the write has no place to go
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        app.controller.editor.start_learning(Action::Record);
        app.controller.editor.learner.keep_going = true;

        assert!(app.learn_message(pressed(40)), "the press is used up");
        let editor = &app.controller.editor;
        assert!(editor.learner.control_waited_for().is_none(), "it does not go on to the next action as if this one was bound");
        assert!(editor.notice.contains("not saved"), "{}", editor.notice);
        assert!(app.view.error.as_deref().is_some_and(|e| e.contains("Could not save")), "{:?}", app.view.error);
        let kept = controller::Preset::parse(&std::fs::read_to_string(dir.join("Pads.txt")).unwrap());
        assert!(kept.bindings_of(None, Action::Record).is_empty(), "the file is as it was");
    }

    #[test]
    fn an_encoder_is_turned_right_and_left_to_find_out_how_it_counts() {
        let (_guard, dir) = temp_dir("detect");
        std::fs::write(
            dir.join("Pads.txt"),
            "name = Pads
port = PADS
kind = midi
bind = cc 13 rel -> tempo
",
        )
        .unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        let saved = || controller::Preset::parse(&std::fs::read_to_string(dir.join("Pads.txt")).unwrap());
        let cc = |cc: u8, val: u8| controller::Msg::Cc { ch: 1, cc, val };
        let start = |app: &mut DrumApp| app.controller.editor.start_detecting(Action::Tempo);

        start(&mut app);
        assert!(app.controller.editor.learner.prompt().unwrap().contains("RIGHT"));
        // Other controls are used up and ignored, also the first cc that fixed the encoder.
        assert!(app.learn_message(pressed(36)));
        for v in [1, 1, 2] {
            assert!(app.learn_message(cc(13, v)));
        }
        assert!(app.learn_message(cc(14, 99)), "another knob does not count");
        assert!(app.learn_message(cc(13, 1)));
        assert!(app.controller.editor.learner.prompt().unwrap().contains("LEFT"));
        for v in [127, 127, 126, 127] {
            assert!(app.learn_message(cc(13, v)));
        }
        assert!(!app.controller.editor.learner.is_detecting());
        assert_eq!(saved().binds[0].reading, Reading::Relative(Encoding::Twos));
        assert_eq!(app.controller.editor.learner.chosen_kind(Action::Tempo), Reading::Relative(Encoding::Twos));
        assert!(app.controller.editor.notice.contains("twos"), "{}", app.controller.editor.notice);

        // A knob that sends positions is not guessed at: nothing changes, and the numbers are shown.
        start(&mut app);
        for v in [10, 40, 90, 100, 80, 20, 5, 60] {
            app.learn_message(cc(13, v));
        }
        assert!(!app.controller.editor.learner.is_detecting());
        assert_eq!(saved().binds[0].reading, Reading::Relative(Encoding::Twos));
        assert!(
            app.controller.editor.notice.contains("Could not tell") && app.controller.editor.notice.contains("10 40 90 100"),
            "{}",
            app.controller.editor.notice
        );
        // Cancelling stops it.
        start(&mut app);
        app.run_op(Some(Op::Cancel));
        assert!(!app.controller.editor.learner.is_detecting());
    }

    #[test]
    fn a_layer_is_added_by_pressing_its_button_and_removed_with_what_it_does() {
        let (_guard, dir) = temp_dir("layers");
        std::fs::write(
            dir.join("Pads.txt"),
            "name = Pads
port = PADS
bind = note 36 -> record
",
        )
        .unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        let saved = || controller::Preset::parse(&std::fs::read_to_string(dir.join("Pads.txt")).unwrap());

        // The layer waits for a button: a release and a fader are used up and change nothing.
        app.controller.editor.start_learning_layer("B".into());
        assert!(app.learn_message(controller::Msg::NoteOn { ch: 1, note: 85, vel: 0 }));
        assert!(app.learn_message(controller::Msg::Bend { ch: 1, value: 100 }));
        assert!(saved().layers.is_empty() && app.controller.editor.learner.layer_waited_for().is_some());
        // A button that does something cannot select a layer: it says why and waits for another.
        assert!(app.learn_message(pressed(36)));
        assert!(app.controller.editor.notice.contains("Record"), "{}", app.controller.editor.notice);
        assert!(saved().layers.is_empty() && app.controller.editor.learner.layer_waited_for().is_some());
        // A free button does, and the new layer is the one shown.
        assert!(app.learn_message(pressed(85)));
        assert_eq!(saved().layers, vec![("B".to_string(), controller::Src::Note(85))]);
        assert_eq!((app.controller.editor.learner.layer_waited_for().map(str::to_string), app.controller.editor.view.layer), (None, 0));
        // Bindings on the layer go with it.
        app.controller.editor.start_learning(Action::Stop);
        assert!(app.learn_message(pressed(40)));
        assert_eq!(saved().bindings_of(Some("B"), Action::Stop).len(), 1);
        app.run_op(Some(Op::DeleteLayer("B".into())));
        assert!(saved().layers.is_empty() && saved().bindings_of(Some("B"), Action::Stop).is_empty());
        assert_eq!(saved().bindings_of(None, Action::Record).len(), 1, "what works on every layer stays");
        assert!(saved().warnings.is_empty(), "{:?}", saved().warnings);
    }

    #[test]
    fn learning_binds_the_controls_one_after_the_other_and_saves_the_preset() {
        let (_guard, dir) = temp_dir("learn");
        std::fs::write(dir.join("Pads.txt"), "name = Pads\nport = PADS\n").unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        assert!(app.editable_path().is_some());
        let saved = || controller::Preset::parse(&std::fs::read_to_string(dir.join("Pads.txt")).unwrap());

        // Nothing waits for a control: messages are not used up.
        assert!(!app.learn_message(pressed(36)));
        // Record waits; the first press binds it and the editor goes on to Listen.
        app.controller.editor.start_learning(Action::Record);
        assert!(app.learn_message(pressed(36)));
        assert_eq!(app.controller.editor.learner.control_waited_for(), Some(Action::Listen));
        assert_eq!(saved().bindings_of(None, Action::Record)[0].input.src, controller::Src::Note(36));
        // A release is used up and changes nothing; the next press binds Listen.
        assert!(app.learn_message(controller::Msg::NoteOn { ch: 1, note: 36, vel: 0 }));
        assert_eq!(app.controller.editor.learner.control_waited_for(), Some(Action::Listen));
        assert!(app.learn_message(pressed(37)));
        assert_eq!(saved().bindings_of(None, Action::Listen)[0].input.src, controller::Src::Note(37));
        // Pressing a used pad takes it from its action, and says so.
        app.controller.editor.start_learning(Action::Stop);
        assert!(app.learn_message(pressed(36)));
        assert!(app.controller.editor.notice.contains("it was Record"), "{}", app.controller.editor.notice);
        assert!(saved().bindings_of(None, Action::Record).is_empty());
        // Without "go on", learning ends after one control.
        app.controller.editor.learner.keep_going = false;
        app.controller.editor.start_learning(Action::Tempo);
        assert!(app.learn_message(controller::Msg::Cc { ch: 2, cc: 13, val: 40 }));
        assert_eq!(app.controller.editor.learner.control_waited_for(), None);
        let tempo = saved().binds.iter().find(|b| b.action == Action::Tempo).cloned().unwrap();
        assert_eq!((tempo.input.src, tempo.input.channel, tempo.reading), (controller::Src::Cc(13), None, Reading::Absolute));
        // The channel is remembered when asked for, and a relative choice is kept for the knob.
        app.controller.editor.learner.remember_channel = true;
        app.controller.editor.learner.choose_kind(Action::RangeFrom, Reading::Relative(Encoding::Twos));
        app.controller.editor.start_learning(Action::RangeFrom);
        assert!(app.learn_message(controller::Msg::Cc { ch: 2, cc: 14, val: 1 }));
        let from = saved().binds.iter().find(|b| b.action == Action::RangeFrom).cloned().unwrap();
        assert_eq!((from.input.channel, from.reading), (Some(2), Reading::Relative(Encoding::Twos)));
        // The file is a normal preset: it parses without complaint and says what was learned.
        assert!(saved().warnings.is_empty(), "{:?}", saved().warnings);
        assert_eq!(saved().port, "PADS");
    }

    #[test]
    fn a_new_preset_is_made_for_a_port_and_a_built_in_one_is_copied_to_be_edited() {
        let (_guard, dir) = temp_dir("new");
        let mut app = DrumApp::with_config(Config { controller_preset: "Behringer X-Touch Mini".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        app.midi.set_inputs(vec!["Some Pad Controller".into()]);
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
        app.controller.editor.start_learning(Action::Record);
        app.learn_message(controller::Msg::Cc { ch: 1, cc: 99, val: 127 });
        let edited = controller::Preset::parse(&std::fs::read_to_string(dir.join("Korg nanoKONTROL2 (copy).txt")).unwrap());
        assert_eq!(edited.bindings_of(None, Action::Record)[0].input.src, controller::Src::Cc(99));
        let original = controller::Preset::parse(controller::builtin_text("Korg nanoKONTROL2").unwrap());
        assert_eq!(original.bindings_of(None, Action::Record)[0].input.src, controller::Src::Cc(45), "the built-in one is untouched");
    }

    #[test]
    fn a_preset_can_be_renamed_and_deleted_and_a_built_in_one_cannot() {
        let (_guard, dir) = temp_dir("rename");
        std::fs::write(dir.join("Pads.txt"), "# my notes\nname = Pads\nport = PADS\nbind = note 1 -> record\n").unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        // A bad name changes nothing and says why.
        app.controller.editor.view.rename_to = "a/b".into();
        app.rename_preset();
        assert!(
            dir.join("Pads.txt").exists() && app.controller.editor.notice.contains("cannot contain"),
            "{}",
            app.controller.editor.notice
        );
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
    }

    #[test]
    fn clicking_a_preset_in_the_list_chooses_it_and_shows_its_assignments() {
        let (_guard, dir) = temp_dir("choose");
        std::fs::write(dir.join("Pads.txt"), "name = Pads\nport = PADS\nbind = note 1 -> record\n").unwrap();
        let mut app = DrumApp::with_config(Config {
            controller_preset: "Korg nanoKONTROL2".into(),
            controller_port: "X".into(),
            ..Config::default()
        });
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        app.controller.editor.start_learning(Action::Record);
        app.controller.editor.view.confirm_delete = true;
        app.select_preset("Pads".into());
        assert_eq!(app.cfg.controller_preset, "Pads");
        assert!(app.cfg.controller_port.is_empty(), "the preset's own port is used");
        assert!(app.controller.editor.learner.control_waited_for().is_none() && !app.controller.editor.view.confirm_delete);
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
    }

    #[test]
    fn a_new_preset_needs_a_port() {
        let (_guard, dir) = temp_dir("noport");
        let mut app = DrumApp::with_config(Config::default());
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        app.new_learned_preset();
        assert!(app.controller.editor.notice.contains("No MIDI input"), "{}", app.controller.editor.notice);
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none(), "nothing was written");
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
        let (_guard, dir) = temp_dir("window");
        app.controller.editor.view.open = false;
        app.paths = Paths::in_dir(dir.parent().unwrap().to_path_buf());
        app.midi.set_inputs(vec!["Some Pad Controller".into()]);
        app.new_learned_preset();
        assert!(app.controller.editor.view.open, "a new preset opens the editor");
        frame(&mut app);
        // The layer rows draw while a layer is being added, waited for and deleted.
        app.controller.editor.view.adding_layer = true;
        app.controller.editor.view.new_layer = "B".into();
        frame(&mut app);
        app.controller.editor.start_learning_layer("B".into());
        frame(&mut app);
        app.controller.editor.stop_learning();
        app.edit_preset(|p| p.add_layer("B", controller::Src::Note(85)).unwrap()).unwrap();
        app.controller.editor.view.confirm_layer_delete = true;
        frame(&mut app);
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
