//! The core crate used the way a tool or another front end would use it: through its public API
//! only, with no window, MIDI port or sound card.

use roll_model_core::analysis::{analyze, expected_notes, Hit, Tolerances};
use roll_model_core::config::Config;
use roll_model_core::controller::{self, Action, Mapper, Msg, Command, State};
use roll_model_core::musicxml::{parse_musicxml, ParseOptions};

const SAMPLE: &str = include_str!("../../examples/basic_rock_beat.musicxml");

#[test]
fn a_perfect_take_of_the_example_score_grades_perfect() {
    let score = parse_musicxml(SAMPLE, &ParseOptions::default()).unwrap();
    let last = score.measures.len() - 1;
    let expected = expected_notes(&score, 0, last, 1.0);
    assert!(!expected.is_empty());
    let hits: Vec<Hit> = expected.iter().map(|e| Hit { t: e.t, note: 0, velocity: 100, lane: e.lane }).collect();
    let report = analyze(&expected, &hits, &Tolerances::default(), f64::INFINITY);
    assert_eq!((report.missed, report.extra), (0, 0));
    assert_eq!(report.perfect, expected.len());
}

#[test]
fn settings_survive_the_text_file() {
    let mut config = Config::default();
    config.zoom = 1.5;
    config.controller_preset = "Korg nanoKONTROL2".to_string();
    assert_eq!(Config::from_text(&config.to_text()), config);
}

#[test]
fn a_built_in_controller_preset_turns_a_button_into_a_command() {
    let preset = controller::load_preset("Behringer X-Touch Mini", None).unwrap();
    assert!(preset.warnings.is_empty(), "{:?}", preset.warnings);
    let mut mapper = Mapper::new(preset);
    let state = State::default();
    // A bound button, found through the preset itself.
    let record = mapper.preset().binds.iter().find(|b| b.action == Action::Record).expect("record is bound");
    let msg = match record.src {
        controller::Src::Note(note) => Msg::NoteOn { ch: 1, note, vel: 127 },
        other => panic!("record is a button, not {other:?}"),
    };
    assert_eq!(mapper.handle(msg, &state), vec![Command::Press(Action::Record)]);
}
