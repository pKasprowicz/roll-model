use super::*;

#[test]
fn settings_of_the_old_name_are_copied_to_the_new_folder_once() {
    let base = std::env::temp_dir().join(format!("roll-model-migrate-{}", std::process::id()));
    let (old, new) = (base.join("DrumTutor"), base.join("RollModel"));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(old.join("sounds")).unwrap();
    std::fs::write(old.join("config.txt"), "latency_ms = 7\nrecent = a.musicxml\n").unwrap();
    std::fs::write(old.join("sounds").join("Kick.wav"), b"wav").unwrap();

    Config::migrate(&old, &new).unwrap();
    let c = Config::from_text(&std::fs::read_to_string(new.join("config.txt")).unwrap());
    assert_eq!(c.latency_ms, 7.0);
    assert_eq!(c.recent_files, ["a.musicxml"]);
    assert!(new.join("sounds").join("Kick.wav").is_file());
    assert!(old.join("config.txt").is_file(), "the old folder is left alone");

    // Settings changed under the new name are never overwritten by the old ones.
    std::fs::write(new.join("config.txt"), "latency_ms = 9\n").unwrap();
    Config::migrate(&old, &new).unwrap();
    assert_eq!(std::fs::read_to_string(new.join("config.txt")).unwrap(), "latency_ms = 9\n");
    // Nothing to migrate is not an error.
    Config::migrate(&base.join("missing"), &base.join("other")).unwrap();
    assert!(!base.join("other").exists());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn every_key_is_written_once_and_read_back() {
    let text = Config::default().to_text();
    for key in KEYS {
        assert_eq!(KEYS.iter().filter(|k| k.name == key.name).count(), 1, "{} twice", key.name);
        assert!(text.lines().any(|l| l.starts_with(&format!("{} = ", key.name))), "{} is not written", key.name);
    }
    let mut c = Config::default();
    (KEYS.iter().find(|k| k.name == "zoom").unwrap().read)(&mut c, "9");
    assert_eq!(c.zoom, 4.0, "a number is kept in its range");
    (KEYS.iter().find(|k| k.name == "zoom").unwrap().read)(&mut c, "nan");
    assert_eq!(c.zoom, 4.0, "and a bad one changes nothing");
}

#[test]
fn roundtrip() {
    let mut c = Config {
        latency_ms: 12.5,
        midi_in_port: "Roland V31 MIDI 1".into(),
        metronome_during_take: false,
        count_in_bars: 2,
        listen_count_in: false,
        playback_audio: false,
        playback_volume: 0.25,
        playback_midi: true,
        playback_channel: 1,
        audio_output: "Speakers (USB Audio)".into(),
        bars_per_row: 4,
        layout_align_bar: 5,
        metro_bpm: 133.5,
        metro_beats: 7,
        metro_sub: 2,
        metro_preview: false,
        hihat_cc_max: 90,
        controller_preset: "Behringer X-Touch Mini".into(),
        controller_port: "X-TOUCH".into(),
        ..Default::default()
    };
    c.note_map.set_lane_notes(Lane::Kick, &[36, 33]);
    c.lane_gain.insert(Lane::Crash1, 0.4);
    c.lane_gain.insert(Lane::Kick, 1.25);
    c.lane_mute.insert(Lane::HiHatClosed);
    c.trigger_enabled = true;
    c.trigger_first = Lane::Ride;
    c.trigger_second = Lane::Snare;
    c.rudiment = 24;
    c.rudiment_repeats = 8;
    c.rudiment_nard_only = true;
    c.rudiment_ladder = true;
    c.rudiment_bpm.insert(24, 72.5);
    c.rudiment_best.insert(16, 96.0);
    let back = Config::from_text(&c.to_text());
    assert_eq!(back, c);
}

#[test]
fn trigger_settings_default_to_off_and_ignore_unknown_instruments() {
    let d = Config::default();
    assert_eq!((d.trigger_enabled, d.trigger_first, d.trigger_second), (false, Lane::Crash1, Lane::Tom1));
    let c = Config::from_text("trigger_enabled = true\ntrigger_first = Splash\ntrigger_second = Nope\n");
    assert!(c.trigger_enabled);
    assert_eq!((c.trigger_first, c.trigger_second), (Lane::Splash, Lane::Tom1), "an unknown name keeps the default");
    let other = Config::from_text("trigger_first = Other\n");
    assert_eq!(other.trigger_first, Lane::Crash1, "\"Other\" is not an instrument to hit");
    assert!(Config::from_text("latency_ms = 5\n").trigger_first == Lane::Crash1, "an old file loads");
}

#[test]
fn rudiment_settings_are_clamped_and_default_when_missing() {
    let c = Config::from_text(
        "rudiment = 99\nrudiment_repeats = 500\nrudiment_bpm.16 = 1000\nrudiment_bpm.99 = 80\nrudiment_bpm.x = 80\n\
         rudiment_best.20 = abc\nrudiment_best.21 = 88\nrudiment_ladder = maybe\n",
    );
    assert_eq!(c.rudiment, 1, "not a rudiment: keeps the default");
    assert_eq!(c.rudiment_repeats, MAX_REPEATS);
    assert_eq!(c.rudiment_bpm.get(&16), Some(&300.0));
    assert_eq!(c.rudiment_bpm.len(), 1, "unknown rudiments are dropped");
    assert_eq!(c.rudiment_best.len(), 1);
    assert_eq!(c.rudiment_best.get(&21), Some(&88.0));
    assert!(!c.rudiment_ladder);
    let old = Config::from_text("latency_ms = 5\n");
    assert_eq!((old.rudiment, old.rudiment_repeats), (1, DEFAULT_REPEATS));
    assert!(old.rudiment_bpm.is_empty() && old.rudiment_best.is_empty());
    assert!(!Config::default().to_text().contains("rudiment_bpm."), "nothing is written for a rudiment never practised");
}

#[test]
fn mute_and_solo_decide_what_is_heard() {
    let mut c = Config::default();
    c.lane_gain.insert(Lane::Kick, 0.5);
    c.lane_mute.insert(Lane::Snare);
    let none = BTreeSet::new();
    assert_eq!(c.effective_gain(&none, Lane::Kick), 0.5);
    assert_eq!(c.effective_gain(&none, Lane::Snare), 0.0, "muted");
    assert_eq!(c.effective_gain(&none, Lane::Ride), 1.0);
    // Soloing silences everything else, even instruments that are not muted...
    let solo: BTreeSet<Lane> = [Lane::Snare, Lane::Ride].into_iter().collect();
    assert_eq!(c.effective_gain(&solo, Lane::Kick), 0.0);
    assert_eq!(c.effective_gain(&solo, Lane::Ride), 1.0);
    // ...and a solo wins over a mute.
    assert_eq!(c.effective_gain(&solo, Lane::Snare), 1.0);
}

#[test]
fn mixer_layout_and_metronome_settings_are_clamped_and_default_when_missing() {
    let c = Config::from_text(
        "bars_per_row = 99\nlayout_align_bar = 1000000\nmetro_bpm = 5\nmetro_beats = 0\nmetro_sub = 9\n\
         gain.Ride = 9\ngain.Snare = abc\ngain.Nope = 1\nmute.Kick = true\nmute.Snare = maybe\naudio_output =\n",
    );
    assert_eq!((c.bars_per_row, c.layout_align_bar), (16, 9999));
    assert_eq!((c.metro_bpm, c.metro_beats, c.metro_sub), (20.0, 1, 3));
    assert_eq!(c.gain_of(Lane::Ride), MAX_LANE_GAIN);
    assert_eq!(c.gain_of(Lane::Snare), 1.0, "an unreadable gain keeps the default");
    assert!(c.lane_mute.contains(&Lane::Kick) && !c.lane_mute.contains(&Lane::Snare));
    assert_eq!(c.audio_output, "", "an empty device name means the system default");
    // A settings file from before these existed loads with the defaults.
    let old = Config::from_text("latency_ms = 5\n");
    assert_eq!((old.bars_per_row, old.layout_align_bar, old.metro_bpm, old.metro_beats), (0, 0, 100.0, 4));
    assert!(old.lane_gain.is_empty() && old.lane_mute.is_empty() && old.audio_output.is_empty());
    assert_eq!((old.controller_preset.as_str(), old.controller_port.as_str()), (DEFAULT_CONTROLLER, ""), "the X-Touch Mini unless another is chosen");
    assert!(Config::from_text("controller_preset = \n").controller_preset.is_empty(), "\"none\" stays chosen");
    // Defaults are not written out for the mixer.
    assert!(!Config::default().to_text().contains("gain."));
}

#[test]
fn recent_files_keep_order_without_duplicates() {
    let mut c = Config::default();
    for p in ["a.musicxml", "b.musicxml", "c.musicxml", "a.musicxml"] {
        c.remember_recent(p);
    }
    assert_eq!(c.recent_files, ["a.musicxml", "c.musicxml", "b.musicxml"]);
    // The list survives a save and load in the same order, including paths with spaces and '='.
    c.remember_recent("D:\\My scores\\a=b.musicxml");
    assert_eq!(Config::from_text(&c.to_text()).recent_files, c.recent_files);
    for i in 0..2 * MAX_RECENT {
        c.remember_recent(&format!("score{i}.musicxml"));
    }
    assert_eq!(c.recent_files.len(), MAX_RECENT);
    assert_eq!(c.recent_files[0], format!("score{}.musicxml", 2 * MAX_RECENT - 1));
    // A hand-edited file with too many or repeated entries is cut down.
    let many: String = (0..30).map(|i| format!("recent = f{}.xml\n", i % 15)).collect();
    assert_eq!(Config::from_text(&many).recent_files.len(), MAX_RECENT);
    if cfg!(windows) {
        c.remember_recent("D:\\Scores\\Beat.musicxml");
        c.remember_recent("d:\\scores\\beat.musicxml");
        assert_eq!(c.recent_files.iter().filter(|p| p.to_lowercase() == "d:\\scores\\beat.musicxml").count(), 1);
    }
}

#[test]
fn playback_settings_are_clamped_and_default_when_missing() {
    let c = Config::from_text("playback_volume = 7\nplayback_channel = 99\nplayback_audio = nonsense\n");
    assert_eq!(c.playback_volume, 1.0);
    assert_eq!(c.playback_channel, 16);
    assert!(c.playback_audio, "an unreadable value keeps the default");
    // A settings file from before playback existed still loads.
    let old = Config::from_text("latency_ms = 5\n");
    assert_eq!((old.playback_audio, old.playback_midi, old.playback_channel), (true, false, 10));
}
