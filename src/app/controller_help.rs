//! The controller help screen: a picture of the controller drawn from its layout
//! (`controller_layout.rs`), each control labelled with what the preset makes it do on the
//! chosen layer. It opens from the controller indicator at the top of the window.

use egui::{Align, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use super::DrumApp;
use crate::config::Config;
use crate::controller::{self, Preset};
use crate::controller_layout::{self as layout, Control, Kind, Layout, Mapping, Meaning, Printed};
use crate::render;

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
            None => match controller::load_preset(&self.cfg.controller_preset, Config::controllers_dir().as_deref()) {
                Ok(preset) => (preset, None),
                Err(e) => {
                    self.error = Some(e);
                    return;
                }
            },
        };
        let layer = layer.and_then(|l| preset.layers.iter().position(|(name, _)| *name == l)).unwrap_or(0);
        self.controller_help = Some(ControllerHelp { chosen: self.cfg.controller_preset.clone(), layout: layout::for_preset(&preset), preset, layer });
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
            ui.label(RichText::new(state).small().weak());
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
                ui.label(RichText::new("There is no picture of this controller yet (only the Behringer X-Touch Mini and Compact have one). Its controls on this layer:").small().weak());
                egui::Grid::new("controller_help_list").num_columns(2).striped(true).show(ui, |ui| {
                    for b in self.preset.binds.iter().filter(|b| b.layer.is_none() || b.layer == layer) {
                        ui.label(format!("{}{}", b.src.text(), if b.relative { " (turn)" } else { "" }));
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
        ui.label(RichText::new(text).small().weak());
    };
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
        .map(|b| format!("{} ({})", b.src.text(), b.action.title()))
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
    let painter = ui.painter_at(area);
    let at = |x: f32, y: f32| area.min + Vec2::new(x, y) * scale;
    let font = |size: f32| egui::FontId::proportional((size * scale).max(8.0));
    let mut chosen = None;

    painter.rect_filled(area, 20.0 * scale, BODY);
    let title = match layer {
        Some(l) => format!("{} · layer {l}", layout.name),
        None => layout.name.to_string(),
    };
    text_box(&painter, Rect::from_min_size(at(36.0, layout.size.1 - 44.0), Vec2::new(520.0, 30.0) * scale), &title, font(15.0), DIM_TEXT, Align::Min);
    let layers: Vec<&Control> = layout.controls.iter().filter(|c| c.kind == Kind::Layer).collect();
    if let [a, b] = layers[..] {
        let middle = ((a.center.0 + b.center.0) / 2.0, (a.center.1 + b.center.1) / 2.0);
        text_box(&painter, Rect::from_center_size(at(middle.0, middle.1), Vec2::new(64.0, 16.0) * scale), "LAYER", font(10.0), DIM_TEXT, Align::Center);
        // The arrows printed beside them: up for A, down for B.
        for (control, up) in [(a, true), (b, false)] {
            triangle(&painter, at(control.center.0 - control.size.0 / 2.0 - 9.0, control.center.1), 5.0 * scale, if up { Dir::Up } else { Dir::Down }, DIM_TEXT);
        }
    }
    for mark in &layout.marks {
        symbol(&painter, at(mark.at.0, mark.at.1), scale, &font, mark.what);
    }

    for (i, control) in layout.controls.iter().enumerate() {
        let rect = Rect::from_center_size(at(control.center.0, control.center.1), Vec2::new(control.size.0, control.size.1) * scale);
        let mapping = layout::mapping(preset, layer, control);
        let sense = if control.kind == Kind::Layer { Sense::click() } else { Sense::hover() };
        let response = ui.interact(rect, ui.id().with(("controller_control", i)), sense);
        let hovered = response.hovered();
        let outline = if hovered { Stroke::new(2.0 * scale.max(0.7), render::ACCENT) } else { Stroke::NONE };
        match control.kind {
            Kind::Button => button(&painter, rect, scale, &font, preset, layer, &mapping, outline),
            Kind::Layer => {
                let this = preset.layers.iter().position(|(_, src)| Some(*src) == control.press);
                let selected = this == Some(shown);
                let (fill, text) = if selected { (render::ACCENT, Color32::WHITE) } else { (Color32::from_rgb(0x5a, 0x5d, 0x66), LIGHT_TEXT) };
                painter.rect(rect, 6.0 * scale, fill, outline);
                let letter = control.name.rsplit(' ').next().unwrap_or("");
                text_box(&painter, rect, letter, font(17.0), text, Align::Center);
                if response.clicked() {
                    chosen = this;
                }
            }
            Kind::Encoder => encoder(&painter, rect, scale, &font, preset, layer, &mapping, outline),
            Kind::Fader => fader(&painter, rect, scale, &font, &mapping, outline),
            Kind::Indicator => indicator(&painter, rect, scale, &font, &control.name),
        }
        response.on_hover_ui_at_pointer(|ui| tooltip(ui, control, &mapping));
    }
    chosen
}

/// `text` in `rect`, wrapped, in `align`, with its middle at the middle of `rect`.
fn text_box(painter: &egui::Painter, rect: Rect, text: &str, font: egui::FontId, color: Color32, align: Align) {
    if text.is_empty() {
        return;
    }
    let mut job = egui::text::LayoutJob::simple(text.to_string(), font, color, rect.width());
    job.halign = align;
    let galley = painter.layout_job(job);
    // Centred text is laid out around the x of the position it is placed at.
    let x = if align == Align::Center { rect.center().x } else { rect.left() };
    let top_left = Pos2::new(x, rect.center().y - galley.size().y / 2.0);
    painter.galley(top_left, galley, color);
}

fn title_of(meaning: &Option<Meaning>) -> Option<String> {
    match meaning {
        Some(Meaning::Action(a)) => Some(a.title()),
        Some(Meaning::Layer(l)) => Some(format!("Layer {l}")),
        None => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn button(painter: &egui::Painter, rect: Rect, scale: f32, font: &dyn Fn(f32) -> egui::FontId, preset: &Preset, layer: Option<&str>, mapping: &Mapping, outline: Stroke) {
    let label = title_of(&mapping.press);
    let (fill, text) = if label.is_some() { (KEY, DARK_TEXT) } else { (KEY_UNUSED, DIM_TEXT) };
    painter.rect(rect, 7.0 * scale, fill, outline);
    let lit = matches!(&mapping.press, Some(Meaning::Action(a)) if layout::is_lit(preset, layer, *a, false));
    // The light sits in the corner; a button with one keeps its text clear of it.
    // A single long word may use the whole width (it cannot be broken, and the light is small).
    let one_word = label.as_deref().is_some_and(|l| !l.contains(' '));
    let inner = rect.shrink2(Vec2::new(if one_word { 2.0 } else if lit { 10.0 } else { 5.0 }, 3.0) * scale);
    // A long word that cannot be broken is set a little smaller instead of being split.
    let size = if label.as_deref().is_some_and(|l| !l.contains(' ') && l.chars().count() > 7) { 10.0 } else { 12.0 };
    text_box(painter, inner, label.as_deref().unwrap_or("—"), font(size), text, Align::Center);
    if lit {
        painter.circle_filled(rect.right_top() + Vec2::new(-7.0, 7.0) * scale, 3.5 * scale, render::GREEN);
    }
}

#[allow(clippy::too_many_arguments)]
fn encoder(painter: &egui::Painter, rect: Rect, scale: f32, font: &dyn Fn(f32) -> egui::FontId, preset: &Preset, layer: Option<&str>, mapping: &Mapping, outline: Stroke) {
    let centre = rect.center();
    let radius = rect.width() / 2.0;
    let ring_lit = matches!(&mapping.turn, Some(Meaning::Action(a)) if layout::is_lit(preset, layer, *a, true));
    // The LED ring: three quarters of a circle, open at the bottom.
    for i in 0..RING_DOTS {
        let angle = (135.0 + 270.0 * i as f32 / (RING_DOTS - 1) as f32).to_radians();
        let at = centre + Vec2::new(angle.cos(), angle.sin()) * (radius + 9.0 * scale);
        painter.circle_filled(at, 3.0 * scale, if ring_lit { render::AMBER } else { RING_OFF });
    }
    painter.circle(centre, radius * 0.86, Color32::from_rgb(0x1b, 0x1c, 0x20), outline);
    painter.circle_filled(centre, radius * 0.7, Color32::from_rgb(0xd2, 0xd3, 0xd8));
    painter.line_segment([centre - Vec2::new(0.0, radius * 0.25), centre - Vec2::new(0.0, radius * 0.66)], Stroke::new(3.0 * scale, DARK_TEXT));

    let (above, below) = (title_of(&mapping.turn), title_of(&mapping.press));
    let box_width = 84.0 * scale;
    if let Some(text) = above {
        let r = Rect::from_min_size(Pos2::new(centre.x - box_width / 2.0, centre.y - radius - 54.0 * scale), Vec2::new(box_width, 38.0 * scale));
        text_box(painter, r, &text, font(12.0), LIGHT_TEXT, Align::Center);
    }
    if let Some(text) = below {
        let r = Rect::from_min_size(Pos2::new(centre.x - box_width / 2.0, centre.y + radius + 16.0 * scale), Vec2::new(box_width, 30.0 * scale));
        text_box(painter, r, &format!("push: {text}"), font(10.0), DIM_TEXT, Align::Center);
    }
}

#[derive(Clone, Copy)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// A small filled triangle with its middle at `at`, `r` from the middle to each corner.
fn triangle(painter: &egui::Painter, at: Pos2, r: f32, dir: Dir, color: Color32) {
    let (dx, dy) = match dir {
        Dir::Up => (0.0, -1.0),
        Dir::Down => (0.0, 1.0),
        Dir::Left => (-1.0, 0.0),
        Dir::Right => (1.0, 0.0),
    };
    let tip = Vec2::new(dx, dy);
    let side = Vec2::new(-dy, dx);
    let points = vec![at + tip * r, at - tip * r * 0.8 + side * r * 0.9, at - tip * r * 0.8 - side * r * 0.9];
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

/// What is printed beside a button, drawn as vectors (the fonts lack these symbols).
fn symbol(painter: &egui::Painter, at: Pos2, scale: f32, font: &dyn Fn(f32) -> egui::FontId, what: Printed) {
    let r = 5.0 * scale;
    match what {
        Printed::Text(text) => text_box(painter, Rect::from_center_size(at, Vec2::new(46.0, 16.0) * scale), text, font(10.0), DIM_TEXT, Align::Center),
        Printed::Rewind => {
            triangle(painter, at - Vec2::new(r * 0.9, 0.0), r, Dir::Left, DIM_TEXT);
            triangle(painter, at + Vec2::new(r * 0.9, 0.0), r, Dir::Left, DIM_TEXT);
        }
        Printed::FastForward => {
            triangle(painter, at - Vec2::new(r * 0.9, 0.0), r, Dir::Right, DIM_TEXT);
            triangle(painter, at + Vec2::new(r * 0.9, 0.0), r, Dir::Right, DIM_TEXT);
        }
        Printed::Stop => {
            painter.rect_filled(Rect::from_center_size(at, Vec2::splat(r * 1.6)), 1.0, DIM_TEXT);
        }
        Printed::Play => triangle(painter, at, r, Dir::Right, DIM_TEXT),
        Printed::Record => {
            painter.circle_filled(at, r * 0.85, DIM_TEXT);
        }
    }
}

/// A light of the device that is no control: a small box with its name and a dark lamp.
fn indicator(painter: &egui::Painter, rect: Rect, scale: f32, font: &dyn Fn(f32) -> egui::FontId, name: &str) {
    painter.rect(rect, 6.0 * scale, Color32::from_rgb(0x23, 0x25, 0x2a), Stroke::new(1.0_f32, RING_OFF));
    painter.rect_filled(Rect::from_center_size(rect.center_top() + Vec2::new(0.0, 12.0 * scale), Vec2::new(10.0, 7.0) * scale), 2.0 * scale, Color32::from_rgb(0x4a, 0x34, 0x1c));
    text_box(painter, Rect::from_center_size(rect.center() + Vec2::new(0.0, 8.0 * scale), Vec2::new(rect.width(), 16.0 * scale)), name, font(9.0), DIM_TEXT, Align::Center);
}

fn fader(painter: &egui::Painter, rect: Rect, scale: f32, font: &dyn Fn(f32) -> egui::FontId, mapping: &Mapping, outline: Stroke) {
    let slot = Rect::from_center_size(rect.center(), Vec2::new(8.0 * scale, rect.height() - 20.0 * scale));
    painter.rect_filled(slot, 4.0 * scale, Color32::from_rgb(0x15, 0x16, 0x19));
    for i in 0..=10 {
        let y = slot.top() + slot.height() * i as f32 / 10.0;
        painter.line_segment([Pos2::new(slot.left() - 9.0 * scale, y), Pos2::new(slot.left() - 3.0 * scale, y)], Stroke::new(1.0_f32, RING_OFF));
    }
    let knob = Rect::from_center_size(Pos2::new(rect.center().x, slot.center().y + slot.height() * 0.15), Vec2::new(rect.width() * 0.8, 44.0 * scale));
    painter.rect(knob, 5.0 * scale, KEY, outline);
    painter.line_segment([Pos2::new(knob.left() + 4.0 * scale, knob.center().y), Pos2::new(knob.right() - 4.0 * scale, knob.center().y)], Stroke::new(2.0 * scale, DARK_TEXT));
    if let Some(text) = title_of(&mapping.turn) {
        let r = Rect::from_min_size(Pos2::new(rect.center().x - 47.0 * scale, rect.top() - 46.0 * scale), Vec2::new(94.0 * scale, 38.0 * scale));
        text_box(painter, r, &text, font(12.0), LIGHT_TEXT, Align::Center);
    }
}

fn tooltip(ui: &mut egui::Ui, control: &Control, mapping: &Mapping) {
    ui.set_max_width(320.0);
    ui.label(RichText::new(&control.name).strong());
    let (press_word, turn_word) = match control.kind {
        Kind::Encoder => ("Push", "Turn"),
        Kind::Fader => ("Press", "Move"),
        _ => ("Press", "Turn"),
    };
    if control.kind == Kind::Indicator {
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
        ui.label(RichText::new(src.text()).small().weak());
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
    fn a_missing_preset_is_reported_instead_of_opening_the_help() {
        let mut app = DrumApp::with_config(Config { controller_preset: "No such controller".into(), ..Config::default() });
        app.toggle_controller_help();
        assert!(app.controller_help.is_none());
        assert!(app.error.is_some());
    }
}
