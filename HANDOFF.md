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
| `xml.rs`, `drums.rs`, `score.rs`, `musicxml.rs`, `analysis.rs`, `config.rs` | Unit tests (31 in the whole crate at 0.3.0) | High |
| `app.rs`, `render.rs`, `loader.rs`, `main.rs` | Ran the app: sample loads, both staves line up, take lifecycle (count-in, record, auto-stop), per-note colours, Results and Settings tabs, error banner | Medium-high |
| Keyboard pads (`app.rs::keyboard_pads`) | Scripted take with synthetic key presses: 20 perfect / 12 good / 1 ok / 11 missed / 1 extra as expected | Medium (frame-time stamps, see below) |
| `midi.rs` | Compiles. The test machine had no MIDI ports, so capture and port selection were not exercised | Low |
| `metronome.rs` | Compiles and the thread starts with each take. The click was **not heard**; audio output and timing are unchecked | Low |
| Listen (`playback.rs`, `synth.rs`, `app.rs`) | Unit tests. Run on Windows with the per-application audio meter: the `drum-tutor` session is silent, starts 0.5 s after `L`, lasts the length of the music, and is silent at once after a stop; the MIDI route into *Microsoft GS Wavetable Synth* also sounds; the "output off" and "MIDI port not found" errors show. **Nobody listened to it**, so how the synthesised kit sounds is unjudged | Medium-high |
| Real V31 timing, note map, latency | Not tested | Unknown |

## Todo for the next session

Requested by the user after 0.3.0. Items 1 and 4 share a design, so do them together; items 3 and 5 are independent (5 only touches `render.rs`, the Score tab and the config); item 2 needs a pair of ears, so ask the user to listen. Still open from earlier: **First tasks, item 4** below (needs the V31), and trying the mouse wheel on sliders with a physical wheel.

**Shared design note (items 1, 2 and 4).** Playback today is a fixed list of `Cue`s handed to the `Player` thread at start (`playback::Player::start`), with every option copied into `PlayOptions`. Nothing can change while it runs. In particular the *click* checkbox and the tempo/count-in sliders on the Score tab stay enabled during Listen but only take effect on the next start, which looks like a bug to the user. A small shared control block (an `Arc` of atomics: notes on/off, click on/off, volumes, per-instrument gain/mute) read by the thread for every cue would cover items 1, 2 and 4 without restarting. Consider also putting the output behind a small trait (`drum(lane, velocity)`, `click(accent)`), so the gating can be unit-tested with a fake instead of a sound card: the loop in `playback::run` is the part with no automated test today.

### 1. Switch the metronome off and on during playback

- [ ] Unticking *click* while Listen plays silences the click from the next beat on, and re-ticking brings it back on the next beat. No restart, no gap in the music.
- [ ] Decide for takes: `metronome.rs` has the same limitation (fixed `clicks` list and `ClickOptions`) and the checkbox is disabled while recording. Probably make it live there too (drummers drop the click once they are comfortable); otherwise document that it is fixed per take.
- [ ] Decide whether muting also mutes the count-in. Suggestion: no, the count-in is the start signal.
- [ ] Make the click volume and the Listen volume live through the same mechanism.
- [ ] Test the gating with a fake output, and check the real thing with the per-application audio meter (see Testing).

### 2. Make the playback sounds adjustable, and fix the cymbals

- [ ] **Cymbals sound odd by default** (reported by the user): crash 1/2, china, splash and ride (`synth.rs::cymbal`, the `Lane::Ride` and `Lane::RideBell` arms). Guess: they are built from sine partials between 410 Hz and about 1.8 kHz, which rings like a gong or bell. Real cymbals are a dense, inharmonic wash mostly above 2 kHz with a short stick transient. Try many more high partials or ring-modulated pairs (3–12 kHz), band-passed noise, a slower decay for the wash. Keep each sample at 2 s or less (the player's `TAIL` is 2.2 s). Add objective guards in the `synth.rs` tests (spectral centroid above some kHz, no strong tonal peak under 2 kHz) so a regression to a bell fails; the final judgement is by ear.
- [ ] **Audition buttons**: on Settings → Sound, one button per instrument that plays it without a score. Needed to tune sounds by ear.
- [ ] **Per-instrument volume and mute/solo** (for example hear only kick and snare, or mute the hi-hat). Persist in the config (one key per lane, like `map.<Lane>`), apply live through the control block.
- [ ] **User sounds**: an optional folder of WAV files named after the instruments (`Lane::name()`: `Kick.wav`, `HiHatClosed.wav`, …), falling back to the synthesised sample for any lane without a file. rodio is built with `default-features = false`, so this needs its `wav` feature (hound) or a direct `hound` dependency. Convert to mono at `synth::SAMPLE_RATE`, normalise, cap the length. Decide where the folder lives (suggestion: next to `config.txt`, `%APPDATA%\DrumTutor\sounds\`) and how it is reloaded.
- [ ] Ask the user whether per-instrument gain plus WAV override is enough, or whether several synthesised kits (brighter, darker, …) are wanted.
- [ ] While in `synth.rs`/`playback.rs`: an open hi-hat is not choked by the next closed or pedal hit (known gap), so cut the open voice when one arrives.

### 3. Audio output setting

- [ ] Let the user choose the sound output device used for everything that goes through the sound card: the click, Listen and the calibration clicks. `metronome::open_audio` calls `rodio::OutputStream::try_default()` today. List devices through `rodio::cpal` (the whole crate is re-exported, so no new dependency: `cpal::traits::HostTrait::output_devices`, `DeviceTrait::name`) and open one with `OutputStream::try_from_device` (rodio 0.19).
- [ ] UI on Settings → Sound: an "Output device" combo box ("System default" first) with a refresh button, like the MIDI ports. Store the device name in the config (`audio_output = …`, empty = default).
- [ ] If the saved device is missing or cannot be opened, fall back to the default and say so in the error bar. `Player::take_errors` exists; `Metronome` has no error channel yet and needs one.
- [ ] Edge cases: devices with the same name, devices that disappear while the app runs, sample-rate mismatch (rodio resamples), Bluetooth latency (mention it in the Latency section of the README; calibration then matters). Consider showing the chosen device in the status line.
- [ ] Keep `--no-default-features` building; without the `audio` feature the setting is hidden.
- [ ] Optional: an output buffer size choice, since it changes latency and therefore calibration.

### 4. Metronome only: a click track

- [ ] A way to play just the click. Suggestion: a *Notes* toggle next to *Click* that stays live during Listen, so click only = notes off (nearly free once item 1's control block exists). A separate button is the alternative.
- [ ] A click track should not end after the selected range. Add a *loop* option (repeat the selected measures until stopped). `Player` plays a finite list and then stops, so generate the cues one pass at a time or reschedule at the end of each pass.
- [ ] Use the score's tempo map, time-signature changes, accented first beat and count-in; `Score::click_track` already builds that.
- [ ] A free metronome that needs no score (also from the welcome panel): BPM, beats per bar, optionally subdivisions (8ths, 16ths) and an accent pattern.
- [ ] Show the beat while it runs (a bar/beat counter or a flashing indicator), and let the tempo change while running; restarting at the new tempo is acceptable as a first version.
- [ ] Later: a tempo ramp (+x BPM every n bars), already in the ideas list.

### 5. Configurable score layout: bars per row, and an alignment bar

Today `render.rs::layout(score, width, zoom)` fills each row greedily with as many bars as fit the window width at the current zoom (`ppq = 56 * zoom`), then stretches the row to the full width (a last row shorter than 75 % of the width keeps its natural width). So the number of bars per row changes with the window size and the zoom, and the user cannot choose where a row breaks. Zoom only scales the horizontal spacing; the staff height does not change.

- [ ] **Bars per row**: *Auto* (today's behaviour) or a fixed number (1 to 8, perhaps more). With a fixed number every row has exactly that many bars and fills the row width. Give the shorter last row the same bar width as the rows above it, so barlines line up vertically from row to row.
- [ ] **Align from bar**: the bar at which a row must start. Rows then start at that bar and every N bars after it, and the bars before it fill the earlier rows backwards, so the first row may be shorter. Example with N = 4 and align at 2: `[1] [2–5] [6–9] …`, which is what a pickup bar or a section that starts mid-row needs. Off by default. Define what it does in *Auto* mode too (suggestion: the alignment bar always starts a row, and the greedy fill continues from there).
- [ ] **Decide the numbering.** `Measure.number` is the printed number from the file: a string (a pickup is often "0", others can be non-numeric) and it **repeats once repeats are expanded**, because `musicxml.rs` copies `rm.number` into every unrolled bar. The Measures sliders and `range_from`/`range_to`, on the other hand, use the 1-based position in the unrolled score, so the two can already disagree. Recommendation: align by the 1-based position, like the sliders, and show the printed number next to it (hover text or label). Either way, say in the README which one the setting means, and consider fixing the existing mismatch at the same time.
- [ ] **A fixed count may not fit** the width at the current zoom. Decide: with a fixed count the bars simply fill the row and zoom no longer matters (simplest), with a minimum bar width below which the app falls back to fewer bars and says so.
- [ ] **UI**: next to *zoom* on the Score tab, a small "Layout" popup with bars per row (Auto, 1…8) and the alignment bar, built from `widgets::slider` (wheel-aware, caption first). Persist it in the config, for example `bars_per_row` (0 = Auto) and `layout_align_bar` (0 = off). Later, optionally remember it per score (keyed by file path), since phrasing differs between pieces.
- [ ] **Plumbing**: new fields on `render::ScoreView` and parameters of `layout()`; the layout is recomputed every frame, so nothing needs caching. Click-to-select, the range highlight and the follow-scroll all read `Layout::systems` and `Layout::measures`, so they should keep working unchanged.
- [ ] **Tests**: `layout()` is a pure function with no tests today. Cover a fixed count, the alignment bar (N = 1, align at bar 1, align beyond the last bar, a score shorter than one row), bars of different lengths (3/4 among 4/4) and the width of the last row.

## First tasks

1. ~~Build.~~ Done, see above.
2. ~~Run the tests.~~ Done. Splitting into `lib.rs` + `main.rs` would **not** by itself avoid building the GUI/MIDI crates, because they are dependencies of the same package. It would only pay off with a workspace or optional dependencies; skipped.
3. ~~Smoke-test the UI.~~ Done. Fixes made along the way:
   - egui 0.29's default fonts have no `●` (U+25CF), `✕` (U+2715) or `→` (U+2192) and draw a box instead. Replaced with `⏺`/`⏹` (from the bundled emoji icon font), a "Dismiss" button and plain text. `■ · … ± − – — •` and `📂 🎼 📊 ⚙ ⟳ 💾 ⏺ ⏹` render fine. `⚠` (U+26A0) was not seen on screen. Check any new glyph in the running app before using it.
   - The two control rows in the Score tab are plain `horizontal` rows now. Sliders do not wrap inside `horizontal_wrapped`, so the old single row ran off the right edge and made the score layout wider than the window.
   - Load errors repeated the file path; the loader no longer adds it.
4. **Test with the V31** (only the user can do this). Confirm the note numbers in the monitor on **Settings → MIDI input** match the default map in `drums.rs::NoteMap::roland_v31()`. Also check that the metronome click is audible and that **Settings → Timing → Calibrate** gives a sensible latency offset.

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
playback.rs    "Listen": Player thread plays drum cues + click to the sound card and/or MIDI out
synth.rs       Software drum kit: one synthesised mono sample per Lane (no GUI/MIDI deps)
widgets.rs     UI helpers: `slider()` (caption first, mouse wheel), `roomy()` (bigger Score-tab style)
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
- **UI conventions** (`widgets.rs`, `app.rs`):
  - Every slider goes through `widgets::slider(ui, caption, &mut value, range, suffix, step)`: the caption comes first, the wheel moves the value by `step` per notch, and the wheel is consumed so a surrounding `ScrollArea` stays put. Do not add a bare `egui::Slider`. Pass `""` as the caption when a `Grid` cell already holds the label.
  - `ui.horizontal` follows its parent's direction, so a `right_to_left` parent reverses it; do not put captioned sliders inside one. `ui.with_layout(left_to_right(Center), …)` is not a substitute (it centres in the whole available height).
  - The controls above the score are wrapped in `ui.scope` + `widgets::roomy` (scoped style, 1.25x text, 30 px rows, 150 px sliders). Settings use the default size, except the page tabs.
  - Settings pages are `SettingsPage` (session-only state, each page has its own scroll position). A new setting goes on the page it belongs to and in `config.rs` (parse, write, default, round-trip test).
  - Recent scores live in `Config::recent_files` (absolute paths, newest first, at most `MAX_RECENT`), written as repeated `recent = …` lines. `open_file` records a score only after it loaded.
  - Shortcuts (`R`, `L`, keyboard pads) are suppressed while egui has a focused widget, see the Listen notes below.
- **Listen** (`app.rs::start_listen`, `playback.rs`):
  - Takes and listening share `DrumApp::plan_run()` (range, tempo, zero = now + 0.5 s lead + count-in, click track from `Score::click_track`), so what is heard is what a take asks for.
  - The notes are `analysis::expected_notes` mapped to `playback::Cue`s; velocity comes from `playback::velocity_for` (ghost 45, grace 70, normal 90, accent 120).
  - One `Player` thread owns the audio stream and the MIDI connection, so the click and the drums can use the same MIDI port (Windows ports are exclusive). It reuses `metronome::{at, wait_until}`; cues later than 250 ms are dropped, not played as a burst.
  - `DrumApp.listen` exists only while the playhead runs; `DrumApp.player` may outlive it by up to 2.2 s so the last cymbal rings out. `stop_listen()` joins the thread and cuts the sound. Starting a take, calibration or opening a file stops playback.
  - The thread reports problems (no sound device, MIDI port missing) through `Player::take_errors()`; `update_listen` shows the first one in the error bar.
  - MIDI notes come from `NoteMap::playback_note`: the standard GM/Roland note if the user's map gives it to the lane, else the lowest mapped note, else the standard note if no other lane claims it. The note-off is sent right after the note-on, as for the click.
  - Keyboard shortcuts (`R`, `L`, the keyboard pads) are ignored while egui has a focused widget (`ctx.wants_keyboard_input()`), which includes a button focused with Tab. Mouse use never focuses buttons, so this only shows with keyboard navigation (and when scripting the UI with posted key messages).

## Known gaps and ideas, roughly by value

- **V31 note map is unverified.** It is the common Roland V-Drums factory map (kick 36, snare 38/40, x-stick 37, toms 48/50 45/47 43/58 41/39, HH 42/22 closed 46/26 open 44 pedal, crash 49/55 57/52, ride 51/59 bell 53). The V31's documentation wasn't found. MIDI-learn exists in Settings.
- **Hi-hat.** CC#4 pedal position and half-open hi-hat are ignored; open/closed relies on the module sending 46 vs 42.
- **Notation.** Proportional spacing, noteheads and stems only; no beams, flags, rests or tuplet brackets. Fine for timing feedback, not a full engraver.
- **MusicXML coverage.** D.S./D.C./coda jumps are not followed. `score-timewise` is rejected. With several parts, only the first drum-looking part is used.
- **Keyboard test input** is timestamped at frame time (~16 ms jitter in a release build), so it is useful for testing, not for timing judgement. In a debug build the frames are slow enough to make it unreliable: test takes with `cargo run --release`.
- **Follow mode** calls `scroll_to_rect` every frame while recording, so the user can't scroll manually during a take.
- **No persistence of takes.** Session history is lost on exit; CSV export exists. A progress log per score would be the natural next feature.
- **Mouse-wheel tuning was not tried with a physical wheel.** The logic is covered by headless egui tests that feed `Event::MouseWheel` (including that a `ScrollArea` around the slider does not scroll), which is the event egui-winit produces. The window itself can't be driven by posted mouse messages (they never reach egui), so end-to-end mouse checks need real input.
- **Recent scores** are not checked for existence until they are drawn (a missing file is greyed out); the list is not trimmed when files disappear.
- **Listen gaps.**
  - The built-in kit is deliberately simple (synthesised, not sampled); it is for hearing rhythm and instrument, not for sounding like a studio kit. An open hi-hat is not choked by the next closed or pedal hit.
  - The playhead follows the clock, so it runs ahead of the sound by the output device's buffer.
  - Notes already graded in the last take stay coloured while listening.
  - Playback uses the take's count-in and click settings; there is no separate "listen without count-in".
  - The click still goes through `metronome.rs` for takes but through `playback.rs` for listening, so click behaviour is implemented twice (a sine blip and a MIDI note).
- **Possible features:** loop the range automatically with a score per pass; tempo ramp (+5 % per clean pass); loop Listen; separate early/late tolerance; drummer-friendly bigger "count-in" overlay; sampled drum sounds.
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
  - the config round-trip and the playback settings defaults;
  - the click track (`Score::click_track`) and the MIDI note choice for playback (`NoteMap::playback_note`);
  - the drum kit (`synth.rs`: every lane renders a finite, bounded, decaying, deterministic sample) and the cue list;
  - the sample score mixed offline the way the player does it: a note is audible at every onset and the mix does not clip;
  - the slider helper in a headless egui context (`widgets.rs`): wheel steps, clamping, integer and fractional ranges, disabled sliders, and that the page behind a slider does not scroll;
  - the recent-files list (order, duplicates, limit, round-trip) and the path shortening used by the welcome panel.
- Listening has no automated test of the real outputs. To check them on Windows, play the sample with `cargo run --release` and read the per-application peak meter of the default output device (Core Audio `IAudioSessionManager2` → `IAudioMeterInformation`, e.g. from PowerShell with C# COM interop): the `drum-tutor` session must stay silent until about 0.5 s after pressing `L` and be silent again after stopping. The device-wide meter also hears every other application, so use the per-application one.
- `examples/basic_rock_beat.musicxml` is generated in MuseScore style (4 bars, 100 BPM, `midi-unpitched` instruments, accents, a tom fill). Tests depend on its exact contents. If you edit it, update `musicxml::tests::parses_sample`.
- There are no UI tests. Add real-world exports (MuseScore 4, Guitar Pro 8, Sibelius) to `examples/` as you find edge cases.
