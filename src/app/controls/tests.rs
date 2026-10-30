use std::path::{Path, PathBuf};

use crate::analysis::{analyze, expected_notes, Hit};
use crate::config::Config;
use crate::config::MAX_LANE_GAIN;
use crate::controller::{Action, Command};
use crate::drums::Lane;
use crate::rudiments::{Lead, MAX_REPEATS};

use super::*;
use super::apply::{register_tap, shifted_range, worst_measure, TAPS_KEPT};
use super::page::copy_preset;
use super::super::{rudiment, DrumApp, Source, Tab};

fn sample() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join("basic_rock_beat.musicxml")
}

fn app_with_score() -> DrumApp {
    let mut app = DrumApp::with_config(Config::default());
    app.open_file(&sample());
    assert!(app.score.is_some());
    app
}

#[test]
fn a_controller_is_noticed_when_plugged_in_or_pulled_out() {
    let ports = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
    let both = ports(&["Roland V31", "X-TOUCH MINI"]);
    assert_eq!(watch_step(None, &both, "X-TOUCH", Some("Roland V31")), Watch::Connect);
    assert_eq!(watch_step(Some("X-TOUCH MINI"), &both, "X-TOUCH", Some("Roland V31")), Watch::Keep);
    let only_drums = ports(&["Roland V31"]);
    assert_eq!(watch_step(Some("X-TOUCH MINI"), &only_drums, "X-TOUCH", Some("Roland V31")), Watch::Lost);
    assert_eq!(watch_step(None, &only_drums, "X-TOUCH", Some("Roland V31")), Watch::Keep, "still waiting");
    // The drum input is never mistaken for the controller.
    assert_eq!(watch_step(None, &only_drums, "Roland", Some("Roland V31")), Watch::Keep);
    assert_eq!(watch_step(None, &only_drums, "Roland", None), Watch::Connect);
}

#[test]
fn tapping_gives_the_tempo_and_a_pause_starts_over() {
    let t0 = Instant::now();
    let at = |ms: u64| t0 + Duration::from_millis(ms);
    let mut taps = Vec::new();
    assert_eq!(register_tap(&mut taps, at(0)), None);
    let bpm = register_tap(&mut taps, at(500)).unwrap();
    assert!((bpm - 120.0).abs() < 1e-6);
    assert!((register_tap(&mut taps, at(1000)).unwrap() - 120.0).abs() < 1e-6);
    // Slowing down: the average follows.
    let slower = register_tap(&mut taps, at(1700)).unwrap();
    assert!(slower < 120.0 && slower > 90.0, "{slower}");
    // A long pause starts a new tap tempo.
    assert_eq!(register_tap(&mut taps, at(10_000)), None);
    assert_eq!(taps.len(), 1);
    // Only the last few taps count.
    for i in 1..20 {
        register_tap(&mut taps, at(10_000 + i * 400));
    }
    assert_eq!(taps.len(), TAPS_KEPT);
}

#[test]
fn the_range_moves_by_its_own_length_and_stops_at_the_ends() {
    assert_eq!(shifted_range(0, 3, 16, true), Some((4, 7)));
    assert_eq!(shifted_range(4, 7, 16, false), Some((0, 3)));
    assert_eq!(shifted_range(0, 3, 16, false), None);
    assert_eq!(shifted_range(12, 15, 16, true), None);
    assert_eq!(shifted_range(10, 13, 16, true), Some((14, 15)), "the last chunk may be shorter");
    assert_eq!(shifted_range(2, 5, 16, false), Some((0, 3)), "not below the first measure");
    assert_eq!(shifted_range(3, 3, 8, true), Some((4, 4)));
}

#[test]
fn the_worst_measure_is_where_most_goes_wrong() {
    let app = app_with_score();
    let score = app.score.as_ref().unwrap();
    let last = score.measures.len() - 1;
    let expected = expected_notes(score, 0, last, 1.0);
    let tol = &app.cfg.tol;
    // Everything played perfectly, except that measure 2 (index 1) is left out.
    let hits: Vec<Hit> = expected
        .iter()
        .filter(|e| score.notes[e.note_idx].measure != 1)
        .map(|e| Hit { t: e.t, note: 36, velocity: 100, lane: e.lane })
        .collect();
    let report = analyze(&expected, &hits, tol, f64::INFINITY);
    assert_eq!(worst_measure(score, &report, &[]), Some(1));
    // Enough stray hits in measure 4 make it the worst one.
    let strays = vec![last; expected.len()];
    assert_eq!(worst_measure(score, &report, &strays), Some(last));
    // Nothing wrong, nothing to practise.
    let all: Vec<Hit> = expected.iter().map(|e| Hit { t: e.t, note: 36, velocity: 100, lane: e.lane }).collect();
    let perfect = analyze(&expected, &all, tol, f64::INFINITY);
    assert_eq!(worst_measure(score, &perfect, &[]), None);
}

#[test]
fn the_tempo_encoder_and_fader_change_the_practice_tempo() {
    let mut app = app_with_score();
    app.apply_command(Command::Turn(Action::Tempo, 5));
    assert_eq!(app.tempo_pct, 105.0);
    app.apply_command(Command::Turn(Action::Tempo, -500));
    assert_eq!(app.tempo_pct, 25.0, "clamped");
    app.apply_command(Command::Set(Action::Tempo, 1.0));
    assert_eq!(app.tempo_pct, 150.0);
    app.apply_command(Command::Press(Action::TempoReset));
    assert_eq!(app.tempo_pct, 100.0);
    app.apply_command(Command::Press(Action::TempoDown));
    assert_eq!(app.tempo_pct, 95.0);
    assert_eq!(app.controller_state().tempo, (95.0 - 25.0) / 125.0);
}

#[test]
fn on_the_metronome_tab_the_tempo_controls_set_its_bpm() {
    let mut app = app_with_score();
    app.view.tab = Tab::Metronome;
    app.apply_command(Command::Turn(Action::Tempo, 10));
    assert_eq!(app.cfg.metro_bpm, 110.0);
    assert_eq!(app.tempo_pct, 100.0, "the practice tempo is left alone");
    app.apply_command(Command::Set(Action::Tempo, 0.0));
    assert_eq!(app.cfg.metro_bpm, 20.0);
    // Without a score the metronome is the only tempo there is.
    let mut bare = DrumApp::with_config(Config::default());
    bare.apply_command(Command::Turn(Action::Tempo, -10));
    assert_eq!(bare.cfg.metro_bpm, 90.0);
}

#[test]
fn range_encoders_keep_the_range_valid() {
    let mut app = app_with_score();
    let last = app.score.as_ref().unwrap().measures.len() - 1;
    assert_eq!((app.range_from, app.range_to), (0, last));
    app.apply_command(Command::Turn(Action::RangeFrom, 2));
    assert_eq!(app.range_from, 2.min(last));
    app.apply_command(Command::Turn(Action::RangeFrom, 100));
    assert_eq!(app.range_from, last, "not past the end");
    app.apply_command(Command::Turn(Action::RangeTo, -100));
    assert_eq!(app.range_to, app.range_from, "the end never goes before the start");
    app.apply_command(Command::Set(Action::RangeFrom, 0.0));
    assert_eq!(app.range_from, 0);
    app.apply_command(Command::Set(Action::RangeTo, 0.5));
    assert!(app.range_to >= app.range_from && app.range_to <= last);
    app.range_from = 0;
    app.range_to = 1;
    app.apply_command(Command::Press(Action::RangeForward));
    assert_eq!((app.range_from, app.range_to), (2, 3.min(last)));
    app.apply_command(Command::Press(Action::RangeBack));
    assert_eq!((app.range_from, app.range_to), (0, 1));
}

#[test]
fn mixer_strips_control_groups_of_instruments() {
    let mut app = DrumApp::with_config(Config::default());
    app.apply_command(Command::Turn(Action::Gain(1), -4));
    for lane in [Lane::Snare, Lane::CrossStick] {
        assert!((app.cfg.gain_of(lane) - 0.8).abs() < 1e-5, "{lane:?}");
    }
    assert_eq!(app.cfg.gain_of(Lane::Kick), 1.0);
    assert!((app.controller_state().gain[1] - 0.8 / MAX_LANE_GAIN).abs() < 1e-5);
    app.apply_command(Command::Turn(Action::Gain(1), 4));
    assert!(app.cfg.lane_gain.is_empty(), "back at the default, nothing is stored");
    app.apply_command(Command::Set(Action::Gain(0), 1.0));
    assert_eq!(app.cfg.gain_of(Lane::Kick), MAX_LANE_GAIN);

    app.apply_command(Command::Press(Action::Mute(2)));
    for lane in [Lane::HiHatClosed, Lane::HiHatOpen, Lane::HiHatPedal] {
        assert!(app.cfg.lane_mute.contains(&lane), "{lane:?}");
    }
    assert!(app.controller_state().mute[2]);
    app.apply_command(Command::Press(Action::Mute(2)));
    assert!(app.cfg.lane_mute.is_empty());

    app.apply_command(Command::Press(Action::Solo(7)));
    assert!(app.playback.solo.contains(&Lane::Ride) && app.controller_state().solo[7]);
    assert_eq!(app.cfg.effective_gain(&app.playback.solo, Lane::Kick), 0.0);
    app.apply_command(Command::Press(Action::Solo(7)));
    assert!(app.playback.solo.is_empty());
}

#[test]
fn volume_encoders_move_in_small_steps_within_range() {
    let mut app = DrumApp::with_config(Config::default());
    app.cfg.click_volume = 0.5;
    app.apply_command(Command::Turn(Action::ClickVolume, 3));
    assert_eq!(app.cfg.click_volume, 0.56);
    app.apply_command(Command::Turn(Action::ClickVolume, 1000));
    assert_eq!(app.cfg.click_volume, 1.0);
    app.apply_command(Command::Set(Action::MusicVolume, 0.25));
    assert_eq!(app.cfg.playback_volume, 0.25);
    app.cfg.count_in_bars = 0;
    app.apply_command(Command::Turn(Action::CountIn, 5));
    assert_eq!(app.cfg.count_in_bars, 1, "one step at a time");
    app.apply_command(Command::Turn(Action::CountIn, 50));
    app.apply_command(Command::Turn(Action::CountIn, 50));
    assert_eq!(app.cfg.count_in_bars, 2);
    app.apply_command(Command::Turn(Action::BarsPerRow, 1));
    assert_eq!(app.cfg.bars_per_row, 1);
    app.apply_command(Command::Set(Action::BarsPerRow, 1.0));
    assert_eq!(app.cfg.bars_per_row, 8);
}

#[test]
fn switches_and_tabs() {
    let mut app = app_with_score();
    let click = app.cfg.metronome_during_take;
    app.apply_command(Command::Press(Action::Click));
    assert_eq!(app.cfg.metronome_during_take, !click);
    assert!(app.playback.notes && !app.playback.looping);
    app.apply_command(Command::Press(Action::Loop));
    app.apply_command(Command::Press(Action::Notes));
    assert!(!app.playback.notes && app.playback.looping);
    let s = app.controller_state();
    assert!(s.looping && !s.notes && !s.record && !s.listen && !s.metronome);

    assert!(app.view.tab == Tab::Score);
    for expected in [Tab::Rudiments, Tab::Metronome, Tab::Results, Tab::Settings, Tab::Score] {
        app.apply_command(Command::Press(Action::NextTab));
        assert!(app.view.tab == expected);
    }
    let mut bare = DrumApp::with_config(Config::default());
    bare.apply_command(Command::Press(Action::NextTab));
    assert!(bare.view.tab == Tab::Rudiments, "no Score tab without a score");
}

#[test]
fn with_a_rudiment_the_range_controls_choose_repetitions_lead_and_rudiment() {
    let mut app = app_with_score();
    app.apply_command(Command::Press(Action::NextTab));
    assert!(app.view.tab == Tab::Rudiments && app.source == Source::Rudiments);
    assert_eq!(app.cfg.rudiment, 1);

    // The encoder that browses the recent scores browses the rudiments, one at a time.
    app.apply_command(Command::Turn(Action::Score, 1));
    assert_eq!(app.cfg.rudiment, 2);
    app.apply_command(Command::Turn(Action::Score, 50));
    assert_eq!(app.cfg.rudiment, 3);
    app.apply_command(Command::Press(Action::RangeForward));
    assert_eq!(app.cfg.rudiment, 4);
    app.apply_command(Command::Press(Action::RangeBack));
    app.apply_command(Command::Press(Action::RangeBack));
    assert_eq!(app.cfg.rudiment, 2);
    assert_eq!(app.score.as_ref().unwrap().title, "Single stroke four", "the score follows at once");

    // First and last measure become the repetitions and the lead; the rings show them.
    let repeats = app.cfg.rudiment_repeats;
    app.apply_command(Command::Turn(Action::RangeFrom, 3));
    assert_eq!(app.cfg.rudiment_repeats, repeats + 3);
    assert_eq!(app.score.as_ref().unwrap().measures.len(), repeats + 3);
    app.apply_command(Command::Turn(Action::RangeTo, 1));
    assert_eq!(app.rudiment_lead(), Lead::Left);
    app.apply_command(Command::Turn(Action::RangeTo, -1));
    assert_eq!(app.rudiment_lead(), Lead::Right);
    app.apply_command(Command::Turn(Action::RangeTo, -1));
    assert_eq!(app.rudiment_lead(), Lead::Alternate, "the choice goes round");
    let s = app.controller_state();
    assert!((s.range_from - (repeats + 2) as f32 / (MAX_REPEATS - 1) as f32).abs() < 1e-6);
    assert_eq!(s.range_to, 1.0);
    app.apply_command(Command::Set(Action::RangeFrom, 0.0));
    assert_eq!(app.cfg.rudiment_repeats, 1);
    app.apply_command(Command::Set(Action::Repeats, 1.0));
    assert_eq!(app.cfg.rudiment_repeats, MAX_REPEATS);

    // The tempo is in BPM, within its own range, and resets to the rudiment's starting tempo.
    assert_eq!(app.tempo_pct, 60.0);
    app.apply_command(Command::Turn(Action::Tempo, 5));
    assert_eq!(app.tempo_pct, 65.0);
    app.apply_command(Command::Set(Action::Tempo, 1.0));
    assert_eq!(app.tempo_pct, rudiment::BPM_RANGE.1);
    app.apply_command(Command::Set(Action::Tempo, 0.0));
    assert_eq!(app.tempo_pct, rudiment::BPM_RANGE.0);
    app.apply_command(Command::Press(Action::TempoReset));
    assert_eq!(app.tempo_pct, 60.0);
    assert!((app.controller_state().tempo - (60.0 - 30.0) / (240.0 - 30.0)).abs() < 1e-6);

    // Back on the Score tab the same controls are about the measures again.
    app.apply_command(Command::Press(Action::NextTab));
    app.apply_command(Command::Press(Action::NextTab));
    app.apply_command(Command::Press(Action::NextTab));
    app.apply_command(Command::Press(Action::NextTab));
    assert!(app.view.tab == Tab::Score && app.source == Source::Score);
    let (from, to) = (app.range_from, app.range_to);
    app.apply_command(Command::Turn(Action::RangeTo, -1));
    assert_eq!((app.range_from, app.range_to), (from, to - 1));
    assert_eq!(app.cfg.rudiment_repeats, MAX_REPEATS, "the rudiment is left alone");
}

#[test]
fn the_rudiment_controls_are_locked_while_a_take_records() {
    let mut app = app_with_score();
    app.cfg.click_audio = false;
    app.cfg.click_midi = false;
    app.apply_command(Command::Press(Action::NextTab));
    app.apply_command(Command::Press(Action::Record));
    assert!(app.take_running());
    app.apply_command(Command::Press(Action::NextRudiment));
    app.apply_command(Command::Turn(Action::Repeats, 3));
    app.apply_command(Command::Turn(Action::Lead, 1));
    assert_eq!(app.cfg.rudiment, 1);
    assert_eq!(app.cfg.rudiment_repeats, crate::rudiments::DEFAULT_REPEATS);
    assert!(app.rud.lead.is_none());
    assert!(app.view.status.contains("Locked"), "{}", app.view.status);
    app.apply_command(Command::Press(Action::Stop));
    app.apply_command(Command::Press(Action::NextRudiment));
    assert_eq!(app.cfg.rudiment, 2);
}

#[test]
fn recent_scores_are_browsed_and_opened() {
    let mut app = DrumApp::with_config(Config::default());
    app.apply_command(Command::Turn(Action::Score, 1));
    assert!(app.view.status.contains("no recent"));
    let sample = sample().to_string_lossy().into_owned();
    app.cfg.recent_files = vec!["missing-1.musicxml".into(), sample, "missing-2.musicxml".into()];
    app.apply_command(Command::Turn(Action::Score, 1));
    assert_eq!(app.controller.cursor, 1);
    assert!(app.view.status.contains("basic_rock_beat") && app.view.status.contains("2/3"), "{}", app.view.status);
    app.apply_command(Command::Turn(Action::Score, 50));
    assert_eq!(app.controller.cursor, 2);
    app.apply_command(Command::Turn(Action::Score, -50));
    assert_eq!(app.controller.cursor, 0);
    app.apply_command(Command::Turn(Action::Score, 1));
    app.apply_command(Command::Press(Action::OpenScore));
    assert!(app.score.is_some() && app.view.tab == Tab::Score);
}

#[test]
fn loop_worst_bar_and_discard_need_a_take() {
    let mut app = app_with_score();
    app.apply_command(Command::Press(Action::LoopWorstBar));
    assert_eq!(app.view.status, "Record a take first.");
    assert!(!app.playback.looping);
    app.apply_command(Command::Press(Action::DiscardTake));
    let mut closed = DrumApp::with_config(Config::default());
    closed.apply_command(Command::Press(Action::Record));
    assert_eq!(closed.view.status, "Open a score first.");
    assert!(closed.take.is_none() && closed.playback.listen.is_none());
}

#[test]
fn the_controller_page_draws_with_and_without_a_controller() {
    let mut app = DrumApp::with_config(Config::default());
    app.refresh_controllers();
    // The built-in presets come first; the user's own (a folder this test does not control) follow.
    assert_eq!(
        app.controller.presets[..6],
        ["Behringer X-Touch Mini", "Behringer X-Touch Compact", "Novation Launch Control XL", "Korg nanoKONTROL2", "Akai MPK Mini", "Arturia BeatStep"]
    );
    assert_eq!(app.controller.kinds["Behringer X-Touch Mini"], controller::PresetKind::Mackie);
    assert_eq!(app.controller.kinds["Korg nanoKONTROL2"], controller::PresetKind::Midi);
    let ctx = egui::Context::default();
    for message in ["", "No controller selected."] {
        app.controller.message = message.into();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.settings_controller(ui));
        });
    }
    // Choosing a preset whose device is not there says so instead of failing.
    app.cfg.controller_preset = "Behringer X-Touch Mini".into();
    app.cfg.controller_port = "no such port".into();
    app.connect_controller();
    // "not found", or on a machine without MIDI (CI) that MIDI could not be started: either way, no connection.
    assert!(app.controller.link.is_none() && app.controller.message.contains("Switch the controller"), "{}", app.controller.message);
    app.cfg.controller_preset = "No such preset".into();
    app.connect_controller();
    assert!(app.controller.message.contains("not found"));
}

#[test]
fn copies_of_a_preset_never_overwrite() {
    let dir = std::env::temp_dir().join(format!("roll-model-copy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let a = copy_preset(&dir, "Pads", "one").unwrap();
    let b = copy_preset(&dir, "Pads", "two").unwrap();
    assert_ne!(a, b);
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "one");
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "two");
    assert_eq!(controller::custom_presets(&dir).len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_run_has_a_count_in_only_when_asked_for() {
    let mut app = app_with_score();
    app.cfg.count_in_bars = 1;
    let beats = app.score.as_ref().unwrap().measures[0].beats;
    assert_eq!(app.plan_run(true).unwrap().count_in_beats, beats);
    assert_eq!(app.plan_run(false).unwrap().count_in_beats, 0, "this is what Listen asks for when its count-in is off");
    assert!(app.cfg.listen_count_in, "Listen has one by default");
}
