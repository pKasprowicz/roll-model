# Handoff — Roll Model

State of the project at handoff (2026-10-24) and what to do next. Read this before changing code.

## Goal

A Rust desktop app that:

1. loads a MusicXML drum score and shows it on the **Score** tab;
2. captures MIDI from a **Roland V31** drum module and draws the hits as notes directly under the score notes, on the same time axis;
3. compares the captured hits with the score and grades the drummer.

All three are implemented. User-facing documentation is in `README.md`. A fourth part, the **Rudiments** tab (the 40 PAS rudiments practised like a score), is on the branch `feature/rudiments`, see below.

## Status: builds, tests pass, UI smoke-tested; V31 never connected

First real build and smoke test done on 2026-10-24 (Windows 11, `x86_64-pc-windows-gnu`, Rust 1.99.1). `cargo build` (debug and release) and `cargo check --no-default-features` succeed with no warnings and **no source changes were needed for the dependency API mismatches** that the old notes feared (`convex_polygon`, `Slider`, rodio 0.19, zip 2).

**Toolchain:** needs a recent Rust. Rust 1.82 fails because the resolved dependency tree (`icu_*` via `url`/`idna`) uses edition 2024. 1.85 is the first release that can parse edition-2024 crates and the dependencies may need more; only 1.99.1 was tested. `Cargo.lock` now exists and is worth committing: it is the set of versions that built.

| Part | Verified how | Confidence |
|---|---|---|
| `xml.rs`, `drums.rs`, `score.rs`, `musicxml.rs`, `analysis.rs`, `config.rs` | Unit tests (67 in the whole crate, uncommitted work after 0.3.0) | High |
| `app.rs`, `render.rs`, `loader.rs`, `main.rs` | Ran the app: sample loads, both staves line up, take lifecycle (count-in, record, auto-stop), per-note colours, Results and Settings tabs, error banner | Medium-high |
| Keyboard pads (`app.rs::keyboard_pads`) | Scripted take with synthetic key presses: 20 perfect / 12 good / 1 ok / 11 missed / 1 extra as expected | Medium (frame-time stamps, see below) |
| `midi.rs` | Compiles. The test machine had no MIDI ports, so capture and port selection were not exercised | Low |
| `metronome.rs` | Compiles and the thread starts with each take. The click was **not heard**; audio output and timing are unchecked | Low |
| Listen (`playback.rs`, `synth.rs`, `app.rs`) | Unit tests. Run on Windows with the per-application audio meter: the `roll-model` session is silent, starts 0.5 s after `L`, lasts the length of the music, and is silent at once after a stop; the MIDI route into *Microsoft GS Wavetable Synth* also sounds; the "output off" and "MIDI port not found" errors show. **Nobody listened to it**, so how the synthesised kit sounds is unjudged | Medium-high |
| Real V31 timing, note map, latency | Not tested | Unknown |

## Todo for the next session

### Done since 0.3.0 (uncommitted; 67 tests pass; suggest releasing as 0.4.0)

The five items requested after 0.3.0 are implemented, tested, and were checked in the running app (per-application audio meter for sound, screenshots for layout; see Testing).

- **Click on/off while playing** (`K` or the checkbox, Listen and takes). A `control::Control` of atomics is shared with the sound threads (`Metronome` for takes and calibration, `Player` for Listen, the free metronome and auditions); the count-in (times before zero) is never muted. What a cue sounds like is decided by `playback::dispatch` against a `Sink` trait, so the logic is tested with a fake output (`playback.rs` tests), including loops, stops and late cues.
- **Click track**: *notes* and *loop* switches next to the tempo (session-only, `DrumApp.listen_notes/listen_loop`). Loop is read at each pass boundary, `Control::music_done` tells the UI when the last pass ended. A change of range or tempo while listening restarts it after 300 ms (`LISTEN_RESTART_DELAY`), without the count-in. Bar/beat label: `listen_position`.
- **Rolling preview** on the metronome tab (with no score it rolls `Score::click_bar`, the metronome's own bar) (`render::rolling`, `DrumApp::rolling_preview`, config `metro_preview`): the Score tab's range repeats; position = `(elapsed / beat_sec) mod range length`, so it is cyclic like the click loop and lines up only when the metronome's bar length matches the range (the *Match score tempo and bar* button). Not looked at on screen yet; the score's own tempo changes are ignored (the metronome tempo rules).
- **Free metronome tab** (`src/app/metro.rs`, key `M`): tempo, beats per bar, subdivision (`playback::Subdivision`), volume; `playback::metronome_cues` builds one looping bar with accent / beat / soft subdivision (`control::ClickLevel`); one light per beat.
- **Audio output device** (`metronome::open_audio(enabled, device)`, `output_devices()`, config `audio_output`): used by takes, calibration, Listen, auditions and the metronome tab. A missing device falls back to the default with a message; `Metronome` now has an error log like `Player`.
- **Score layout** (`render::LayoutOpts`, `layout()`, `row_starts()`; config `bars_per_row`, `layout_align_bar`; *Layout* button): see the README. Alignment is by the 1-based position in the score as shown, and the popup shows the printed number. A fixed count draws every bar at the same width (`MIN_PPQ` = 28 px per quarter at the narrowest); when it does not fit, `ViewOutput::fit_bars` reports the smaller number and the legend row says so.
- **Sounds**: cymbals rebuilt (`synth::cymbal`, `Cymbal` parameters) as dense inharmonic partial clusters plus shaped noise, with spectral guard tests; instrument mixer (`src/app/mixer.rs`, config `gain.<Lane>` / `mute.<Lane>`, solo is session-only, `Config::effective_gain`); audition button (`DrumApp::audition`, its own `Control` so it never disturbs Listen); WAV override (`synth::load_wav`, `Kit::load`, folder `Config::sounds_dir()` = `sounds` next to `config.txt`, uses the `hound` crate); open hi-hat choked by closed/pedal (`Lane::chokes`, a fade-out in `playback::Voice`).

### Controller (uncommitted, after 0.5.0)

Mackie Control support, see `README.md` ("Controller"). `controller.rs` is pure and unit-tested (parsing, layers, shift, fader pickup, lights); `app/controls.rs` applies the commands to `DrumApp` and is tested through `apply_command`. No device was connected.

- [ ] **Try the X-Touch Mini** (MC mode) and check `presets/x-touch-mini.txt` against the monitor on Settings -> Controller: note numbers of the two button rows, the LAYER A/B buttons (84/85 assumed), the encoder pushes (32-39), the fader (pitch bend ch 1), and whether the button lights (note on, 127/0) and the ring values (cc 48-55, `0x20 | position`) are accepted. Fix the numbers in the preset file, not in the code.
- [ ] Does the Mini in MC mode already switch its own layers (and change what it sends)? The program switches layers itself from the layer buttons' messages; if the device remaps instead, the layer lines are wrong.
- [ ] `presets/x-touch-compact.txt` is from the Mackie Control spec only; the faders are not motorised by the program (a fader takes over when it reaches the value).
- [ ] Not done: motor faders, scribble strip / time display feedback, a second (XT) unit, blinking lights, MIDI-learn for custom presets (the monitor shows the numbers to type).
- The controller's port is looked for every 2 s (`watch_controller`): plugging it in after the start connects it, pulling it out drops it. Not tried with a real unplug.

### Rudiments (branch `feature/rudiments`, after 0.5.0)

The Rudiments tab sits between Score and Metronome. `docs/rudiments.md` is the catalogue and its source (the PAS chart, read off the notation); `src/rudiments.rs` is that catalogue as patterns plus `Rudiment::build(repeats, lead) -> Score`, so Listen, takes, grading, the staff, the Results tab and the controller all work on a rudiment as on a file. `src/app/rudiment.rs` is the tab. 132 tests pass; the tab was looked at in the running app (screenshots of two rudiments), nothing was heard and no V31 was connected.

- [ ] **Check the ⚑ rows of `docs/rudiments.md` against the PAS chart**: the closing note values of rolls 7-15, drag paradiddles #1 and #2, and the flam drag were read from a small engraving and may be off. Only the note values are in doubt (the sticking was read carefully and is tested). The chart is `https://www.coloradomesa.edu/marching-band/documents/rudiments.pdf` (PAS, 1984).
- [ ] **Try it with the V31.** Fast diddles on a mesh pad may double-trigger (extra hits); the adaptive window helps but the pad's retrigger setting matters. Listen to the rudiments (grace notes are 35 ms per step before the main note, `analysis::GRACE_SPACING`).
- [ ] Try the controller on the Rudiments tab (encoder 2 repeats, 3 lead, 8 rudiments, range buttons prev/next) on a real X-Touch Mini.
- [ ] Not done: the multiple bounce (buzz) roll is listen-only; flam/drag graces are optional (a spacing check would need them required); the sticking is shown but never graded (the module cannot hear hands), only compared per hand; orchestration of a rudiment around the kit; user-defined rudiment files (like the controller presets); practice sets / routines; a progress history beyond the best clean tempo.
- [ ] The score view draws no beams or tuplet brackets, so triplet and 32nd-note rudiments rely on proportional spacing and the beat grid.

### Still open

- [ ] **Listen to the new cymbals and the rest of the kit.** Nobody has heard them; only their spectra are tested (centroid, share below 2 kHz, lone low partials). Tune `Cymbal` parameters in `synth.rs` by ear, then adjust the guard bands if needed. The hi-hats are untouched and still very bright (spectral centroid about 13 kHz).
- [ ] **Ask the user** whether per-instrument volume plus WAV files is enough, or whether several synthesised kits (brighter, darker, …) are wanted.
- [ ] Try with a real mouse and wheel: the Layout popup, the mixer's mute/solo/play buttons, the wheel on sliders. Verified only through the keyboard, config keys, and headless egui tests.
- [ ] The WAV override and the hi-hat choke have unit tests but were never heard.
- [ ] The numbering mismatch is still there: bars drawn in the score show their printed number (repeated after unrolling repeats), while the Measures sliders and *start a row at bar* use the position. The Layout popup shows the printed number of the chosen bar; consider showing both on the bars, or making the sliders use printed numbers.
- [ ] Ideas from the first list, not done: tempo ramp (+x BPM every n bars) for Listen and the metronome, accent patterns and tap tempo for the free metronome, remembering the layout per score, a Listen without count-in setting of its own.
- [ ] The click is implemented twice (a sine blip in `metronome::click_audio`, used by takes and by the player); `Metronome` (takes, calibration) and `Player` (Listen, metronome tab) are two threads with separate loops. Merging them would remove the duplication but touches the take timing, which works.
- [ ] Shortcuts (`R`, `L`, `K`, `M`, keyboard pads) are ignored while a widget has keyboard focus (after Tab); `Esc` clears it.

### Name

The application was renamed from "Drum Tutor" to **Roll Model** (crate and binary `roll-model`). The settings folder moved to `RollModel` / `roll-model` with a one-time copy from the old one (`Config::migrate`, tested). The brainstorm is kept in [`docs/name-ideas.md`](docs/name-ideas.md).

- [ ] Check the name (GitHub, web, trademark, domain) before shipping it widely.
- [ ] Rename the local folder `D:\projects\DrumTutor` if wanted (the GitHub repository is already `roll-model`).
- [ ] Release it as a minor version (0.5.0): the changelog entry is under `[Unreleased]`.

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
midi.rs        midir input (callback -> Arc<Mutex<Vec<RawEvent>>>) and output; `connect_controller` for the control surface (raw messages in a queue, lights out)
rudiments.rs   The 40 PAS rudiments as stroke patterns (a tiny pattern language, see its header) and `Rudiment::build` -> `Score`; no GUI or MIDI
controller.rs  Mackie Control surfaces, pure: `parse`, `Preset` (text files in `presets/` and the user's `controllers` folder), `Mapper` (message -> `Command`, layers, shift, fader pickup, lights), `State`
metronome.rs   Click thread for takes and calibration: precise sleep+spin; audio (feature "audio") and/or MIDI; also opens the sound output device (`open_audio`, `output_devices`)
playback.rs    "Listen" and the free metronome: Player thread plays cues to a `Sink` (sound card and/or MIDI out); `dispatch` decides what sounds
control.rs     `Control`: atomics shared with the sound threads (click, notes, loop, volumes, per-instrument gain), `ClickLevel`, `ErrorLog`
synth.rs       Software drum kit: one synthesised mono sample per Lane, WAV override loader (no GUI/MIDI deps)
widgets.rs     UI helpers: `slider()` (caption first, mouse wheel), `roomy()` (bigger Score-tab style)
app/metro.rs   The Metronome tab (free click track)
app/mixer.rs   The instrument mixer on Settings -> Sound
app/rudiment.rs The Rudiments tab: choosing, repeats, lead, tempo per rudiment, best clean tempo, ladder
app/controls.rs The controller: connection, `Command` -> app actions, the Settings -> Controller page
config.rs      key = value settings file, auto-saved when Config changes
```

### Key conventions and invariants

- **Time units.** Score positions are in quarter notes (`q`) from the start of the *unrolled* score. Seconds come only from `TempoMap::q_to_sec(q, scale)`, where `scale` is the practice tempo factor (1.0 = written tempo). `sec_to_q` is its inverse and extrapolates for negative times (count-in).
- **Practice sources.** `DrumApp` has one set of *practice fields* (`score`, `score_path`, `range_from`, `range_to`, `tempo_pct`, `take`, `takes_started`, `history`) and `source` says whose they are: the Score tab's file or the Rudiments tab's rudiment. The other source's fields sit in `parked`; `activate(source)` swaps them (and stops Listen and a take). `sync_source()` runs every frame and when the controller changes tab: the Score tab activates the file, the Rudiments tab the rudiment, the other tabs leave it as it is. `open_file` always activates the Score source. Everything that works on "the score" (`plan_run`, `start_take`, Listen, Results, the controller) therefore works on a rudiment unchanged. `score_gen` counts the scores put in place; Listen's restart signature includes it, so a new rudiment restarts a running Listen.
- **Rudiment scores** are written at `BASE_BPM` = 100 and the practice tempo `tempo_pct` is then the BPM (range 30-240, `rudiment::BPM_RANGE`); the tempo is stored per rudiment in `Config::rudiment_bpm` only when it differs from `Rudiment::default_bpm()`. One bar per repetition, one `ScoreNote` per stroke plus one grace note per flam/drag grace (`grace_order` 1 = nearest the main note), `hand` on every note. `Take.adaptive` makes `analysis::analyze_with` shrink the hit window to the note spacing.
- **Controller context.** `controls.rs::in_context` maps `RangeFrom/RangeTo/RangeBack/RangeForward/Score` to `Repeats/Lead/PrevRudiment/NextRudiment/Rudiment` while the source is the rudiment, and `controller_state` fills the range rings with the repeats and the lead, so the built-in presets need no change.
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
  - Shortcuts (`R`, `L`, `K`, `M`, keyboard pads) are suppressed while egui has a focused widget, see the Listen notes below.
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
- **Hi-hat.** CC#4 is attached to each hit (`RawEvent.hihat`) and `Lane::by_hihat_pedal` turns a 42/46 hit into closed (pedal >= 3/4 of `Config::hihat_cc_max`) or open; never tried with a real module. Half-open is not a separate state, and the pedal polarity (127 = closed) is the Roland convention, unverified on the V31.
- **Notation.** Proportional spacing, noteheads and stems only; no beams, flags, rests or tuplet brackets. Fine for timing feedback, not a full engraver.
- **MusicXML coverage.** D.S./D.C./coda jumps are not followed. `score-timewise` is rejected. With several parts, only the first drum-looking part is used.
- **Keyboard test input** is timestamped at frame time (~16 ms jitter in a release build), so it is useful for testing, not for timing judgement. In a debug build the frames are slow enough to make it unreliable: test takes with `cargo run --release`.
- **Follow mode** calls `scroll_to_rect` every frame while recording, so the user can't scroll manually during a take.
- **No persistence of takes.** Session history is lost on exit; CSV export exists. A progress log per score would be the natural next feature.
- **Mouse-wheel tuning was not tried with a physical wheel.** The logic is covered by headless egui tests that feed `Event::MouseWheel` (including that a `ScrollArea` around the slider does not scroll), which is the event egui-winit produces. The window itself can't be driven by posted mouse messages (they never reach egui), so end-to-end mouse checks need real input.
- **Recent scores** are not checked for existence until they are drawn (a missing file is greyed out); the list is not trimmed when files disappear.
- **Listen gaps.**
  - The built-in kit is deliberately simple (synthesised, not sampled); it is for hearing rhythm and instrument, not for sounding like a studio kit. Unheard so far, see the todo list; users can replace any instrument with a WAV file.
  - The playhead follows the clock, so it runs ahead of the sound by the output device's buffer.
  - Notes already graded in the last take stay coloured while listening.
  - Playback uses the take's count-in setting; there is no separate "listen without count-in".
  - Restarting after a change of range or tempo opens the sound device again, so there is half a second of silence after the change settles.
  - The click is played by two threads with two loops (`Metronome` for takes, `Player` for the rest).
- **Possible features:** automatic loop with a score per pass for takes; tempo ramp (+5 % per clean pass); separate early/late tolerance; drummer-friendly bigger "count-in" overlay; sampled drum kits.
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
  - the recent-files list (order, duplicates, limit, round-trip) and the path shortening used by the welcome panel;
  - the playback core against a fake output (`playback.rs`): the click switch (count-in unaffected), notes off, per-instrument gain and mute, output routing, passes and looping, stop, late cues; the metronome bar (`metronome_cues`) and `Control`;
  - the score layout (`render.rs`): fixed bars per row, the alignment bar, zoom independence, mixed bar lengths, too narrow a window, degenerate scores, every bar in exactly one row;
  - the synthesised kit (`synth.rs`): bounded, decaying, deterministic samples, the spectral guard for the cymbals, the WAV loader (bit depths, channels, resampling, bad files) and the kit's fallback to the built-in sound;
  - mixer settings (`Config::effective_gain`, clamping and defaults of the new keys) and the listing of sound outputs.
- Listening has no automated test of the real outputs. To check them on Windows, play the sample with `cargo run --release` and read the per-application peak meter of the default output device (Core Audio `IAudioSessionManager2` → `IAudioMeterInformation`, e.g. from PowerShell with C# COM interop): the `roll-model` session must stay silent until about 0.5 s after pressing `L` and be silent again after stopping. The device-wide meter also hears every other application, so use the per-application one.
- `examples/basic_rock_beat.musicxml` is generated in MuseScore style (4 bars, 100 BPM, `midi-unpitched` instruments, accents, a tom fill). Tests depend on its exact contents. If you edit it, update `musicxml::tests::parses_sample`.
- There are no UI tests. Add real-world exports (MuseScore 4, Guitar Pro 8, Sibelius) to `examples/` as you find edge cases.
