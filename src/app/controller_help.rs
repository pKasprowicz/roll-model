//! The controller help screen: a picture of the controller drawn from its layout
//! (`controller_layout.rs`), each control labelled with what the preset makes it do on the
//! chosen layer. It opens from the controller indicator at the top of the window.

use egui::{Align, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use super::DrumApp;
use crate::controller::{self, Preset};
use crate::controller_layout::{self as layout, Control, ControlKind, Layout, Mapping, Meaning, Printed};
use crate::render;
use crate::widgets;

const BODY: Color32 = Color32::from_rgb(0x2a, 0x2c, 0x31);
const KEY: Color32 = Color32::from_rgb(0xe6, 0xe6, 0xea);
const KEY_UNUSED: Color32 = Color32::from_rgb(0x42, 0x44, 0x4a);
const DARK_TEXT: Color32 = Color32::from_rgb(0x1d, 0x1e, 0x22);
const LIGHT_TEXT: Color32 = Color32::from_rgb(0xee, 0xee, 0xf0);
const DIM_TEXT: Color32 = Color32::from_rgb(0x9a, 0x9c, 0xa3);
const RING_OFF: Color32 = Color32::from_rgb(0x55, 0x57, 0x5e);
/// Positions of an LED ring (0 to 10), spread over three quarters of a circle.
const RING_DOTS: usize = 11;
/// The smallest the picture is drawn, in screen pixels per layout unit.
const MIN_SCALE: f32 = 0.6;

/// An open help screen.
pub(super) struct ControllerHelp {
    /// The preset chosen in the settings when it was opened; the screen closes when it changes.
    chosen: String,
    preset: Preset,
    layout: Option<Layout>,
    /// The layer shown (an index of `preset.layers`).
    layer: usize,
}

impl DrumApp {
    /// Opens the help screen, or closes it if it is open: the click on the controller indicator.
    pub(super) fn toggle_controller_help(&mut self) {
        if self.controller_help.take().is_some() {
            return;
        }
        let (preset, layer) = match self.controller.live() {
            Some((preset, layer)) => (preset.clone(), layer.map(str::to_string)),
            None => match controller::load_preset(&self.cfg.controller_preset, self.paths.controllers_dir().as_deref()) {
                Ok(preset) => (preset, None),
                Err(e) => {
                    self.error = Some(e.to_string());
                    return;
                }
            },
        };
        let layer = layer.and_then(|l| preset.layers.iter().position(|(name, _)| *name == l)).unwrap_or(0);
        self.controller_help = Some(ControllerHelp { chosen: self.cfg.controller_preset.clone(), layout: layout::for_preset(&preset).or_else(|| layout::generated(&preset)), preset, layer });
    }

    pub(super) fn controller_help_window(&mut self, ctx: &egui::Context) {
        let Some(mut help) = self.controller_help.take() else { return };
        if help.chosen != self.cfg.controller_preset {
            return;
        }
        let live_layer = self.controller.live().and_then(|(_, layer)| layer.map(str::to_string));
        let connected = self.controller.live().is_some();
        let mut open = true;
        let width = (ctx.screen_rect().width() - 40.0).clamp(480.0, 960.0);
        egui::Window::new(format!("{} — what the controls do", help.preset.name))
            .id(egui::Id::new("controller_help"))
            .open(&mut open)
            .collapsible(false)
            .default_width(width)
            .vscroll(true)
            .default_height((ctx.screen_rect().height() - 80.0).max(300.0))
            .default_pos(egui::pos2(20.0, 50.0))
            .show(ctx, |ui| help.show(ui, connected, live_layer.as_deref()));
        if open {
            self.controller_help = Some(help);
        }
    }
}

impl ControllerHelp {
    fn show(&mut self, ui: &mut egui::Ui, connected: bool, live_layer: Option<&str>) {
        ui.horizontal(|ui| {
            if self.preset.layers.len() > 1 {
                ui.label("Layer");
                for (i, (name, _)) in self.preset.layers.iter().enumerate() {
                    ui.selectable_value(&mut self.layer, i, name.as_str());
                }
            }
            let state = match (connected, live_layer) {
                (true, Some(l)) => format!("The controller is connected and on layer {l}."),
                (true, None) => "The controller is connected.".to_string(),
                (false, _) => "The controller is not connected.".to_string(),
            };
            widgets::hint(ui, format!("{}. {state}", self.preset.kind.label()));
        });
        let layer = self.preset.layers.get(self.layer).map(|l| l.0.clone());
        match &self.layout {
            Some(layout) => {
                if let Some(chosen) = draw(ui, &self.preset, layout, layer.as_deref(), self.layer) {
                    self.layer = chosen;
                }
                notes(ui, &self.preset, layout, layer.as_deref());
            }
            None => {
                widgets::hint(ui, "This preset binds no controls yet. Teach it some under Assignments on the Controller page.");
                egui::Grid::new("controller_help_list").num_columns(2).striped(true).show(ui, |ui| {
                    for b in self.preset.binds.iter().filter(|b| b.layer.is_none() || b.layer == layer) {
                        ui.label(format!("{}{}", b.input.src.text(), if b.reading.is_relative() { " (turn)" } else { "" }));
                        ui.label(b.action.title());
                        ui.end_row();
                    }
                });
            }
        }
    }
}

/// The lines under the picture.
fn notes(ui: &mut egui::Ui, preset: &Preset, layout: &Layout, layer: Option<&str>) {
    ui.add_space(4.0);
    let small = |ui: &mut egui::Ui, text: String| {
        widgets::hint(ui, text);
    };
    if layout.generated {
        small(ui, "This picture is made from the preset: knobs, faders and buttons in order of their message numbers. It does not show where they are on the device.".into());
    }
    small(ui, "Hover a control to see what it does. A green dot on a button: its light follows the program. Amber dots around an encoder: its LED ring shows the value.".into());
    let shift = layout.controls.iter().find(|c| matches!(layout::mapping(preset, layer, c).press, Some(Meaning::Action(controller::Action::Shift))));
    if let Some(shift) = shift {
        let at = if shift.name.eq_ignore_ascii_case("shift") { String::new() } else { format!(" ({})", shift.name.to_lowercase()) };
        small(ui, format!("Hold Shift{at} while turning an encoder: every step counts five times."));
    }
    let lost: Vec<String> = layout
        .unplaced(preset)
        .iter()
        .filter(|b| b.layer.is_none() || b.layer.as_deref() == layer)
        .map(|b| format!("{} ({})", b.input.src.text(), b.action.title()))
        .collect();
    if !lost.is_empty() {
        ui.label(RichText::new(format!("The preset also binds {}, but the picture does not show where those are.", lost.join(", "))).small().color(render::AMBER));
    }
    small(ui, "On the Rudiments tab the two measure encoders set the repetitions and the lead hand, and the range buttons step through the rudiments.".into());
    small(ui, format!("Not yet tried on a real {}: press a control and read the monitor on the Controller page of Settings; if a number differs, change it in the preset file.", layout.name.trim_start_matches("Behringer ")));
}

/// Draws the controller and returns the layer whose button was clicked, if one was.
fn draw(ui: &mut egui::Ui, preset: &Preset, layout: &Layout, layer: Option<&str>, shown: usize) -> Option<usize> {
    // As wide as the window allows, but a tall device (the Compact) is made to fit the screen's height too;
    // below `MIN_SCALE` the texts get too small, and the window scrolls instead.
    let max_height = (ui.ctx().screen_rect().height() - 250.0).max(300.0);
    let scale = (ui.available_width().min(960.0).max(300.0) / layout.size.0).min(max_height / layout.size.1).max(MIN_SCALE);
    let width = layout.size.0 * scale;
    let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), layout.size.1 * scale), Sense::hover());
    let area = Rect::from_center_size(row.center(), Vec2::new(width, layout.size.1 * scale));
    let paint = Paint { painter: ui.painter_at(area), scale };
    let at = |x: f32, y: f32| area.min + paint.vec(x, y);
    let mut chosen = None;

    paint.painter.rect_filled(area, paint.px(20.0), BODY);
    let title = match layer {
        Some(l) => format!("{} · layer {l}", layout.name),
        None => layout.name.to_string(),
    };
    paint.text(Rect::from_min_size(at(36.0, layout.size.1 - 44.0), paint.vec(520.0, 30.0)), &title, 15.0, DIM_TEXT, Align::Min);
    let layers: Vec<&Control> = layout.controls.iter().filter(|c| c.kind == ControlKind::Layer).collect();
    if let [a, b] = layers[..] {
        let middle = ((a.center.0 + b.center.0) / 2.0, (a.center.1 + b.center.1) / 2.0);
        paint.text(Rect::from_center_size(at(middle.0, middle.1), paint.vec(64.0, 16.0)), "LAYER", 10.0, DIM_TEXT, Align::Center);
        // The arrows printed beside them: up for A, down for B.
        for (control, up) in [(a, true), (b, false)] {
            paint.triangle(at(control.center.0 - control.size.0 / 2.0 - 9.0, control.center.1), paint.px(5.0), if up { Dir::Up } else { Dir::Down }, DIM_TEXT);
        }
    }
    for mark in &layout.marks {
        paint.symbol(at(mark.at.0, mark.at.1), mark.what);
    }

    for (i, control) in layout.controls.iter().enumerate() {
        let rect = Rect::from_center_size(at(control.center.0, control.center.1), paint.vec(control.size.0, control.size.1));
        let mapping = layout::mapping(preset, layer, control);
        let sense = if control.kind == ControlKind::Layer { Sense::click() } else { Sense::hover() };
        let response = ui.interact(rect, ui.id().with(("controller_control", i)), sense);
        let outline = if response.hovered() { Stroke::new(2.0 * scale.max(0.7), render::ACCENT) } else { Stroke::NONE };
        match control.kind {
            ControlKind::Button => paint.button(rect, preset, layer, &mapping, outline),
            ControlKind::Layer => {
                let this = preset.layers.iter().position(|(_, src)| Some(*src) == control.press);
                let selected = this == Some(shown);
                let (fill, text) = if selected { (render::ACCENT, Color32::WHITE) } else { (Color32::from_rgb(0x5a, 0x5d, 0x66), LIGHT_TEXT) };
                paint.painter.rect(rect, paint.px(6.0), fill, outline);
                let letter = control.name.rsplit(' ').next().unwrap_or("");
                paint.text(rect, letter, 17.0, text, Align::Center);
                if response.clicked() {
                    chosen = this;
                }
            }
            ControlKind::Encoder => paint.encoder(rect, preset, layer, &mapping, outline),
            ControlKind::Fader => paint.fader(rect, &mapping, outline),
            ControlKind::Indicator => paint.indicator(rect, &control.name),
        }
        response.on_hover_ui_at_pointer(|ui| tooltip(ui, control, &mapping));
    }
    chosen
}

fn title_of(meaning: &Option<Meaning>) -> Option<String> {
    match meaning {
        Some(Meaning::Action(a)) => Some(a.title()),
        Some(Meaning::Layer(l)) => Some(format!("Layer {l}")),
        None => None,
    }
}

#[derive(Clone, Copy)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// What the drawing functions share: the painter and how many pixels a layout unit is.
struct Paint {
    painter: egui::Painter,
    scale: f32,
}

impl Paint {
    /// Pixels for `units` of the layout.
    fn px(&self, units: f32) -> f32 {
        units * self.scale
    }

    fn vec(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new(x, y) * self.scale
    }

    /// A font `size` layout units tall, never smaller than 8 px.
    fn font(&self, size: f32) -> egui::FontId {
        egui::FontId::proportional((size * self.scale).max(8.0))
    }

    /// `text` in `rect`, wrapped, in `align`, with its middle at the middle of `rect`.
    fn text(&self, rect: Rect, text: &str, size: f32, color: Color32, align: Align) {
        if text.is_empty() {
            return;
        }
        let mut job = egui::text::LayoutJob::simple(text.to_string(), self.font(size), color, rect.width());
        job.halign = align;
        let galley = self.painter.layout_job(job);
        // Centred text is laid out around the x of the position it is placed at.
        let x = if align == Align::Center { rect.center().x } else { rect.left() };
        let top_left = Pos2::new(x, rect.center().y - galley.size().y / 2.0);
        self.painter.galley(top_left, galley, color);
    }

    /// A small filled triangle with its middle at `at`, `r` pixels from the middle to each corner.
    fn triangle(&self, at: Pos2, r: f32, dir: Dir, color: Color32) {
        let (dx, dy) = match dir {
            Dir::Up => (0.0, -1.0),
            Dir::Down => (0.0, 1.0),
            Dir::Left => (-1.0, 0.0),
            Dir::Right => (1.0, 0.0),
        };
        let tip = Vec2::new(dx, dy);
        let side = Vec2::new(-dy, dx);
        let points = vec![at + tip * r, at - tip * r * 0.8 + side * r * 0.9, at - tip * r * 0.8 - side * r * 0.9];
        self.painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
    }

    /// What is printed beside a button, drawn as vectors (the fonts lack these symbols).
    fn symbol(&self, at: Pos2, what: Printed) {
        let r = self.px(5.0);
        match what {
            Printed::Text(text) => self.text(Rect::from_center_size(at, self.vec(46.0, 16.0)), text, 10.0, DIM_TEXT, Align::Center),
            Printed::Rewind => {
                self.triangle(at - Vec2::new(r * 0.9, 0.0), r, Dir::Left, DIM_TEXT);
                self.triangle(at + Vec2::new(r * 0.9, 0.0), r, Dir::Left, DIM_TEXT);
            }
            Printed::FastForward => {
                self.triangle(at - Vec2::new(r * 0.9, 0.0), r, Dir::Right, DIM_TEXT);
                self.triangle(at + Vec2::new(r * 0.9, 0.0), r, Dir::Right, DIM_TEXT);
            }
            Printed::Stop => {
                self.painter.rect_filled(Rect::from_center_size(at, Vec2::splat(r * 1.6)), 1.0, DIM_TEXT);
            }
            Printed::Play => self.triangle(at, r, Dir::Right, DIM_TEXT),
            Printed::Record => {
                self.painter.circle_filled(at, r * 0.85, DIM_TEXT);
            }
        }
    }

    fn button(&self, rect: Rect, preset: &Preset, layer: Option<&str>, mapping: &Mapping, outline: Stroke) {
        let label = title_of(&mapping.press);
        let (fill, text) = if label.is_some() { (KEY, DARK_TEXT) } else { (KEY_UNUSED, DIM_TEXT) };
        self.painter.rect(rect, self.px(7.0), fill, outline);
        let lit = matches!(&mapping.press, Some(Meaning::Action(a)) if layout::is_lit(preset, layer, *a, false));
        // The light sits in the corner; a button with one keeps its text clear of it. A single long word
        // cannot be broken, so it may use the whole width (the light is small) and is set a little smaller.
        let one_word = label.as_deref().is_some_and(|l| !l.contains(' '));
        let inner = rect.shrink2(self.vec(if one_word { 2.0 } else if lit { 10.0 } else { 5.0 }, 3.0));
        let size = if label.as_deref().is_some_and(|l| !l.contains(' ') && l.chars().count() > 7) { 10.0 } else { 12.0 };
        self.text(inner, label.as_deref().unwrap_or("—"), size, text, Align::Center);
        if lit {
            self.painter.circle_filled(rect.right_top() + self.vec(-7.0, 7.0), self.px(3.5), render::GREEN);
        }
    }

    fn encoder(&self, rect: Rect, preset: &Preset, layer: Option<&str>, mapping: &Mapping, outline: Stroke) {
        let centre = rect.center();
        let radius = rect.width() / 2.0;
        let ring_lit = matches!(&mapping.turn, Some(Meaning::Action(a)) if layout::is_lit(preset, layer, *a, true));
        // The LED ring: three quarters of a circle, open at the bottom.
        for i in 0..RING_DOTS {
            let angle = (135.0 + 270.0 * i as f32 / (RING_DOTS - 1) as f32).to_radians();
            let at = centre + Vec2::new(angle.cos(), angle.sin()) * (radius + self.px(9.0));
            self.painter.circle_filled(at, self.px(3.0), if ring_lit { render::AMBER } else { RING_OFF });
        }
        self.painter.circle(centre, radius * 0.86, Color32::from_rgb(0x1b, 0x1c, 0x20), outline);
        self.painter.circle_filled(centre, radius * 0.7, Color32::from_rgb(0xd2, 0xd3, 0xd8));
        self.painter.line_segment([centre - Vec2::new(0.0, radius * 0.25), centre - Vec2::new(0.0, radius * 0.66)], Stroke::new(self.px(3.0), DARK_TEXT));

        let (above, below) = (title_of(&mapping.turn), title_of(&mapping.press));
        let box_width = self.px(84.0);
        if let Some(text) = above {
            let r = Rect::from_min_size(Pos2::new(centre.x - box_width / 2.0, centre.y - radius - self.px(54.0)), Vec2::new(box_width, self.px(38.0)));
            self.text(r, &text, 12.0, LIGHT_TEXT, Align::Center);
        }
        if let Some(text) = below {
            let r = Rect::from_min_size(Pos2::new(centre.x - box_width / 2.0, centre.y + radius + self.px(16.0)), Vec2::new(box_width, self.px(30.0)));
            self.text(r, &format!("push: {text}"), 10.0, DIM_TEXT, Align::Center);
        }
    }

    /// A light of the device that is no control: a small box with its name and a dark lamp.
    fn indicator(&self, rect: Rect, name: &str) {
        self.painter.rect(rect, self.px(6.0), Color32::from_rgb(0x23, 0x25, 0x2a), Stroke::new(1.0_f32, RING_OFF));
        self.painter.rect_filled(Rect::from_center_size(rect.center_top() + self.vec(0.0, 12.0), self.vec(10.0, 7.0)), self.px(2.0), Color32::from_rgb(0x4a, 0x34, 0x1c));
        self.text(Rect::from_center_size(rect.center() + self.vec(0.0, 8.0), Vec2::new(rect.width(), self.px(16.0))), name, 9.0, DIM_TEXT, Align::Center);
    }

    fn fader(&self, rect: Rect, mapping: &Mapping, outline: Stroke) {
        let slot = Rect::from_center_size(rect.center(), Vec2::new(self.px(8.0), rect.height() - self.px(20.0)));
        self.painter.rect_filled(slot, self.px(4.0), Color32::from_rgb(0x15, 0x16, 0x19));
        for i in 0..=10 {
            let y = slot.top() + slot.height() * i as f32 / 10.0;
            self.painter.line_segment([Pos2::new(slot.left() - self.px(9.0), y), Pos2::new(slot.left() - self.px(3.0), y)], Stroke::new(1.0_f32, RING_OFF));
        }
        let knob = Rect::from_center_size(Pos2::new(rect.center().x, slot.center().y + slot.height() * 0.15), Vec2::new(rect.width() * 0.8, self.px(44.0)));
        self.painter.rect(knob, self.px(5.0), KEY, outline);
        self.painter.line_segment([Pos2::new(knob.left() + self.px(4.0), knob.center().y), Pos2::new(knob.right() - self.px(4.0), knob.center().y)], Stroke::new(self.px(2.0), DARK_TEXT));
        if let Some(text) = title_of(&mapping.turn) {
            let r = Rect::from_min_size(Pos2::new(rect.center().x - self.px(47.0), rect.top() - self.px(46.0)), self.vec(94.0, 38.0));
            self.text(r, &text, 12.0, LIGHT_TEXT, Align::Center);
        }
    }
}

fn tooltip(ui: &mut egui::Ui, control: &Control, mapping: &Mapping) {
    ui.set_max_width(320.0);
    ui.label(RichText::new(&control.name).strong());
    let (press_word, turn_word) = match control.kind {
        ControlKind::Encoder => ("Push", "Turn"),
        ControlKind::Fader => ("Press", "Move"),
        _ => ("Press", "Turn"),
    };
    if control.kind == ControlKind::Indicator {
        ui.label("Lit when the controller is in Mackie Control mode, the mode this program needs. The controller's manual says how to switch to it.");
        return;
    }
    if control.press.is_none() && control.turn.is_none() {
        ui.label(RichText::new("The program does not use it, and which message it sends is not known.").weak());
        return;
    }
    let rows = [(press_word, control.press, &mapping.press), (turn_word, control.turn, &mapping.turn)];
    for (word, src, meaning) in rows {
        let Some(src) = src else { continue };
        match meaning {
            Some(Meaning::Action(a)) => {
                ui.label(RichText::new(format!("{word}: {}", a.title())).strong());
                ui.label(a.help());
            }
            Some(Meaning::Layer(l)) => {
                ui.label(RichText::new(format!("{word}: layer {l}")).strong());
                ui.label(format!("Shows the controls of layer {l}. The program switches the controller's layer itself."));
            }
            None => {
                ui.label(RichText::new(format!("{word}: nothing on this layer")).weak());
            }
        }
        widgets::hint(ui, src.text());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn app_with_mini() -> DrumApp {
        let mut app = DrumApp::with_config(Config { controller_preset: "Behringer X-Touch Mini".into(), ..Config::default() });
        app.refresh_controllers();
        app
    }

    fn frame(app: &mut DrumApp, ctx: &egui::Context) {
        let _ = ctx.run(egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 820.0))), ..Default::default() }, |ctx| app.controller_help_window(ctx));
    }

    #[test]
    fn the_indicator_opens_and_closes_the_help_and_it_draws_for_both_layers() {
        let ctx = egui::Context::default();
        let mut app = app_with_mini();
        frame(&mut app, &ctx);
        assert!(app.controller_help.is_none());
        app.toggle_controller_help();
        let help = app.controller_help.as_ref().expect("opened without a connected controller");
        assert!(help.layout.is_some(), "the Mini has a picture");
        assert_eq!(help.layer, 0);
        frame(&mut app, &ctx);
        app.controller_help.as_mut().unwrap().layer = 1;
        frame(&mut app, &ctx);
        assert!(app.controller_help.is_some(), "it stays open");
        app.toggle_controller_help();
        assert!(app.controller_help.is_none(), "a second click closes it");
    }

    #[test]
    fn a_controller_without_a_picture_is_listed_and_a_changed_preset_closes_the_help() {
        let ctx = egui::Context::default();
        let mut app = app_with_mini();
        app.cfg.controller_preset = "Behringer X-Touch Compact".into();
        app.toggle_controller_help();
        let help = app.controller_help.as_mut().unwrap();
        assert!(help.layout.is_some(), "the Compact has a picture too");
        assert_eq!(help.preset.layers.len(), 0, "and no layers");
        frame(&mut app, &ctx);
        app.controller_help.as_mut().unwrap().layout = None; // a controller without a picture: the list
        frame(&mut app, &ctx);
        assert!(app.controller_help.is_some());
        app.cfg.controller_preset = "Behringer X-Touch Mini".into();
        frame(&mut app, &ctx);
        assert!(app.controller_help.is_none(), "the picture would be of the wrong controller");
    }

    #[test]
    fn a_preset_without_a_drawn_layout_gets_a_generated_picture() {
        let ctx = egui::Context::default();
        let mut app = DrumApp::with_config(Config { controller_preset: "Korg nanoKONTROL2".into(), ..Config::default() });
        app.refresh_controllers();
        app.toggle_controller_help();
        let help = app.controller_help.as_ref().expect("opened");
        assert!(help.layout.as_ref().is_some_and(|l| l.generated), "made from the preset");
        frame(&mut app, &ctx);
        frame(&mut app, &ctx);
        assert!(app.controller_help.is_some());
    }

    #[test]
    fn a_missing_preset_is_reported_instead_of_opening_the_help() {
        let mut app = DrumApp::with_config(Config { controller_preset: "No such controller".into(), ..Config::default() });
        app.toggle_controller_help();
        assert!(app.controller_help.is_none());
        assert!(app.error.is_some());
    }
}
