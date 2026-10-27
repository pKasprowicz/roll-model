//! What a controller looks like: where its buttons, encoders and fader are, and which MIDI
//! message each one sends. Together with a preset (`controller.rs`) this gives what every
//! control does on a layer, for the help screen. Pure geometry and lookups; the drawing is
//! in `app/controller_help.rs`.
//!
//! Layouts: the Behringer X-Touch Mini and the X-Touch Compact. Both are drawn from memory of the
//! devices: both are drawn from photographs of their fronts (positions measured on the photo, the
//! height of the Mini stretched to make room for labels).

use crate::controller::{Action, Binding, Preset, Src};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
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
    pub kind: Kind,
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
    pub name: &'static str,
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
        preset.binds.iter().filter(|b| !known.contains(&b.src)).collect()
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

/// Behringer X-Touch Mini: a row of eight encoders, two rows of eight buttons below them, the
/// two LAYER buttons and the fader on the right.
pub fn x_touch_mini() -> Layout {
    let mut controls = Vec::new();
    let x = |i: usize| COLUMN_X + i as f32 * COLUMN_PITCH;
    for i in 0..COLUMNS {
        controls.push(Control {
            kind: Kind::Encoder,
            name: format!("Encoder {}", i + 1),
            center: (x(i), 95.0),
            size: (62.0, 62.0),
            press: Some(Src::Note(ENCODER_PUSH + i as u8)),
            turn: Some(Src::Cc(ENCODER_CC + i as u8)),
        });
    }
    for (row, notes, y) in [("Top row", &TOP_ROW, 200.0), ("Bottom row", &BOTTOM_ROW, 273.0)] {
        for (i, note) in notes.iter().enumerate() {
            controls.push(Control {
                kind: Kind::Button,
                name: format!("{row}, button {}", i + 1),
                center: (x(i), y),
                size: (62.0, 48.0),
                press: Some(Src::Note(*note)),
                turn: None,
            });
        }
    }
    // The LAYER buttons sit in line with the two rows of buttons.
    for (i, note) in LAYER_NOTES.iter().enumerate() {
        controls.push(Control {
            kind: Kind::Layer,
            name: format!("Layer {}", ["A", "B"][i]),
            center: (897.0, 200.0 + i as f32 * 73.0),
            size: (62.0, 48.0),
            press: Some(Src::Note(*note)),
            turn: None,
        });
    }
    controls.push(Control { kind: Kind::Indicator, name: "MC MODE".into(), center: (897.0, 133.0), size: (74.0, 44.0), press: None, turn: None });
    controls.push(Control { kind: Kind::Fader, name: "Fader".into(), center: (817.0, 182.0), size: (44.0, 238.0), press: None, turn: Some(Src::Bend(1)) });
    // What is printed beside the buttons of the bottom row.
    let printed = [Some(Printed::Text("MC")), None, Some(Printed::Rewind), Some(Printed::FastForward), Some(Printed::Text("LOOP")), Some(Printed::Stop), Some(Printed::Play), Some(Printed::Record)];
    let marks = printed.into_iter().enumerate().filter_map(|(i, what)| Some(Mark { at: (x(i) + 47.0, 273.0), what: what? })).collect();
    Layout { name: "Behringer X-Touch Mini", size: (952.0, 371.0), controls, marks }
}

/// Behringer X-Touch Compact, laid out from a photograph of the front: a row of eight encoders over
/// three rows of eight buttons (REC, SOLO, MUTE in Mackie Control terms), nine faders with a row of
/// buttons under them, eight more encoders and six transport buttons on the right, and the two LAYER
/// buttons at the bottom right. The messages are those of the Mackie Control table; the ones this
/// picture does not know (the right-hand encoders, the LAYER buttons) are left out.
pub fn x_touch_compact() -> Layout {
    let x = |i: usize| 105.0 + i as f32 * 96.0;
    let mut controls = Vec::new();
    // The encoders of the top row come first: the LED ring of encoder n is addressed as cc 48 + n.
    for i in 0..COLUMNS {
        controls.push(Control {
            kind: Kind::Encoder,
            name: format!("Encoder {}", i + 1),
            center: (x(i), 120.0),
            size: (66.0, 66.0),
            press: Some(Src::Note(ENCODER_PUSH + i as u8)),
            turn: Some(Src::Cc(ENCODER_CC + i as u8)),
        });
    }
    // The strip buttons: REC, SOLO, MUTE above the faders, SELECT below them.
    for (word, first, y) in [("REC", 0u8, 222.0), ("SOLO", 8, 277.0), ("MUTE", 16, 332.0), ("SELECT", 24, 840.0)] {
        for i in 0..COLUMNS {
            controls.push(Control {
                kind: Kind::Button,
                name: format!("Strip {} · {word}", i + 1),
                center: (x(i), y),
                size: (72.0, 38.0),
                press: Some(Src::Note(first + i as u8)),
                turn: None,
            });
        }
    }
    for i in 0..COLUMNS {
        controls.push(Control { kind: Kind::Fader, name: format!("Fader {}", i + 1), center: (x(i), 595.0), size: (40.0, 340.0), press: None, turn: Some(Src::Bend(i as u8 + 1)) });
    }
    controls.push(Control { kind: Kind::Fader, name: "Fader 9 (main)".into(), center: (885.0, 595.0), size: (40.0, 340.0), press: None, turn: Some(Src::Bend(9)) });
    controls.push(Control { kind: Kind::Button, name: "Button under fader 9".into(), center: (885.0, 840.0), size: (72.0, 38.0), press: None, turn: None });
    // The eight encoders on the right, in two columns, read row by row. What they send is not known.
    for i in 0..8 {
        controls.push(Control {
            kind: Kind::Encoder,
            name: format!("Side encoder {}", i + 1),
            center: (993.0 + (i % 2) as f32 * 97.0, 120.0 + (i / 2) as f32 * 109.0),
            size: (66.0, 66.0),
            press: None,
            turn: None,
        });
    }
    // The six transport buttons (their order is a guess), then the LAYER buttons.
    let transport: [(&str, u8); 6] = [("Rewind", 91), ("Fast forward", 92), ("Stop", 93), ("Play", 94), ("Record", 95), ("Cycle", 86)];
    for (i, (name, note)) in transport.into_iter().enumerate() {
        controls.push(Control {
            kind: Kind::Button,
            name: name.into(),
            center: (993.0 + (i % 2) as f32 * 95.0, 587.0 + (i / 2) as f32 * 78.0),
            size: (72.0, 40.0),
            press: Some(Src::Note(note)),
            turn: None,
        });
    }
    for (i, name) in ["Layer A button", "Layer B button"].into_iter().enumerate() {
        controls.push(Control { kind: Kind::Button, name: name.into(), center: (993.0 + i as f32 * 95.0, 840.0), size: (72.0, 38.0), press: None, turn: None });
    }
    Layout { name: "Behringer X-Touch Compact", size: (1200.0, 910.0), controls, marks: Vec::new() }
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
        preset.position(layer, src).map(|i| Meaning::Action(preset.binds[i].action))
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
        assert_eq!(for_preset(&load_preset(MINI, None).unwrap()).map(|l| l.name), Some(MINI));
        assert_eq!(for_preset(&load_preset(COMPACT, None).unwrap()).map(|l| l.name), Some(COMPACT));
        assert!(for_preset(&Preset::default()).is_none());
    }

    #[test]
    fn the_mini_has_every_control_of_the_device_once() {
        let (_, layout) = mini();
        let count = |kind| layout.controls.iter().filter(|c| c.kind == kind).count();
        assert_eq!((count(Kind::Encoder), count(Kind::Button), count(Kind::Layer), count(Kind::Fader), count(Kind::Indicator)), (8, 16, 2, 1, 1));
        let y = |name: &str| find(&layout, name).center.1;
        assert_eq!((y("Layer A"), y("Layer B")), (y("Top row, button 1"), y("Bottom row, button 1")), "the LAYER buttons are in line with the rows");
    }

    #[test]
    fn what_is_printed_beside_the_minis_bottom_row_is_where_the_photograph_has_it() {
        let (_, layout) = mini();
        let bottom = find(&layout, "Bottom row, button 1").center.1;
        let kinds: Vec<Printed> = layout.marks.iter().map(|m| m.what).collect();
        assert_eq!(kinds, [Printed::Text("MC"), Printed::Rewind, Printed::FastForward, Printed::Text("LOOP"), Printed::Stop, Printed::Play, Printed::Record]);
        let bottom_row: Vec<&Control> = layout.controls.iter().filter(|c| c.kind == Kind::Button && c.center.1 == bottom).collect();
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
        assert_eq!((count(Kind::Encoder), count(Kind::Fader), count(Kind::Layer)), (16, 9, 0));
        // REC, SOLO, MUTE and SELECT of eight strips, the one under the ninth fader, six transport and two LAYER buttons.
        assert_eq!(count(Kind::Button), 4 * 8 + 1 + 6 + 2);
        assert!(layout.controls[..8].iter().all(|c| c.kind == Kind::Encoder && c.turn.is_some()), "the encoders of the top row come first");
    }

    #[test]
    fn the_built_in_presets_bind_nothing_the_pictures_lack() {
        let (preset, layout) = mini();
        assert!(layout.unplaced(&preset).is_empty(), "the Mini's preset is all on the picture");
        let (preset, layout) = device(COMPACT);
        let mut lost: Vec<String> = layout.unplaced(&preset).iter().map(|b| b.src.text()).collect();
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
                    assert_eq!(encoder.kind, Kind::Encoder);
                    assert_eq!(mapping(&preset, layer, encoder).turn, Some(Meaning::Action(light.action)), "{name}: ring of {}", encoder.name);
                } else {
                    let Out::Note(note) = light.out else { panic!("a light is set with a note") };
                    let button = layout.controls.iter().find(|c| c.press == Some(Src::Note(note))).expect("a button with that note");
                    assert_eq!(button.kind, Kind::Button);
                    assert_eq!(mapping(&preset, layer, button).press, Some(Meaning::Action(light.action)), "{name}: light of {}", button.name);
                }
            }
        }
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
