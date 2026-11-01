//! The MIDI import through the public API. Files are built in code with the crate's small writer,
//! so no binary fixture is needed.

use roll_model_core::drums::Lane;
use roll_model_core::import::smf::{write_smf, Ev};
use roll_model_core::import::{check_invariants, import_bytes, ImportError, ImportOptions, Limits, Loaded};

const PPQ: u16 = 480;
const SAMPLE_XML: &str = include_str!("../../examples/basic_rock_beat.musicxml");

const KICK: u8 = 36;
const SNARE: u8 = 38;
const HAT: u8 = 42;
const CRASH: u8 = 49;
const TOM_HIGH: u8 = 50;
const TOM_FLOOR: u8 = 43;

/// The groove of `examples/basic_rock_beat.musicxml` as `(position in quarters, GM key, velocity)`.
fn rock_beat() -> Vec<(f64, u8, u8)> {
    let mut notes = Vec::new();
    for bar in 0..3 {
        let b = f64::from(bar) * 4.0;
        for eighth in 0..8 {
            notes.push((b + f64::from(eighth) / 2.0, HAT, if eighth % 2 == 0 { 100 } else { 70 }));
        }
        for (q, key) in [(0.0, KICK), (1.0, SNARE), (2.0, KICK), (3.0, SNARE)] {
            notes.push((b + q, key, 100));
        }
    }
    notes.push((12.0, KICK, 100));
    notes.push((12.0, CRASH, 120));
    for (q, key) in [(13.0, SNARE), (13.5, SNARE), (14.0, TOM_HIGH), (14.5, TOM_HIGH), (15.0, TOM_FLOOR), (15.5, TOM_FLOOR)] {
        notes.push((q, key, 100));
    }
    notes
}

fn track_of(notes: &[(f64, u8, u8)], channel: u8) -> Vec<(u64, Ev)> {
    let mut track = Vec::new();
    for &(q, key, vel) in notes {
        let tick = (q * f64::from(PPQ)).round() as u64;
        track.push((tick, Ev::NoteOn { ch: channel, key, vel }));
        track.push((tick + 60, Ev::NoteOff { ch: channel, key }));
    }
    track
}

fn conductor(us: u32) -> Vec<(u64, Ev)> {
    vec![(0, Ev::Tempo(us)), (0, Ev::TimeSig(4, 2))]
}

/// The bytes of the sample `.mid`.
fn rock_beat_file() -> Vec<u8> {
    write_smf(1, PPQ, &[conductor(600_000), track_of(&rock_beat(), 9)])
}

fn open(bytes: &[u8]) -> Result<Loaded, ImportError> {
    let loaded = import_bytes(bytes, "groove.mid", &ImportOptions::default())?;
    check_invariants(&loaded.score).unwrap();
    Ok(loaded)
}

fn placed(score: &roll_model_core::score::Score) -> Vec<(i64, usize, Lane)> {
    score.notes.iter().map(|n| ((n.q * 1000.0).round() as i64, n.measure, n.lane)).collect()
}

#[test]
fn the_sample_midi_file_is_what_the_code_writes() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/basic_rock_beat.mid");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, rock_beat_file()).unwrap();
    }
    let on_disk = std::fs::read(&path).expect("examples/basic_rock_beat.mid; run with UPDATE_SNAPSHOTS=1 to write it");
    assert_eq!(on_disk, rock_beat_file());
}

#[test]
fn a_midi_groove_opens_as_the_same_score_as_its_musicxml() {
    let midi = open(&rock_beat_file()).unwrap();
    let xml = import_bytes(SAMPLE_XML.as_bytes(), "sample.musicxml", &ImportOptions::default()).unwrap();
    assert_eq!(placed(&midi.score), placed(&xml.score));
    assert_eq!(midi.score.measures.len(), xml.score.measures.len());
    assert!((midi.score.tempo.bpm_at(0.0) - 100.0).abs() < 1e-9);
    assert_eq!(midi.report.format.name(), "MIDI");
}

#[test]
fn tempo_changes_and_a_new_time_signature_are_kept() {
    // Two bars of 4/4 at 120, then 3/4 at 90.
    let mut conductor = vec![(0, Ev::Tempo(500_000)), (0, Ev::TimeSig(4, 2))];
    conductor.push((3840, Ev::TimeSig(3, 2)));
    conductor.push((3840, Ev::Tempo(666_667)));
    let drums = track_of(&[(0.0, KICK, 100), (4.0, SNARE, 100), (8.0, KICK, 100), (9.0, HAT, 100), (10.0, SNARE, 100), (11.0, KICK, 100)], 9);
    let loaded = open(&write_smf(1, PPQ, &[conductor, drums])).unwrap();
    let s = loaded.score;
    let lens: Vec<(f64, u32, u32)> = s.measures.iter().map(|m| (m.len_q, m.beats, m.beat_type)).collect();
    assert_eq!(lens, [(4.0, 4, 4), (4.0, 4, 4), (3.0, 3, 4), (3.0, 3, 4)]);
    assert!((s.tempo.bpm_at(0.0) - 120.0).abs() < 1e-6);
    assert!((s.tempo.bpm_at(8.0) - 90.0).abs() < 1e-3);
    assert_eq!(s.notes.iter().map(|n| n.measure).collect::<Vec<_>>(), [0, 1, 2, 2, 2, 3]);
}

#[test]
fn drums_are_found_on_channel_ten_whatever_the_track() {
    let kick = track_of(&[(0.0, KICK, 100), (1.0, KICK, 100)], 9);
    let snare = track_of(&[(0.5, SNARE, 100)], 9);
    let piano = track_of(&[(0.0, 60, 90), (1.0, 64, 90)], 0);
    let s = open(&write_smf(1, PPQ, &[conductor(500_000), piano, kick, snare])).unwrap().score;
    assert_eq!(s.notes.iter().map(|n| n.lane).collect::<Vec<_>>(), [Lane::Kick, Lane::Snare, Lane::Kick]);
}

#[test]
fn without_channel_ten_a_track_named_like_drums_is_used() {
    let mut drums = vec![(0, Ev::TrackName("Drum Kit".into()))];
    drums.extend(track_of(&[(0.0, KICK, 100), (1.0, SNARE, 100)], 0));
    let piano = track_of(&[(0.0, 60, 90), (2.0, 62, 90)], 1);
    let loaded = open(&write_smf(1, PPQ, &[conductor(500_000), piano, drums])).unwrap();
    assert_eq!(loaded.score.notes.len(), 2);
    assert!(loaded.score.warnings.iter().any(|w| w.contains("named like a drum part")), "{:?}", loaded.score.warnings);
}

#[test]
fn without_a_name_a_channel_that_looks_like_drums_is_used_and_a_melody_is_not() {
    let drums = track_of(&[(0.0, KICK, 100), (1.0, SNARE, 100), (2.0, KICK, 100), (3.0, SNARE, 100)], 3);
    // A pitched channel in the same range, but with many different keys.
    let melody: Vec<(f64, u8, u8)> = (0..16).map(|i| (f64::from(i) / 2.0, 48 + i as u8, 90)).collect();
    let loaded = open(&write_smf(1, PPQ, &[conductor(500_000), track_of(&melody, 0), drums])).unwrap();
    assert_eq!(loaded.score.notes.len(), 4);
    assert!(loaded.score.warnings.iter().any(|w| w.contains("channel 4")), "{:?}", loaded.score.warnings);

    let err = open(&write_smf(1, PPQ, &[conductor(500_000), track_of(&melody, 0)])).unwrap_err();
    assert_eq!(err, ImportError::NoDrumPart);
}

#[test]
fn a_file_without_notes_has_no_drum_part() {
    assert_eq!(open(&write_smf(1, PPQ, &[conductor(500_000)])).unwrap_err(), ImportError::NoDrumPart);
    assert_eq!(open(&write_smf(1, PPQ, &[])).unwrap_err(), ImportError::NoDrumPart);
}

#[test]
fn a_note_on_with_velocity_zero_is_a_note_off_and_unknown_keys_are_counted() {
    let mut track = track_of(&[(0.0, KICK, 100)], 9);
    track.push((480, Ev::NoteOn { ch: 9, key: SNARE, vel: 0 }));
    track.extend(track_of(&[(1.0, 90, 100), (2.0, 91, 100)], 9));
    let loaded = open(&write_smf(0, PPQ, &[[conductor(500_000), track].concat()])).unwrap();
    assert_eq!(loaded.score.notes.len(), 3, "the velocity-0 note is not an onset");
    assert!(loaded.score.warnings.iter().any(|w| w.starts_with("2 notes are on keys")), "{:?}", loaded.score.warnings);
}

#[test]
fn a_file_without_tempo_or_signature_gets_the_defaults_and_says_so() {
    let loaded = open(&write_smf(0, PPQ, &[track_of(&[(0.0, KICK, 100), (4.0, KICK, 100)], 9)])).unwrap();
    let s = loaded.score;
    assert_eq!(s.measures.len(), 2);
    assert!((s.tempo.bpm_at(0.0) - 120.0).abs() < 1e-9);
    assert!(s.warnings.iter().any(|w| w.contains("No time signature")));
    assert!(s.warnings.iter().any(|w| w.contains("No tempo")));
}

#[test]
fn files_that_cannot_be_read_say_why() {
    let say = |bytes: &[u8]| open(bytes).unwrap_err().to_string();
    assert!(say(&write_smf(2, PPQ, &[conductor(500_000)])).contains("format 2"));
    let mut smpte = write_smf(1, PPQ, &[conductor(500_000)]);
    smpte[12..14].copy_from_slice(&[0xE7, 0x28]);
    assert!(say(&smpte).contains("SMPTE"));
    let zero_division = write_smf(1, 0, &[conductor(500_000)]);
    assert!(say(&zero_division).contains("zero") || say(&zero_division).contains("MIDI error"), "{}", say(&zero_division));
    assert!(say(b"MThd").starts_with("MIDI error"));
}

#[test]
fn running_status_is_understood() {
    let mut bytes = b"MThd".to_vec();
    bytes.extend([0, 0, 0, 6, 0, 0, 0, 1, 0x01, 0xE0]);
    // 0x99 note on channel 10; the second and third events leave the status byte out.
    let body = [0x00, 0x99, 36, 100, 0x83, 0x60, 38, 100, 0x00, 42, 100, 0x00, 0xFF, 0x2F, 0x00];
    bytes.extend(b"MTrk");
    bytes.extend((body.len() as u32).to_be_bytes());
    bytes.extend(body);
    let s = open(&bytes).unwrap().score;
    assert_eq!(s.notes.iter().map(|n| n.lane).collect::<Vec<_>>(), [Lane::Kick, Lane::Snare, Lane::HiHatClosed]);
}

#[test]
fn limits_stop_a_file_that_asks_for_too_much() {
    let mut tight = ImportOptions::default();
    tight.limits = Limits { max_events: 10, ..Limits::default() };
    let many: Vec<(f64, u8, u8)> = (0..20).map(|i| (f64::from(i), KICK, 100)).collect();
    let bytes = write_smf(1, PPQ, &[conductor(500_000), track_of(&many, 9)]);
    assert!(matches!(import_bytes(&bytes, "x.mid", &tight), Err(ImportError::TooLarge(_))));

    // One note a very long way from the start would otherwise lay down millions of bars.
    let far = vec![(0, Ev::NoteOn { ch: 9, key: KICK, vel: 100 }), (200_000_000, Ev::NoteOn { ch: 9, key: KICK, vel: 100 })];
    let err = import_bytes(&write_smf(0, PPQ, &[far]), "x.mid", &ImportOptions::default()).unwrap_err();
    assert!(matches!(err, ImportError::TooLarge(_)), "{err:?}");
}

#[test]
fn damaged_files_never_panic_or_hang() {
    let good = rock_beat_file();
    // Every truncation of a valid file.
    for len in 0..good.len() {
        let _ = import_bytes(&good[..len], "cut.mid", &ImportOptions::default());
    }
    // A track that claims to be four gigabytes long.
    let mut huge = good[..14].to_vec();
    huge.extend(b"MTrk");
    huge.extend(0xFFFF_FFF0u32.to_be_bytes());
    huge.extend([0, 0x99, 36, 100]);
    let _ = import_bytes(&huge, "huge.mid", &ImportOptions::default());
    // Bytes flipped by a fixed pseudo-random sequence.
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..2000 {
        let mut bytes = good.clone();
        for _ in 0..1 + next() % 4 {
            let at = (next() % bytes.len() as u64) as usize;
            bytes[at] = next() as u8;
        }
        if let Ok(loaded) = import_bytes(&bytes, "flipped.mid", &ImportOptions::default()) {
            check_invariants(&loaded.score).unwrap();
        }
    }
}

/// Opens every `.mid` under the folder named by `ROLL_MODEL_MIDI_DIR` and prints what came out.
/// Not part of the normal run: `ROLL_MODEL_MIDI_DIR=... cargo test -p roll-model-core --test midi_import real_files -- --ignored --nocapture`.
#[test]
#[ignore]
fn real_files() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| ["mid", "midi", "xml", "musicxml", "mxl", "mscz"].iter().any(|x| e == *x)) {
                out.push(path);
            }
        }
    }
    let Some(dir) = std::env::var_os("ROLL_MODEL_MIDI_DIR") else { return };
    let mut files = Vec::new();
    walk(std::path::Path::new(&dir), &mut files);
    files.sort();
    let (mut ok, mut failed, mut warnings) = (0, 0, std::collections::BTreeMap::<String, usize>::new());
    let (mut free, mut offsets, mut shifts, mut merged, mut notes, mut accents, mut ghosts, mut moved) = (0, Vec::new(), Vec::new(), 0, 0, 0, 0, 0);
    let mut subdivisions = std::collections::BTreeMap::<u32, usize>::new();
    for path in &files {
        match roll_model_core::import::load_score(path, &ImportOptions::default()) {
            Ok(loaded) => {
                ok += 1;
                if std::env::var_os("ROLL_MODEL_VERBOSE").is_some() {
                    let mut lanes = std::collections::BTreeMap::<String, usize>::new();
                    for n in &loaded.score.notes {
                        *lanes.entry(format!("{:?}", n.lane)).or_default() += 1;
                    }
                    println!("FILE {}: {} bars, {:.0} BPM, {} accents {} ghosts, {lanes:?}", path.file_name().unwrap().to_string_lossy(), loaded.score.measures.len(), loaded.score.tempo.bpm_at(0.0), loaded.score.notes.iter().filter(|n| n.accent).count(), loaded.score.notes.iter().filter(|n| n.ghost).count());
                }
                if let Some(m) = &loaded.report.midi {
                    free += usize::from(m.free_time);
                    offsets.push(m.offset_ms);
                    shifts.push(m.max_shift_ms);
                    merged += m.merged;
                    moved += m.moved;
                    notes += loaded.score.notes.len();
                    for (n, c) in &m.beats_by_subdivision {
                        *subdivisions.entry(*n).or_default() += c;
                    }
                }
                accents += loaded.score.notes.iter().filter(|n| n.accent).count();
                ghosts += loaded.score.notes.iter().filter(|n| n.ghost).count();
                check_invariants(&loaded.score).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                for w in &loaded.score.warnings {
                    if w.contains("No drum notes") || w.contains("no recognisable") { println!("NOTE {}: {w}", path.file_name().unwrap().to_string_lossy()); }
                    let key: String = w.split_once("(keys").map_or(w.clone(), |(_, k)| format!("keys{}", k.split(")").next().unwrap_or(""))).chars().filter(|c| !c.is_ascii_alphabetic() || true).collect();
                    *warnings.entry(key).or_default() += 1;
                }
            }
            Err(e) => {
                failed += 1;
                println!("FAILED {}: {e}", path.display());
            }
        }
    }
    println!("{} files: {ok} opened, {failed} failed", files.len());
    offsets.sort_by(f64::total_cmp);
    shifts.sort_by(f64::total_cmp);
    let pct = |v: &Vec<f64>, p: usize| v.get(v.len().saturating_sub(1) * p / 100).copied().unwrap_or(0.0);
    println!("free time: {free}; offset ms p5/p50/p95: {:.1} {:.1} {:.1}; max shift ms p50/p90/max: {:.1} {:.1} {:.1}", pct(&offsets, 5), pct(&offsets, 50), pct(&offsets, 95), pct(&shifts, 50), pct(&shifts, 90), pct(&shifts, 100));
    println!("notes {notes}, moved over {} ms: {moved}, merged: {merged}, accents {accents} ({:.1}%), ghosts {ghosts} ({:.1}%)", roll_model_core::import::MOVED_MS, accents as f64 * 100.0 / notes.max(1) as f64, ghosts as f64 * 100.0 / notes.max(1) as f64);
    println!("beats by subdivision: {subdivisions:?}");
    for (w, n) in warnings {
        println!("{n:5} x {w}");
    }
}

#[test]
fn the_edge_keys_of_roland_modules_have_instruments() {
    let track = track_of(&[(0.0, 22, 100), (1.0, 26, 100), (2.0, 58, 100)], 9);
    let s = open(&write_smf(0, PPQ, &[[conductor(500_000), track].concat()])).unwrap().score;
    assert_eq!(s.notes.iter().map(|n| n.lane).collect::<Vec<_>>(), [Lane::HiHatClosed, Lane::HiHatOpen, Lane::Tom3]);
    assert!(s.warnings.iter().all(|w| !w.contains("no known drum instrument")));
}

#[test]
fn the_first_bar_can_start_later_and_the_music_before_it_is_a_short_bar() {
    // A pickup note on the beat before the downbeat, then two bars.
    let notes = [(0.0, SNARE, 100), (1.0, KICK, 100), (3.0, SNARE, 100), (5.0, KICK, 100), (7.0, SNARE, 100)];
    let bytes = write_smf(1, PPQ, &[conductor(500_000), track_of(&notes, 9)]);
    let mut options = ImportOptions::default();
    let plain = import_bytes(&bytes, "x.mid", &options).unwrap().score;
    assert_eq!(plain.measures.len(), 2);

    options.midi.first_bar_beats = 1;
    let shifted = import_bytes(&bytes, "x.mid", &options).unwrap().score;
    check_invariants(&shifted).unwrap();
    let lens: Vec<f64> = shifted.measures.iter().map(|m| m.len_q).collect();
    assert_eq!(lens, [1.0, 4.0, 4.0]);
    assert_eq!(shifted.notes.iter().map(|n| n.measure).collect::<Vec<_>>(), [0, 1, 1, 2, 2]);
    assert!(shifted.measures.iter().all(|m| (m.beats, m.beat_type) == (4, 4)), "the signature is the file's");
    assert_eq!(placed(&shifted).iter().map(|p| p.0).collect::<Vec<_>>(), [0, 1000, 3000, 5000, 7000], "no note moves");
}

#[test]
fn the_first_bar_does_not_start_later_when_the_time_signature_changes() {
    let mut conductor = conductor(500_000);
    conductor.push((3840, Ev::TimeSig(3, 2)));
    let bytes = write_smf(1, PPQ, &[conductor, track_of(&[(0.0, SNARE, 100), (9.0, KICK, 100)], 9)]);
    let mut options = ImportOptions::default();
    options.midi.first_bar_beats = 1;
    let s = import_bytes(&bytes, "x.mid", &options).unwrap().score;
    assert_eq!(s.measures[0].len_q, 4.0);
    assert!(s.warnings.iter().any(|w| w.contains("cannot start later")), "{:?}", s.warnings);
}

/// Compares two files of the same music: `ROLL_MODEL_A` and `ROLL_MODEL_B`.
#[test]
#[ignore]
fn compare_two_files() {
    let (Some(a), Some(b)) = (std::env::var_os("ROLL_MODEL_A"), std::env::var_os("ROLL_MODEL_B")) else { return };
    let mut options = ImportOptions::default();
    if std::env::var_os("ROLL_MODEL_GRID32").is_some() {
        options.midi.grid = roll_model_core::import::Grid::ThirtySecond;
    }
    let load = |p: &std::ffi::OsString| roll_model_core::import::load_score(std::path::Path::new(p), &options).unwrap().score;
    let (a, b) = (load(&a), load(&b));
    let key = |s: &roll_model_core::score::Score| -> Vec<(i64, String)> { s.notes.iter().map(|n| ((n.q * 1000.0).round() as i64, format!("{:?}", n.lane))).collect() };
    let (ka, kb) = (key(&a), key(&b));
    println!("A: {} notes, bars {}, first q {:?}, last q {:?}", ka.len(), a.measures.len(), ka.first(), ka.last());
    println!("B: {} notes, bars {}, first q {:?}, last q {:?}", kb.len(), b.measures.len(), kb.first(), kb.last());
    let sa: std::collections::HashSet<_> = ka.iter().cloned().collect();
    let sb: std::collections::HashSet<_> = kb.iter().cloned().collect();
    let only_a: Vec<_> = ka.iter().filter(|k| !sb.contains(k)).take(12).collect();
    let only_b: Vec<_> = kb.iter().filter(|k| !sa.contains(k)).take(12).collect();
    println!("only in A: {} e.g. {only_a:?}", ka.iter().filter(|k| !sb.contains(k)).count());
    println!("only in B: {} e.g. {only_b:?}", kb.iter().filter(|k| !sa.contains(k)).count());
    for (name, s) in [("A", &a), ("B", &b)] {
        let lens: Vec<String> = s.measures.iter().take(14).map(|m| format!("{}:{}/{}", m.len_q, m.beats, m.beat_type)).collect();
        println!("{name} first bars: {lens:?}; tempo {:?}", s.tempo);
    }
}
