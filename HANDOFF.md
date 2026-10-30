# Handoff — Roll Model

> **Made with AI.** This project was created with extensive use of AI (Anthropic's Claude), directed and reviewed by the author. Check anything that matters.

State of the project at handoff (2026-10-24) and what to do next. Read this before changing code.

## Goal

A Rust desktop app that:

1. loads a MusicXML drum score and shows it on the **Score** tab;
2. captures MIDI from a **Roland V31** drum module and draws the hits as notes directly under the score notes, on the same time axis;
3. compares the captured hits with the score and grades the drummer.

All three are implemented. User-facing documentation is in `README.md`. A fourth part, the **Rudiments** tab (the 40 PAS rudiments practised like a score), is released too (0.6.0). Since 0.6.0 (now 0.7.0): the start-up splash, the start trigger, controller help pictures, generic MIDI controllers and the Manage window; see `CHANGELOG.md`.

## Status: builds, tests pass, UI smoke-tested; V31 never connected

First real build and smoke test done on 2026-10-24 (Windows 11, `x86_64-pc-windows-gnu`, Rust 1.99.1). `cargo build` (debug and release) and `cargo check --no-default-features` succeed with no warnings and **no source changes were needed for the dependency API mismatches** that the old notes feared (`convex_polygon`, `Slider`, rodio 0.19, zip 2).

**Toolchain:** needs a recent Rust. Rust 1.82 fails because the resolved dependency tree (`icu_*` via `url`/`idna`) uses edition 2024. 1.85 is the first release that can parse edition-2024 crates and the dependencies may need more; only 1.99.1 was tested. `Cargo.lock` now exists and is worth committing: it is the set of versions that built.

| Part | Verified how | Confidence |
|---|---|---|
| `xml.rs`, `drums.rs`, `score.rs`, `musicxml.rs`, `analysis.rs`, `config.rs` | Unit tests (201 in the whole crate at 0.7.0) | High |
| `app.rs`, `render.rs`, `loader.rs`, `main.rs` | Ran the app: sample loads, both staves line up, take lifecycle (count-in, record, auto-stop), per-note colours, Results and Settings tabs, error banner | Medium-high |
| Keyboard pads (`app.rs::keyboard_pads`) | Scripted take with synthetic key presses: 20 perfect / 12 good / 1 ok / 11 missed / 1 extra as expected | Medium (frame-time stamps, see below) |
| `midi.rs` | Compiles. The test machine had no MIDI ports, so capture and port selection were not exercised | Low |
| `metronome.rs` | Compiles and the thread starts with each take. The click was **not heard**; audio output and timing are unchecked | Low |
| Listen (`playback.rs`, `synth.rs`, `app.rs`) | Unit tests. Run on Windows with the per-application audio meter: the `roll-model` session is silent, starts 0.5 s after `L`, lasts the length of the music, and is silent at once after a stop; the MIDI route into *Microsoft GS Wavetable Synth* also sounds; the "output off" and "MIDI port not found" errors show. **Nobody listened to it**, so how the synthesised kit sounds is unjudged | Medium-high |
| Real V31 timing, note map, latency | Not tested | Unknown |

## Todo for the next session

### Task index

The hardware tasks (HW-*) have a step-by-step plan in `docs/hardware-test-plan.md`, with a results sheet. Every open task has a unique ID; refer to tasks by it. IDs are never reused: a finished task is ticked and keeps its ID, a dropped one is struck through. **Needs**: *device* = a real V31 or controller, *ears* = listening, *decision* = the owner has to choose, *code* = can be done at the desk.

| ID | Task | Needs |
|---|---|---|
| **HW-1** | Try the X-Touch Mini against its preset | device |
| **HW-2** | Does the Mini in MC mode switch its own layers? | device |
| **HW-3** | Check the X-Touch Compact preset | device |
| **HW-4** | Compare the drawn pictures with the real devices | device |
| **HW-5** | Find what the Compact's side encoders and LAYER buttons send | device |
| **HW-6** | Check the four generic presets on real devices | device |
| **HW-7** | Try Learn and the Manage window with a real controller and mouse | device |
| **HW-8** | Try unplugging and plugging in the controller | device |
| **HW-9** | Try the controller on the Rudiments tab | device |
| **HW-10** | Try the rudiments with the V31 | device, ears |
| **HW-11** | Time the start-up with the devices attached | device |
| **CTL-1** | Create and delete layers in the Manage window | code |
| **CTL-2** | Edit lights and LED rings in the Manage window | code |
| **CTL-3** | Guess a relative encoder's encoding while learning | code |
| **CTL-4** | `init` system-exclusive messages for controllers | code |
| **CTL-5** | Motor faders | code, device |
| **CTL-6** | Scribble strip and time display feedback | code, device |
| **CTL-7** | A second (XT) unit | code, device |
| **CTL-8** | Blinking lights | code |
| **RUD-1** | Check the flagged rows of `docs/rudiments.md` against the PAS chart | decision |
| **RUD-2** | Make the buzz roll recordable | code |
| **RUD-3** | Required flam and drag graces with a spacing check | code |
| **RUD-4** | Grade the sticking | code, device |
| **RUD-5** | Orchestrate a rudiment around the kit | code |
| **RUD-6** | User-defined rudiment files | code |
| **RUD-7** | Practice sets and routines | code |
| **RUD-8** | A progress history on disk | code |
| **SND-1** | Listen to the cymbals and the rest of the kit | ears |
| **SND-2** | Ask whether volume plus WAV files is enough or several kits are wanted | decision |
| **SND-3** | Listen to the WAV override and the hi-hat choke | ears |
| **UI-1** | Try with a real mouse and wheel | device |
| **UI-2** | The bar numbering mismatch (printed number against position) | decision, code |
| **UI-3** | Shortcuts are ignored while a widget has the focus | code |
| **UI-4** | Notation: beams, flags, rests and tuplet brackets | code |
| **FEAT-1** | Tempo ramp for Listen and the metronome | code |
| **FEAT-2** | Accent patterns and tap tempo for the free metronome | code |
| **FEAT-3** | Remember the layout per score | code |
| **FEAT-4** | A Listen without count-in setting of its own | code |
| **NAME-1** | Check the name (GitHub, web, trademark, domain) | decision |
| **NAME-2** | Rename the local folder `D:\projects\DrumTutor` | decision |
| **CD-1** | Split `app.rs` and `DrumApp` | code |
| **CD-2** | Split `controller.rs` into a folder of modules | code |
| **CD-3** | One table for actions (shotgun surgery) | code |
| **CD-4** | Table-driven settings keys | code |
| **CD-5** | Merge the two timing threads | code, device |
| **CD-6** | Split `render::show` | code |
| **CD-7** | Value types in the controller domain | code |
| **CD-8** | One identity for a preset | code |
| **CD-9** | One place that chooses a preset | code |
| **CD-10** | Typed errors instead of `String` | code |
| **CD-11** | Long parameter lists | code |
| **CD-12** | Encapsulate the controller and editor state | code |
| **CD-13** | MIDI I/O off the UI thread | code, device |
| **CD-14** | Per-frame work in the UI | code |
| **CD-15** | Inject the settings folders | code |
| **CD-16** | Constructors and constants in `controller_layout.rs` | code |
| **CD-17** | Simplify `Preset::line` and `Preset::position` | code |
| **CD-18** | Split `handle_event` and `update` into named steps | code |
| **CD-19** | Separate the editor's view state from its learn state | code |
| **CD-20** | Split the pure core into a library crate | code |
| **CD-26** | Rest of CD-7: layer newtype, one channel for every control, one binding type | code |
| **CD-27** | Rest of CD-1: group the fields of `DrumApp` into sub-structs | code |
| **CD-28** | Split `app/controls.rs` (1,198 lines) | code |
| **CD-29** | Drawing helpers as methods of `Canvas` | code |
| **CD-21** to **CD-25** | Polish suggestions (see the review) | code |

### Done in 0.4.0 (released; the list of that time)

The five items requested after 0.3.0 are implemented, tested, and were checked in the running app (per-application audio meter for sound, screenshots for layout; see Testing).

- **Click on/off while playing** (`K` or the checkbox, Listen and takes). A `control::Control` of atomics is shared with the sound threads (`Metronome` for takes and calibration, `Player` for Listen, the free metronome and auditions); the count-in (times before zero) is never muted. What a cue sounds like is decided by `playback::dispatch` against a `Sink` trait, so the logic is tested with a fake output (`playback.rs` tests), including loops, stops and late cues.
- **Click track**: *notes* and *loop* switches next to the tempo (session-only, `DrumApp.listen_notes/listen_loop`). Loop is read at each pass boundary, `Control::music_done` tells the UI when the last pass ended. A change of range or tempo while listening restarts it after 300 ms (`LISTEN_RESTART_DELAY`), without the count-in. Bar/beat label: `listen_position`.
- **Rolling preview** on the metronome tab (with no score it rolls `Score::click_bar`, the metronome's own bar) (`render::rolling`, `DrumApp::rolling_preview`, config `metro_preview`): the Score tab's range repeats; position = `(elapsed / beat_sec) mod range length`, so it is cyclic like the click loop and lines up only when the metronome's bar length matches the range (the *Match score tempo and bar* button). Not looked at on screen yet; the score's own tempo changes are ignored (the metronome tempo rules).
- **Free metronome tab** (`src/app/metro.rs`, key `M`): tempo, beats per bar, subdivision (`playback::Subdivision`), volume; `playback::metronome_cues` builds one looping bar with accent / beat / soft subdivision (`control::ClickLevel`); one light per beat.
- **Audio output device** (`metronome::open_audio(enabled, device)`, `output_devices()`, config `audio_output`): used by takes, calibration, Listen, auditions and the metronome tab. A missing device falls back to the default with a message; `Metronome` now has an error log like `Player`.
- **Score layout** (`render::LayoutOpts`, `layout()`, `row_starts()`; config `bars_per_row`, `layout_align_bar`; *Layout* button): see the README. Alignment is by the 1-based position in the score as shown, and the popup shows the printed number. A fixed count draws every bar at the same width (`MIN_PPQ` = 28 px per quarter at the narrowest); when it does not fit, `ViewOutput::fit_bars` reports the smaller number and the legend row says so.
- **Sounds**: cymbals rebuilt (`synth::cymbal`, `Cymbal` parameters) as dense inharmonic partial clusters plus shaped noise, with spectral guard tests; instrument mixer (`src/app/mixer.rs`, config `gain.<Lane>` / `mute.<Lane>`, solo is session-only, `Config::effective_gain`); audition button (`DrumApp::audition`, its own `Control` so it never disturbs Listen); WAV override (`synth::load_wav`, `Kit::load`, folder `Config::sounds_dir()` = `sounds` next to `config.txt`, uses the `hound` crate); open hi-hat choked by closed/pedal (`Lane::chokes`, a fade-out in `playback::Voice`).

### Controller (since 0.6.0)

Mackie Control support, see `README.md` ("Controller"). `controller.rs` is pure and unit-tested (parsing, layers, shift, fader pickup, lights); `app/controls.rs` applies the commands to `DrumApp` and is tested through `apply_command`. No device was connected.

- [ ] **HW-1** **Try the X-Touch Mini** (MC mode) and check `presets/x-touch-mini.txt` against the monitor on Settings -> Controller: note numbers of the two button rows, the LAYER A/B buttons (84/85 assumed), the encoder pushes (32-39), the fader (pitch bend ch 1), and whether the button lights (note on, 127/0) and the ring values (cc 48-55, `0x20 | position`) are accepted. Fix the numbers in the preset file, not in the code.
- [ ] **HW-2** Does the Mini in MC mode already switch its own layers (and change what it sends)? The program switches layers itself from the layer buttons' messages; if the device remaps instead, the layer lines are wrong.
- [ ] **HW-3** `presets/x-touch-compact.txt` is from the Mackie Control spec only; the faders are not motorised by the program (a fader takes over when it reaches the value).
- The help screen's picture of the Mini (`controller_layout::x_touch_mini`) follows a photograph (positions measured, the height stretched 2.1x against 1.7x in width to make room for labels; the printed symbols are `Layout::marks`). Layer A's bottom row does what is printed beside the buttons (rewind, fast forward, LOOP, stop, play, record; the first two, MC and a blank one, are Loop worst bar and Discard take), a test keeps the picture's printed marks and the bindings in step; Shift and the tempo buttons moved to the top row. Whether the Mini's bottom row really sends notes 87, 88, 91, 92, 86, 93, 94, 95 in order is unverified. The Compact's (`x_touch_compact`) follows a photograph of its front, but the printed labels were too small to read: the order of the six transport buttons on the right (Rewind, Fast forward / Stop, Play / Record, Cycle) is a guess, and the message of the eight side encoders, the two LAYER buttons and the button under the ninth fader is unknown (they are drawn and unbound). The Compact preset's function buttons (Shift, Click, Notes, Metronome, Tap tempo, Worst bar, Discard take, Next tab) are on the SELECT row, notes 24-31, the standard Mackie numbers and unverified; the earlier numbers (84, 85, 87, ...) belonged to buttons the Compact lacks. A test (`the_built_in_presets_bind_nothing_the_pictures_lack`) fails when a preset binds a message no control of its picture sends. Another controller gets a picture by adding a `Layout` and picking it in `for_preset`.
- [ ] **HW-4** Compare the drawn pictures with the real devices (the Mini: row order, LAYER buttons, fader; the Compact: the order of the six transport buttons on the right, the SELECT, transport and strip numbers) and fix the numbers in `controller_layout.rs` and the presets.
- [ ] **HW-5** Find what the Compact's eight side encoders, its two LAYER buttons and the button under the ninth fader send (the monitor shows it); they are drawn and unbound, free for more actions.
- [ ] **HW-6** **The four generic presets** (`presets/novation-launch-control-xl.txt`, `korg-nanokontrol2.txt`, `akai-mpk-mini.txt`, `arturia-beatstep.txt`) are from memory: only LCXL knob 1 = cc 13, BeatStep knobs 1-2 = cc 10 / 74 and nanoKONTROL2 record = cc 64 were confirmed by a web search; the MPK Mini binds two candidate knob ranges and guessed pad notes. Check each with the monitor on a real device. They have no LED feedback or pictures; see `docs/generic-midi-controller-concept.md` for the planned learn UI, channel filter and relative encodings.
- Generic MIDI controllers (built, see the concept): phase 1 of `docs/generic-midi-controller-concept.md` is done (`Binding.channel`, `Encoding`, `Light.on/off/channel/scale`, all in `controller.rs` and unit-tested). Phase 2 is done too: `Preset::to_text`, `assign`, `clear`, `set_kind` and `controller::learn` (pure), and the editor `app/controls/editor.rs` (learn flow in `update_controller` via `learn_message`; tested against a temporary folder through `EditorState.dir`; the learning against a real device and the table with the mouse were not tried). The Manage window (`controller_editor_window` → `manage_ui`: `preset_list` on the left with Add / Copy / Delete, `preset_header` (rename) and `assignments_body` on the right; clicking a preset chooses it as the one in use, `select_preset`, because learning needs its connection; opened by the Manage button next to the selector) is an immediate viewport; inside the main window where the platform has no second window, and presets carry `kind` (`PresetKind`, inferred for files without one). Phase 3 is done: `controller_layout::generated` draws a picture from any preset without a drawn layout (`Layout.generated`, nine controls to a row). What is not done is listed as CTL-1 to CTL-4 and HW-7. A copy of a preset made before this keeps working; the layout pictures ignore channels (`Preset::position(.., None)`).
- [ ] **HW-7** Try Learn and the Manage window with a real controller (Mackie and a generic one) and with a real mouse: the learn flow, a relative encoding on a real endless encoder, the table, Add, Copy, Rename and Delete.
- [ ] **CTL-1** Creating and deleting layers in the Manage window (a layer needs the message that selects it, learned like any control).
- [ ] **CTL-2** Editing button lights and LED rings in the Manage window (the model, `Preset.lights`, already holds them).
- [ ] **CTL-3** Guess the encoding of a relative encoder while learning it (turn a few clicks; the first message after a stop and its sign tell it).
- [ ] **CTL-4** An `init` line for connect-time system-exclusive messages (some controllers need one to enter a mode).
- [ ] **CTL-5** Motor faders (the program never moves a fader; it waits for the fader to reach the value).
- [ ] **CTL-6** Scribble strip and time display feedback.
- [ ] **CTL-7** A second (XT) unit.
- [ ] **CTL-8** Blinking lights.
- [ ] **HW-8** The controller's port is looked for every 2 s (`watch_controller`): plugging it in after the start connects it, pulling it out drops it. Not tried with a real unplug.

### Rudiments (since 0.6.0)

The Rudiments tab sits between Score and Metronome. `docs/rudiments.md` is the catalogue and its source (the PAS chart, read off the notation); `core/src/rudiments.rs` is that catalogue as patterns plus `Rudiment::build(repeats, lead) -> Score`, so Listen, takes, grading, the staff, the Results tab and the controller all work on a rudiment as on a file. `src/app/rudiment.rs` is the tab. 132 tests pass; the tab was looked at in the running app (screenshots of two rudiments), nothing was heard and no V31 was connected.

- [ ] **RUD-1** **Check the ⚑ rows of `docs/rudiments.md` against the PAS chart**: the closing note values of rolls 7-15, drag paradiddles #1 and #2, and the flam drag were read from a small engraving and may be off. Only the note values are in doubt (the sticking was read carefully and is tested). The chart is `https://www.coloradomesa.edu/marching-band/documents/rudiments.pdf` (PAS, 1984).
- [ ] **HW-10** **Try it with the V31.** Fast diddles on a mesh pad may double-trigger (extra hits); the adaptive window helps but the pad's retrigger setting matters. Listen to the rudiments (grace notes are 35 ms per step before the main note, `analysis::GRACE_SPACING`).
- [ ] **HW-11** Time the start-up with the V31 and a controller attached (how long do the MIDI-port and drum-module steps take?), before deciding **CD-13**; the first measurement had no devices.
- [ ] **HW-9** Try the controller on the Rudiments tab (encoder 2 repeats, 3 lead, 8 rudiments, range buttons prev/next) on a real X-Touch Mini.
- [ ] **RUD-2** The multiple bounce (buzz) roll can only be listened to; make it recordable.
- [ ] **RUD-3** Flam and drag graces are optional; a spacing check would need them required (`analysis::GRACE_SPACING`, optional graces).
- [ ] **RUD-4** Grade the sticking: it is shown but never graded, only compared per hand, because the module cannot hear hands (needs hand-aware input hardware).
- [ ] **RUD-5** Orchestration of a rudiment around the kit (a map from hand or stroke to `Lane` in `Rudiment::build`).
- [ ] **RUD-6** User-defined rudiment files (like the controller presets).
- [ ] **RUD-7** Practice sets and routines (a list of rudiment, repeats, BPM stepped through when a take is clean).
- [ ] **RUD-8** A progress history beyond the best clean tempo (takes are only in memory today).
- [ ] **UI-4** The score view draws no beams or tuplet brackets, so triplet and 32nd-note rudiments rely on proportional spacing and the beat grid.

### User manual

`docs/manual/manual.template.html` is the manual; `tools/build_manual.py` (standard library only) fills in the version/label/date and generates the rudiment tables from the `rud(...)` lines of `core/src/rudiments.rs` (it fails unless it finds 40). The release workflow has a `manual` job whose artifact goes into both archives and the release. **When behaviour or UI text changes, update the template** (it is written by hand; settings keys, shortcuts and controller tables are copied from the code). The manual says in two places that the V31 map and the controller presets are untested; update that when they are tried.

### Developer guide

`docs/developer/developer.template.html` + `tools/build_devguide.py`, same style as the manual (`docs/style.css` is shared). Placeholders `{{code:PATH::TEXT}}` pull the Rust item whose first line contains TEXT (with doc comments and attributes, highlighted) so excerpts never go stale; the module table, test counts, dependency table and the rudiment pattern language are generated. **A refactoring that renames or moves a shown item fails the guide build** (and the release): change the TEXT in the template. The prose is hand-written; update it when architecture changes (sections: layers, threads, practice sources, controller, rudiments, invariants, tutorials).

### Licence

`LICENSE` is PolyForm Noncommercial 1.0.0 (copyright notice line on top). A copyright review on 2026-10-26 found no infringement: no binary assets, synthesised sounds, an original sample score, dependencies all permissive (no GPL/MPL), PAS rudiments used as facts with own wording. `tools/build_notices.py` makes `THIRD_PARTY_NOTICES.txt` for the archives (run by the `manual` job). After adding a dependency nothing needs doing: the file is rebuilt from `cargo metadata`. Not done: the product names are unchecked as trademarks ("Roll Model" included).

### Still open

- [ ] **SND-1** **Listen to the new cymbals and the rest of the kit.** Nobody has heard them; only their spectra are tested (centroid, share below 2 kHz, lone low partials). Tune `Cymbal` parameters in `synth.rs` by ear, then adjust the guard bands if needed. The hi-hats are untouched and still very bright (spectral centroid about 13 kHz).
- [ ] **SND-2** **Ask the user** whether per-instrument volume plus WAV files is enough, or whether several synthesised kits (brighter, darker, …) are wanted.
- [ ] **UI-1** Try with a real mouse and wheel: the Layout popup, the mixer's mute/solo/play buttons, the wheel on sliders. Verified only through the keyboard, config keys, and headless egui tests.
- [ ] **SND-3** The WAV override and the hi-hat choke have unit tests but were never heard.
- [ ] **UI-2** The numbering mismatch is still there: bars drawn in the score show their printed number (repeated after unrolling repeats), while the Measures sliders and *start a row at bar* use the position. The Layout popup shows the printed number of the chosen bar; consider showing both on the bars, or making the sliders use printed numbers.
- [ ] **FEAT-1** Tempo ramp (+x BPM every n bars) for Listen and the metronome.
- [ ] **FEAT-2** Accent patterns and tap tempo for the free metronome.
- [ ] **FEAT-3** Remember the layout per score.
- [x] **FEAT-4** A Listen without count-in setting of its own. **Done**: `listen_count_in` and the *in Listen* box.
- The click is implemented twice (a sine blip in `metronome::click_audio`, used by takes and by the player) and `Metronome` (takes, calibration) and `Player` (Listen, metronome tab) are two threads with separate loops; this is task **CD-5** below.
- [ ] **UI-3** Shortcuts (`R`, `L`, `K`, `M`, keyboard pads) are ignored while a widget has keyboard focus (after Tab); `Esc` clears it.

### Clean-design review (2026-10-29)

A review of the whole source tree (`src/`, 15.3 k lines with tests) against clean-design rules: SOLID, DRY, YAGNI, KISS, Law of Demeter, cohesion and coupling, the usual smells. **Nothing here has been changed**; this is the list to work from. Measurements are from a script run over the tree (non-test lines; the function lengths are brace-matched).

**What is already good, so nobody "fixes" it:** the pure core really is pure (no GUI, MIDI or audio imports in `analysis`, `musicxml`, `score`, `drums`, `xml`, `rudiments`, `trigger`, `controller`, `controller_layout`); the real-time seam (`Sink`) is a trait with a fake; there are no `TODO`s, no `dbg!`/`eprintln!`, one `unwrap` in non-test code and no `expect`; the only lint suppressions are four (`too_many_arguments` twice, `let_unit_value` twice); every module has tests; the public items carry doc comments that say why.

**Numbers worth knowing:** `app.rs` 1,922 lines, `controller.rs` 1,185 (+598 test lines), `app/controls.rs` 797, `render.rs` 635, `app/controls/editor.rs` 584. `DrumApp` has 48 fields and nine `impl` blocks. Longest functions: `render::show` 218, `musicxml::parse_musicxml` 144, `parse_measures` 132, `DrumApp::results_tab` 128, `DrumApp::update` 114, `settings_controller` 108, `Config::from_text` 93, `Preset::line` 90. 271 `format!` calls, 102 `.clone()`, 39 repeated `.small().weak()` labels, 13 `Result<_, String>`. `Action::` appears 189 times in `controller.rs` and 59 in `app/controls.rs`.

Severity: **High** = a structural problem that makes changes costly or risky; **Medium** = raises cognitive load; **Suggestion** = polish. Effort: S = hours, M = a day or two, L = several days. Every item should be done as its own commit with the tests green; none needs a behaviour change.

#### High

- [x] **CD-1 `app.rs` is a god file and `DrumApp` a god struct** (SRP, cohesion). 1,922 lines, 48 fields, nine `impl` blocks. It holds the take, Listen, audition and calibration lifecycles, five Settings pages, the Results tab with CSV export, the top bar, the Home tab, the MIDI event handling and `update`. Inconsistent already: the controller's Settings page lives in `app/controls.rs`, the others in `app.rs`. **First half done** (the god *file*): `app/session.rs` (take, Listen, audition, calibration), `app/score_tab.rs`, `app/results.rs`, `app/settings.rs` (four pages and `SettingsPage`) and `app/home.rs`; `app.rs` went from 1,961 to 693 lines. The methods moved unchanged. Grouping the 48 fields into sub-structs is now **CD-27**.
  *Fix:* first move methods without touching fields: `app/settings.rs` (the four pages), `app/results.rs` (`results_tab`, `export_csv`, `histogram`, `grade_letter_color`), `app/session.rs` (take, listen, audition, calibration), `app/home.rs`. Then group fields into sub-structs that own their invariants: `Playback { listen, player, listen_notes, listen_loop, listen_pending, audition, audition_control, control }`, `MidiState { midi_inputs, midi_outputs, midi_in, queue, monitor, learn, lane_text }`, `MetroState`, `UiState { tab, settings_page, status, error, fit_bars }`. Effort L, risk medium (the practice-source swap in `Parked` must keep naming every field).
- [x] **CD-2 `controller.rs` has six jobs in one 1,185-line file** (SRP, cohesion). Messages and parsing, actions and their texts, relative encodings, the preset format (parse, write, validate), the mapper with pickup and lights, and the learning and editing operations. **Done**: `core/src/controller/` with `message.rs`, `action.rs`, `preset.rs`, `mapper.rs`, `error.rs` and `tests.rs`; `mod.rs` re-exports the same names, so no caller changed. The largest file is `preset.rs` (about 430 lines).
  *Fix:* a `controller/` folder: `msg.rs` (`Msg`, `parse`, `Encoding`), `action.rs` (`Action`, `State`, titles and help), `preset.rs` (`Preset`, `Binding`, `Light`, parse, `to_text`, `assign`), `mapper.rs` (`Mapper`, `Pickup`, `Command`), `learn.rs`; `controller.rs` re-exports so callers do not change. Keep every part pure. The 598 test lines move with their code. Effort M, risk low (mechanical, the compiler finds everything).
- [x] **CD-3 Shotgun surgery to add an action** (OCP). A new `Action` needs edits in: the enum, `SIMPLE`, `title`, `help`, `is_level`/`is_browse`/`has_light`, `State` (`lit`/`level`), `apply_command` (`press`/`turn`/`set`), `Action::sections` and, for a light, `State`. `title` and `help` are two parallel 40-arm matches. The compiler catches the matches but not `SIMPLE` or `sections`. **Done**: `ACTIONS` in `controller/action.rs` (name, title, help, class, light per action); `parse`, `text`, `title`, `help`, `is_level`, `is_browse`, `has_light` and `sections` read it. A new action is an enum variant, one row, and (for a light) a `State` field. The strip actions stay computed. `title`/`help` still return `String` (CD-14).
  *Fix:* one table `ACTIONS: &[ActionInfo { action, name, title, help, class }]` (class: button, value, list) from which `parse`, `text`, `title`, `help`, `is_level`, `is_browse` and `sections` derive; the per-strip actions (`Mute(i)` …) stay computed. Keep the exhaustive test that every action is in the editor once. Also makes `Action::title/help` return `&'static str`/`Cow` (CD-14). Effort M.
- [x] **CD-4 Shotgun surgery to add a setting** (OCP, DRY). A key is written in the struct, `Default`, `from_text` (a 93-line `match`), `to_text` (62 lines), a round-trip test and a Settings page. **Done**: `KEYS` in `config.rs` (`key!("zoom", c.zoom, number(0.3, 4.0))`) is both the reader and the writer of every one-line setting; lists and maps (`recent`, `gain.*`, `mute.*`, `map.*`, `rudiment_bpm.*`, `rudiment_best.*`) stay by hand. A number that is NaN or infinite is now ignored like any other bad value.
  *Fix:* table-driven keys (`const KEYS: &[Key { name, parse, write }]`) or a small declarative macro, so a setting is one line plus its page; keep tolerant loading (unknown keys ignored, bad values keep the default). Effort M, risk low (the round-trip test compares whole `Config` values).
- [ ] **CD-5 Two timing threads with duplicated click and loop logic** (DRY; already a known gap). `metronome.rs` (takes, calibration) and `playback.rs` (Listen, the metronome tab, audition) each have a loop that sleeps, spins, clicks and checks stop. `playback::run_with` takes 8 parameters and `run` 7.
  *Fix:* one scheduler thread that plays a list of timed cues to a `Sink`, used by both; takes would use the `Player` with a clock-zero argument. Touches take timing, which works: do it behind the existing fake-`Sink` tests first, and verify with a real take on a release build. Effort L, risk high. At least group the parameters into a `RunSpec` struct (S).
- [x] **CD-6 `render::show` is 218 lines and its helpers take 5 parameters** (long method, data clump). One function lays out systems, draws both staves, notes, hits, velocity bars and the playhead; `draw_head`, `draw_ledgers`, `draw_staff`, `draw_hand` all take `(painter, x, y, ...)` again. **Done**: `show` is 65 lines: it builds a `Frame` (view, layout, `Canvas` of painter and colours, buckets of notes) and calls `draw_system`, `draw_measure_marks`, `draw_score_note` and `draw_played_note`; `Rows` holds the vertical positions of a system. Checked by dumping every shape drawn for a sample score before and after (identical). The small helpers (`draw_head`, `draw_ledgers`, `draw_staff`, `draw_hand`) keep their signatures because `rolling` uses them too.
  *Fix:* a `Canvas { painter, scale, colours }` context passed to small functions; split `show` into phases (`systems`, `score_notes`, `played_notes`, `playhead`). The layout (`layout`, `row_starts`) is already separate and tested. Effort M.

#### Medium

- [x] **CD-7 Primitive obsession in the controller domain.** (a) `relative: bool` plus `encoding: Encoding` allows nonsense (an absolute control with an encoding): make it `enum ControlKind { Absolute, Relative(Encoding) }`; the editor's `KINDS: [(bool, Encoding); 5]` and `kinds: BTreeMap<String, (bool, Encoding)>` disappear into it. (b) A layer is an `Option<String>` everywhere: a `LayerId` newtype or an index into `Preset.layers`. (c) `Src::Bend(u8)` carries a MIDI channel while the others use `Binding.channel`: one `Control { src, channel }`. (d) `Learned` repeats `Binding`'s `src`, `channel`, `relative`, `encoding` (a data clump): `Binding { control: Control, action, layer }` and `Learned = Control`. Effort M. **Part (a) done**: `Reading { Absolute, Relative(Encoding) }` replaces `relative: bool` plus `encoding` in `Binding`, `Learned`, `learn`, `set_reading` and the editor. Parts (b) to (d) are now **CD-26**.
- [x] **CD-8 A preset has three identities.** The list and `config.txt` use the file stem, the help title and `Preset.name` use the `name =` line, and rename rewrites both (`with_name`). `load_preset` returns a built-in preset first when a file has the same name, so "editable" is decided by a name check (`editable_path`). **Done**: the name a preset is known by (list, settings, help title) is the built-in name or the file stem; `load_preset` ignores a file's `name` line, `PresetSource` and `preset_source` decide built-in against file in one place, and rename is a plain file rename (`with_name` is gone).
  *Fix:* one `PresetId { source: BuiltIn(&'static str) | File(PathBuf) }` used for choosing, listing and editing; the `name =` line becomes display-only or is dropped on write. Effort M.
- [x] **CD-9 Duplicated "choose a preset" code** (DRY). The page's selector (`settings_controller`), `select_preset`, `use_new_preset`, `delete_preset` and `rename_preset` each set `cfg.controller_preset`, clear `cfg.controller_port`, reset the editor and call `connect_controller` in slightly different ways.
  *Fix:* `select_preset` (already in `editor.rs`) as the only entry, called from all of them and moved next to `connect_controller`. Effort S, do this first.
- [x] **CD-10 Stringly errors** (`Result<_, String>`, 13 places, including the pure modules `controller` and `loader`). The text is built where the error happens, so the UI cannot choose its wording and tests cannot match on a variant. **Done**: `PresetError` and `LoadPresetError` (controller), `XmlError`, `ScoreError`, `LoadError`, `MidiError` and `ConfigError`, each with a `Display` that keeps the old wording; the UI turns them into text only where it stores a message.
  *Fix:* `PresetError`, `LoadError`, `MidiError` enums with `Display` (the UI keeps the wording); keep `String` only at the very edge (`self.error`). Effort M.
- [x] **CD-11 Long parameter lists.** `controller_help::button` and `::encoder` take 8 (with `#[allow(clippy::too_many_arguments)]`), `text_box` 6, `triangle` 5, `widgets::slider` 6 and `slider_with_zero` 7, `musicxml::parse_note` 6, `synth::pitched` 7, `Score::click_track` 5, `controller::learn` 5. **Done for the help drawing** (a `Paint` context with the drawing as methods; both `allow`s are gone). Left as they are: `widgets::slider` (6-7 parameters, 25 call sites), `musicxml::parse_note`, `synth::pitched` (a table of numbers), and `controller::learn`, which CD-7 reshapes.
  *Fix:* a `Paint { painter, scale, font }` for the help drawing (removes both `allow`s), option structs for `slider` and `learn`. Effort S.
- [x] **CD-12 Law of Demeter and encapsulation around `Controller`.** The editor (a child module) reads and writes `self.controller.editor.learning`, `.notice`, `.confirm_delete`, `self.controller.link`, `self.controller.kinds` directly, six-plus times per function. That is allowed by the module privacy and makes the state machine hard to see. **Done**: the learn state machine and the notices go through `EditorState::{say, start_learning, stop_learning, chosen_kind, choose_kind}`, and the connection through `Controller::{kind_of, replace_preset, is_connected}`. Plain widget flags (`open`, `keep_going`, `confirm_delete`, the rename text) are still touched directly (CD-19 separates them).
  *Fix:* methods on `EditorState` (`start_learning`, `finish_learning`, `cancel`, `say`) and on `Controller` (`link_mut`, `kind_of`); the learn flow then reads as a sequence of named steps. Effort S.
- [ ] **CD-13 Blocking I/O on the UI thread.** `watch_controller` calls `midi::list_inputs()` (which creates a `MidiInput` each time) every two seconds from the UI thread; the start-up steps `MidiPorts`, `DrumInput` and `Controller` still open and list MIDI ports on the UI thread (only the sound-output scan is on a thread), and opening a vendor driver can take seconds. The first measurement of start-up could not time them (no devices attached).
  *Fix:* one worker thread for MIDI enumeration and opening that answers over a channel, as `scan_sound_outputs` does; the splash then keeps animating. Time it with a real V31 and X-Touch connected first. Effort M.
- [ ] **CD-14 Per-frame work in the UI.** `preset_being_edited` clones the whole preset (and reads and parses its file when disconnected) every frame the Manage window is open; `Action::title/help` return a new `String` per call and the table calls them for ~90 rows; `Action::sections()` builds `Vec`s every frame; `refresh_controllers` parses every preset file on each refresh; `recent_row` calls `is_file` per row per frame.
  *Fix:* cache the edited preset in `EditorState` (invalidate in `edit_preset`, the one writer), return `&'static str`/`Cow` from `title/help` (with CD-3), make `sections()` a `static`. Measure first; none of this is visible at the current sizes. Effort S to M.
- [x] **CD-15 Test seams in production types, and a fragile test.** `EditorState.dir` and `Startup.load_settings` exist for tests, while `Config::controllers_dir()` and `Config::path()` are called from a dozen places. `the_controller_page_draws_with_and_without_a_controller` once failed because it listed the real settings folder, which held the owner's own presets. **Done**: `config::Paths` (`system()`, `in_dir()`, `none()`; `config_file`, `sounds_dir`, `controllers_dir`) is held by `DrumApp.paths` and used everywhere; `Config::load_from(&paths)` and `save(&paths)` replace the global path functions. `EditorState.dir` and `Startup.load_settings` are gone. A test `DrumApp` has `Paths::none()`, so no test can read the owner's files.
  *Fix:* an injected `Paths { settings_dir, controllers_dir }` held by `DrumApp` and used everywhere; tests build one on a temporary folder. Effort M.
- [x] **CD-16 `controller_layout.rs` repeats literals.** `x_touch_mini`, `x_touch_compact` and `generated` build `Control { kind, name, center, size, press, turn }` about twenty times with the same shapes; `generated` (79 lines) has magic numbers (90, 130, 280, 95, 62, 190…). The name `Kind` clashes with `PresetKind` and the editor's kind tuple. **Done**: `Control::{button, encoder, fader, new}` with `.pressing()` and `.turning()`, `encoder_row` shared by the two drawn layouts, the numbers of `generated` in a `rows` module, and `Kind` renamed `ControlKind`.
  *Fix:* `Control::button/encoder/fader(name, at, press|turn)` constructors, named row constants, rename `Kind` to `ControlKind`. Effort S.
- [x] **CD-17 `Preset::line` (90 lines) and `Preset::position` (nested closures).** `line` is one `match` that parses every key; `position` finds a binding in four passes with closures that capture each other. **Done**: `Preset::line` dispatches to `parse_layer`, `parse_bind` and `parse_light`; `Preset::position` is one ranked lookup (layer first, then exact channel, then the first line).
  *Fix:* `parse_bind`, `parse_light`, `parse_layer` functions; `position` as one `max_by_key` over a score (layer match, channel match). Keep the existing tests. Effort S.
- [x] **CD-18 `DrumApp::handle_event` mixes five concerns** (drum-note MIDI-learn, the monitor, calibration, the start trigger, take recording) in one function, and `update` (114 lines) is a flat list of phases. **Done**: `handle_event` is a sequence of named steps (`learn_note`, monitor, `record_calibration_hit`, trigger, `record_take_hit`), not a handler list, because learn, monitor and calibration all see every event and only the trigger can consume it. `update` is `update_state`, `shortcut_keys`, `draw`, `save_settings_if_changed` and `schedule_repaint`.
  *Fix:* a small list of handlers tried in order (`learn`, `monitor`, `calibration`, `trigger`, `take`), each returning whether it used the event. Effort S to M.
- [x] **CD-19 The editor's UI state lives in controller state.** `rename_to`, `rename_for`, `confirm_delete`, `open`, `layer` are widget state stored in `Controller.editor`, next to the learn state machine that drives behaviour. **Done**: `ManageView { layer, rename_to, rename_for, confirm_delete, open }` is `EditorState.view`; `EditorState` keeps the learn state machine and its choices.
  *Fix:* split `EditorState` into the learn state (behaviour, tested) and a `ManageView` (what the window remembers). Effort S.
- [x] **CD-20 Binary-only crate.** The pure core cannot be a dependency of a test or tool without building eframe, midir and rodio, and there is no `tests/` folder or `cargo doc` for it. **Done**: a Cargo workspace; `core/` is the crate `roll-model-core` (analysis, musicxml, xml, loader, score, drums, rudiments, trigger, controller, controller_layout, config) with only `zip` as a dependency, so `cargo test -p roll-model-core` builds no GUI, MIDI or audio crate; `core/tests/practice_session.rs` is an integration test through the public API. `default-members` keep `cargo build` and `cargo test` covering both crates. Release: bump `Cargo.toml`, `core/Cargo.toml` and `Cargo.lock` together. `analysis::analyze` is now a normal public function (it was test-only).
- [x] **CD-26 Rest of CD-7.** (b) a layer is an `Option<String>` everywhere: a `LayerId` newtype or an index into `Preset.layers`. (c) `Src::Bend(u8)` carries a MIDI channel while the others use `Binding.channel`: one `Control { src, channel }`. (d) `Learned` repeats `Binding`'s `src` and `channel` (a data clump): `Binding { control, reading, action, layer }` with `Learned` as the control and reading. Effort M. **Part (d) done**: `Input { src, channel }` is the one control type; `Binding { layer, input, reading, action }` and `Learned { input, reading }` share it. Parts (b) and (c) are left on purpose: (c) `Src::Bend` carrying the channel matches MIDI (every channel is another fader) and the file syntax `bend N`, so merging it into `Input.channel` would change the lookup rules for little gain; (b) a layer newtype touches about 55 sites for a type that is only ever compared with names from the same file. Do them only if a second kind of name (a layer index in `Light`, say) makes the confusion real. **Closed**: (b) and (c) are decided against, not postponed.
- [x] **CD-27 Rest of CD-1: `DrumApp` still has 48 fields.** Group them into sub-structs that own their invariants: `Playback { listen, player, listen_notes, listen_loop, listen_pending, audition, audition_control, control }`, `MidiState { midi_inputs, midi_outputs, midi_in, queue, monitor, learn, lane_text }`, `MetroState`, `UiState { tab, settings_page, status, error, fit_bars }`. The methods are already in the files that use them, so each group is a change of `self.x` to `self.group.x` plus methods on the group. Risk medium: the practice-source swap in `Parked` must keep naming every field. Effort M. **Done** for the four groups with clear owners: `ViewState { tab, settings_page, status, error, fit_bars }`, `MidiState { inputs, outputs, input, queue, monitor, learn, lane_text }`, `Playback { listen, player, notes, looping, pending, control, solo, audition, audition_control }` and `MetroState { run, player, control }`; `DrumApp` went from 48 fields to 29. The practice-source fields (`Parked`) and the take/calibration fields stay flat because `activate` swaps them as a set.
- [x] **CD-28 `app/controls.rs` is the largest file (1,198 lines).** The Controller page, the plug-and-play watch, building the `State`, `apply_command` with `press`/`turn`/`set`, and about 350 lines of tests. *Fix:* `controls/page.rs` (Settings page and indicator), `controls/apply.rs` (`apply_command` and what each action does in the app), `controls/state.rs` (`controller_state`, lights), keeping `Controller` and the connection in `controls.rs`; tests next to the part they test. Effort S to M, risk low (same pattern as CD-1). **Done**: `controls.rs` is 276 lines (connection, watch, preset choice); `controls/apply.rs` (state, `apply_command` and every action), `controls/page.rs` (Settings page) and `controls/tests.rs` hold the rest.
- [x] **CD-29 Drawing helpers still take `(painter, x, y, ...)`** (what CD-6 left). `draw_head`, `draw_ledgers`, `draw_staff`, `draw_clef`, `draw_hand` in `render.rs` take a painter plus 4-5 values, and `rolling` repeats the staff drawing of `show`. *Fix:* make them methods of `Canvas`, used by both `show` and `rolling`; the shape-dump comparison used for CD-6 (all shapes of a sample score before and after) is the safety net. Effort S. **Done**: `head`, `ledgers`, `staff`, `clef`, `hand` and a new `written_note` (head, ledgers, hand letter, stem, ghost, accent) are methods of `Canvas`; `show` and `rolling` both use `written_note`, which removes the second copy of the note drawing. The shapes drawn by `show` and `rolling` for a sample score are identical to before.
  *Fix:* a workspace with a `roll-model-core` library (pure modules) and the application crate on top; the layering in the developer guide already says where the line is. Effort L, mechanical.

#### Suggestions

- ~~**CD-21**~~ (hint helper done: `widgets::hint`, 30 single-line uses replaced; the nine multi-line forms, the named spacing constants and the string table are left) `RichText::new(..).small().weak()` appears 39 times and `ui.add_space(n)` 45: a `hint(ui, text)` helper in `widgets.rs` and named spacing constants. The 271 `format!` calls are literal UI strings; a string table is the prerequisite of any translation (known gap).
- ~~**CD-22**~~ (done: without the `audio` feature `Audio` is `Option<Infallible>`, so the stub has no unit value and both `#[allow(clippy::let_unit_value)]` are gone; clippy reports nothing on either feature set. Still open from the idea: run `cargo clippy -- -W clippy::pedantic` once and decide which lints to adopt. A build without the `audio` feature warns that `synth::Kit.samples`/`sample` are unused, which is harmless but could be silenced with a `cfg`.)
- ~~**CD-23**~~ (done: `playback/tests.rs`, `analysis/tests.rs` and `config/tests.rs` next to `playback.rs` (468 lines), `analysis.rs` (536) and `config.rs` (519); the developer guide builder now finds every `.rs` file under `src`, `core/src` and `core/tests`.)
- **CD-24** Visibility conventions: `pub(super)` inside `app/` means "the `app` module", `pub(super)` inside `app/controls/` means `controls.rs` only, and there is one `pub(in crate::app)`. The core crate exports everything it shows in `lib.rs` as `pub`; keep new core items `pub` only when the app or a test of `core/tests/` uses them. Documented in the developer guide; nothing to do but keep to it.
- **CD-25** Dependency versions are deliberately old (eframe 0.29 and so on); upgrading is its own project (see "Known gaps and ideas").

**Suggested order (open tasks):** at the desk: CD-29, CD-28, CD-22, CD-23 (all small, no risk; CD-29 first because it has the strongest safety net). Then with measurements: CD-14 (profile the Manage window first; fix only what shows), CD-13 (needs a real V31 and controller to time the port calls), and last CD-5 (needs a real take on a release build). CD-24 and CD-25 are conventions, not work. Done so far: CD-1 to CD-4, CD-6 to CD-12, CD-15 to CD-21, CD-26, CD-27.

### Name

The application was renamed from "Drum Tutor" to **Roll Model** (crate and binary `roll-model`). The settings folder moved to `RollModel` / `roll-model` with a one-time copy from the old one (`Config::migrate`, tested). The brainstorm is kept in [`docs/name-ideas.md`](docs/name-ideas.md).

- [ ] **NAME-1** Check the name (GitHub, web, trademark, domain) before shipping it widely.
- [ ] **NAME-2** Rename the local folder `D:\projects\DrumTutor` if wanted (the GitHub repository is already `roll-model`).
- [x] Released as 0.6.0 (the rename, rudiments, controller, licence and docs): see `CHANGELOG.md`.

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
trigger.rs     The start trigger: `Detector::feed(sequence, lane, at)` says when two hits in a row (0.5 s window) complete it; pure. `app/trigger.rs` decides when it may fire (Score/Rudiments tab, no take recording), starts the take, has the Settings section and the hint
controller_layout.rs What a controller looks like, pure: `Layout` (positions of encoders, buttons, faders and the message each sends; `x_touch_mini()` and `x_touch_compact()`, chosen by the preset's port in `for_preset`), `mapping()` = what a control does on a layer, `is_lit()`; `Action::title()/help()` in controller.rs are the texts
app/controller_help.rs The help screen: an `egui::Window` that draws the layout with the painter (opened by a click on the controller indicator in the top bar, `toggle_controller_help`)
controller.rs  Mackie Control surfaces, pure: `parse`, `Preset` (text files in `presets/` and the user's `controllers` folder), `Mapper` (message -> `Command`, layers, shift, fader pickup, lights), `State`
metronome.rs   Click thread for takes and calibration: precise sleep+spin; audio (feature "audio") and/or MIDI; also opens the sound output device (`open_audio`, `output_devices`)
playback.rs    "Listen" and the free metronome: Player thread plays cues to a `Sink` (sound card and/or MIDI out); `dispatch` decides what sounds
control.rs     `Control`: atomics shared with the sound threads (click, notes, loop, volumes, per-instrument gain), `ClickLevel`, `ErrorLog`
synth.rs       Software drum kit: one synthesised mono sample per Lane, WAV override loader (no GUI/MIDI deps)
widgets.rs     UI helpers: `slider()` (caption first, mouse wheel), `roomy()` (bigger Score-tab style)
app/startup.rs The start-up splash (an immediate egui viewport, closed by the first key or click after the start-up): `DrumApp::new` only sets up; the slow work (settings, MIDI ports, drum module, controller, sound outputs on a thread, a score from the command line) runs as steps, one per frame, after the splash naming the step was drawn (eframe shows the window only after the first frame)
app/metro.rs   The Metronome tab (free click track)
app/mixer.rs   The instrument mixer on Settings -> Sound
app/rudiment.rs The Rudiments tab: choosing, repeats, lead, tempo per rudiment, best clean tempo, ladder
app/controls.rs The controller: connection, `Command` -> app actions, the Settings -> Controller page
config.rs      key = value settings file, auto-saved when Config changes
```

### Key conventions and invariants

- **Start-up.** Keep `DrumApp::new` free of device and file work: the window is hidden until the first frame, so anything slow there delays it. Add a `Step` in `app/startup.rs` instead (a label, the work, the line it leaves in the log). Measured before the splash: listing the sound outputs (cpal) took 260-400 ms, MIDI ports about 30 ms; opening the V31 and the controller was not measured (neither connected). While `DrumApp.startup` is `Some` the splash window is drawn; until the steps are done the main window is empty (`Flow::Starting`). The main window comes up in front of the splash, so the splash asks for the focus on frame 2 and when the steps finish. On a platform without second windows egui draws it inside the main window. Looked at on Windows only.
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
