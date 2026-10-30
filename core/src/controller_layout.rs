//! What a controller looks like: where its buttons, encoders and fader are, and which MIDI
//! message each one sends. Together with a preset (`controller.rs`) this gives what every
//! control does on a layer, for the help screen. Pure geometry and lookups; the drawing is
//! in `app/controller_help.rs`.
//!
//! Layouts: the Behringer X-Touch Mini and the X-Touch Compact, both drawn from photographs of
//! their fronts (positions measured on the photo, the height of the Mini stretched to make room
//! for labels), and one made from any preset (`generated`).

use crate::controller::{Action, Binding, Preset, Src};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Button,
    /// An endless rotary encoder with a push switch and an LED ring.
    Encoder,
    Fader,
    /// A button that selects a layer.
    Layer,
    /// A light that shows a state of the device (MC mode); it sends nothing.
    Indicator,
}

#[derive(Clone, Debug)]
pub struct Control {
    pub kind: ControlKind,
    /// Where it is on the device: "Encoder 3", "Top row, button 2".
    pub name: String,
    /// In layout units, from the top left corner of the device.
    pub center: (f32, f32),
    pub size: (f32, f32),
    /// What it sends when pressed (a button, the push of an encoder).
    pub press: Option<Src>,
    /// What it sends when turned or moved (an encoder, the fader).
    pub turn: Option<Src>,
}

impl Control {
    /// A control with a place and a size and no messages yet.
    fn new(kind: ControlKind, name: impl Into<String>, center: (f32, f32), size: (f32, f32)) -> Control {
        Control { kind, name: name.into(), center, size, press: None, turn: None }
    }

    fn button(name: impl Into<String>, center: (f32, f32), size: (f32, f32)) -> Control {
        Control::new(ControlKind::Button, name, center, size)
    }

    fn encoder(name: impl Into<String>, center: (f32, f32), size: (f32, f32)) -> Control {
        Control::new(ControlKind::Encoder, name, center, size)
    }

    fn fader(name: impl Into<String>, center: (f32, f32), size: (f32, f32)) -> Control {
        Control::new(ControlKind::Fader, name, center, size)
    }

    /// What it sends when pressed.
    fn pressing(mut self, src: Src) -> Control {
        self.press = Some(src);
        self
    }

    /// What it sends when turned or moved.
    fn turning(mut self, src: Src) -> Control {
        self.turn = Some(src);
        self
    }
}

/// What is printed beside a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Printed {
    Text(&'static str),
    Rewind,
    FastForward,
    Stop,
    Play,
    Record,
}

/// A printed mark on the device, at a place in layout units.
#[derive(Clone, Copy, Debug)]
pub struct Mark {
    pub at: (f32, f32),
    pub what: Printed,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub name: String,
    /// Made from the preset (`generated`), not drawn after the device.
    pub generated: bool,
    /// The device's size in layout units.
    pub size: (f32, f32),
    pub controls: Vec<Control>,
    /// The symbols and words printed on the device beside its buttons.
    pub marks: Vec<Mark>,
}

impl Layout {
    /// The preset's bindings whose message no control of the picture sends, so the picture cannot
    /// show them (the layout does not know where on the device they are, or the device lacks them).
    pub fn unplaced<'a>(&self, preset: &'a Preset) -> Vec<&'a Binding> {
        let known = self.controls.iter().flat_map(|c| [c.press, c.turn]).flatten();
        let known: Vec<Src> = known.chain(preset.layers.iter().map(|(_, s)| *s)).collect();
        preset.binds.iter().filter(|b| !known.contains(&b.input.src)).collect()
    }
}

/// The layout of the controller the preset is for, if there is one. The preset's MIDI port
/// name decides, so a copy of a built-in preset keeps the picture.
pub fn for_preset(preset: &Preset) -> Option<Layout> {
    let port = preset.port.to_ascii_lowercase();
    if port.contains("x-touch mini") {
        Some(x_touch_mini())
    } else if port.contains("x-touch compact") {
        Some(x_touch_compact())
    } else {
        None
    }
}

const COLUMNS: usize = 8;
const COLUMN_X: f32 = 78.0;
const COLUMN_PITCH: f32 = 92.0;

/// Notes of the two rows of buttons, as the Mini sends them in Mackie Control mode.
const TOP_ROW: [u8; COLUMNS] = [89, 90, 40, 41, 42, 43, 44, 45];
const BOTTOM_ROW: [u8; COLUMNS] = [87, 88, 91, 92, 86, 93, 94, 95];
/// Encoders: turn is cc 16-23, push is note 32-39. The fader is pitch bend on channel 1.
const ENCODER_CC: u8 = 16;
const ENCODER_PUSH: u8 = 32;
const LAYER_NOTES: [u8; 2] = [84, 85];

/// The eight encoders of a Mackie surface's top row, in the order of their numbers, at `y`.
fn encoder_row(x: impl Fn(usize) -> f32, y: f32, size: f32) -> Vec<Control> {
    (0..COLUMNS)
        .map(|i| Control::encoder(format!("Encoder {}", i + 1), (x(i), y), (size, size)).pressing(Src::Note(ENCODER_PUSH + i as u8)).turning(Src::Cc(ENCODER_CC + i as u8)))
        .collect()
}

/// Behringer X-Touch Mini: a row of eight encoders, two rows of buttons below them, the
/// two LAYER buttons and the fader on the right.
pub fn x_touch_mini() -> Layout {
    const ENCODER_Y: f32 = 95.0;
    const ROWS_Y: [f32; 2] = [200.0, 273.0];
    const BUTTON: (f32, f32) = (62.0, 48.0);
    const RIGHT_X: f32 = 897.0;
    let x = |i: usize| COLUMN_X + i as f32 * COLUMN_PITCH;
    let mut controls = encoder_row(x, ENCODER_Y, 62.0);
    for (row, (notes, y)) in [("Top row", (&TOP_ROW, ROWS_Y[0])), ("Bottom row", (&BOTTOM_ROW, ROWS_Y[1]))] {
        for (i, note) in notes.iter().enumerate() {
            controls.push(Control::button(format!("{row}, button {}", i + 1), (x(i), y), BUTTON).pressing(Src::Note(*note)));
        }
    }
    // The LAYER buttons sit in line with the two rows of buttons.
    for (i, note) in LAYER_NOTES.iter().enumerate() {
        controls.push(Control::new(ControlKind::Layer, format!("Layer {}", ["A", "B"][i]), (RIGHT_X, ROWS_Y[i]), BUTTON).pressing(Src::Note(*note)));
    }
    controls.push(Control::new(ControlKind::Indicator, "MC MODE", (RIGHT_X, 133.0), (74.0, 44.0)));
    controls.push(Control::fader("Fader", (817.0, 182.0), (44.0, 238.0)).turning(Src::Bend(1)));
    // What is printed beside the buttons of the bottom row.
    let printed = [Some(Printed::Text("MC")), None, Some(Printed::Rewind), Some(Printed::FastForward), Some(Printed::Text("LOOP")), Some(Printed::Stop), Some(Printed::Play), Some(Printed::Record)];
    let marks = printed.into_iter().enumerate().filter_map(|(i, what)| Some(Mark { at: (x(i) + 47.0, ROWS_Y[1]), what: what? })).collect();
    Layout { name: "Behringer X-Touch Mini".into(), size: (952.0, 371.0), controls, marks, generated: false }
}

/// Behringer X-Touch Compact, laid out from a photograph of the front: a row of eight encoders over
/// three rows of eight buttons (REC, SOLO, MUTE in Mackie Control terms), nine faders with a row of
/// buttons under them, eight more encoders and six transport buttons on the right, and the two LAYER
/// buttons at the bottom right. The messages are those of the Mackie Control table; the ones this
/// picture does not know (the right-hand encoders, the LAYER buttons) are left out.
pub fn x_touch_compact() -> Layout {
    const STRIP_BUTTON: (f32, f32) = (72.0, 38.0);
    const FADER: (f32, f32) = (40.0, 340.0);
    const FADER_Y: f32 = 595.0;
    const SELECT_Y: f32 = 840.0;
    const MAIN_X: f32 = 885.0;
    const SIDE_X: f32 = 993.0;
    let x = |i: usize| 105.0 + i as f32 * 96.0;
    // The encoders of the top row come first: the LED ring of encoder n is addressed as cc 48 + n.
    let mut controls = encoder_row(x, 120.0, 66.0);
    // The strip buttons: REC, SOLO, MUTE above the faders, SELECT below them.
    for (word, first, y) in [("REC", 0u8, 222.0), ("SOLO", 8, 277.0), ("MUTE", 16, 332.0), ("SELECT", 24, SELECT_Y)] {
        for i in 0..COLUMNS {
            controls.push(Control::button(format!("Strip {} · {word}", i + 1), (x(i), y), STRIP_BUTTON).pressing(Src::Note(first + i as u8)));
        }
    }
    for i in 0..COLUMNS {
        controls.push(Control::fader(format!("Fader {}", i + 1), (x(i), FADER_Y), FADER).turning(Src::Bend(i as u8 + 1)));
    }
    controls.push(Control::fader("Fader 9 (main)", (MAIN_X, FADER_Y), FADER).turning(Src::Bend(9)));
    controls.push(Control::button("Button under fader 9", (MAIN_X, SELECT_Y), STRIP_BUTTON));
    // The eight encoders on the right, in two columns, read row by row. What they send is not known.
    for i in 0..8 {
        controls.push(Control::encoder(format!("Side encoder {}", i + 1), (SIDE_X + (i % 2) as f32 * 97.0, 120.0 + (i / 2) as f32 * 109.0), (66.0, 66.0)));
    }
    // The six transport buttons (their order is a guess), then the LAYER buttons.
    let transport: [(&str, u8); 6] = [("Rewind", 91), ("Fast forward", 92), ("Stop", 93), ("Play", 94), ("Record", 95), ("Cycle", 86)];
    for (i, (name, note)) in transport.into_iter().enumerate() {
        controls.push(Control::button(name, (SIDE_X + (i % 2) as f32 * 95.0, 587.0 + (i / 2) as f32 * 78.0), (72.0, 40.0)).pressing(Src::Note(note)));
    }
    for (i, name) in ["Layer A button", "Layer B button"].into_iter().enumerate() {
        controls.push(Control::button(name, (SIDE_X + i as f32 * 95.0, SELECT_Y), STRIP_BUTTON));
    }
    Layout { name: "Behringer X-Touch Compact".into(), size: (1200.0, 910.0), controls, marks: Vec::new(), generated: false }
}

/// Controls to a row of a generated picture: eight strips and a master fader fit one row.
const PER_ROW: usize = 9;

/// The rows of a generated picture, from the top. Each group reserves room for the labels above
/// its controls and a gap under them.
mod rows {
    pub const KNOB: f32 = 62.0;
    /// From the top of the group to the middle of its first row of knobs (room for their labels).
    pub const KNOB_FIRST: f32 = 90.0;
    pub const KNOB_PITCH: f32 = 130.0;
    pub const KNOB_GAP: f32 = 60.0;
    pub const FADER_LABEL: f32 = 50.0;
    pub const FADER: (f32, f32) = (44.0, 190.0);
    pub const FADER_PITCH: f32 = 280.0;
    pub const FADER_GAP: f32 = 30.0;
    pub const BUTTON: (f32, f32) = (62.0, 48.0);
    pub const BUTTON_FIRST: f32 = 30.0;
    pub const BUTTON_PITCH: f32 = 62.0;
    pub const BUTTON_GAP: f32 = 30.0;
    /// Under the last group: room for the device's name.
    pub const BOTTOM: f32 = 70.0;
}

/// A picture of a controller made from its preset alone, for the controllers that have no drawn
/// layout (a learned preset, the generic built-in ones): the encoders and knobs in rows at the top,
/// then the faders, then the buttons, each group sorted by message number, nine to a row. It
/// shows what the preset binds, not where the controls are on the device. `None` if the preset
/// binds nothing and has no layers.
pub fn generated(preset: &Preset) -> Option<Layout> {
    let mut srcs: Vec<Src> = preset.binds.iter().map(|b| b.input.src).collect();
    srcs.sort();
    srcs.dedup();
    let layer_srcs: Vec<(String, Src)> = preset.layers.iter().filter(|(_, s)| !srcs.contains(s)).cloned().collect();
    if srcs.is_empty() && layer_srcs.is_empty() {
        return None;
    }
    let kind_of = |src: Src| -> ControlKind {
        let bind = preset.binds.iter().find(|b| b.input.src == src);
        match (src, bind) {
            (Src::Bend(_), _) => ControlKind::Fader,
            (Src::Cc(_), Some(b)) if b.reading.is_relative() => ControlKind::Encoder,
            (Src::Cc(_), Some(b)) if b.action.is_level() => match b.action {
                Action::Gain(_) | Action::MusicVolume => ControlKind::Fader,
                _ => ControlKind::Encoder,
            },
            _ => ControlKind::Button,
        }
    };
    let group = |kind: ControlKind| -> Vec<Src> { srcs.iter().copied().filter(|s| kind_of(*s) == kind).collect() };
    let x = |i: usize| COLUMN_X + (i % PER_ROW) as f32 * COLUMN_PITCH;
    let row = |i: usize| (i / PER_ROW) as f32;
    let name = |word: &str, src: Src| format!("{word} ({})", src.text());
    let mut controls = Vec::new();
    let mut y = 0.0_f32;

    let knobs = group(ControlKind::Encoder);
    for (i, src) in knobs.iter().enumerate() {
        controls.push(Control::encoder(name("Knob", *src), (x(i), y + rows::KNOB_FIRST + row(i) * rows::KNOB_PITCH), (rows::KNOB, rows::KNOB)).turning(*src));
    }
    if !knobs.is_empty() {
        y += rows::KNOB_FIRST + row(knobs.len() - 1) * rows::KNOB_PITCH + rows::KNOB_GAP;
    }

    let faders = group(ControlKind::Fader);
    for (i, src) in faders.iter().enumerate() {
        controls.push(Control::fader(name("Fader", *src), (x(i), y + rows::FADER_LABEL + rows::FADER.1 / 2.0 + row(i) * rows::FADER_PITCH), rows::FADER).turning(*src));
    }
    if !faders.is_empty() {
        y += rows::FADER_LABEL + rows::FADER.1 + row(faders.len() - 1) * rows::FADER_PITCH + rows::FADER_GAP;
    }

    let mut buttons: Vec<(ControlKind, String, Src)> = group(ControlKind::Button).into_iter().map(|s| (ControlKind::Button, name("Button", s), s)).collect();
    buttons.extend(layer_srcs.iter().map(|(layer, s)| (ControlKind::Layer, format!("Layer {layer} button ({})", s.text()), *s)));
    for (i, (kind, label, src)) in buttons.iter().enumerate() {
        controls.push(Control::new(*kind, label.clone(), (x(i), y + rows::BUTTON_FIRST + row(i) * rows::BUTTON_PITCH), rows::BUTTON).pressing(*src));
    }
    if !buttons.is_empty() {
        y += rows::BUTTON_FIRST + row(buttons.len() - 1) * rows::BUTTON_PITCH + rows::BUTTON_GAP;
    }

    let title = if preset.name.is_empty() { "Controller".to_string() } else { preset.name.clone() };
    Some(Layout { name: title, size: (COLUMN_X * 2.0 + COLUMN_PITCH * (PER_ROW - 1) as f32, y + rows::BOTTOM), controls, marks: Vec::new(), generated: true })
}

/// What a message of a control means on a layer.
#[derive(Clone, Debug, PartialEq)]
pub enum Meaning {
    Action(Action),
    /// Selects this layer.
    Layer(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mapping {
    pub press: Option<Meaning>,
    pub turn: Option<Meaning>,
}

/// What `control` does on `layer` (`None`: a preset without layers) according to `preset`.
pub fn mapping(preset: &Preset, layer: Option<&str>, control: &Control) -> Mapping {
    let meaning = |src: Src| -> Option<Meaning> {
        if let Some((name, _)) = preset.layers.iter().find(|(_, s)| *s == src) {
            return Some(Meaning::Layer(name.clone()));
        }
        preset.position(layer, src, None).map(|i| Meaning::Action(preset.binds[i].action))
    };
    Mapping { press: control.press.and_then(meaning), turn: control.turn.and_then(meaning) }
}

/// Whether the controller shows `action` with a light (`ring` false) or an LED ring (`ring` true) on `layer`.
pub fn is_lit(preset: &Preset, layer: Option<&str>, action: Action, ring: bool) -> bool {
    preset.lights.iter().any(|l| l.ring == ring && l.action == action && (l.layer.is_none() || l.layer.as_deref() == layer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::{load_preset, Out};

    const MINI: &str = "Behringer X-Touch Mini";
    const COMPACT: &str = "Behringer X-Touch Compact";

    fn device(name: &str) -> (Preset, Layout) {
        let preset = load_preset(name, None).unwrap();
        let layout = for_preset(&preset).unwrap_or_else(|| panic!("{name} has a layout"));
        (preset, layout)
    }

    fn mini() -> (Preset, Layout) {
        device(MINI)
    }

    fn sources(layout: &Layout) -> Vec<Src> {
        layout.controls.iter().flat_map(|c| [c.press, c.turn]).flatten().collect()
    }

    fn find<'a>(layout: &'a Layout, name: &str) -> &'a Control {
        layout.controls.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no control {name}"))
    }

    #[test]
    fn the_mini_and_the_compact_have_a_picture_and_other_controllers_do_not() {
        assert_eq!(for_preset(&load_preset(MINI, None).unwrap()).map(|l| l.name), Some(MINI.to_string()));
        assert_eq!(for_preset(&load_preset(COMPACT, None).unwrap()).map(|l| l.name), Some(COMPACT.to_string()));
        assert!(for_preset(&Preset::default()).is_none());
    }

    #[test]
    fn the_mini_has_every_control_of_the_device_once() {
        let (_, layout) = mini();
        let count = |kind| layout.controls.iter().filter(|c| c.kind == kind).count();
        assert_eq!((count(ControlKind::Encoder), count(ControlKind::Button), count(ControlKind::Layer), count(ControlKind::Fader), count(ControlKind::Indicator)), (8, 16, 2, 1, 1));
        let y = |name: &str| find(&layout, name).center.1;
        assert_eq!((y("Layer A"), y("Layer B")), (y("Top row, button 1"), y("Bottom row, button 1")), "the LAYER buttons are in line with the rows");
    }

    #[test]
    fn what_is_printed_beside_the_minis_bottom_row_is_where_the_photograph_has_it() {
        let (_, layout) = mini();
        let bottom = find(&layout, "Bottom row, button 1").center.1;
        let kinds: Vec<Printed> = layout.marks.iter().map(|m| m.what).collect();
        assert_eq!(kinds, [Printed::Text("MC"), Printed::Rewind, Printed::FastForward, Printed::Text("LOOP"), Printed::Stop, Printed::Play, Printed::Record]);
        let bottom_row: Vec<&Control> = layout.controls.iter().filter(|c| c.kind == ControlKind::Button && c.center.1 == bottom).collect();
        // The button a mark is printed beside: the nearest to its left.
        let beside = |mark: &Mark| bottom_row.iter().copied().filter(|c| c.center.0 < mark.at.0).max_by(|a, b| a.center.0.total_cmp(&b.center.0)).unwrap();
        for mark in &layout.marks {
            assert!(mark.at.0 < layout.size.0 && (mark.at.1 - bottom).abs() < 1.0, "on the device, in line with the bottom row");
            assert!(mark.at.0 - beside(mark).center.0 < 70.0, "close to its button");
        }
        // Rewind and fast forward are printed beside buttons 3 and 4, which send notes 91 and 92 in MC mode.
        let note_of = |what: Printed| beside(layout.marks.iter().find(|m| m.what == what).unwrap()).press;
        assert_eq!(note_of(Printed::Rewind), Some(Src::Note(91)));
        assert_eq!(note_of(Printed::FastForward), Some(Src::Note(92)));
        assert_eq!(note_of(Printed::Record), Some(Src::Note(95)));
        assert_eq!(note_of(Printed::Text("LOOP")), Some(Src::Note(86)));
        // And what is printed is what the preset makes them do on layer A.
        let (preset, _) = mini();
        let does = |what: Printed| mapping(&preset, Some("A"), beside(layout.marks.iter().find(|m| m.what == what).unwrap())).press;
        for (what, action) in [
            (Printed::Rewind, Action::RangeBack),
            (Printed::FastForward, Action::RangeForward),
            (Printed::Text("LOOP"), Action::Loop),
            (Printed::Stop, Action::Stop),
            (Printed::Play, Action::Listen),
            (Printed::Record, Action::Record),
        ] {
            assert_eq!(does(what), Some(Meaning::Action(action)), "{what:?}");
        }
    }

    #[test]
    fn the_compact_has_what_the_photograph_shows() {
        let (_, layout) = device(COMPACT);
        let count = |kind| layout.controls.iter().filter(|c| c.kind == kind).count();
        assert_eq!((count(ControlKind::Encoder), count(ControlKind::Fader), count(ControlKind::Layer)), (16, 9, 0));
        // REC, SOLO, MUTE and SELECT of eight strips, the one under the ninth fader, six transport and two LAYER buttons.
        assert_eq!(count(ControlKind::Button), 4 * 8 + 1 + 6 + 2);
        assert!(layout.controls[..8].iter().all(|c| c.kind == ControlKind::Encoder && c.turn.is_some()), "the encoders of the top row come first");
    }

    #[test]
    fn the_built_in_presets_bind_nothing_the_pictures_lack() {
        let (preset, layout) = mini();
        assert!(layout.unplaced(&preset).is_empty(), "the Mini's preset is all on the picture");
        let (preset, layout) = device(COMPACT);
        let mut lost: Vec<String> = layout.unplaced(&preset).iter().map(|b| b.input.src.text()).collect();
        lost.sort();
        assert!(lost.is_empty(), "the Compact's preset binds nothing the picture lacks: {lost:?}");
    }

    #[test]
    fn no_message_is_sent_by_two_controls_and_names_are_unique() {
        for name in [MINI, COMPACT] {
            let (_, layout) = device(name);
            let mut all = sources(&layout);
            let n = all.len();
            all.sort();
            all.dedup();
            assert_eq!(all.len(), n, "{name}: a message is sent by two controls");
            let names: std::collections::BTreeSet<&str> = layout.controls.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(names.len(), layout.controls.len(), "{name}: names are unique");
        }
    }

    #[test]
    fn the_controls_lie_on_the_device_and_not_on_each_other() {
        let rect = |c: &Control| (c.center.0 - c.size.0 / 2.0, c.center.1 - c.size.1 / 2.0, c.center.0 + c.size.0 / 2.0, c.center.1 + c.size.1 / 2.0);
        for name in [MINI, COMPACT] {
            let (_, layout) = device(name);
            for c in &layout.controls {
                let r = rect(c);
                assert!(r.0 >= 0.0 && r.1 >= 0.0 && r.2 <= layout.size.0 && r.3 <= layout.size.1, "{name}: {} is off the device", c.name);
            }
            for (i, a) in layout.controls.iter().enumerate() {
                for b in &layout.controls[i + 1..] {
                    let (ra, rb) = (rect(a), rect(b));
                    let apart = ra.2 <= rb.0 || rb.2 <= ra.0 || ra.3 <= rb.1 || rb.3 <= ra.1;
                    assert!(apart, "{name}: {} overlaps {}", a.name, b.name);
                }
            }
        }
    }

    #[test]
    fn every_binding_and_layer_of_the_mini_is_on_a_control() {
        let (preset, layout) = mini();
        let known = sources(&layout);
        for (layer, src) in &preset.layers {
            assert!(known.contains(src), "layer {layer} is on no control");
        }
        assert!(layout.unplaced(&preset).is_empty());
    }

    #[test]
    fn every_light_is_on_the_control_it_belongs_to() {
        for name in [MINI, COMPACT] {
            let (preset, layout) = device(name);
            for light in &preset.lights {
                let layer = light.layer.as_deref();
                if light.ring {
                    // A ring is addressed by the number of its encoder, cc 48 + n.
                    let Out::Cc(cc) = light.out else { panic!("a ring is set with a cc") };
                    let encoder = &layout.controls[(cc - 48) as usize];
                    assert_eq!(encoder.kind, ControlKind::Encoder);
                    assert_eq!(mapping(&preset, layer, encoder).turn, Some(Meaning::Action(light.action)), "{name}: ring of {}", encoder.name);
                } else {
                    let Out::Note(note) = light.out else { panic!("a light is set with a note") };
                    let button = layout.controls.iter().find(|c| c.press == Some(Src::Note(note))).expect("a button with that note");
                    assert_eq!(button.kind, ControlKind::Button);
                    assert_eq!(mapping(&preset, layer, button).press, Some(Meaning::Action(light.action)), "{name}: light of {}", button.name);
                }
            }
        }
    }

    #[test]
    fn a_picture_is_made_from_any_preset_that_binds_something() {
        assert!(generated(&Preset::default()).is_none(), "nothing to show");
        for (name, text) in crate::controller::BUILTIN {
            let preset = Preset::parse(text);
            let layout = generated(&preset).unwrap_or_else(|| panic!("{name}"));
            assert!(layout.generated && layout.name == preset.name, "{name}");
            // Every control the preset binds, and every layer button, is on the picture, once.
            let known = sources(&layout);
            for b in &preset.binds {
                assert!(known.contains(&b.input.src), "{name}: {} is not on the picture", b.input.src.text());
            }
            for (layer, src) in &preset.layers {
                assert!(known.contains(src), "{name}: layer {layer} is not on the picture");
            }
            assert!(layout.unplaced(&preset).is_empty(), "{name}");
            let mut all = known.clone();
            all.sort();
            all.dedup();
            assert_eq!(all.len(), known.len(), "{name}: a message is on two controls");
            // On the device, not on each other.
            let rect = |c: &Control| (c.center.0 - c.size.0 / 2.0, c.center.1 - c.size.1 / 2.0, c.center.0 + c.size.0 / 2.0, c.center.1 + c.size.1 / 2.0);
            for (i, a) in layout.controls.iter().enumerate() {
                let r = rect(a);
                assert!(r.0 >= 0.0 && r.1 >= 0.0 && r.2 <= layout.size.0 && r.3 <= layout.size.1, "{name}: {} is off the picture", a.name);
                for b in &layout.controls[i + 1..] {
                    let rb = rect(b);
                    assert!(r.2 <= rb.0 || rb.2 <= r.0 || r.3 <= rb.1 || rb.3 <= r.1, "{name}: {} overlaps {}", a.name, b.name);
                }
            }
        }
    }

    #[test]
    fn the_generated_picture_sorts_controls_by_what_they_are() {
        let preset = Preset::parse(&crate::controller::builtin_text("Korg nanoKONTROL2").unwrap());
        let layout = generated(&preset).unwrap();
        let kind = |name: &str| layout.controls.iter().find(|c| c.name.contains(name)).unwrap_or_else(|| panic!("{name}")).kind;
        assert_eq!(kind("cc 16"), ControlKind::Encoder, "a knob that sets tempo");
        assert_eq!(kind("cc 0)"), ControlKind::Fader, "the volume of a group is a fader");
        assert_eq!(kind("cc 32"), ControlKind::Button, "solo");
        // Knobs above faders above buttons, each group in order of message number.
        let top = |k: ControlKind| layout.controls.iter().filter(|c| c.kind == k).map(|c| c.center.1).fold(f32::MAX, f32::min);
        assert!(top(ControlKind::Encoder) < top(ControlKind::Fader) && top(ControlKind::Fader) < top(ControlKind::Button));
        let knob_numbers: Vec<Src> = layout.controls.iter().filter(|c| c.kind == ControlKind::Encoder).filter_map(|c| c.turn).collect();
        let mut sorted = knob_numbers.clone();
        sorted.sort();
        assert_eq!(knob_numbers, sorted);
        // A relative cc is an encoder, a fader (bend) is a fader, and the layer buttons of a preset are on it.
        let p = Preset::parse("layer = A note 1\nlayer = B note 2\nbind = A cc 5 rel twos -> tempo\nbind = B bend 1 -> music_volume\nbind = A note 9 -> record\n");
        let layout = generated(&p).unwrap();
        let count = |k: ControlKind| layout.controls.iter().filter(|c| c.kind == k).count();
        assert_eq!((count(ControlKind::Encoder), count(ControlKind::Fader), count(ControlKind::Button), count(ControlKind::Layer)), (1, 1, 1, 2));
        // What a control does on each layer is found as for a drawn picture.
        let encoder = layout.controls.iter().find(|c| c.kind == ControlKind::Encoder).unwrap();
        assert_eq!(mapping(&p, Some("A"), encoder).turn, Some(Meaning::Action(Action::Tempo)));
        assert_eq!(mapping(&p, Some("B"), encoder).turn, None);
        // Many controls wrap into rows and the picture grows with them.
        let many: String = (0..20).map(|n| format!("bind = note {n} -> record\n")).collect();
        let tall = generated(&Preset::parse(&many)).unwrap();
        let short = generated(&Preset::parse("bind = note 1 -> record\n")).unwrap();
        assert!(tall.size.1 > short.size.1 && tall.size.0 == short.size.0);
    }

    #[test]
    fn what_a_mini_control_does_depends_on_the_layer() {
        let (preset, layout) = mini();
        let record = find(&layout, "Bottom row, button 8");
        assert_eq!(mapping(&preset, Some("A"), record).press, Some(Meaning::Action(Action::Record)));
        assert_eq!(mapping(&preset, Some("B"), record).press, Some(Meaning::Action(Action::Solo(7))));
        let shift = find(&layout, "Top row, button 1");
        assert_eq!(mapping(&preset, Some("A"), shift).press, Some(Meaning::Action(Action::Shift)));
        assert_eq!(mapping(&preset, Some("B"), shift).press, Some(Meaning::Action(Action::Mute(0))));
        let a = mapping(&preset, Some("A"), find(&layout, "Encoder 1"));
        assert_eq!((a.turn, a.press), (Some(Meaning::Action(Action::Tempo)), Some(Meaning::Action(Action::TempoReset))));
        assert_eq!(mapping(&preset, Some("B"), find(&layout, "Fader")).turn, Some(Meaning::Action(Action::MusicVolume)));
        assert_eq!(mapping(&preset, None, find(&layout, "Layer B")).press, Some(Meaning::Layer("B".into())));
    }

    #[test]
    fn the_compact_controls_do_what_its_preset_says() {
        let (preset, layout) = device(COMPACT);
        let press = |name: &str| mapping(&preset, None, find(&layout, name)).press;
        let turn = |name: &str| mapping(&preset, None, find(&layout, name)).turn;
        assert_eq!(press("Strip 1 · REC"), Some(Meaning::Action(Action::Audition(0))));
        assert_eq!(press("Strip 6 · SOLO"), Some(Meaning::Action(Action::Solo(5))));
        assert_eq!(press("Strip 8 · MUTE"), Some(Meaning::Action(Action::Mute(7))));
        assert_eq!(turn("Fader 3"), Some(Meaning::Action(Action::Gain(2))));
        assert_eq!(turn("Fader 9 (main)"), Some(Meaning::Action(Action::MusicVolume)));
        assert_eq!(press("Play"), Some(Meaning::Action(Action::Listen)));
        assert_eq!(press("Record"), Some(Meaning::Action(Action::Record)));
        assert_eq!(press("Cycle"), Some(Meaning::Action(Action::Loop)));
        assert_eq!(press("Rewind"), Some(Meaning::Action(Action::RangeBack)));
        assert_eq!(press("Strip 1 · SELECT"), Some(Meaning::Action(Action::Shift)));
        assert_eq!(press("Strip 2 · SELECT"), Some(Meaning::Action(Action::Click)));
        assert_eq!(press("Strip 8 · SELECT"), Some(Meaning::Action(Action::NextTab)));
        assert_eq!(press("Layer A button"), None);
        assert_eq!(turn("Side encoder 1"), None);
        assert_eq!(turn("Encoder 8"), Some(Meaning::Action(Action::Score)));
        assert_eq!(turn("Encoder 7"), None, "the seventh encoder is not used");
    }

    #[test]
    fn lights_and_rings_are_found_per_layer() {
        let (preset, _) = mini();
        assert!(is_lit(&preset, Some("A"), Action::Record, false));
        assert!(!is_lit(&preset, Some("A"), Action::Mute(0), false));
        assert!(is_lit(&preset, Some("B"), Action::Mute(0), false));
        assert!(is_lit(&preset, Some("A"), Action::Tempo, true));
        assert!(!is_lit(&preset, Some("A"), Action::Score, true), "no ring shows the recent scores");
        let (compact, _) = device(COMPACT);
        assert!(is_lit(&compact, None, Action::Mute(7), false) && is_lit(&compact, None, Action::Tempo, true));
    }

    #[test]
    fn every_action_the_built_in_presets_use_has_a_title_and_an_explanation() {
        for name in [MINI, COMPACT] {
            let (preset, _) = device(name);
            for b in &preset.binds {
                assert!(!b.action.title().is_empty(), "{}", b.action.text());
                assert!(b.action.help().ends_with('.'), "{}", b.action.text());
            }
        }
        assert_eq!(Action::Mute(5).title(), "Mute Tom 3/4");
    }
}
