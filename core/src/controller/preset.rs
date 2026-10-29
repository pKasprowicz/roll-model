//! The preset format: parse, write, and the presets that come with the application.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::action::ACTIONS;
use super::mapper::Learned;
use super::{Action, Encoding, LoadPresetError, Out, PresetError, Reading, Src};

#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    /// `None`: on every layer.
    pub layer: Option<String>,
    pub src: Src,
    /// Only messages on this MIDI channel (1-based); `None`: on any channel.
    pub channel: Option<u8>,
    /// Absolute, or an endless encoder and how it counts (`Mackie` unless the line says otherwise).
    pub reading: Reading,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub layer: Option<String>,
    pub action: Action,
    pub out: Out,
    /// A ring around an encoder showing a level, rather than a button light.
    pub ring: bool,
    /// The MIDI channel the light is set on (1-based).
    pub channel: u8,
    /// Values sent to light and to darken a button (127 and 0 unless the line says otherwise).
    pub on: u8,
    pub off: u8,
    /// A ring shows its level as a plain value 0..=scale; `None`: Mackie ring values.
    pub scale: Option<u8>,
}

/// What a preset's controller speaks: Mackie Control (layers, ring and light values as the
/// Mackie table has them) or plain MIDI (notes, cc and pitch bend as the device sends them).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum PresetKind {
    #[default]
    Mackie,
    Midi,
}

impl PresetKind {
    pub fn parse(word: &str) -> Option<PresetKind> {
        match word.trim().to_ascii_lowercase().as_str() {
            "mackie" => Some(PresetKind::Mackie),
            "midi" => Some(PresetKind::Midi),
            _ => None,
        }
    }

    /// The word used in preset files.
    pub fn word(self) -> &'static str {
        match self {
            PresetKind::Mackie => "mackie",
            PresetKind::Midi => "midi",
        }
    }

    /// For the screen.
    pub fn label(self) -> &'static str {
        match self {
            PresetKind::Mackie => "Mackie Control",
            PresetKind::Midi => "Generic MIDI",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preset {
    pub name: String,
    /// Part of the MIDI port's name.
    pub port: String,
    /// Mackie Control or plain MIDI; a ring of a MIDI preset shows a plain value 0 to 127 unless `scale=` says otherwise.
    pub kind: PresetKind,
    /// Layer names, in the order of their selecting messages.
    pub layers: Vec<(String, Src)>,
    pub binds: Vec<Binding>,
    pub lights: Vec<Light>,
    /// Lines that could not be used, with their line numbers.
    pub warnings: Vec<String>,
}

/// Takes "ch N" (a MIDI channel from 1 to 16) out of the words of a line.
fn take_channel(words: &mut Vec<&str>) -> Result<Option<u8>, PresetError> {
    let Some(i) = words.iter().position(|w| w.eq_ignore_ascii_case("ch")) else { return Ok(None) };
    let n: u8 = words.get(i + 1).and_then(|w| w.parse().ok()).filter(|n| (1..=16).contains(n)).ok_or(PresetError::BadChannel)?;
    words.drain(i..=i + 1);
    Ok(Some(n))
}

fn split_layer<'a>(words: &mut Vec<&'a str>, first_is_not: &[&str]) -> Option<String> {
    let first = words.first()?;
    if first_is_not.iter().any(|k| first.eq_ignore_ascii_case(k)) {
        return None;
    }
    Some(words.remove(0).to_uppercase())
}

impl Preset {
    pub fn parse(text: &str) -> Preset {
        let mut p = Preset::default();
        let kind_given = text.lines().filter_map(|l| l.split('#').next()?.split_once('=')).any(|(key, _)| key.trim() == "kind");
        for (i, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let result = match line.split_once('=') {
                None => Err(PresetError::ExpectedKeyValue),
                Some((k, v)) => p.line(k.trim(), v.trim()),
            };
            if let Err(e) = result {
                p.warnings.push(format!("line {}: {e}", i + 1));
            }
        }
        p.validate();
        if !kind_given {
            // A file from before presets said what they speak: layers and lights are Mackie Control's, plain notes and cc are MIDI.
            p.kind = if p.layers.is_empty() && p.lights.is_empty() { PresetKind::Midi } else { PresetKind::Mackie };
        }
        p
    }

    fn line(&mut self, key: &str, value: &str) -> Result<(), PresetError> {
        match key {
            "name" => self.name = value.to_string(),
            "port" => self.port = value.to_string(),
            "kind" => self.kind = PresetKind::parse(value).ok_or_else(|| PresetError::UnknownKind(value.to_string()))?,
            "layer" => self.parse_layer(value)?,
            "bind" => self.parse_bind(value)?,
            "led" => self.parse_light(false, value)?,
            "ring" => self.parse_light(true, value)?,
            other => return Err(PresetError::UnknownKey(other.to_string())),
        }
        Ok(())
    }

    /// `layer = NAME note N`
    fn parse_layer(&mut self, value: &str) -> Result<(), PresetError> {
        let mut w: Vec<&str> = value.split_whitespace().collect();
        if w.len() < 2 {
            return Err(PresetError::ExpectedLayer);
        }
        let name = w.remove(0).to_uppercase();
        self.layers.push((name, Src::parse(&w)?));
        Ok(())
    }

    /// `bind = [LAYER] note|cc|bend N [ch C] [abs | rel [ENCODING]] -> action`
    fn parse_bind(&mut self, value: &str) -> Result<(), PresetError> {
        let (left, right) = value.split_once("->").ok_or(PresetError::ExpectedArrow)?;
        let action = Action::parse(right).ok_or_else(|| PresetError::UnknownAction(right.trim().to_string()))?;
        let mut w: Vec<&str> = left.split_whitespace().collect();
        let layer = split_layer(&mut w, &["note", "cc", "bend"]);
        let channel = take_channel(&mut w)?;
        let mut encoding = Encoding::Mackie;
        if let Some(e) = w.last().and_then(|l| Encoding::parse(l)) {
            w.pop();
            if !w.last().is_some_and(|x| x.eq_ignore_ascii_case("rel")) {
                return Err(PresetError::EncodingNeedsRel(e.word()));
            }
            encoding = e;
        }
        let mut relative = false;
        if let Some(last) = w.last() {
            if last.eq_ignore_ascii_case("rel") {
                relative = true;
                w.pop();
            } else if last.eq_ignore_ascii_case("abs") {
                w.pop();
            }
        }
        let src = Src::parse(&w)?;
        if relative && !matches!(src, Src::Cc(_)) {
            return Err(PresetError::OnlyCcRelative);
        }
        if matches!(src, Src::Bend(_)) && !action.is_level() {
            return Err(PresetError::FaderOnlyValue);
        }
        if action.is_browse() && !relative {
            return Err(PresetError::OnlyTurned(action.text()));
        }
        if let (Src::Bend(c), Some(_)) = (src, channel) {
            return Err(PresetError::FaderChannel(c));
        }
        let reading = if relative { Reading::Relative(encoding) } else { Reading::Absolute };
        self.binds.push(Binding { layer, src, channel, reading, action });
        Ok(())
    }

    /// `led = [LAYER] action -> note|cc N [ch C] [on=V] [off=V]` and `ring = ... [scale=V]`
    fn parse_light(&mut self, ring: bool, value: &str) -> Result<(), PresetError> {
        let (left, right) = value.split_once("->").ok_or(PresetError::ExpectedArrow)?;
        let mut w: Vec<&str> = left.split_whitespace().collect();
        let layer = split_layer(&mut w, &ACTIONS.iter().map(|i| i.name).chain(["mute", "solo", "gain", "audition"]).collect::<Vec<_>>());
        let action = Action::parse(&w.join(" ")).ok_or_else(|| PresetError::UnknownAction(w.join(" ")))?;
        if ring && !action.is_level() || !ring && !action.has_light() {
            return Err(PresetError::CannotBeShown { action: action.text(), ring });
        }
        let mut r: Vec<&str> = right.split_whitespace().collect();
        let channel = take_channel(&mut r)?.unwrap_or(1);
        let (mut on, mut off, mut scale) = (127u8, 0u8, None);
        let mut words = Vec::new();
        for word in r {
            let Some((name, number)) = word.split_once('=') else {
                words.push(word);
                continue;
            };
            let n: u8 = number.parse().ok().filter(|n| *n <= 127).ok_or_else(|| PresetError::BadLightNumber(word.to_string()))?;
            match (name.to_ascii_lowercase().as_str(), ring) {
                ("on", false) => on = n,
                ("off", false) => off = n,
                ("scale", true) => scale = Some(n),
                ("on" | "off", true) => return Err(PresetError::RingTakesScale(name.to_string())),
                ("scale", false) => return Err(PresetError::LightTakesOnOff),
                _ => return Err(PresetError::UnknownSetting(word.to_string())),
            }
        }
        let out = match Src::parse(&words)? {
            Src::Note(n) => Out::Note(n),
            Src::Cc(n) => Out::Cc(n),
            Src::Bend(_) => return Err(PresetError::LightNeedsNoteOrCc),
        };
        self.lights.push(Light { layer, action, out, ring, channel, on, off, scale });
        Ok(())
    }

    /// The binding of `src` on `layer` (`None`: only the lines for every layer) for a message on
    /// MIDI channel `channel` (`None`: any binding, whatever its channel, as a picture of the
    /// controller needs). The line for the layer wins over one for all layers, and a line for the
    /// channel over one for any channel.
    pub fn position(&self, layer: Option<&str>, src: Src, channel: Option<u8>) -> Option<usize> {
        self.binds
            .iter()
            .enumerate()
            .filter(|(_, b)| b.src == src && channel.map_or(true, |c| b.channel.map_or(true, |bc| bc == c)))
            .filter_map(|(i, b)| {
                let layer_rank = if layer.is_some() && b.layer.as_deref() == layer {
                    2
                } else if b.layer.is_none() {
                    1
                } else {
                    return None;
                };
                let exact_channel = b.channel.is_some() && b.channel == channel;
                // The best rank wins; among equals the first line of the file.
                Some(((layer_rank, exact_channel), std::cmp::Reverse(i)))
            })
            .max()
            .map(|(_, std::cmp::Reverse(i))| i)
    }

    /// The preset as a file: what `Preset::parse` reads back as the same preset (comments are lost).
    pub fn to_text(&self) -> String {
        let mut s = String::from("# Edited in Roll Model; the lines are explained in the README (\"Your own preset\").\n");
        s.push_str(&format!("name = {}\nport = {}\nkind = {}\n", self.name, self.port, self.kind.word()));
        for (name, src) in &self.layers {
            s.push_str(&format!("layer = {name} {}\n", src.text()));
        }
        for b in &self.binds {
            let mut left = String::new();
            if let Some(layer) = &b.layer {
                left.push_str(&format!("{layer} "));
            }
            left.push_str(&b.src.text());
            if let Some(ch) = b.channel {
                left.push_str(&format!(" ch {ch}"));
            }
            if matches!(b.src, Src::Cc(_)) {
                match b.reading {
                    Reading::Absolute => left.push_str(" abs"),
                    Reading::Relative(Encoding::Mackie) => left.push_str(" rel"),
                    Reading::Relative(e) => left.push_str(&format!(" rel {}", e.word())),
                }
            }
            s.push_str(&format!("bind = {left} -> {}\n", b.action.text()));
        }
        for l in &self.lights {
            let key = if l.ring { "ring" } else { "led" };
            let layer = l.layer.as_ref().map(|x| format!("{x} ")).unwrap_or_default();
            let out = match l.out {
                Out::Note(n) => format!("note {n}"),
                Out::Cc(n) => format!("cc {n}"),
            };
            let mut right = out;
            if l.channel != 1 {
                right.push_str(&format!(" ch {}", l.channel));
            }
            if let Some(scale) = l.scale {
                right.push_str(&format!(" scale={scale}"));
            }
            if !l.ring && (l.on, l.off) != (127, 0) {
                right.push_str(&format!(" on={} off={}", l.on, l.off));
            }
            s.push_str(&format!("{key} = {layer}{} -> {right}\n", l.action.text()));
        }
        s
    }

    /// The bindings of `action` on exactly `layer` (`None`: the lines for every layer).
    pub fn bindings_of(&self, layer: Option<&str>, action: Action) -> Vec<&Binding> {
        self.binds.iter().filter(|b| b.layer.as_deref() == layer && b.action == action).collect()
    }

    /// Binds `action` to the control `found` on `layer`. The control is taken from whatever it was
    /// bound to on that layer (returned), and the action's old control of the same kind (note, cc or
    /// fader) is replaced; a control of another kind, such as a fader next to an encoder, stays.
    pub fn assign(&mut self, layer: Option<&str>, action: Action, found: Learned) -> Option<Action> {
        let layer = layer.map(str::to_string);
        let kind = |src: Src| match src {
            Src::Note(_) => 0,
            Src::Cc(_) => 1,
            Src::Bend(_) => 2,
        };
        let mut taken_from = None;
        self.binds.retain(|b| {
            let same = b.layer == layer && b.src == found.src && b.channel == found.channel;
            if same && b.action != action {
                taken_from = Some(b.action);
            }
            !same
        });
        self.binds.retain(|b| !(b.layer == layer && b.action == action && kind(b.src) == kind(found.src)));
        self.binds.push(Binding { layer, src: found.src, channel: found.channel, reading: found.reading, action });
        taken_from
    }

    /// Takes every control of `action` on `layer` away.
    pub fn clear(&mut self, layer: Option<&str>, action: Action) {
        self.binds.retain(|b| !(b.layer.as_deref() == layer && b.action == action));
    }

    /// Changes how the cc controls of `action` on `layer` are read: as positions, or as an endless encoder.
    pub fn set_reading(&mut self, layer: Option<&str>, action: Action, reading: Reading) {
        for b in self.binds.iter_mut().filter(|b| b.layer.as_deref() == layer && b.action == action && matches!(b.src, Src::Cc(_))) {
            b.reading = reading;
        }
    }

    /// Drops lines that name a layer the preset does not declare, and reports repeated controls.
    fn validate(&mut self) {
        let known: Vec<String> = self.layers.iter().map(|l| l.0.clone()).collect();
        let ok = |l: &Option<String>| l.as_ref().map_or(true, |l| known.contains(l));
        let before = (self.binds.len(), self.lights.len());
        self.binds.retain(|b| ok(&b.layer));
        self.lights.retain(|l| ok(&l.layer));
        if before != (self.binds.len(), self.lights.len()) {
            self.warnings.push("some lines name a layer that no \"layer =\" line declares; they were ignored".into());
        }
        let mut seen: BTreeMap<(Option<String>, Src, Option<u8>), Action> = BTreeMap::new();
        let mut repeated = Vec::new();
        for b in &self.binds {
            if let Some(first) = seen.insert((b.layer.clone(), b.src, b.channel), b.action) {
                repeated.push(format!("{} is bound to both \"{}\" and \"{}\"", b.src.text(), first.text(), b.action.text()));
            }
        }
        for r in repeated {
            self.warnings.push(format!("{r}; the first one is used"));
        }
    }
}

/// The presets that come with the application: (name, file text).
pub const BUILTIN: &[(&str, &str)] = &[
    ("Behringer X-Touch Mini", include_str!("../../../presets/x-touch-mini.txt")),
    ("Behringer X-Touch Compact", include_str!("../../../presets/x-touch-compact.txt")),
    // Generic MIDI controllers (no Mackie Control); written from memory, not verified on the devices.
    ("Novation Launch Control XL", include_str!("../../../presets/novation-launch-control-xl.txt")),
    ("Korg nanoKONTROL2", include_str!("../../../presets/korg-nanokontrol2.txt")),
    ("Akai MPK Mini", include_str!("../../../presets/akai-mpk-mini.txt")),
    ("Arturia BeatStep", include_str!("../../../presets/arturia-beatstep.txt")),
];

pub fn builtin_text(name: &str) -> Option<&'static str> {
    BUILTIN.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

/// A preset the user wrote: a `.txt` file in the controllers folder, named after its file.
pub fn custom_presets(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<(String, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("txt")))
        .filter_map(|p| Some((p.file_stem()?.to_string_lossy().into_owned(), p)))
        .collect();
    found.sort();
    found
}

/// Where a preset comes from. A preset is known by one name, the one in the list and in the
/// settings: the name of the built-in preset, or the name of the file without `.txt`.
#[derive(Clone, Debug, PartialEq)]
pub enum PresetSource {
    BuiltIn(&'static str),
    File(PathBuf),
}

/// Where the preset called `name` is: a built-in one first (a built-in name always wins over a
/// file of the same name), then a file of `dir`.
pub fn preset_source(name: &str, dir: Option<&Path>) -> Option<PresetSource> {
    if let Some(text) = builtin_text(name) {
        return Some(PresetSource::BuiltIn(text));
    }
    let dir = dir?;
    custom_presets(dir).into_iter().find(|(n, _)| n == name).map(|(_, path)| PresetSource::File(path))
}

/// The preset called `name`. Its `name` is always the name it is known by: for a file, the `name`
/// line is ignored (a renamed file is the same preset under its new name).
pub fn load_preset(name: &str, dir: Option<&Path>) -> Result<Preset, LoadPresetError> {
    let source = preset_source(name, dir).ok_or_else(|| LoadPresetError::NotFound(name.to_string()))?;
    let mut preset = match &source {
        PresetSource::BuiltIn(text) => Preset::parse(text),
        PresetSource::File(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| LoadPresetError::Unreadable { path: path.clone(), reason: e.to_string() })?;
            Preset::parse(&text)
        }
    };
    if matches!(source, PresetSource::File(_)) || preset.name.is_empty() {
        preset.name = name.to_string();
    }
    Ok(preset)
}
