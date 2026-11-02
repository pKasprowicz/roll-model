# Status — Roll Model

> **Made with AI.** This project was created with extensive use of AI (Anthropic's Claude), directed and reviewed by the author. Check anything that matters.

What is true now, on one page. The task list is [`HANDOFF.md`](HANDOFF.md) and the GitHub issues; user documentation is `README.md` and the manual; the architecture is explained in the developer guide (`roll-model-developer-guide.html`, built from `docs/developer/`). Older notes are in [`docs/history/`](docs/history/handoff-2026-11-02.md).

## What it is

A Rust desktop program (eframe/egui 0.29) that loads a MusicXML or MIDI drum score and shows it (Score tab), practises the 40 PAS rudiments like a score (Rudiments tab), captures MIDI from a Roland V31 drum module and draws the hits under the notes, grades the drummer, plays the score back (Listen), has a free metronome, and can be driven by a control surface. Version: see `Cargo.toml`. It is released on GitHub (Windows and Linux archives).

**Never run on the real V31 or heard on real speakers by the author's tools.** The grading, matching and scheduling are tested against fakes (`Sink`, a manual `Clock`, simulated callbacks); the hardware tasks (HW-*, AUD-*, SND-*) in `HANDOFF.md` are for the owner, with the plan in `docs/hardware-test-plan.md`.

## The two crates

- `core/` (`roll-model-core`): everything that needs no window, MIDI port or sound card: `score`, `units`, `analysis`, `practice`, `practice_tempo`, `rudiments`, `import` (MusicXML and MIDI to `Score`), `notation`, `drums`, `trigger`, `controller` and `controller_layout`, `config`, `cues` and `clock` (what a run plays, and the MIDI clock of it), `progress`, `export`, `stats`, `xml`. It does file I/O (settings, progress, presets, score files) and no device I/O; it must not depend on the app crate or on eframe, midir or cpal.
- the app (`roll-model`): `app/` (state and the tabs), `render.rs` (the score drawn with egui's painter), `theme.rs`, `midi.rs`, `audio/` (the engine: mixer, clock, resampling, cpal backend), `playback.rs` (the scheduler of Listen, the metronome tab and auditions), `metronome.rs` (the other scheduler: takes and calibration; the two are to become one, RV-30), `control.rs` (atomics shared with the sound threads), `synth.rs` (the software kit), `log.rs`, `sync.rs`, `widgets.rs`.

## Invariants worth knowing

- **Units are types.** A position in a score is `Quarters`, a time `Seconds`, a short offset `Millis` (`core/src/units.rs`). Quarters become seconds only through `TempoMap::q_to_sec`; the renderer's pixel geometry and the playback scheduler still use `f64` and convert at the border.
- **One practice at a time.** `DrumApp` holds two `Practice` values (everything that belongs to a source: score, range, tempo, take, history); `activate(source)` swaps them, and `show_tab` changes a tab and activates its source. The Rudiments tab builds a score at `BASE_BPM`; `PracticeTempo` says whether the tempo is a percentage (a score) or BPM (a rudiment).
- **One command path.** The keys, the on-screen buttons and a controller's buttons all end in `DrumApp::press`/`turn`/`set`; the lock of a recording is asked once (`allowed_now`) and these functions have no wildcard arm, so a new `Action` does not compile until each of them says what it does.
- **Drawing does not change the domain.** The Rudiments picker returns what was asked for (`Picked`) and `apply_picked` changes the state; a tab is changed by `show_tab`. A widget's own value (a slider, a text field) is written where it is drawn.
- **A take's time zero** is the start of the first measure of the range; `Take.t0` is that measure's absolute time, so `q = sec_to_q(hit.t + t0)`. In `start_take` zero is `now + 0.5 s + the count-in`. Hit times come from `Instant::now()` in the MIDI callback, less `latency_ms`.
- **Grading** (`analysis::analyze`): candidate pairs within the window, greedy cheapest-first, one to one; a related pad costs `related_pad_penalty` more; expected notes after `elapsed` are `Outcome::Pending`; grace notes are optional. Every number of the score and the advice is in `ScoringModel` (the weights are first guesses to tune with real takes); `Window::Adaptive` shrinks the window to the spacing of the notes (rudiments).
- **Ranges are written once** in `config::limits` (`Span`): the settings file, the sliders and the controller's encoders and faders use the same one.
- **Settings and progress.** `config.txt` is written whole or not at all, a moment after a change and on exit; a damaged file is kept as `config.txt.bad`. The best clean tempo of each rudiment is in `progress.txt`. Problems the sound threads report go to `roll-model.log`.
- **Start-up** is a splash that runs `Step`s one frame at a time (`app/startup.rs`); keep `DrumApp::new` free of device and file work.
- **The core crate is tested without devices** (`cargo test -p roll-model-core`); the app's timing tests use a manual `Clock` and take 0.1 s.

## How to work

- `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release --locked` are what CI runs on every push to `develop` (the site build too: the manual and the developer guide read the source for their excerpts, so a renamed item shows up as a failed build there).
- One commit per change, with the tests green. User-visible changes go under `[Unreleased]` in `CHANGELOG.md`; a release bumps `Cargo.toml`, `core/Cargo.toml` and `Cargo.lock`, merges `develop` into `main` and tags `vX.Y.Z` (the tag builds the release).
- `UPDATE_SNAPSHOTS=1 cargo test -p roll-model-core --test musicxml_import` rewrites the import snapshots; `docs/manual/rudiments.json` is rewritten by `cargo run -p roll-model-core --example dump_rudiments` (a test says when it is stale).
- `examples/basic_rock_beat.musicxml` is the sample every test leans on; if you edit it, update `parses_sample` and the snapshots.

## Known gaps

- The V31's note map is the common Roland map, unverified; the hi-hat pedal polarity is the Roland convention, unverified. MIDI learn exists in Settings.
- The kit is synthesised and simple; cymbals have been tested by their spectra, not by ear.
- The click is scheduled by two threads (`Metronome`, `Player`): RV-30.
- Not drawn: ties; D.S./D.C./coda are not followed; `score-timewise` is refused; only the first drum part of a file is read.
- Takes are not kept between sessions (the CSV export exists); a progress log per score is the natural next feature.
- Keyboard pads are timestamped at frame time (about 16 ms): for testing, not for timing.
- Dependencies are old on purpose (eframe 0.29, cpal 0.17): newer ones do not resolve together or change APIs the code uses.
