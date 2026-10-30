use super::mapper::Learned;
use super::action::ACTIONS;
use super::mapper::ring_value;
use super::*;

fn note(n: u8) -> Msg {
    Msg::NoteOn { ch: 1, note: n, vel: 127 }
}
fn release(n: u8) -> Msg {
    Msg::NoteOn { ch: 1, note: n, vel: 0 }
}
fn cc(n: u8, v: u8) -> Msg {
    Msg::Cc { ch: 1, cc: n, val: v }
}

fn mini() -> Mapper {
    Mapper::new(load_preset("Behringer X-Touch Mini", None).unwrap())
}

#[test]
fn messages_are_parsed() {
    assert_eq!(parse(&[0x90, 89, 127]), Some(Msg::NoteOn { ch: 1, note: 89, vel: 127 }));
    assert_eq!(parse(&[0x89, 5, 0]), Some(Msg::NoteOff { ch: 10, note: 5 }));
    assert_eq!(parse(&[0xB0, 16, 65]), Some(Msg::Cc { ch: 1, cc: 16, val: 65 }));
    assert_eq!(parse(&[0xE0, 0x7F, 0x7F]), Some(Msg::Bend { ch: 1, value: 16383 }));
    assert_eq!(parse(&[0xE2, 0, 0x40]), Some(Msg::Bend { ch: 3, value: 8192 }));
    assert_eq!(parse(&[0xF8]), None, "clock");
    assert_eq!(parse(&[0x90, 3]), None, "truncated");
    assert_eq!(parse(&[]), None);
}

#[test]
fn encoder_turns_are_signed() {
    assert_eq!([1, 3, 63].map(|v| Encoding::Mackie.decode(v)), [1, 3, 63]);
    assert_eq!([65, 67, 127].map(|v| Encoding::Mackie.decode(v)), [-1, -3, -63]);
    assert_eq!(Encoding::Mackie.decode(0), 0);
}

#[test]
fn the_rudiment_actions_can_be_bound_in_a_custom_preset() {
    let p = Preset::parse(
        "layer = A note 84\n\
         bind = A cc 16 rel -> repeats\n\
         bind = A cc 17 rel -> lead\n\
         bind = A cc 18 rel -> rudiment\n\
         bind = A note 70 -> next_rudiment\n\
         bind = A note 71 -> prev_rudiment\n\
         bind = A bend 1 -> repeats\n\
         ring = A repeats -> cc 48\n\
         ring = A lead -> cc 49\n",
    );
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    for text in ["repeats", "lead", "rudiment", "next_rudiment", "prev_rudiment"] {
        assert_eq!(Action::parse(text).unwrap().text(), text);
    }
    // Browsing the rudiments is only turned, like browsing the scores.
    let bad = Preset::parse("layer = A note 84\nbind = A cc 18 -> rudiment\nbind = A bend 1 -> rudiment\n");
    assert_eq!(bad.warnings.len(), 2, "{:?}", bad.warnings);
    assert!(Preset::parse("layer = A note 84\nring = A rudiment -> cc 48\n").warnings.len() == 1, "no ring for a list");

    let mut m = Mapper::new(p);
    let s = State { repeats: 0.25, lead: 0.5, ..State::default() };
    assert!(m.handle(note(84), &s).is_empty(), "the layer switch");
    assert_eq!(m.handle(cc(16, 2), &s), [Command::Turn(Action::Repeats, 2)]);
    assert_eq!(m.handle(cc(18, 65), &s), [Command::Turn(Action::Rudiment, -1)]);
    assert_eq!(m.handle(note(70), &s), [Command::Press(Action::NextRudiment)]);
    assert_eq!(s.level(Action::Repeats), Some(0.25));
    assert_eq!(s.level(Action::Lead), Some(0.5));
    // A fader catches up with the repetitions before it takes over.
    assert!(m.handle(Msg::Bend { ch: 1, value: 0 }, &s).is_empty());
    assert_eq!(m.lights(&s), [[0xB0, 48, ring_value(0.25)], [0xB0, 49, ring_value(0.5)]], "the rings show the values");
}

#[test]
fn built_in_presets_have_no_problems() {
    for (name, text) in BUILTIN {
        let p = Preset::parse(text);
        assert_eq!(&p.name, name);
        assert!(p.warnings.is_empty(), "{name}: {:?}", p.warnings);
        assert!(!p.binds.is_empty() && !p.port.is_empty());
    }
}

#[test]
fn the_mini_has_two_layers_and_every_button_does_something() {
    let p = Preset::parse(builtin_text("Behringer X-Touch Mini").unwrap());
    assert_eq!(p.layers.iter().map(|l| l.0.as_str()).collect::<Vec<_>>(), ["A", "B"]);
    for layer in ["A", "B"] {
        let on_layer = |src: Src| p.binds.iter().any(|b| b.layer.as_deref() == Some(layer) && b.input.src == src);
        let buttons = [89, 90, 40, 41, 42, 43, 44, 45, 87, 88, 91, 92, 86, 93, 94, 95];
        assert!(buttons.iter().all(|n| on_layer(Src::Note(*n))), "layer {layer}");
        assert!((16..24).all(|n| on_layer(Src::Cc(n))), "eight encoders on layer {layer}");
        let pushes = if layer == "A" { vec![32, 39] } else { (32..40).collect() };
        assert!(pushes.iter().all(|n| on_layer(Src::Note(*n))), "encoder pushes on layer {layer}");
        assert!(on_layer(Src::Bend(1)), "the fader on layer {layer}");
    }
}

#[test]
fn a_layer_message_switches_what_the_controls_do() {
    let mut m = mini();
    let s = State::default();
    assert_eq!(m.layer_name(), Some("A"));
    assert_eq!(m.handle(note(95), &s), [Command::Press(Action::Record)]);
    assert!(m.handle(note(85), &s).is_empty(), "the switch itself does nothing else");
    assert_eq!(m.layer_name(), Some("B"));
    assert_eq!(m.handle(note(89), &s), [Command::Press(Action::Mute(0))]);
    assert_eq!(m.handle(note(87), &s), [Command::Press(Action::Solo(0))]);
    assert_eq!(m.handle(cc(18, 65), &s), [Command::Turn(Action::Gain(2), -1)]);
    m.handle(note(84), &s);
    assert_eq!(m.handle(cc(18, 2), &s), [Command::Turn(Action::RangeTo, 2)]);
}

#[test]
fn what_is_printed_on_the_minis_transport_buttons_is_what_they_do_on_layer_a() {
    let mut m = mini();
    let s = State::default();
    // Rewind, fast forward, LOOP, stop, play and record are printed beside buttons 3 to 8 of the bottom row.
    for (note_number, action) in [
        (91, Action::RangeBack),
        (92, Action::RangeForward),
        (86, Action::Loop),
        (93, Action::Stop),
        (94, Action::Listen),
        (95, Action::Record),
    ] {
        assert_eq!(m.handle(note(note_number), &s), [Command::Press(action)], "note {note_number}");
    }
}

#[test]
fn a_button_acts_when_pressed_not_when_released() {
    let mut m = mini();
    let s = State::default();
    assert_eq!(m.handle(note(94), &s), [Command::Press(Action::Listen)]);
    assert!(m.handle(release(94), &s).is_empty());
    assert!(m.handle(Msg::NoteOff { ch: 1, note: 94 }, &s).is_empty());
    assert!(m.handle(note(1), &s).is_empty(), "an unbound button");
}

#[test]
fn shift_makes_encoder_turns_larger_while_held() {
    let mut m = mini();
    let s = State::default();
    assert_eq!(m.handle(cc(16, 1), &s), [Command::Turn(Action::Tempo, 1)]);
    assert!(m.handle(note(89), &s).is_empty());
    assert!(m.shift());
    assert_eq!(m.handle(cc(16, 1), &s), [Command::Turn(Action::Tempo, 5)]);
    assert_eq!(m.handle(cc(16, 65), &s), [Command::Turn(Action::Tempo, -5)]);
    assert_eq!(m.handle(cc(23, 1), &s), [Command::Turn(Action::Score, 1)], "browsing the scores is not sped up");
    m.handle(release(89), &s);
    assert!(!m.shift());
    assert_eq!(m.handle(cc(16, 1), &s), [Command::Turn(Action::Tempo, 1)]);
    // Let go on the other layer (where the same button is something else): Shift still ends.
    m.handle(note(89), &s);
    m.handle(note(85), &s);
    assert!(m.shift());
    assert!(m.handle(release(89), &s).is_empty());
    assert!(!m.shift());
}

#[test]
fn the_compact_has_no_layers_and_its_select_row_holds_the_function_buttons() {
    let mut m = Mapper::new(load_preset("Behringer X-Touch Compact", None).unwrap());
    let s = State::default();
    assert_eq!(m.layer_name(), None);
    assert_eq!(m.handle(note(25), &s), [Command::Press(Action::Click)]);
    assert_eq!(m.handle(note(29), &s), [Command::Press(Action::LoopWorstBar)]);
    assert_eq!(m.handle(note(94), &s), [Command::Press(Action::Listen)]);
    assert!(m.handle(cc(16, 1), &s) == [Command::Turn(Action::Tempo, 1)]);
    // The first SELECT button is Shift, held while turning.
    m.handle(note(24), &s);
    assert!(m.shift());
    assert_eq!(m.handle(cc(16, 1), &s), [Command::Turn(Action::Tempo, 5)]);
    m.handle(release(24), &s);
    assert!(!m.shift());
    // What the Compact does not have is no longer bound.
    for n in [84, 85, 87, 88, 89, 54, 47, 70] {
        assert!(m.handle(note(n), &s).is_empty(), "note {n}");
    }
    assert!(m.handle(cc(60, 1), &s).is_empty(), "there is no jog wheel");
}

#[test]
fn the_generic_midi_presets_use_absolute_knobs_and_plain_buttons() {
    let s = State::default();
    // (preset, a message, what it must do)
    let cases: [(&str, Msg, Option<Command>); 8] = [
        ("Korg nanoKONTROL2", cc(41, 127), Some(Command::Press(Action::Listen))),
        ("Korg nanoKONTROL2", cc(41, 0), None),
        ("Korg nanoKONTROL2", cc(32, 127), Some(Command::Press(Action::Solo(0)))),
        ("Novation Launch Control XL", note(73), Some(Command::Press(Action::Mute(0)))),
        ("Novation Launch Control XL", cc(108, 127), Some(Command::Press(Action::Record))),
        ("Akai MPK Mini", note(36), Some(Command::Press(Action::Record))),
        ("Akai MPK Mini", cc(70, 64), None), // a fader-like control far from the value: waits
        ("Arturia BeatStep", note(52), Some(Command::Press(Action::Mute(0)))),
    ];
    for (name, msg, expected) in cases {
        let mut m = Mapper::new(load_preset(name, None).unwrap());
        let got = m.handle(msg, &s);
        assert_eq!(got.first().copied(), expected, "{name}: {msg:?}");
    }
    // An absolute knob moves a value once it has reached it (here the tempo is 0.5).
    let mut m = Mapper::new(load_preset("Korg nanoKONTROL2", None).unwrap());
    let s = State { tempo: 0.5, ..Default::default() };
    let got = m.handle(cc(16, 64), &s);
    assert!(matches!(got[..], [Command::Set(Action::Tempo, x)] if (x - 0.5).abs() < 0.01), "{got:?}");
    // Controls the Mackie presets rely on are not bound by the generic ones.
    for name in ["Korg nanoKONTROL2", "Novation Launch Control XL", "Akai MPK Mini", "Arturia BeatStep"] {
        let p = load_preset(name, None).unwrap();
        assert!(p.layers.is_empty() && p.lights.is_empty(), "{name}: no layers and no lights");
        assert!(p.warnings.is_empty(), "{name}: {:?}", p.warnings);
    }
}

#[test]
fn every_relative_encoding_counts_its_own_way() {
    // (encoding, values for one step up, three steps up, one step down, three steps down, no turn)
    let table = [
        (Encoding::Mackie, [1, 3, 65, 67, 0]),
        (Encoding::Twos, [1, 3, 127, 125, 0]),
        (Encoding::Offset, [65, 67, 63, 61, 64]),
        (Encoding::SignBit2, [65, 67, 1, 3, 0]),
    ];
    for (e, [up1, up3, down1, down3, none]) in table {
        assert_eq!([up1, up3, down1, down3, none].map(|v| e.decode(v)), [1, 3, -1, -3, 0], "{e:?}");
    }
    assert_eq!(Encoding::Offset.decode(64 + 63), 63);
    assert_eq!(Encoding::Twos.decode(64), -64, "the far end of two's complement");
    for e in [Encoding::Mackie, Encoding::Twos, Encoding::Offset, Encoding::SignBit2] {
        assert_eq!(Encoding::parse(e.word()), Some(e), "the word of {e:?} parses back");
    }
    assert_eq!(Encoding::parse("TWOS"), Some(Encoding::Twos));
    assert_eq!(Encoding::parse("binary"), None);
}

#[test]
fn an_encoder_line_can_name_its_encoding_and_a_knob_moves_the_value_it_is_bound_to() {
    let p = Preset::parse("bind = cc 1 rel twos -> tempo\nbind = cc 2 rel offset -> range_from\nbind = cc 3 rel -> range_to\nbind = cc 4 rel signbit2 -> click_volume\n");
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let mut m = Mapper::new(p);
    let s = State::default();
    assert_eq!(m.handle(cc(1, 127), &s), [Command::Turn(Action::Tempo, -1)], "two's complement");
    assert_eq!(m.handle(cc(2, 66), &s), [Command::Turn(Action::RangeFrom, 2)], "offset");
    assert!(m.handle(cc(2, 64), &s).is_empty(), "offset: 64 is no turn");
    assert_eq!(m.handle(cc(3, 65), &s), [Command::Turn(Action::RangeTo, -1)], "Mackie unless said otherwise");
    assert_eq!(m.handle(cc(4, 65), &s), [Command::Turn(Action::ClickVolume, 1)], "signed bit 2");
    // Shift still makes the steps larger, whatever the encoding.
    let p = Preset::parse("bind = note 9 -> shift\nbind = cc 1 rel twos -> tempo\n");
    let mut m = Mapper::new(p);
    m.handle(note(9), &s);
    assert_eq!(m.handle(cc(1, 127), &s), [Command::Turn(Action::Tempo, -5)]);
}

#[test]
fn a_line_can_be_for_one_midi_channel_and_a_channel_line_wins() {
    let p = Preset::parse("bind = note 36 ch 10 -> record\nbind = note 36 -> stop\nbind = cc 7 ch 2 abs -> tempo\n");
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    assert_eq!(p.binds[0].input.channel, Some(10));
    assert_eq!(p.binds[1].input.channel, None);
    let mut m = Mapper::new(p);
    let s = State { tempo: 0.5, ..Default::default() };
    let on = |ch: u8, n: u8| Msg::NoteOn { ch, note: n, vel: 100 };
    assert_eq!(m.handle(on(10, 36), &s), [Command::Press(Action::Record)], "the line for channel 10");
    assert_eq!(m.handle(on(1, 36), &s), [Command::Press(Action::Stop)], "any other channel");
    let cc_on = |ch: u8, val: u8| Msg::Cc { ch, cc: 7, val };
    assert!(m.handle(cc_on(1, 64), &s).is_empty(), "cc 7 on channel 1 is not bound");
    assert_eq!(m.handle(cc_on(2, 64), &s).len(), 1, "cc 7 on channel 2 is");
    assert_eq!(m.bound_to(on(10, 36)).as_deref(), Some("record"));
    assert_eq!(m.bound_to(on(3, 36)).as_deref(), Some("stop"));
    assert_eq!(m.bound_to(cc_on(1, 1)), None);
    // The same note on two channels is two controls, not a repeated line.
    let two = Preset::parse("bind = note 36 ch 1 -> record\nbind = note 36 ch 2 -> stop\n");
    assert!(two.warnings.is_empty(), "{:?}", two.warnings);
    let again = Preset::parse("bind = note 36 ch 1 -> record\nbind = note 36 ch 1 -> stop\n");
    assert_eq!(again.warnings.len(), 1, "{:?}", again.warnings);
}

#[test]
fn bad_channel_encoding_and_light_words_are_reported() {
    for line in [
        "bind = note 1 ch 0 -> record",
        "bind = note 1 ch 17 -> record",
        "bind = note 1 ch -> record",
        "bind = cc 1 twos -> tempo",
        "bind = cc 1 abs twos -> tempo",
        "bind = note 1 rel twos -> record",
        "bind = bend 1 ch 2 -> tempo",
        "led = record -> note 1 on=200",
        "led = record -> note 1 scale=10",
        "ring = tempo -> cc 1 on=10",
        "led = record -> note 1 colour=3",
        "led = record -> note 1 ch 20",
    ] {
        let p = Preset::parse(line);
        assert_eq!(p.warnings.len(), 1, "{line}: {:?}", p.warnings);
        assert!(p.binds.is_empty() && p.lights.is_empty(), "{line}: the line is not used");
    }
}

#[test]
fn a_light_can_have_other_values_another_channel_and_a_ring_a_plain_scale() {
    let p = Preset::parse("led = record -> note 36 ch 10 on=21 off=3\nring = tempo -> cc 20 scale=100\nled = listen -> note 5\n");
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let mut m = Mapper::new(p);
    let mut s = State { record: true, tempo: 0.5, ..Default::default() };
    let first = m.lights(&s);
    assert!(first.contains(&[0x99, 36, 21]), "record is on, channel 10, value 21: {first:?}");
    assert!(first.contains(&[0xB0, 20, 50]), "the ring shows 0.5 of 100: {first:?}");
    assert!(first.contains(&[0x90, 5, 0]), "an ordinary light: channel 1, off = 0: {first:?}");
    s.record = false;
    assert_eq!(m.lights(&s), [[0x99, 36, 3]], "off is the line's own off value");
    // A light that is no longer wanted gets its own off value, not 0.
    let p = Preset::parse("layer = A note 1\nlayer = B note 2\nled = A record -> note 36 ch 10 on=21 off=3\n");
    let mut m = Mapper::new(p);
    let s = State { record: true, ..Default::default() };
    assert_eq!(m.lights(&s), [[0x99, 36, 21]]);
    m.handle(note(2), &s);
    assert_eq!(m.lights(&s), [[0x99, 36, 3]], "put out with the preset's off value");
}

#[test]
fn a_preset_written_as_text_reads_back_the_same() {
    for (name, text) in BUILTIN {
        let original = Preset::parse(text);
        let again = Preset::parse(&original.to_text());
        assert!(again.warnings.is_empty(), "{name}: {:?}", again.warnings);
        assert_eq!(original, again, "{name}");
    }
    let odd = Preset::parse(
        "name = X\nport = Y\nlayer = A note 1\nlayer = B note 2\nbind = A cc 5 ch 3 rel twos -> tempo\nbind = cc 6 abs -> click_volume\nbind = B bend 2 -> music_volume\n\
         led = A record -> note 9 ch 10 on=21 off=3\nring = B tempo -> cc 4 scale=100\nled = shift -> cc 8\n",
    );
    assert!(odd.warnings.is_empty(), "{:?}", odd.warnings);
    assert_eq!(Preset::parse(&odd.to_text()), odd);
}

#[test]
fn every_action_is_in_the_editor_once() {
    let all: Vec<Action> = Action::sections().into_iter().flat_map(|(_, a)| a).collect();
    let mut texts: Vec<String> = all.iter().map(|a| a.text()).collect();
    let n = texts.len();
    texts.sort();
    texts.dedup();
    assert_eq!(texts.len(), n, "no action twice");
    for info in ACTIONS {
        assert!(all.iter().any(|a| a.text() == info.name), "{} is missing", info.name);
    }
    for i in 0..GROUPS as u8 {
        for a in [Action::Gain(i), Action::Mute(i), Action::Solo(i), Action::Audition(i)] {
            assert!(all.contains(&a), "{}", a.text());
        }
    }
    for a in &all {
        assert_eq!(Action::parse(&a.text()), Some(*a), "{}", a.text());
    }
}

#[test]
fn a_pressed_control_is_learned_the_way_its_action_needs() {
    let on = |ch: u8, n: u8, vel: u8| Msg::NoteOn { ch, note: n, vel };
    let t = Encoding::Twos;
    // A button wants a press: a note on, or a cc that is not zero (a button that sends cc).
    assert_eq!(learn(on(1, 36, 100), Action::Record, Reading::Absolute, false).map(|l| l.input.src), Some(Src::Note(36)));
    assert_eq!(learn(on(1, 36, 0), Action::Record, Reading::Absolute, false), None, "the release");
    assert_eq!(learn(Msg::NoteOff { ch: 1, note: 36 }, Action::Record, Reading::Absolute, false), None);
    assert_eq!(learn(cc(7, 127), Action::Record, Reading::Absolute, false).map(|l| l.input.src), Some(Src::Cc(7)));
    assert_eq!(learn(cc(7, 0), Action::Record, Reading::Absolute, false), None);
    assert_eq!(learn(Msg::Bend { ch: 1, value: 5 }, Action::Record, Reading::Absolute, false), None, "a fader is no button");
    // A value takes a cc (any value) or a fader, but not a note.
    assert_eq!(learn(on(1, 36, 100), Action::Tempo, Reading::Absolute, false), None);
    let knob = learn(cc(13, 0), Action::Tempo, Reading::Absolute, false).unwrap();
    assert_eq!((knob.input.src, knob.reading), (Src::Cc(13), Reading::Absolute));
    let endless = learn(cc(14, 65), Action::Tempo, Reading::Relative(t), false).unwrap();
    assert_eq!(endless.reading, Reading::Relative(t));
    assert_eq!(learn(Msg::Bend { ch: 3, value: 5 }, Action::Tempo, Reading::Absolute, true).map(|l| (l.input.src, l.input.channel)), Some((Src::Bend(3), None)));
    // The lists are only turned, so they are always relative.
    assert!(learn(cc(15, 1), Action::Score, Reading::Absolute, false).unwrap().reading.is_relative());
    assert_eq!(learn(Msg::Bend { ch: 1, value: 5 }, Action::Score, Reading::Absolute, false), None);
    // The channel is remembered only when asked for.
    assert_eq!(learn(on(10, 36, 100), Action::Record, Reading::Absolute, true).and_then(|l| l.input.channel), Some(10));
    assert_eq!(learn(on(10, 36, 100), Action::Record, Reading::Absolute, false).and_then(|l| l.input.channel), None);
}

#[test]
fn assigning_replaces_the_old_control_of_the_same_kind_and_takes_a_used_one() {
    let mut p = Preset::parse("bind = note 1 -> record\nbind = note 2 -> listen\nbind = cc 5 abs -> tempo\nbind = bend 1 -> tempo\n");
    let found = |src| Learned { input: Input { src, channel: None }, reading: Reading::Absolute };
    // Record moves to note 3: the old note goes.
    assert_eq!(p.assign(None, Action::Record, found(Src::Note(3))), None);
    assert_eq!(p.bindings_of(None, Action::Record).len(), 1);
    assert_eq!(p.bindings_of(None, Action::Record)[0].input.src, Src::Note(3));
    // Stop takes note 2 from Listen.
    assert_eq!(p.assign(None, Action::Stop, found(Src::Note(2))), Some(Action::Listen));
    assert!(p.bindings_of(None, Action::Listen).is_empty());
    // Tempo gets a new knob; the fader stays.
    p.assign(None, Action::Tempo, found(Src::Cc(6)));
    let tempo: Vec<Src> = p.bindings_of(None, Action::Tempo).iter().map(|b| b.input.src).collect();
    assert_eq!(tempo, [Src::Bend(1), Src::Cc(6)]);
    // Another layer's lines are not touched, and clearing is per layer.
    let mut q = Preset::parse("layer = A note 8\nlayer = B note 9\nbind = A note 1 -> record\nbind = B note 1 -> mute 1\n");
    assert_eq!(q.assign(Some("A"), Action::Stop, found(Src::Note(1))), Some(Action::Record));
    assert_eq!(q.bindings_of(Some("B"), Action::Mute(0)).len(), 1);
    q.clear(Some("B"), Action::Mute(0));
    assert!(q.bindings_of(Some("B"), Action::Mute(0)).is_empty() && q.bindings_of(Some("A"), Action::Stop).len() == 1);
    // The kind of a cc can be changed afterwards.
    p.set_reading(None, Action::Tempo, Reading::Relative(Encoding::Offset));
    let cc_bind = p.binds.iter().find(|b| b.input.src == Src::Cc(6)).unwrap();
    assert_eq!(cc_bind.reading, Reading::Relative(Encoding::Offset));
    assert!(!p.binds.iter().find(|b| b.input.src == Src::Bend(1)).unwrap().reading.is_relative(), "a fader is not touched");
}

#[test]
fn an_edited_preset_replaces_the_one_in_use_and_keeps_the_layer() {
    let p = Preset::parse("layer = A note 8\nlayer = B note 9\nbind = A note 1 -> record\nbind = B note 1 -> stop\n");
    let mut m = Mapper::new(p.clone());
    let s = State::default();
    m.handle(note(9), &s);
    assert_eq!(m.layer_name(), Some("B"));
    let mut edited = p;
    edited.assign(Some("B"), Action::Listen, Learned { input: Input { src: Src::Note(1), channel: None }, reading: Reading::Absolute });
    m.replace_preset(edited);
    assert_eq!(m.layer_name(), Some("B"), "still on layer B");
    assert_eq!(m.handle(note(1), &s), [Command::Press(Action::Listen)]);
}

#[test]
fn a_preset_says_whether_it_speaks_mackie_or_plain_midi() {
    // Without a kind line the file is read as Mackie if it has layers or lights, as MIDI otherwise.
    assert_eq!(Preset::parse("layer = A note 1\nbind = A note 2 -> record\n").kind, PresetKind::Mackie);
    assert_eq!(Preset::parse("led = record -> note 1\n").kind, PresetKind::Mackie);
    assert_eq!(Preset::parse("bind = note 2 -> record\n").kind, PresetKind::Midi);
    assert_eq!(Preset::parse("kind = mackie\nbind = note 2 -> record\n").kind, PresetKind::Mackie, "a kind line wins");
    assert_eq!(Preset::parse("kind = midi\n").kind, PresetKind::Midi);
    assert_eq!(Preset::parse("kind = MIDI\n").kind, PresetKind::Midi);
    let bad = Preset::parse("kind = osc\n");
    assert_eq!((bad.kind, bad.warnings.len()), (PresetKind::Mackie, 1), "{:?}", bad.warnings);
    for (name, text) in BUILTIN {
        let kind = Preset::parse(text).kind;
        let generic = ["Novation Launch Control XL", "Korg nanoKONTROL2", "Akai MPK Mini", "Arturia BeatStep"].contains(name);
        assert_eq!(kind, if generic { PresetKind::Midi } else { PresetKind::Mackie }, "{name}");
    }
    // The kind survives a trip through a file.
    let p = Preset::parse("kind = midi\nbind = note 1 -> record\n");
    assert_eq!(Preset::parse(&p.to_text()).kind, PresetKind::Midi);
    // A ring of a MIDI preset shows a plain value, unless it says how far.
    let midi = |extra: &str| {
        let mut m = Mapper::new(Preset::parse(&format!("kind = midi\nring = tempo -> cc 20{extra}\n")));
        m.lights(&State { tempo: 0.5, ..Default::default() })
    };
    assert_eq!(midi(""), [[0xB0, 20, 64]]);
    assert_eq!(midi(" scale=100"), [[0xB0, 20, 50]]);
    let mut mackie = Mapper::new(Preset::parse("ring = tempo -> cc 20\n"));
    assert_eq!(mackie.lights(&State { tempo: 0.5, ..Default::default() }), [[0xB0, 20, 0x20 | 6]], "Mackie ring values");
}

#[test]
fn a_fader_waits_until_it_reaches_the_value() {
    let mut m = mini();
    let s = State { tempo: 0.5, ..Default::default() };
    let bend = |v: f32| Msg::Bend { ch: 1, value: (v * 16383.0) as u16 };
    assert!(m.handle(bend(0.0), &s).is_empty(), "far below: ignored, no jump");
    assert!(m.handle(bend(0.3), &s).is_empty());
    assert_eq!(m.handle(bend(0.52), &s).len(), 1, "close enough: takes over");
    let followed = State { tempo: 0.52, ..Default::default() };
    let moved = m.handle(bend(0.6), &followed);
    assert!(matches!(moved[..], [Command::Set(Action::Tempo, x)] if (x - 0.6).abs() < 0.01), "{moved:?}");
    // The value changed elsewhere: the fader has to catch up again.
    let elsewhere = State { tempo: 0.1, ..Default::default() };
    assert!(m.handle(bend(0.62), &elsewhere).is_empty());
    assert_eq!(m.handle(bend(0.12), &elsewhere).len(), 1, "back near the value");
    // Crossing the value in one big move also takes over.
    let mut m = mini();
    assert!(m.handle(bend(0.1), &s).is_empty());
    assert_eq!(m.handle(bend(0.9), &s).len(), 1, "jumped across the value");
    // A new layer starts waiting again.
    m.handle(note(85), &s);
    assert!(m.handle(bend(0.0), &State { music_volume: 0.8, ..Default::default() }).is_empty());
}

#[test]
fn lights_follow_the_state_and_only_changes_are_sent() {
    let mut m = mini();
    let mut s = State { record: true, tempo: 1.0, ..Default::default() };
    let first = m.lights(&s);
    assert!(first.contains(&[0x90, 95, 127]), "record lit");
    assert!(first.contains(&[0x90, 94, 0]), "listen dark");
    assert!(first.contains(&[0xB0, 48, 0x20 | 11]), "tempo ring full");
    assert!(m.lights(&s).is_empty(), "nothing changed");
    s.record = false;
    s.listen = true;
    let changed = m.lights(&s);
    assert_eq!(changed.len(), 2, "record and listen changed, stop is lit in both cases: {changed:?}");
    assert!(changed.contains(&[0x90, 95, 0]) && changed.contains(&[0x90, 94, 127]));
    // Layer B: the mute and solo lights replace the transport lights.
    s.mute[0] = true;
    m.handle(note(85), &s);
    let b = m.lights(&s);
    assert!(b.contains(&[0x90, 89, 127]), "mute 1 is lit on the button that was Shift: {b:?}");
    assert!(b.contains(&[0x90, 93, 0]), "Stop's button now belongs to solo 6: {b:?}");
    assert!(b.contains(&[0xB0, 48, 0x20]), "the first ring now shows gain 1, which is 0 here: {b:?}");
}

#[test]
fn a_light_of_another_layer_is_put_out() {
    let p = Preset::parse("layer = A note 1\nlayer = B note 2\nled = A record -> note 10\nled = B click -> note 11\n");
    let mut m = Mapper::new(p);
    let s = State { record: true, click: true, ..Default::default() };
    assert_eq!(m.lights(&s), [[0x90, 10, 127]]);
    m.handle(note(2), &s);
    let mut b = m.lights(&s);
    b.sort();
    assert_eq!(b, [[0x90, 10, 0], [0x90, 11, 127]]);
}

#[test]
fn a_line_for_the_layer_overrides_one_for_all_layers() {
    let p = Preset::parse("layer = A note 1\nlayer = B note 2\nbind = note 9 -> record\nbind = B note 9 -> listen\n");
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let mut m = Mapper::new(p);
    let s = State::default();
    assert_eq!(m.handle(note(9), &s), [Command::Press(Action::Record)]);
    m.handle(note(2), &s);
    assert_eq!(m.handle(note(9), &s), [Command::Press(Action::Listen)]);
    assert_eq!(m.bound_to(note(9)).as_deref(), Some("listen"));
    assert_eq!(m.bound_to(note(2)).as_deref(), Some("layer switch"));
}

#[test]
fn bad_lines_are_reported_and_the_rest_is_used() {
    let p = Preset::parse(
        "name = Test # a comment\n\
         port = TEST\n\
         bind = note 1 -> record\n\
         bind = note 2 -> fly\n\
         bind = note 300 -> stop\n\
         bind = bend 1 -> record\n\
         bind = note 3 rel -> tempo\n\
         bind = C note 4 -> stop\n\
         bind = note 1 -> listen\n\
         led = record -> note 5\n\
         led = tempo -> note 6\n\
         ring = record -> cc 7\n\
         nonsense\n\
         colour = red\n",
    );
    assert_eq!(p.name, "Test");
    assert_eq!(p.binds.len(), 2, "record on note 1, and listen on note 1 (a repeat)");
    assert_eq!(p.lights.len(), 1);
    for fragment in ["line 4", "line 5", "line 6", "line 7", "line 11", "line 12", "line 13", "line 14", "layer", "both"] {
        assert!(p.warnings.iter().any(|w| w.contains(fragment)), "no warning with \"{fragment}\": {:?}", p.warnings);
    }
}

#[test]
fn actions_round_trip_through_their_names() {
    for info in ACTIONS {
        assert_eq!(Action::parse(info.name), Some(info.action));
    }
    for i in 0..GROUPS as u8 {
        for a in [Action::Mute(i), Action::Solo(i), Action::Audition(i), Action::Gain(i)] {
            assert_eq!(Action::parse(&a.text()), Some(a));
        }
    }
    assert_eq!(Action::parse("MUTE 1"), Some(Action::Mute(0)));
    assert_eq!(Action::parse("mute 9"), None);
    assert_eq!(Action::parse("mute 0"), None);
    assert_eq!(Action::parse("mute"), None);
    assert_eq!(Action::parse("record 1"), None);
}

#[test]
fn the_groups_cover_every_instrument_once() {
    let mut seen = Vec::new();
    for i in 0..GROUPS {
        seen.extend_from_slice(group_lanes(i));
    }
    for lane in Lane::ALL {
        let n = seen.iter().filter(|l| **l == lane).count();
        assert_eq!(n, if lane == Lane::Other { 0 } else { 1 }, "{lane:?}");
    }
}

#[test]
fn custom_presets_are_read_from_a_folder() {
    let dir = std::env::temp_dir().join(format!("roll-model-presets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("My pads.txt"), "port = PADS\nbind = note 36 -> record\n").unwrap();
    std::fs::write(dir.join("readme.md"), "not a preset").unwrap();
    assert_eq!(custom_presets(&dir).iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["My pads"]);
    let p = load_preset("My pads", Some(&dir)).unwrap();
    assert_eq!((p.name.as_str(), p.port.as_str(), p.binds.len()), ("My pads", "PADS", 1));
    // The file's name is the preset's name, whatever its `name` line says.
    std::fs::write(dir.join("Renamed.txt"), "name = Something else\nport = P\n").unwrap();
    assert_eq!(load_preset("Renamed", Some(&dir)).unwrap().name, "Renamed");
    assert!(matches!(preset_source("My pads", Some(&dir)), Some(PresetSource::File(_))));
    assert!(matches!(preset_source("Korg nanoKONTROL2", Some(&dir)), Some(PresetSource::BuiltIn(_))));
    assert_eq!(preset_source("Nope", Some(&dir)), None);
    assert!(load_preset("Nope", Some(&dir)).is_err());
    assert!(load_preset("Nope", None).is_err());
    assert!(custom_presets(&dir.join("missing")).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_action_has_a_name_a_title_and_an_explanation() {
    for (_, actions) in Action::sections() {
        for a in actions {
            assert!(!a.text().is_empty() && !a.title().is_empty() && !a.help().is_empty(), "{a:?} is not in the ACTIONS table");
        }
    }
}

#[test]
fn layers_are_added_and_removed_with_what_belongs_to_them() {
    let mut p = Preset::parse("name = T\nport = P\nbind = note 1 -> record\n");
    assert_eq!(p.add_layer("A", Src::Note(84)), Ok(()));
    assert_eq!(p.add_layer("B", Src::Note(85)), Ok(()));
    assert_eq!(p.add_layer("A", Src::Note(86)), Err(LayerError::Taken("A".into())));
    assert_eq!(p.add_layer("C", Src::Note(85)), Err(LayerError::ControlInUse("it selects layer B".into())));
    assert!(matches!(p.add_layer("C", Src::Note(1)), Err(LayerError::ControlInUse(_))), "a control that does something cannot select a layer");
    assert_eq!(p.add_layer("two words", Src::Note(9)), Err(LayerError::BadName));
    assert_eq!(p.add_layer("", Src::Note(9)), Err(LayerError::BadName));
    assert!(matches!(p.add_layer("Record", Src::Note(9)), Err(LayerError::Reserved(_))));
    assert!(matches!(p.add_layer("note", Src::Note(9)), Err(LayerError::Reserved(_))));
    // What is bound on a layer goes with it.
    p.assign(Some("B"), Action::Stop, Learned { input: Input { src: Src::Note(2), channel: None }, reading: Reading::Absolute });
    p.assign(Some("A"), Action::Listen, Learned { input: Input { src: Src::Note(2), channel: None }, reading: Reading::Absolute });
    p.remove_layer("B");
    assert_eq!(p.layers, vec![("A".to_string(), Src::Note(84))]);
    assert!(p.binds.iter().all(|b| b.layer.as_deref() != Some("B")) && p.binds.iter().any(|b| b.action == Action::Listen));
    // It survives the file.
    let back = Preset::parse(&p.to_text());
    assert_eq!((back.layers, back.warnings), (p.layers.clone(), Vec::<String>::new()));
}

#[test]
fn lights_and_rings_are_set_replaced_and_cleared_per_layer() {
    let mut p = Preset::parse("name = T\nport = P\nlayer = A note 84\n");
    p.set_light(Light::new(Some("A"), Action::Record, Out::Note(95), false));
    p.set_light(Light::new(Some("A"), Action::Tempo, Out::Cc(48), true));
    p.set_light(Light::new(None, Action::Record, Out::Note(1), false));
    assert_eq!(p.lights.len(), 3);
    // Setting again replaces: the old light of that layer, action and kind goes.
    let mut louder = Light::new(Some("A"), Action::Record, Out::Note(96), false);
    louder.on = 60;
    louder.off = 5;
    p.set_light(louder);
    assert_eq!(p.lights.len(), 3);
    let kept = p.lights_of(Some("A"), Action::Record, false).unwrap();
    assert_eq!((kept.out, kept.on, kept.off), (Out::Note(96), 60, 5));
    assert!(p.lights_of(Some("A"), Action::Record, true).is_none(), "a ring is not a light");
    p.clear_light(Some("A"), Action::Record, false);
    assert!(p.lights_of(Some("A"), Action::Record, false).is_none());
    assert!(p.lights_of(None, Action::Record, false).is_some(), "the light for every layer stays");
    // What was set is in the file, and reads back.
    let back = Preset::parse(&p.to_text());
    assert_eq!(back.lights, p.lights);
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
}
