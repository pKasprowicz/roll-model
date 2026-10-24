# Handoff — Drum Tutor

State of the project at handoff (2026-10-24) and what to do next. Read this before changing code.

## Goal

A Rust desktop app that:

1. loads a MusicXML drum score and shows it on the **Score** tab;
2. captures MIDI from a **Roland V31** drum module and draws the hits as notes directly under the score notes, on the same time axis;
3. compares the captured hits with the score and grades the drummer.

All three are implemented. User-facing documentation is in `README.md`.

## Status: builds, tests pass, UI smoke-tested; V31 never connected

First real build and smoke test done on 2026-10-24 (Windows 11, `x86_64-pc-windows-gnu`, Rust 1.99.1). `cargo build` (debug and release) and `cargo check --no-default-features` succeed with no warnings and **no source changes were needed for the dependency API mismatches** that the old notes feared (`convex_polygon`, `Slider`, rodio 0.19, zip 2).

**Toolchain:** needs a recent Rust. Rust 1.82 fails because the resolved dependency tree (`icu_*` via `url`/`idna`) uses edition 2024. 1.85 is the first release that can parse edition-2024 crates and the dependencies may need more; only 1.99.1 was tested. `Cargo.lock` now exists and is worth committing: it is the set of versions that built.

| Part | Verified how | Confidence |
|---|---|---|
| `xml.rs`, `drums.rs`, `score.rs`, `musicxml.rs`, `analysis.rs`, `config.rs` | 14 unit tests pass | High |
| `app.rs`, `render.rs`, `loader.rs`, `main.rs` | Ran the app: sample loads, both staves line up, take lifecycle (count-in, record, auto-stop), per-note colours, Results and Settings tabs, error banner | Medium-high |
| Keyboard pads (`app.rs::keyboard_pads`) | Scripted take with synthetic key presses: 20 perfect / 12 good / 1 ok / 11 missed / 1 extra as expected | Medium (frame-time stamps, see below) |
| `midi.rs` | Compiles. The test machine had no MIDI ports, so capture and port selection were not exercised | Low |
| `metronome.rs` | Compiles and the thread starts with each take. The click was **not heard**; audio output and timing are unchecked | Low |
| Real V31 timing, note map, latency | Not tested | Unknown |

## First tasks

1. ~~Build.~~ Done, see above.
2. ~~Run the tests.~~ Done, 14 pass. Splitting into `lib.rs` + `main.rs` would **not** by itself avoid building the GUI/MIDI crates, because they are dependencies of the same package. It would only pay off with a workspace or optional dependencies; skipped.
3. ~~Smoke-test the UI.~~ Done. Fixes made along the way:
   - egui 0.29's default fonts have no `●` (U+25CF), `✕` (U+2715) or `→` (U+2192) and draw a box instead. Replaced with `⏺`/`⏹` (from the bundled emoji icon font), a "Dismiss" button and plain text. `■ · … ± − – — •` and `📂 🎼 📊 ⚙ ⟳ 💾 ⏺ ⏹` render fine. `⚠` (U+26A0) was not seen on screen. Check any new glyph in the running app before using it.
   - The two control rows in the Score tab are plain `horizontal` rows now. Sliders do not wrap inside `horizontal_wrapped`, so the old single row ran off the right edge and made the score layout wider than the window.
   - Load errors repeated the file path; the loader no longer adds it.
4. **Test with the V31** (only the user can do this). Confirm the note numbers in **Settings → Monitor** match the default map in `drums.rs::NoteMap::roland_v31()`. Also check that the metronome click is audible and that **Settings → Timing → Calibrate** gives a sensible latency offset.

## Architecture

```
main.rs        eframe entry; optional CLI arg = file to open
app.rs         DrumApp state + Score/Results/Settings tabs; take lifecycle
render.rs      Draws score staff + "You" staff per system; layout, hit-testing
loader.rs      Read .musicxml/.xml (UTF-8/16) and .mxl (zip)
musicxml.rs    MusicXML -> Score; repeats/voltas unrolled in playback_order()
xml.rs         Dependency-free XML DOM parser
score.rs       Score, Measure, ScoreNote, TempoMap (q <-> seconds)
drums.rs       Lane enum, GM + Roland note maps, staff positions, name heuristics
analysis.rs    expected_notes(), analyze() -> Report (matching + metrics + feedback)
midi.rs        midir input (callback -> Arc<Mutex<Vec<RawEvent>>>) and output
metronome.rs   Click thread: precise sleep+spin; audio (feature "audio") and/or MIDI
config.rs      key = value settings file, auto-saved when Config changes
```

### Key conventions and invariants

- **Time units.** Score positions are in quarter notes (`q`) from the start of the *unrolled* score. Seconds come only from `TempoMap::q_to_sec(q, scale)`, where `scale` is the practice tempo factor (1.0 = written tempo). `sec_to_q` is its inverse and extrapolates for negative times (count-in).
- **Take time zero** is the start of the first measure of the practice range. `Take.t0` is that measure's absolute time in seconds, so hit `q = sec_to_q(hit.t + t0, scale)`. In `start_take`, zero is `now + 0.5 s lead + count-in`, so the first score note comes 0.5 s later than the count-in alone suggests. Anything that scripts a take from outside has to add that lead.
- **Hit timestamps** come from `Instant::now()` in the midir callback (not midir's `stamp`). `latency_ms` from config is subtracted when a hit is recorded.
- **Staff steps.** Step 0 is the bottom line (E4), the top line (F5) is step 8. Played notes are drawn at the position the score uses for the same lane (`Score::staff_for_lane`), falling back to `Lane::default_staff`.
- **Matching** (`analysis::analyze`):
  - All (expected, hit) pairs within `window_ms` are candidates: same lane at cost |dt|, related lane (`Lane::related`) at cost |dt| + 40 ms.
  - Pairs are assigned greedily, cheapest first, one-to-one.
  - Expected notes later than `elapsed` stay `Pending` and are excluded from the stats.
  - Grace notes are optional: never counted as missed, and a hit that matches one is not extra.
- **Overall score** = 55 % accuracy + 25 % timing + 10 % (1 − extra ratio) + 10 % dynamics. Dynamics weight is redistributed when the score has no accents or ghost notes. The weights are arbitrary first guesses; tune with real takes.
- **Lane identification order** in MusicXML: `midi-unpitched` (GM, 1-based, so minus 1) → `instrument-name` keywords → staff position + notehead.
- **Settings** are saved whenever `cfg != saved_cfg` at the end of a frame. Practice tempo and measure range are session-only.

## Known gaps and ideas, roughly by value

- **V31 note map is unverified.** It is the common Roland V-Drums factory map (kick 36, snare 38/40, x-stick 37, toms 48/50 45/47 43/58 41/39, HH 42/22 closed 46/26 open 44 pedal, crash 49/55 57/52, ride 51/59 bell 53). The V31's documentation wasn't found. MIDI-learn exists in Settings.
- **Hi-hat.** CC#4 pedal position and half-open hi-hat are ignored; open/closed relies on the module sending 46 vs 42.
- **Notation.** Proportional spacing, noteheads and stems only; no beams, flags, rests or tuplet brackets. Fine for timing feedback, not a full engraver.
- **MusicXML coverage.** D.S./D.C./coda jumps are not followed. `score-timewise` is rejected. With several parts, only the first drum-looking part is used.
- **Keyboard test input** is timestamped at frame time (~16 ms jitter in a release build), so it is useful for testing, not for timing judgement. In a debug build the frames are slow enough to make it unreliable: test takes with `cargo run --release`.
- **Follow mode** calls `scroll_to_rect` every frame while recording, so the user can't scroll manually during a take.
- **No persistence of takes.** Session history is lost on exit; CSV export exists. A progress log per score would be the natural next feature.
- **Possible features:** loop the range automatically with a score per pass; tempo ramp (+5 % per clean pass); playback of the score via MIDI out; separate early/late tolerance; drummer-friendly bigger "count-in" overlay.
- **Dependency versions are deliberately old** (eframe 0.29, rodio 0.19, zip 2) because those APIs were known. Upgrading eframe to 0.3x needs at least: `rect_stroke` gained a `StrokeKind`, `Rounding` became `CornerRadius`, `ComboBox::from_label` is unchanged.

## Testing

- `cargo test` covers:
  - the XML parser;
  - staff/name heuristics;
  - repeats with voltas;
  - tempo map round-trips;
  - the sample file import;
  - matching, grading, feedback;
  - a simulated take on the sample;
  - the config round-trip.
- `examples/basic_rock_beat.musicxml` is generated in MuseScore style (4 bars, 100 BPM, `midi-unpitched` instruments, accents, a tom fill). Tests depend on its exact contents. If you edit it, update `musicxml::tests::parses_sample`.
- There are no UI tests. Add real-world exports (MuseScore 4, Guitar Pro 8, Sibelius) to `examples/` as you find edge cases.
