//! The preset editor on the Controller page: a table of the program's actions with the control
//! each is bound to, and *Learn* (press or turn a control on the device to bind it). The preset
//! being edited is a file in the controllers folder; every change is written at once and put in
//! place in the running controller. The built-in presets are not edited: copy one first, or start
//! a new preset. The editing itself (`assign`, `clear`, `to_text`) is in `controller.rs`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{RichText, ViewportBuilder, ViewportClass, ViewportId};

use super::*;
use crate::controller::{Binding, Encoding, Preset};

/// What the editor remembers between frames.
pub(super) struct EditorState {
    /// The action that waits for the next control the device sends.
    pub(super) learning: Option<Action>,
    /// The layer being edited (an index of the preset's layers).
    pub(super) layer: usize,
    /// After a control is learned, wait for one for the next action in the list.
    pub(super) keep_going: bool,
    /// Limit a learned control to the MIDI channel it sent on.
    pub(super) remember_channel: bool,
    /// How a value's cc is read, chosen before it is learned (by the action's name).
    pub(super) kinds: BTreeMap<String, (bool, Encoding)>,
    pub(super) notice: String,
    /// The editor window is open.
    pub(super) open: bool,
    /// The folder of the user's presets; `None`: the settings folder (tests use another).
    pub(super) dir: Option<PathBuf>,
}

impl Default for EditorState {
    fn default() -> EditorState {
        EditorState { learning: None, layer: 0, keep_going: true, remember_channel: false, kinds: BTreeMap::new(), notice: String::new(), open: false, dir: None }
    }
}

/// What a click on the page asks for; done after the table is drawn.
enum Op {
    Learn(Action),
    Cancel,
    Clear(Action),
    Kind(Action, bool, Encoding),
    NewPreset,
    CopyToEdit,
}

const KINDS: [(bool, Encoding); 5] = [
    (false, Encoding::Mackie),
    (true, Encoding::Mackie),
    (true, Encoding::Twos),
    (true, Encoding::Offset),
    (true, Encoding::SignBit2),
];

fn kind_text(relative: bool, encoding: Encoding) -> String {
    if relative {
        format!("relative, {}", encoding.word())
    } else {
        "absolute".to_string()
    }
}

fn control_text(b: &Binding) -> String {
    match b.channel {
        Some(ch) => format!("{} ch {ch}", b.src.text()),
        None => b.src.text(),
    }
}

/// The actions in the order the editor lists them.
fn action_order() -> Vec<Action> {
    Action::sections().into_iter().flat_map(|(_, actions)| actions).collect()
}

impl DrumApp {
    fn preset_dir(&self) -> Option<PathBuf> {
        self.controller.editor.dir.clone().or_else(Config::controllers_dir)
    }

    /// The file of the chosen preset if it is one the user may edit: not a built-in one.
    pub(super) fn editable_path(&self) -> Option<PathBuf> {
        let name = &self.cfg.controller_preset;
        if name.is_empty() || controller::builtin_text(name).is_some() {
            return None;
        }
        let dir = self.preset_dir()?;
        controller::custom_presets(&dir).into_iter().find(|(n, _)| n == name).map(|(_, path)| path)
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
                self.error = Some(e);
                return None;
            }
        };
        let result = change(&mut preset);
        if let Err(e) = std::fs::write(&path, preset.to_text()) {
            self.error = Some(format!("Could not save {}: {e}", path.display()));
            return None;
        }
        if let Some(link) = self.controller.link.as_mut() {
            link.mapper.replace_preset(preset);
        }
        Some(result)
    }

    /// The layer the table edits: `None` for a preset without layers.
    fn editor_layer(&self, preset: &Preset) -> Option<String> {
        preset.layers.get(self.controller.editor.layer).map(|l| l.0.clone())
    }

    /// While an action waits for its control, the next message from the device is taken for it
    /// (and never acts). True if the message was used up. A message that cannot be the control
    /// of that action (a release, a fader for a button) is used up and ignored.
    pub(super) fn learn_message(&mut self, msg: controller::Msg) -> bool {
        let Some(action) = self.controller.editor.learning else { return false };
        let Some(preset) = self.preset_being_edited() else {
            self.controller.editor.learning = None;
            return false;
        };
        let layer = self.editor_layer(&preset);
        let existing = preset.bindings_of(layer.as_deref(), action).into_iter().find(|b| matches!(b.src, controller::Src::Cc(_))).map(|b| (b.relative, b.encoding));
        let (relative, encoding) = existing.or_else(|| self.controller.editor.kinds.get(&action.text()).copied()).unwrap_or((action.is_browse(), Encoding::Mackie));
        let Some(found) = controller::learn(msg, action, relative, encoding, self.controller.editor.remember_channel) else { return true };
        let taken_from = self.edit_preset(|p| p.assign(layer.as_deref(), action, found)).flatten();
        let mut notice = format!("{}: {}", action.title(), control_text(&Binding { layer: None, src: found.src, channel: found.channel, relative: found.relative, encoding: found.encoding, action }));
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

    /// A new, empty preset for the chosen port, to be taught by pressing the controls.
    fn new_learned_preset(&mut self) {
        let drum = self.midi_in.as_ref().map(|h| h.port_name.clone());
        let port = if !self.cfg.controller_port.is_empty() {
            Some(self.cfg.controller_port.clone())
        } else {
            self.midi_inputs.iter().find(|p| Some(*p) != drum.as_ref()).cloned()
        };
        let Some(port) = port else {
            self.controller.editor.notice = "No MIDI input to use. Plug the controller in, then use Refresh and reconnect.".into();
            return;
        };
        let Some(dir) = self.preset_dir() else {
            self.error = Some("No settings folder is available on this system.".into());
            return;
        };
        let text = format!("name = My controller\nport = {port}\n");
        match unused_path(&dir, "My controller").and_then(|path| std::fs::write(&path, text).map(|_| path)) {
            Ok(path) => self.use_new_preset(&path, &format!("Saved {}", path.display())),
            Err(e) => self.error = Some(format!("Could not create the preset: {e}")),
        }
    }

    /// A copy of the chosen preset (built-in or not) that can be edited.
    fn copy_to_edit(&mut self) {
        let name = self.cfg.controller_preset.clone();
        let Some(dir) = self.preset_dir() else {
            self.error = Some("No settings folder is available on this system.".into());
            return;
        };
        let text = match controller::builtin_text(&name) {
            Some(text) => text.to_string(),
            None => match controller::custom_presets(&dir).into_iter().find(|(n, _)| *n == name).map(|(_, p)| std::fs::read_to_string(p)) {
                Some(Ok(text)) => text,
                _ => {
                    self.error = Some(format!("Could not read the preset \"{name}\"."));
                    return;
                }
            },
        };
        match copy_preset(&dir, &name, &text) {
            Ok(path) => self.use_new_preset(&path, &format!("Copied to {}", path.display())),
            Err(e) => self.error = Some(format!("Could not copy the preset: {e}")),
        }
    }

    /// Chooses the preset in `path` (just created) and connects it.
    fn use_new_preset(&mut self, path: &std::path::Path, status: &str) {
        self.refresh_controllers();
        if let Some(stem) = path.file_stem() {
            self.cfg.controller_preset = stem.to_string_lossy().into_owned();
            self.cfg.controller_port.clear();
        }
        self.status = status.to_string();
        let dir = self.controller.editor.dir.take();
        self.controller.editor = EditorState { dir, open: true, ..EditorState::default() };
        self.connect_controller();
    }

    /// The Assignments section of the Controller page: what the editor is for, and the buttons
    /// that open it. The editor itself is a window of its own (`controller_editor_window`).
    pub(super) fn controller_assignments_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.heading("Assignments");
        ui.label(
            RichText::new("Teach the program which control does what: press Learn, then the control on the device. The editor opens in a window of its own and every change is saved to the preset file at once. The built-in presets are copied first.")
                .small()
                .weak(),
        );
        let mut op: Option<Op> = None;
        ui.horizontal(|ui| {
            if ui.button("Edit assignments…").clicked() {
                self.controller.editor.open = true;
            }
            if ui.add_enabled(!self.cfg.controller_preset.is_empty(), egui::Button::new("Copy the chosen preset to edit")).clicked() {
                op = Some(Op::CopyToEdit);
            }
            if ui.button("New preset by learning").clicked() {
                op = Some(Op::NewPreset);
            }
        });
        if !self.controller.editor.notice.is_empty() && !self.controller.editor.open {
            ui.label(RichText::new(self.controller.editor.notice.clone()).small().color(render::AMBER));
        }
        self.run_op(op);
    }

    /// The editor window, while it is open: a window of its own (inside the main window where the
    /// platform has no second window).
    pub(in crate::app) fn controller_editor_window(&mut self, ctx: &egui::Context) {
        if !self.controller.editor.open {
            return;
        }
        let name = if self.cfg.controller_preset.is_empty() { "no preset".to_string() } else { self.cfg.controller_preset.clone() };
        let title = format!("Controller assignments: {name}");
        let builder = ViewportBuilder::default().with_title(title.clone()).with_inner_size([660.0, 780.0]).with_min_inner_size([480.0, 320.0]);
        let closed = ctx.show_viewport_immediate(ViewportId::from_hash_of("controller_editor"), builder, |ctx, class| {
            if class == ViewportClass::Embedded {
                let mut open = true;
                egui::Window::new(title.clone()).id(egui::Id::new("controller_editor_window")).open(&mut open).default_width(620.0).vscroll(true).show(ctx, |ui| self.editor_body(ui));
                !open
            } else {
                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.editor_body(ui));
                });
                ctx.input(|i| i.viewport().close_requested())
            }
        });
        if closed {
            self.controller.editor.open = false;
            self.controller.editor.learning = None;
        }
    }

    /// The table of actions (the contents of the editor window).
    fn editor_body(&mut self, ui: &mut egui::Ui) {
        let mut op: Option<Op> = None;
        let Some(path) = self.editable_path() else {
            ui.label(RichText::new("The built-in presets cannot be edited. Copy one to change its assignments, or start a new preset and teach it your controls by pressing them.").small().weak());
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.cfg.controller_preset.is_empty(), egui::Button::new("Copy the chosen preset to edit")).clicked() {
                    op = Some(Op::CopyToEdit);
                }
                if ui.button("New preset by learning").clicked() {
                    op = Some(Op::NewPreset);
                }
            });
            self.run_op(op);
            if !self.controller.editor.notice.is_empty() {
                ui.label(RichText::new(self.controller.editor.notice.clone()).small().color(render::AMBER));
            }
            return;
        };
        let Some(preset) = self.preset_being_edited() else { return };
        let connected = self.controller.link.is_some();
        ui.label(RichText::new(format!("Editing {}. Every change is saved at once.", path.display())).small().weak());
        ui.horizontal(|ui| {
            if ui.button("Copy this preset").on_hover_text("Make a copy to try something without changing this one").clicked() {
                op = Some(Op::CopyToEdit);
            }
            if ui.button("New preset by learning").clicked() {
                op = Some(Op::NewPreset);
            }
        });
        ui.checkbox(&mut self.controller.editor.keep_going, "After learning a control, go on to the next action");
        ui.checkbox(&mut self.controller.editor.remember_channel, "Limit a learned control to the MIDI channel it sends on");
        if preset.layers.len() > 1 {
            ui.horizontal(|ui| {
                ui.label("Layer");
                for (i, (name, _)) in preset.layers.iter().enumerate() {
                    ui.selectable_value(&mut self.controller.editor.layer, i, name.as_str());
                }
            });
        }
        let layer = self.editor_layer(&preset);
        if !connected {
            ui.label(RichText::new("The controller is not connected: connect it to learn controls.").small().color(render::AMBER));
        }
        if let Some(action) = self.controller.editor.learning {
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
                                .find(|b| matches!(b.src, controller::Src::Cc(_)))
                                .map(|b| (b.relative, b.encoding))
                                .or_else(|| self.controller.editor.kinds.get(&action.text()).copied())
                                .unwrap_or((action.is_browse(), Encoding::Mackie));
                            egui::ComboBox::from_id_salt(("controller_kind", action.text())).selected_text(kind_text(current.0, current.1)).width(150.0).show_ui(ui, |ui| {
                                for (relative, encoding) in KINDS {
                                    if action.is_browse() && !relative {
                                        continue;
                                    }
                                    if ui.selectable_label(current == (relative, encoding), kind_text(relative, encoding)).clicked() {
                                        op = Some(Op::Kind(action, relative, encoding));
                                    }
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
        self.run_op(op);
    }

    fn run_op(&mut self, op: Option<Op>) {
        let Some(op) = op else { return };
        match op {
            Op::Learn(action) => {
                if self.controller.link.is_none() {
                    self.controller.editor.notice = "Connect the controller first (Refresh and reconnect, above).".into();
                } else {
                    self.controller.editor.learning = Some(action);
                    self.controller.editor.notice.clear();
                }
            }
            Op::Cancel => self.controller.editor.learning = None,
            Op::Clear(action) => {
                if let Some(preset) = self.preset_being_edited() {
                    let layer = self.editor_layer(&preset);
                    self.edit_preset(|p| p.clear(layer.as_deref(), action));
                }
            }
            Op::Kind(action, relative, encoding) => {
                self.controller.editor.kinds.insert(action.text(), (relative, encoding));
                if let Some(preset) = self.preset_being_edited() {
                    let layer = self.editor_layer(&preset);
                    self.edit_preset(|p| p.set_kind(layer.as_deref(), action, relative, encoding));
                }
            }
            Op::NewPreset => self.new_learned_preset(),
            Op::CopyToEdit => self.copy_to_edit(),
        }
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
        assert_eq!(kind_text(false, Encoding::Mackie), "absolute");
        assert_eq!(kind_text(true, Encoding::Twos), "relative, twos");
        assert_eq!(KINDS.iter().filter(|(relative, _)| !relative).count(), 1);
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
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn pressed(note: u8) -> controller::Msg {
        controller::Msg::NoteOn { ch: 1, note, vel: 100 }
    }

    #[test]
    fn learning_binds_the_controls_one_after_the_other_and_saves_the_preset() {
        let dir = temp_dir("learn");
        std::fs::write(dir.join("Pads.txt"), "name = Pads\nport = PADS\n").unwrap();
        let mut app = DrumApp::with_config(Config { controller_preset: "Pads".into(), ..Config::default() });
        app.controller.editor.dir = Some(dir.clone());
        assert!(app.editable_path().is_some());
        let saved = || controller::Preset::parse(&std::fs::read_to_string(dir.join("Pads.txt")).unwrap());

        // Nothing waits for a control: messages are not used up.
        assert!(!app.learn_message(pressed(36)));
        // Record waits; the first press binds it and the editor goes on to Listen.
        app.controller.editor.learning = Some(Action::Record);
        assert!(app.learn_message(pressed(36)));
        assert_eq!(app.controller.editor.learning, Some(Action::Listen));
        assert_eq!(saved().bindings_of(None, Action::Record)[0].src, controller::Src::Note(36));
        // A release is used up and changes nothing; the next press binds Listen.
        assert!(app.learn_message(controller::Msg::NoteOn { ch: 1, note: 36, vel: 0 }));
        assert_eq!(app.controller.editor.learning, Some(Action::Listen));
        assert!(app.learn_message(pressed(37)));
        assert_eq!(saved().bindings_of(None, Action::Listen)[0].src, controller::Src::Note(37));
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
        assert_eq!((tempo.src, tempo.channel, tempo.relative), (controller::Src::Cc(13), None, false));
        // The channel is remembered when asked for, and a relative choice is kept for the knob.
        app.controller.editor.remember_channel = true;
        app.controller.editor.kinds.insert(Action::RangeFrom.text(), (true, Encoding::Twos));
        app.controller.editor.learning = Some(Action::RangeFrom);
        assert!(app.learn_message(controller::Msg::Cc { ch: 2, cc: 14, val: 1 }));
        let from = saved().binds.iter().find(|b| b.action == Action::RangeFrom).cloned().unwrap();
        assert_eq!((from.channel, from.relative, from.encoding), (Some(2), true, Encoding::Twos));
        // The file is a normal preset: it parses without complaint and says what was learned.
        assert!(saved().warnings.is_empty(), "{:?}", saved().warnings);
        assert_eq!(saved().port, "PADS");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_preset_is_made_for_a_port_and_a_built_in_one_is_copied_to_be_edited() {
        let dir = temp_dir("new");
        let mut app = DrumApp::with_config(Config { controller_preset: "Behringer X-Touch Mini".into(), ..Config::default() });
        app.controller.editor.dir = Some(dir.clone());
        app.midi_inputs = vec!["Some Pad Controller".into()];
        assert!(app.editable_path().is_none(), "a built-in preset is not edited");
        app.new_learned_preset();
        assert_eq!(app.cfg.controller_preset, "My controller");
        let text = std::fs::read_to_string(dir.join("My controller.txt")).unwrap();
        assert!(text.contains("port = Some Pad Controller"), "{text}");
        assert!(app.editable_path().is_some());
        assert_eq!(app.controller.editor.dir, Some(dir.clone()), "the folder is kept");
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
        assert_eq!(edited.bindings_of(None, Action::Record)[0].src, controller::Src::Cc(99));
        let original = controller::Preset::parse(controller::builtin_text("Korg nanoKONTROL2").unwrap());
        assert_eq!(original.bindings_of(None, Action::Record)[0].src, controller::Src::Cc(45), "the built-in one is untouched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_preset_needs_a_port() {
        let dir = temp_dir("noport");
        let mut app = DrumApp::with_config(Config::default());
        app.controller.editor.dir = Some(dir.clone());
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
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.controller_assignments_section(ui));
                app.controller_editor_window(ctx);
            });
        };
        frame(&mut app);
        assert!(!app.controller.editor.open, "closed until asked for");
        app.controller.editor.open = true;
        frame(&mut app);
        frame(&mut app);
        assert!(app.controller.editor.open, "it stays open");
        // Making a preset opens the editor on it.
        let dir = temp_dir("window");
        app.controller.editor.open = false;
        app.controller.editor.dir = Some(dir.clone());
        app.midi_inputs = vec!["Some Pad Controller".into()];
        app.new_learned_preset();
        assert!(app.controller.editor.open, "a new preset opens the editor");
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
                egui::CentralPanel::default().show(ctx, |ui| app.editor_body(ui));
            });
        }
        assert!(app.editable_path().is_none(), "the Mini is built in");
        app.cfg.controller_preset = String::new();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.editor_body(ui));
        });
        // A message while nothing is being learned is not used up.
        assert!(!app.learn_message(controller::Msg::NoteOn { ch: 1, note: 1, vel: 100 }));
    }
}
