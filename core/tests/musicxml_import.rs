//! Characterisation of the MusicXML import: what a file becomes, written down as text and
//! compared with the snapshots in `tests/snapshots/`. The snapshots were taken before the import
//! was reworked (IMP-1) and must not change with it. After an intended change, regenerate them
//! with `UPDATE_SNAPSHOTS=1 cargo test -p roll-model-core --test musicxml_import` and read the diff.

use std::fmt::Write as _;
use std::io::{Cursor, Write as _};
use std::path::{Path, PathBuf};

use roll_model_core::import::{check_invariants, import_bytes, load_score, ImportError, ImportOptions, Limits};
use roll_model_core::score::Score;

const SAMPLE: &str = include_str!("../../examples/basic_rock_beat.musicxml");

// The two functions the tests go through, so a change of the import's API touches only these.

fn parse(text: &str, expand_repeats: bool) -> Result<Score, String> {
    parse_named(text, "", expand_repeats)
}

fn parse_named(text: &str, name: &str, expand_repeats: bool) -> Result<Score, String> {
    let options = ImportOptions::default().with_repeats(expand_repeats);
    let loaded = import_bytes(text.as_bytes(), name, &options).map_err(|e| e.to_string())?;
    check_invariants(&loaded.score).unwrap();
    Ok(loaded.score)
}

fn load(path: &Path) -> Result<Score, String> {
    let loaded = load_score(path, &ImportOptions::default()).map_err(|e| e.to_string())?;
    check_invariants(&loaded.score).unwrap();
    Ok(loaded.score)
}

// Snapshots.

fn check(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots").join(format!("{name}.txt"));
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("no snapshot {}; run with UPDATE_SNAPSHOTS=1", path.display()));
    // Git may have turned the line ends into CRLF on Windows.
    assert_eq!(want.replace("\r\n", "\n"), actual, "snapshot {name}");
}

fn dump(s: &Score) -> String {
    let mut out = String::new();
    writeln!(out, "title: {}", s.title).unwrap();
    writeln!(out, "part: {}", s.part_name).unwrap();
    writeln!(out, "tempo: {:?}", s.tempo).unwrap();
    for (i, m) in s.measures.iter().enumerate() {
        writeln!(out, "measure {i}: #{} start={:.6} len={:.6} {}/{}", m.number, m.start_q, m.len_q, m.beats, m.beat_type).unwrap();
    }
    for n in &s.notes {
        writeln!(
            out,
            "note q={:.6} m={} {:?} step={} head={:?} accent={} ghost={} grace={}/{} stem={:?}",
            n.q, n.measure, n.lane, n.staff_step, n.head, n.accent, n.ghost, n.grace, n.grace_order, n.stem_up
        )
        .unwrap();
    }
    for r in &s.rests {
        writeln!(out, "rest q={:.6} m={} dur={:.6} up={} whole_bar={}", r.q, r.measure, r.dur_q, r.up, r.whole_bar).unwrap();
    }
    for w in &s.warnings {
        writeln!(out, "warning: {w}").unwrap();
    }
    out
}

// Hand-written scores. Divisions are 2 unless a score says otherwise: a quarter note lasts 2.

const HEAD: &str = r#"<score-partwise version="4.0"><work><work-title>Fixture</work-title></work><part-list><score-part id="P1"><part-name>Drumset</part-name>
<score-instrument id="K"><instrument-name>Bass Drum 1</instrument-name></score-instrument>
<score-instrument id="S"><instrument-name>Acoustic Snare</instrument-name></score-instrument>
<score-instrument id="H"><instrument-name>Closed Hi-Hat</instrument-name></score-instrument>
<score-instrument id="C"><instrument-name>Crash Cymbal 1</instrument-name></score-instrument>
<midi-instrument id="K"><midi-unpitched>37</midi-unpitched></midi-instrument>
<midi-instrument id="S"><midi-unpitched>39</midi-unpitched></midi-instrument>
<midi-instrument id="H"><midi-unpitched>43</midi-unpitched></midi-instrument>
<midi-instrument id="C"><midi-unpitched>50</midi-unpitched></midi-instrument>
</score-part></part-list><part id="P1">"#;

const FOUR_FOUR: &str = "<attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>percussion</sign></clef></attributes>";

fn score(measures: &str) -> String {
    format!("{HEAD}{measures}</part></score-partwise>")
}

fn measure(number: &str, body: &str) -> String {
    format!("<measure number=\"{number}\">{body}</measure>")
}

/// A note of instrument `inst` (K, S, H or C) lasting `dur` divisions, with extra child elements.
fn note(inst: &str, dur: u32, extra: &str) -> String {
    let (step, octave) = match inst {
        "K" => ("F", 4),
        "S" => ("C", 5),
        "H" | "C" => ("G", 5),
        other => panic!("no such instrument {other}"),
    };
    format!(
        "<note><unpitched><display-step>{step}</display-step><display-octave>{octave}</display-octave></unpitched><duration>{dur}</duration><instrument id=\"{inst}\"/><voice>1</voice>{extra}</note>"
    )
}

fn repeat_score() -> String {
    // |: kick snare | hat [1. snare :| [2. crash | kick
    let fwd = "<barline location=\"left\"><repeat direction=\"forward\"/></barline>";
    let first = "<barline location=\"left\"><ending number=\"1\" type=\"start\"/></barline>";
    let first_end = "<barline location=\"right\"><ending number=\"1\" type=\"stop\"/><repeat direction=\"backward\"/></barline>";
    let second = "<barline location=\"left\"><ending number=\"2\" type=\"start\"/></barline>";
    let second_end = "<barline location=\"right\"><ending number=\"2\" type=\"stop\"/></barline>";
    score(&[
        measure("1", &format!("{FOUR_FOUR}{fwd}{}{}", note("K", 2, ""), note("S", 2, ""))),
        measure("2", &note("H", 4, "")),
        measure("3", &format!("{first}{}{first_end}", note("S", 8, ""))),
        measure("4", &format!("{second}{}{second_end}", note("C", 8, ""))),
        measure("5", &note("K", 8, "")),
    ].concat())
}

fn tempo_score() -> String {
    let at_start = "<direction><sound tempo=\"90\"/></direction>";
    let half_note = "<direction><direction-type><metronome><beat-unit>half</beat-unit><per-minute>60</per-minute></metronome></direction-type><offset>2</offset></direction>";
    let mid_bar = "<sound tempo=\"150\"/>";
    score(&[
        measure("1", &format!("{FOUR_FOUR}{at_start}{}", note("K", 8, ""))),
        measure("2", &format!("{half_note}{}", note("S", 8, ""))),
        measure("3", &format!("{}{mid_bar}{}", note("H", 4, ""), note("H", 4, ""))),
    ].concat())
}

fn kitchen_sink_score() -> String {
    let accent = "<notations><articulations><accent/></articulations></notations>";
    let ghost = "<notehead parentheses=\"yes\">normal</notehead>";
    let open = "<notations><technical><open/></technical></notations>";
    let tie_stop = "<tie type=\"stop\"/>";
    let grace = format!("<note><grace/><unpitched><display-step>C</display-step><display-octave>5</display-octave></unpitched><instrument id=\"S\"/><voice>1</voice></note>");
    let chord_kick = "<note><chord/><unpitched><display-step>F</display-step><display-octave>4</display-octave></unpitched><duration>2</duration><instrument id=\"K\"/><voice>1</voice></note>";
    let cue = "<note><cue/><unpitched><display-step>C</display-step><display-octave>5</display-octave></unpitched><duration>2</duration><instrument id=\"S\"/><voice>1</voice></note>";
    let no_instrument = "<note><unpitched><display-step>G</display-step><display-octave>5</display-octave></unpitched><duration>2</duration><voice>1</voice><notehead>x</notehead></note>";
    let six_eight = "<attributes><time><beats>6</beats><beat-type>8</beat-type></time></attributes>";
    let two_plus_three = "<attributes><time><beats>2+3</beats><beat-type>8</beat-type></time></attributes>";
    score(&[
        // A pickup bar of one beat.
        format!("<measure number=\"0\" implicit=\"yes\">{FOUR_FOUR}{}</measure>", note("S", 2, "")),
        measure(
            "1",
            &format!(
                "{}{grace}{}{}{}{}{chord_kick}{}{cue}",
                note("H", 2, accent),
                note("S", 2, ghost),
                note("H", 2, open),
                note("H", 2, tie_stop),
                note("K", 2, ""),
                note("K", 2, ""), // the same instant as the chord above: one note survives
            ),
        ),
        measure("2", &format!("{six_eight}{}{}", note("K", 3, ""), note("S", 3, ""))),
        measure("3", &format!("{two_plus_three}{no_instrument}{}", note("C", 2, ""))),
    ].concat())
}

fn triplet_score() -> String {
    let triplet = "<time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification>";
    score(&measure(
        "1",
        &format!(
            "<attributes><divisions>3</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>{}{}{}{}{}{}",
            note("H", 1, triplet),
            note("H", 1, triplet),
            note("H", 1, triplet),
            note("S", 3, ""),
            note("K", 3, ""),
            note("S", 3, ""),
        ),
    ))
}

fn voices_score() -> String {
    // The hands in voice 1 with a rest, the feet in voice 2 with a rest, hidden rests, a whole-bar rest.
    let n = |voice: u32, inst: &str, step: &str, octave: u32, stem: &str| {
        format!("<note><unpitched><display-step>{step}</display-step><display-octave>{octave}</display-octave></unpitched><duration>2</duration><instrument id=\"{inst}\"/><voice>{voice}</voice><stem>{stem}</stem></note>")
    };
    let rest = |voice: u32, dur: u32, extra: &str| format!("<note><rest{extra}/><duration>{dur}</duration><voice>{voice}</voice></note>");
    let hidden = "<note print-object=\"no\"><rest/><duration>2</duration><voice>2</voice></note>";
    score(&[
        measure(
            "1",
            &format!(
                "{FOUR_FOUR}{}{}{}<backup><duration>8</duration></backup>{}{}{hidden}",
                n(1, "H", "G", 5, "up"),
                rest(1, 2, ""),
                rest(1, 4, ""),
                n(2, "K", "F", 4, "down"),
                rest(2, 4, ""),
            ),
        ),
        measure("2", &rest(1, 8, " measure=\"yes\"")),
        measure("3", &format!("<forward><duration>4</duration></forward>{}", n(1, "S", "C", 5, "up"))),
    ].concat())
}

#[test]
fn the_example_score() {
    check("sample", &dump(&parse(SAMPLE, true).unwrap()));
}

#[test]
fn repeats_and_endings_unrolled_or_not() {
    let text = repeat_score();
    check("repeats_unrolled", &dump(&parse(&text, true).unwrap()));
    check("repeats_as_written", &dump(&parse(&text, false).unwrap()));
}

#[test]
fn tempo_marks() {
    check("tempo", &dump(&parse(&tempo_score(), true).unwrap()));
}

#[test]
fn graces_accents_ghosts_ties_chords_pickups_and_odd_meters() {
    check("kitchen_sink", &dump(&parse(&kitchen_sink_score(), true).unwrap()));
}

#[test]
fn triplets() {
    check("triplets", &dump(&parse(&triplet_score(), true).unwrap()));
}

#[test]
fn voices_and_rests() {
    check("voices", &dump(&parse(&voices_score(), true).unwrap()));
}

#[test]
fn a_file_with_several_parts_uses_the_drum_part() {
    let text = format!(
        "<score-partwise><part-list><score-part id=\"P1\"><part-name>Piano</part-name></score-part><score-part id=\"P2\"><part-name>Drums</part-name></score-part></part-list>\
         <part id=\"P1\"><measure number=\"1\"><attributes><divisions>1</divisions><clef><sign>G</sign></clef></attributes><note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration></note></measure></part>\
         <part id=\"P2\"><measure number=\"1\"><attributes><divisions>1</divisions><clef><sign>percussion</sign></clef></attributes><note><unpitched><display-step>C</display-step><display-octave>5</display-octave></unpitched><duration>4</duration></note></measure></part></score-partwise>"
    );
    check("two_parts", &dump(&parse(&text, true).unwrap()));
}

#[test]
fn the_title_comes_from_the_work_the_movement_or_a_credit() {
    let body = measure("1", &note("K", 8, ""));
    let head = "<part-list><score-part id=\"P1\"><part-name>D</part-name></score-part></part-list>";
    let wrap = |front: &str| format!("<score-partwise>{front}{head}<part id=\"P1\">{body}</part></score-partwise>");
    let titles: Vec<String> = [
        "<work><work-title>Work</work-title></work><movement-title>Movement</movement-title>",
        "<movement-title>Movement</movement-title>",
        "<credit><credit-words>Credit</credit-words></credit>",
        "",
    ]
    .iter()
    .map(|front| parse(&wrap(front), true).unwrap().title)
    .collect();
    assert_eq!(titles, ["Work", "Movement", "Credit", "Untitled"]);
}

#[test]
fn what_is_not_a_readable_score_says_why() {
    let cases = [
        ("not_xml", "this is not xml"),
        ("timewise", "<score-timewise/>"),
        ("other_root", "<html/>"),
        ("no_parts", "<score-partwise><part-list/></score-partwise>"),
        ("no_measures", "<score-partwise><part-list/><part id=\"P1\"/></score-partwise>"),
    ];
    let mut out = String::new();
    for (name, text) in cases {
        writeln!(out, "{name}: {}", parse_named(text, "bad.musicxml", true).unwrap_err()).unwrap();
    }
    check("parse_errors", &out);
}

// Files: plain text, UTF-16, and the compressed .mxl container.

fn mxl(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, content) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn utf16(text: &str, big_endian: bool) -> Vec<u8> {
    let mut out = if big_endian { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
    for unit in text.encode_utf16() {
        out.extend(if big_endian { unit.to_be_bytes() } else { unit.to_le_bytes() });
    }
    out
}

struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str, bytes: &[u8]) -> TempFile {
        let dir = std::env::temp_dir().join(format!("roll-model-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        TempFile(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn files_of_every_container_and_encoding() {
    let text = score(&measure("1", &format!("{FOUR_FOUR}{}", note("K", 8, ""))));
    let untitled = text.replace("<work><work-title>Fixture</work-title></work>", "");
    let container = "<container><rootfiles><rootfile full-path=\"inner/score.xml\"/></rootfiles></container>";
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("plain.musicxml", text.clone().into_bytes()),
        ("little_endian.xml", utf16(&text, false)),
        ("big_endian.xml", utf16(&text, true)),
        ("with_container.mxl", mxl(&[("META-INF/container.xml", container), ("inner/score.xml", &text), ("other.xml", "<x/>")])),
        ("container_names_a_missing_file.mxl", mxl(&[("META-INF/container.xml", container), ("fallback.musicxml", &text)])),
        ("no_container.mxl", mxl(&[("readme.txt", "x"), ("score.xml", &text)])),
        ("no_document.mxl", mxl(&[("readme.txt", "x")])),
        ("not_a_zip.mxl", b"PK this is not an archive".to_vec()),
        ("untitled_gets_the_file_name.musicxml", untitled.into_bytes()),
        ("garbage.musicxml", b"\x00\x01\x02".to_vec()),
    ];
    let mut out = String::new();
    for (name, bytes) in &cases {
        let file = TempFile::new(name, bytes);
        match load(&file.0) {
            Ok(s) => writeln!(out, "{name}: ok title={:?} measures={} notes={}", s.title, s.measures.len(), s.notes.len()).unwrap(),
            Err(e) => writeln!(out, "{name}: error {e}").unwrap(),
        }
    }
    // A file that is not there; the text is the operating system's, so only its kind is kept.
    let missing = std::env::temp_dir().join("roll-model-import-there-is-no-such-file.musicxml");
    writeln!(out, "missing: error={}", load(&missing).is_err()).unwrap();
    check("files", &out);
}

#[test]
fn bytes_that_are_not_a_readable_score_say_what_they_are() {
    let say = |bytes: &[u8], name: &str| import_bytes(bytes, name, &ImportOptions::default()).unwrap_err().to_string();
    assert_eq!(say(b"just some words", "notes.txt"), "not a score file this program reads (it opens .musicxml, .xml, .mxl, .mid, .midi)");
    assert_eq!(say(b"PK\x03\x04", "song.mscz"), "MuseScore files are not read directly; export the score as MusicXML from MuseScore and open that file");
    assert_eq!(say(b"\x18FICHIER GUITAR PRO v5.00", "song"), "Guitar Pro files are not read directly; export the score as MusicXML from Guitar Pro and open that file");
    let mut tight = ImportOptions::default();
    tight.limits = Limits { max_bytes: 10, ..Limits::default() };
    let err = import_bytes(b"<score-partwise/>, much longer than ten bytes", "x.musicxml", &tight).unwrap_err();
    assert!(matches!(err, ImportError::TooLarge(_)), "{err:?}");
}

#[test]
fn measure_repeats_copy_the_music_of_the_measures_before() {
    let start = |n: &str| format!("<attributes><measure-style><measure-repeat type=\"start\">{n}</measure-repeat></measure-style></attributes>");
    let stop = "<attributes><measure-style><measure-repeat type=\"stop\"/></measure-style></attributes>";
    let whole_bar_rest = "<note><rest measure=\"yes\"/><duration>8</duration><voice>1</voice></note>";
    // bar 1 kick, bar 2 and 3 = "%" (one bar back, the second with a rest written in it), bar 4 snare,
    // bars 5 and 6 are a one-bar group again, bars 7 and 8 repeat bars 5 and 6 two at a time.
    let text = score(&[
        measure("1", &format!("{FOUR_FOUR}{}", note("K", 2, ""))),
        measure("2", &start("1")),
        measure("3", whole_bar_rest),
        measure("4", &format!("{stop}{}", note("S", 2, ""))),
        measure("5", &note("H", 2, "")),
        measure("6", &note("C", 2, "")),
        measure("7", &start("2")),
        measure("8", ""),
    ].concat());
    let s = parse(&text, true).unwrap();
    let lanes: Vec<(usize, String)> = s.notes.iter().map(|n| (n.measure, format!("{:?}", n.lane))).collect();
    let expected: Vec<(usize, String)> =
        [(0, "Kick"), (1, "Kick"), (2, "Kick"), (3, "Snare"), (4, "HiHatClosed"), (5, "Crash1"), (6, "HiHatClosed"), (7, "Crash1")]
            .iter()
            .map(|(m, l)| (*m, l.to_string()))
            .collect();
    assert_eq!(lanes, expected);
    check("measure_repeat", &dump(&s));
}

#[test]
fn a_drum_set_written_as_one_instrument_is_read_from_the_staff_positions() {
    // The shape of a Sibelius or Dolet export: one "Drum Set" instrument, no General MIDI numbers,
    // so every note is told apart by where it sits on the staff and by its notehead.
    let n = |step: &str, octave: u32, head: &str, dur: u32| {
        format!("<note><unpitched><display-step>{step}</display-step><display-octave>{octave}</display-octave></unpitched><duration>{dur}</duration><instrument id=\"P1-I1\"/><voice>1</voice><notehead>{head}</notehead></note>")
    };
    let text = format!(
        "<score-partwise><part-list><score-part id=\"P1\"><part-name>Drum Set</part-name><score-instrument id=\"P1-I1\"><instrument-name>Drum Set</instrument-name></score-instrument><midi-instrument id=\"P1-I1\"><midi-channel>10</midi-channel><midi-program>1</midi-program></midi-instrument></score-part></part-list><part id=\"P1\">{}</part></score-partwise>",
        measure(
            "1",
            &format!(
                "{FOUR_FOUR}{}{}{}{}{}{}{}{}",
                n("G", 5, "x", 2),
                n("F", 4, "normal", 2),
                n("G", 5, "x", 2),
                n("C", 5, "normal", 2),
                n("A", 5, "x", 2),
                n("E", 5, "normal", 2),
                n("D", 5, "normal", 2),
                n("A", 4, "normal", 2),
            ),
        )
    );
    let s = parse(&text, true).unwrap();
    let lanes: Vec<String> = s.notes.iter().map(|n| format!("{:?}", n.lane)).collect();
    assert_eq!(lanes[..4], ["HiHatClosed", "Kick", "HiHatClosed", "Snare"], "{lanes:?}");
    assert!(s.warnings.iter().any(|w| w.contains("guessed from staff position")), "{:?}", s.warnings);
    check("one_instrument_by_staff", &dump(&s));
}
