//! From a message to a command, and from the application state to the lights.

use std::collections::BTreeMap;

use super::{Action, Input, Msg, Out, Preset, PresetKind, Reading, Src, State};

/// A control found by pressing or turning it, ready to be bound to an action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Learned {
    pub input: Input,
    pub reading: Reading,
}

/// What a message from the controller says about the control that sent it, when `action` is
/// waiting for one. A button needs a press (not the release), a value a cc or a fader, and the
/// lists (`Score`, `Rudiment`) an endless encoder. `reading` is the choice for
/// a cc that sets a value; `remember_channel` limits the binding to the channel the message came on.
/// `None`: not a control this action can use (a note off, a note for a knob's value, ...).
pub fn learn(msg: Msg, action: Action, reading: Reading, remember_channel: bool) -> Option<Learned> {
    let channel = |src: Src| (remember_channel && !matches!(src, Src::Bend(_))).then(|| msg.channel());
    let value_like = action.is_level() || action.is_browse();
    let found = |src: Src, channel: Option<u8>, reading: Reading| Some(Learned { input: Input { src, channel }, reading });
    match msg {
        Msg::NoteOn { note, vel, .. } if vel > 0 && !value_like => found(Src::Note(note), channel(Src::Note(note)), Reading::Absolute),
        Msg::Cc { cc, .. } if value_like => {
            // The lists are only turned.
            let reading = if action.is_browse() { Reading::Relative(reading.encoding()) } else { reading };
            found(Src::Cc(cc), channel(Src::Cc(cc)), reading)
        }
        Msg::Cc { cc, val, .. } if val > 0 => found(Src::Cc(cc), channel(Src::Cc(cc)), Reading::Absolute),
        Msg::Bend { ch, .. } if action.is_level() => found(Src::Bend(ch), None, Reading::Absolute),
        _ => None,
    }
}

/// What the application should do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Press(Action),
    /// Encoder turned by this many steps (negative = counter-clockwise).
    Turn(Action, i32),
    /// Fader moved to this position (0..=1).
    Set(Action, f32),
}

/// A fader that does not move by itself must be caught up with the value it
/// controls before it takes over, or the value would jump.
#[derive(Clone, Copy, Debug, Default)]
struct Pickup {
    engaged: bool,
    last: Option<f32>,
}

impl Pickup {
    /// How near the fader must be to take over, and how far the value may drift
    /// (changed with the mouse, say) before the fader has to catch up again.
    const CATCH: f32 = 0.03;
    const DRIFT: f32 = 0.25;

    fn take(&mut self, pos: f32, target: f32) -> bool {
        if self.engaged && (pos - target).abs() > Self::DRIFT {
            self.engaged = false;
        }
        if !self.engaged {
            let crossed = self.last.is_some_and(|l| (l - target) * (pos - target) <= 0.0);
            self.engaged = (pos - target).abs() <= Self::CATCH || crossed;
        }
        self.last = Some(pos);
        self.engaged
    }
}

/// What a light is told apart by: where it is set, whether it is a ring, and the MIDI channel.
type LightKey = (Out, bool, u8);

pub struct Mapper {
    preset: Preset,
    layer: usize,
    shift: bool,
    /// The control that holds Shift down.
    shift_src: Option<Src>,
    pickups: Vec<Pickup>,
    /// What was last sent to each light (the value and the value that darkens it), so that only changes are sent.
    sent: BTreeMap<LightKey, (u8, u8)>,
}

/// How much one click of an encoder moves a value while Shift is held, relative to normal.
const SHIFT_FACTOR: i32 = 5;

fn pressed(msg: Msg) -> Option<bool> {
    match msg {
        Msg::NoteOn { vel, .. } => Some(vel > 0),
        Msg::NoteOff { .. } => Some(false),
        Msg::Cc { val, .. } => Some(val > 0),
        Msg::Bend { .. } => None,
    }
}

/// The value that makes the LED ring of a Mackie Control show a level as a bar.
pub(super) fn ring_value(level: f32) -> u8 {
    let position = if level <= 0.001 { 0 } else { (level.clamp(0.0, 1.0) * 10.0).round() as u8 + 1 };
    0x20 | position
}

impl Mapper {
    pub fn new(preset: Preset) -> Mapper {
        let pickups = vec![Pickup::default(); preset.binds.len()];
        Mapper { preset, layer: 0, shift: false, shift_src: None, pickups, sent: BTreeMap::new() }
    }

    pub fn preset(&self) -> &Preset {
        &self.preset
    }

    /// Puts an edited preset in place of the one in use. The layer and Shift stay; faders have to
    /// catch up again and every light is sent again.
    pub fn replace_preset(&mut self, preset: Preset) {
        self.layer = self.layer.min(preset.layers.len().saturating_sub(1));
        self.pickups = vec![Pickup::default(); preset.binds.len()];
        self.sent.clear();
        self.preset = preset;
    }

    /// The layer in use, if the preset has layers.
    pub fn layer_name(&self) -> Option<&str> {
        self.preset.layers.get(self.layer).map(|l| l.0.as_str())
    }

    pub fn shift(&self) -> bool {
        self.shift
    }

    fn find(&self, src: Src, channel: u8) -> Option<usize> {
        self.preset.position(self.layer_name(), src, Some(channel))
    }

    /// What the message is bound to in the current layer, for the monitor.
    pub fn bound_to(&self, msg: Msg) -> Option<String> {
        let src = Src::of(msg);
        if self.preset.layers.iter().any(|l| l.1 == src) {
            return Some("layer switch".into());
        }
        self.find(src, msg.channel()).map(|i| self.preset.binds[i].action.text())
    }

    pub fn handle(&mut self, msg: Msg, state: &State) -> Vec<Command> {
        let src = Src::of(msg);
        let down = pressed(msg);
        if let Some(i) = self.preset.layers.iter().position(|l| l.1 == src) {
            if down == Some(true) && i != self.layer {
                self.layer = i;
                self.pickups.iter_mut().for_each(|p| *p = Pickup::default());
            }
            return Vec::new();
        }
        // Shift ends when its own button is let go, also after the layer changed under it.
        if down == Some(false) && self.shift_src == Some(src) {
            self.shift = false;
            self.shift_src = None;
            return Vec::new();
        }
        let Some(i) = self.find(src, msg.channel()) else { return Vec::new() };
        let (action, reading) = (self.preset.binds[i].action, self.preset.binds[i].reading);
        match action {
            Action::Shift => {
                if down == Some(true) {
                    self.shift = true;
                    self.shift_src = Some(src);
                }
                Vec::new()
            }
            a if a.is_level() || a.is_browse() => match msg {
                Msg::Cc { val, .. } if reading.is_relative() => match reading.encoding().decode(val) {
                    0 => Vec::new(),
                    n => vec![Command::Turn(a, if self.shift && !a.is_browse() { n * SHIFT_FACTOR } else { n })],
                },
                Msg::Cc { val, .. } if a.is_level() => self.absolute(i, a, val as f32 / 127.0, state),
                Msg::Bend { value, .. } if a.is_level() => self.absolute(i, a, value as f32 / 16383.0, state),
                _ => Vec::new(),
            },
            a if down == Some(true) => vec![Command::Press(a)],
            _ => Vec::new(),
        }
    }

    fn absolute(&mut self, bind: usize, action: Action, pos: f32, state: &State) -> Vec<Command> {
        let Some(target) = state.level(action) else { return Vec::new() };
        if self.pickups[bind].take(pos, target) {
            vec![Command::Set(action, pos)]
        } else {
            Vec::new()
        }
    }

    /// The messages that bring the controller's lights up to date with `state`:
    /// only what changed since the last call, and a light of another layer is put out.
    pub fn lights(&mut self, state: &State) -> Vec<[u8; 3]> {
        let layer = self.layer_name().map(str::to_string);
        let mut want: BTreeMap<LightKey, (u8, u8)> = BTreeMap::new();
        // Lines for every layer first, so a line for this layer overrides them.
        for specific in [false, true] {
            for l in self.preset.lights.iter().filter(|l| l.layer.is_some() == specific) {
                if specific && l.layer != layer {
                    continue;
                }
                let scale = l.scale.or((self.preset.kind == PresetKind::Midi).then_some(127));
                let value = if l.ring {
                    state.level(l.action).map(|x| match scale {
                        Some(scale) => (x.clamp(0.0, 1.0) * scale as f32).round() as u8,
                        None => ring_value(x),
                    })
                } else if l.action == Action::Shift {
                    Some(if self.shift { l.on } else { l.off })
                } else {
                    state.lit(l.action).map(|on| if on { l.on } else { l.off })
                };
                if let Some(v) = value {
                    want.insert((l.out, l.ring, l.channel), (v, if l.ring { 0 } else { l.off }));
                }
            }
        }
        let mut out = Vec::new();
        let message = |(o, _, ch): LightKey, v: u8| {
            let ch = ch.clamp(1, 16) - 1;
            match o {
                Out::Note(n) => [0x90 | ch, n, v],
                Out::Cc(n) => [0xB0 | ch, n, v],
            }
        };
        for (&key, &(v, off)) in &want {
            if self.sent.get(&key).map(|s| s.0) != Some(v) {
                out.push(message(key, v));
                self.sent.insert(key, (v, off));
            }
        }
        let gone: Vec<LightKey> = self.sent.keys().filter(|k| !want.contains_key(k)).copied().collect();
        for key in gone {
            let off = self.sent.remove(&key).map_or(0, |s| s.1);
            out.push(message(key, off));
        }
        out
    }
}
