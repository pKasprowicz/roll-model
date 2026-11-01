# Harsh code review: Roll Model 0.10.1

- **Reviewed:** commit `e8da83d` ("Merge develop: release 0.10.1"), the whole tree: `src/` (app crate), `core/src/` (`roll-model-core`), tests, CI, `HANDOFF.md`.
- **Date:** 2026-11-01.
- **Focus:** cleanliness, maintainability, technical debt, semantics, instability, architecture, code smells, and how much a person has to hold in their head to change the code safely.
- **Tone:** deliberately harsh. Things that are good are listed at the end so nobody "fixes" them.

## How to use this report (for the next session)

- Each finding has an ID (`RV-n`), a severity, an effort and the files involved. Do **one commit per finding** with the tests green, like the CD-* tasks in `HANDOFF.md`.
- Severity: **S1** a defect or a design flaw that already causes wrong behaviour or will on the next change; **S2** structural debt that makes changes costly or risky; **S3** cognitive load or readability; **S4** polish.
- Effort: **S** = hours, **M** = a day or two, **L** = several days.
- "Verified" means a test or a command was run. "By reading" means the code was traced by hand and not executed.
- Do not reopen what `HANDOFF.md` records as decided (CD-26 b/c, CD-14's "measure first", AUD-9). Where this report disagrees with an earlier decision it says so explicitly.

### Verification done for this review

| Check | Result |
|---|---|
| `cargo test --release` (app and core, Linux, rustc 1.97.0, with `libasound2-dev`) | 164 app tests and 163 core tests pass (2 ignored) |
| `cargo clippy -p roll-model --all-targets` (default lints) | **16 warnings**, among them `too_many_arguments` (8/7) and 2× `type_complexity`. `HANDOFF.md` (CD-22) says clippy reports nothing. |
| `cargo clippy -- -W clippy::pedantic` | 812 warnings in the app crate; about 400 in core (100× `must_use`, 32× float `==`/`!=`, 9× `usize as u8` truncation, many lossy casts) |
| `cargo fmt --check` | **1,116 diff hunks**: the code is not rustfmt-formatted, and there is no `rustfmt.toml` to explain why |
| Lines over 140 / 200 characters | 218 / 27 |
| A throwaway test for RV-1 (reverted afterwards) | **Failed: the bug is real** (see RV-1) |

---

## Executive summary: the ten that matter most

1. **RV-1 (S1, verified bug):** a score loaded in the background ends up in the *rudiment's* slot if the user is on the Rudiments tab when it arrives. The cause is the "parked practice fields" swap design (RV-10).
2. **RV-2 (S1):** settings are rewritten to disk on **every frame** in which `Config` changed (a slider drag is about 60 writes a second). The write is not atomic. A crash in the middle leaves a truncated file, the loader silently falls back to defaults, and the next frame **overwrites** the user's settings with them.
3. **RV-10 (S2):** `DrumApp` swaps eight fields with `Parked` by hand, by tab. Some fields that belong to a source (`score_report`, `midi_options`, `loading`, `rud.built`) are not swapped. The invariant lives in the programmer's head, and RV-1 shows it is already broken.
4. **RV-11 (S2):** `tempo_pct` means *per cent* on the Score tab and *BPM* on the Rudiments tab. It only works because `BASE_BPM == 100`. The ramp, tap tempo, the controller and the sliders all depend on this coincidence.
5. **RV-20 (S2):** time units are plain `f64` everywhere: quarter notes, seconds, milliseconds, tenths of a BPM, "per cent". The project's main invariant ("positions are `q`; seconds only via `TempoMap`") is enforced only by naming.
6. **RV-30 (S2):** CD-5 is still open, and the duplication has *grown*. There are now two schedulers, two MIDI-delay mechanisms (`midi_delay_before_next` vs the `HeldNote` queue), two engine-run setups and two MIDI-output openers.
7. **RV-12 (S2):** controller commands and keyboard shortcuts are two parallel command paths with different rules. `press/turn/set` end in `_ => {}`, which switches off exhaustiveness checking, and use **match guards with side effects** (`if self.unlocked()` writes the status line).
8. **RV-40 (S2):** there are three or four sources of truth for every range: config clamps, UI slider ranges, controller normalisation constants, and hover texts with the numbers written in. They already disagree.
9. **RV-60 (S2):** pure derived data is recomputed every frame: `analyze_with` over the whole take, `notation::analyse`, the layout, per-measure buckets, `played_notes`, `note_grades`, the edited preset (cloned, or read and parsed from disk).
10. **RV-90 (S2):** `HANDOFF.md` is 92 KB and contradicts itself. It is the mandatory entry point (per `CLAUDE.md`), and it is stale in its "Architecture" and "Listen" sections.

---

## A. Defects found by the review

### RV-1: A background load that finishes on the Rudiments tab puts the file into the rudiment slot (S1, verified, effort S)

**Where:** `src/app.rs:637-653` (`update_state`: `sync_source` runs *before* `poll_loading`), `src/app/import_menu.rs:42-60` (`poll_loading` → `install`), `src/app.rs:470-488` (`activate`), `src/app/rudiment.rs:56-79` (`ensure_rudiment` returns early when `rud.built == params && score.is_some()`).

**Reproduction (verified with a throwaway test):** set `background_bytes = 0`, `open_file(sample)`, switch to `Tab::Rudiments` and call `sync_source()`, then poll until the load is done, then `sync_source()` again. Result:

- The Score tab shows **no score**.
- The Rudiments tab shows **"Basic Rock Beat"** as the rudiment.
- `ensure_rudiment` never rebuilds, because `rud.built` still matches.

**Why:** `install` writes into whatever practice fields are active, and on that frame they belong to the rudiment. It then sets `tab = Score`, and the swap in `draw()` parks the new file as the rudiment's score.

**Fix:**
- Short term: `poll_loading` (and `install`) must call `self.activate(Source::Score)` first.
- Real fix: RV-10.
- Add the scenario above as a regression test.

### RV-2: Settings are written every frame, non-atomically, and a corrupted file silently resets everything (S1, by reading, effort S)

**Where:**
- `src/app.rs:720-727` (`save_settings_if_changed` after every frame).
- `core/src/config.rs:512-519` (`std::fs::write` straight onto `config.txt`).
- `core/src/config.rs:501-510` (`load_from`: any read error → `unwrap_or_default()`; the migrate error is dropped with `let _ =`).

**Why it hurts:**
- Dragging any `cfg` slider (zoom, volume, count-in, ramp) writes the file about 60 times a second.
- A crash, power loss or full disk during a write leaves a partial file.
- `from_text` is tolerant, so a truncated file parses as "the first N keys, defaults for the rest". The user loses the note map, `rudiment_best` (practice history, RV-71), the controller choice and the latency calibration without any message. The next frame then persists that state.

**Fix:**
- Write to `config.txt.tmp`, then `rename` (atomic on the same volume).
- Debounce saving: save when the value has been stable for about 500 ms, and on exit (`eframe::App::on_exit` / `save`).
- On a read or parse failure, keep a backup (`config.txt.bad`) and show a message.
- Apply the same write-then-rename to controller presets (`editor.rs:213`, `std::fs::write` on every edit).

### RV-3: Learn reports success and moves on when saving the preset failed (S1, by reading, effort S)

**Where:** `src/app/controls/editor.rs:228-258`.

`edit_preset` returns `None` on a load or write failure, and `.flatten()` merges that with "nothing was taken from another action". The notice "Record: note 89" is then shown, and with `keep_going` the editor moves on to the next action as if the binding had been saved. `Op::DeleteLayer` (`:765`), `Op::Clear`, `Op::Kind` and `Op::SetLight` also ignore the result. `DeleteLayer` announces "Layer X deleted." regardless.

**Fix:** have `edit_preset` return `Result<R, EditError>`, and stop the learn flow when it fails.

### RV-4: Light editing does read-parse-write of a file on every drag frame (S1 or S2, by reading, effort S)

**Where:** `editor.rs:798-843` (`light_cell`: each `DragValue::changed()` → `Op::SetLight`) → `run_op` → `edit_preset` → `load_preset` (read and parse the file) → `std::fs::write` → `Mapper::replace_preset`.

**Why it hurts:** dragging a number writes the preset file at frame rate, non-atomically (RV-2), and rebuilds the live mapper each time.

**Fix:** edit an in-memory copy and persist on `drag_stopped()` or `lost_focus()`, or debounce.

### RV-5: Invariants are checked only in debug builds, and the UI indexes as if they always hold (S2, effort S)

**Where:**
- `core/src/import/finish.rs:71`: `debug_assert!(check_invariants(&score).is_ok())`.
- UI code indexes `score.measures[...]` directly and computes `measures.len() - 1` in about 20 places (`session.rs:84-117, 375, 382`, `score_tab.rs:20`, `metro.rs:205-208`, `controls/apply.rs:347`, `render.rs:231-268`).

**Why it hurts:**
- A malformed import that slips past the parsers produces an invalid `Score` in release builds.
- An empty `measures` makes `n_measures - 1` underflow and panic.
- "Non-empty measures" is an invariant of the importer and of `Rudiment::build`. It is not encoded in a type.

**Fix:**
- Run `check_invariants` in release as well and turn a failure into `ImportError::Invalid`. It is O(n).
- Or make `Score` construct only through a validating constructor, with `measures` private and a `NonEmpty` accessor.

### RV-6: Some editor code paths turn a channel into a note number (S3, by reading, effort S)

**Where:** `editor.rs:730-733`: `controller::Src::Note(n) | controller::Src::Bend(n) => controller::Out::Note(n)`.

`Src::Bend(n)` carries a **MIDI channel**, so this arm would light "note <channel>". It is unreachable today only because the `wanted` filter above excludes `Bend` for lights and rings. That is a landmine for the next person who loosens the filter. The same predicate (`matches!(Cc)` / `!matches!(Bend)`) is written twice (`:726` and `:834`).

**Fix:** use `Src::light_target(self, ring: bool) -> Option<Out>` in core, with the reasoning in one place.

### RV-7: The drum MIDI queue is unbounded; the controller queue is capped (S3, effort S)

**Where:** `src/midi.rs:23` (`EventQueue = Arc<Mutex<Vec<RawEvent>>>`, no cap) vs `src/midi.rs:141` (`MAX_QUEUED = 2000` for the controller).

**Why it hurts:** if the window stops repainting (minimised, or a modal OS dialog such as `rfd` blocking the UI thread), drum events pile up without limit and are then all processed in one frame.

**Fix:** use the same cap, or a fixed-size `rtrb` ring buffer. The project already depends on `rtrb`.

### RV-8: A poisoned mutex is handled differently in each place (S3, effort S)

**Where:**
- `src/app.rs:394` (drum queue): returns, and every later frame drops all input silently.
- `controls.rs:214`: an empty `Vec`.
- `controls.rs:174`: ignored.
- `audio/handle.rs:81`: mapped to `PlayError::QueueFull`, which is **semantically wrong** (it reports overload when the real problem is a panic on another thread).
- `control.rs:136/142`: silently ignored.

**Fix:** pick one policy. `lock().unwrap_or_else(PoisonError::into_inner)` is the usual choice for these plain data queues. Do not map poison to an unrelated error variant.

### RV-9: Hand-written CSV export without quoting (S3, effort S)

**Where:** `src/app/results.rs:185-227`.

`m.number` comes from the score file, and the fields are joined with `,` without quoting or escaping. A measure number such as `"12,a"` or a quote character breaks the rows. The export is also formatting logic for domain data that lives in the GUI crate: it belongs in core next to `Report`, where it could be tested without egui.

---

## B. Architecture

### RV-10: Practice state is swapped by hand (`Parked`) instead of modelled (S2, effort M)

**Where:** `src/app.rs:59-87`, `:470-488`, `:490-505`; `src/app/rudiment.rs:42-54`.

`DrumApp` holds *one* set of practice fields plus a `Parked` copy, and `activate()` swaps eight fields with `mem::swap`, one line each. The consequences:

- **Not every source-specific field is swapped:** `score_report`, `midi_options`, `loading`, `rud.built`, `playback.pending`, `controller.cursor`. Every access to those needs `self.source == ...` guards (`is_midi_score`, `has_main_score`, `main_path`), which is exactly where RV-1 slipped in.
- `sync_source()` runs **twice per frame** (`app.rs:638` and `:694`), once more from `next_tab` (`apply.rs:335`), and indirectly from drawing code. Drawing therefore mutates domain state.
- Adding a field means remembering to add it to `Parked`, `Parked::new`, `activate`, and to every `match self.source` accessor. The compiler does not help.

**Fix:** a `Practice` struct (`score`, `path`, `range`, `tempo`, `take`, `takes_started`, `history`, `report`, `import_options`) and `practices: [Practice; 2]` (or `score_practice`, `rudiment_practice`) with `fn practice(&self) -> &Practice` chosen by `source`. Swapping disappears; "which one is active" is one index. Background loads write into `score_practice` regardless of the tab.

### RV-11: `tempo_pct` is per cent or BPM depending on the tab (S2, effort M)

**Where:** `src/app.rs` (`tempo_pct: f32`), `src/app/rudiment.rs:12-15` ("the practice tempo in percent is the BPM"), `session.rs:86` (`scale = tempo_pct / 100`), `session.rs:160-165` (`listen_ramp`: `step = ramp_percent / tempo_pct`), `controls/apply.rs:82-87, 247-252, 340-351` (tap tempo converts BPM to per cent for scores, but not for rudiments), `core/src/rudiments.rs:22` (`BASE_BPM = 100.0`).

**Why it hurts:**
- Everything works only because rudiment scores are written at exactly 100 BPM. Change `BASE_BPM` and tempo, ramp, tap tempo and status texts all become silently wrong. Nothing would fail to compile and probably no test would fail.
- The `Listen` signature stores `tempo_pct: i32`, `Plan` has `scale`, `Take` has `tempo_scale`: three representations of the same quantity.

**Fix:** use `enum PracticeTempo { Percent(f32), Bpm(f32) }` or, better, always store a target BPM and derive `scale = bpm / written_bpm` in one function. `BPM_RANGE` / `TEMPO_RANGE` then become properties of the source.

### RV-12: Two command paths (keyboard vs controller) with different rules, and match guards with side effects (S2, effort M)

**Where:** `src/app.rs:655-685` (`shortcut_keys`) vs `src/app/controls/apply.rs:168-315` (`press`/`turn`/`set`).

- `R`, `L`, `M` and `Esc` reimplement `Action::Record/Listen/Metronome/Stop` with different preconditions. For example, the key path never calls `unlocked()` and never says "Open a score first."
- `press`, `turn` and `set` each end in `_ => {}` (`apply.rs:234, 282, 314`). A new `Action` compiles and silently does nothing in all three. The CD-3 table made adding actions easy and made forgetting to implement them easy too.
- Guards such as `Action::TapTempo if self.unlocked() => ...` call a `&mut self` method that **writes the status line** while the match is being decided. A reader has to know guard evaluation order to predict the status message. `TempoDown`/`TempoUp` call `unlocked()` twice (`press` → `turn`).
- `in_context` (`apply.rs:93-105`) silently re-maps `RangeFrom` → `Repeats` and so on by tab: the same button means something different and the code shows no sign of it at the call site.

**Fix:**
- One `fn perform(&mut self, Intent)` used by keyboard, controller and buttons.
- Make the lock check explicit: `if action.needs_unlock() && self.take_running() { return self.say_locked(); }`.
- Remove the wildcard arms (list the no-op actions explicitly), so the compiler enforces completeness.

### RV-13: `DrumApp` is still a god object; the split was by file, not by responsibility (S2, effort L)

**Facts:**
- `app.rs` is **885 lines** again (`HANDOFF.md` says it went down to 693).
- About 30 fields plus 4 groups.
- `impl DrumApp` blocks are spread over 15 files, all with `pub(super)` access to every field.

Every "module" (`session.rs`, `rudiment.rs`, `metro.rs`, `controls/*.rs`, `import_menu.rs`) is a partial class: it reads and writes any field of `DrumApp`. The file split hides the coupling but does not reduce it. For example, `controls/apply.rs` touches `cfg`, `take`, `history`, `playback`, `metro_run`, `rud`, `range_*`, `tempo_pct`, `view` and `controller`.

**Fix (incremental):** give the sub-structs behaviour and private fields. Begin with `Playback` (owns its players, the restart debounce, `sync_control`), `Metro` (RV-31) and `Practice` (RV-10). A test of the Listen lifecycle should not need a `DrumApp`.

### RV-14: Domain logic and data live in the GUI crate (S2, effort M)

The "pure core" boundary is drawn too narrowly. These are pure, testable domain code and belong in `roll-model-core`:

- `Take`, `Plan`, `plan_run`, `TakeSummary`, `stop_take`'s summary, the calibration median, `played_notes`, `note_grades`, `worst_measure`, `shifted_range`, `register_tap`, `nudged_volume`, the CSV export (RV-9).
- `playback::{Cue, Ramp, metronome_cues, cues, velocity_for, Subdivision}`, and `ClickLevel`.

Today they can only be tested by building eframe, midir and cpal, which is the problem CD-20 was meant to solve. `Subdivision` is even persisted in `Config` as `metro_sub: usize` ("index into `playback::Subdivision::ALL`"), so core stores an index into an app-crate enum it cannot see (RV-41).

### RV-15: Core is not as pure as `CLAUDE.md` claims (S3, effort S to M)

Core does file-system and environment I/O:

- `Config::load_from` / `save` / `migrate`
- `Paths::dir_named` (env vars)
- `controller::load_preset` / `custom_presets`
- `import::load_score` (`std::fs`)

That is defensible, but then `CLAUDE.md`'s description ("needs no window, MIDI port or sound card") is the accurate one and "pure" is not. Either move the I/O to thin `fs` adapters (`Config::from_text` and `to_text` are already pure) or stop calling the crate pure in the docs and guide.

### RV-16: Similar names for unrelated concepts (S3, effort M)

A newcomer has to keep apart:

| Name | Meanings |
|---|---|
| `control::Control` | atomics shared with the sound threads |
| `app::controls::Controller` | the hardware surface connection |
| `core::controller` | presets, mapper, actions |
| `app/controls.rs` | the app-side controller module |
| `Layout` | `render::Layout` (rows of bars), `controller_layout::Layout` (picture of a device), `egui::Layout` (imported in the same files) |
| `Report` | `analysis::Report` (grading), `import::Report` (file info); both used in `DrumApp` |
| `Command` | `controller::Command` (Press/Turn/Set), `audio::mixer::Command` (Play/CancelRun) |
| `State` | `controller::State` (lights), `app::rudiment::State` (tab memory) |
| `Frame` | `render::Frame` (drawing context), `eframe::Frame` (in `app.rs`) |
| `Source` | "practice source" (score vs rudiment), next to "source code" and `Src` (MIDI message origin) |
| `Action::Score` / `Action::Rudiment` | "browse recent scores / rudiments", not a score or a rudiment |

**Fix:** rename `Control` → `LiveMix`/`SoundSwitches`, `controls` → `surface`, `render::Layout` → `RowLayout`/`SystemLayout`, `Action::Score` → `BrowseScores`, `analysis::Report` → `Grading`, and so on. These are mechanical, compiler-checked renames.

---

## C. State, semantics and hidden coupling

### RV-17: The status line is used as a return channel (S2, effort S)

- `startup.rs`: the `DrumInput` step calls `connect_midi()` and then **reads `self.view.status`** to build its log line. The `Score` step reads `view.error` and `view.status` after `open_file`.
- `rudiment.rs:146-164` (`note_clean_rudiment`) **appends** to the status that `stop_take` has just written (`format!("{} · {message}", self.view.status)`): this is temporal coupling between two functions in two files.

**Fix:** make `connect_midi`, `open_file` and `stop_take` return outcomes (`Result<Connected, MidiError>`, `TakeOutcome`) and let the callers decide what to display.

### RV-18: Drawing functions mutate domain state (S2, effort M)

- `rudiments_tab` calls `ensure_rudiment()` (rebuilds the score, resets the range, drops the take).
- `score_tab` writes `range_from/to` (slider readback), and `remember_tempo` writes `cfg.rudiment_bpm` from `sync_source`, which is called from `draw`.
- `preset_header` writes `rename_for/rename_to` while drawing.
- `rudiment_picker` sets `cfg.rudiment_repeats` and `rud.lead` and relies on a later `ensure_rudiment()` in the same function to make them take effect.

Immediate mode tempts this, but the result is that the order of widget calls is semantically significant.

**Fix:** collect UI intents (as `editor.rs` does with `Op`) and apply them in one place after drawing. `editor.rs` already shows the pattern.

### RV-19: Boolean and illegal-state soup in the score model (S2, effort M)

**Where:** `core/src/score.rs` (`ScoreNote`), `core/src/import/ir.rs` (`RawNote`), `core/src/analysis.rs` (`Expected`).

- `accent: bool, ghost: bool, grace: bool, grace_order: u8, stem_up: Option<bool>`. Accent and ghost together is representable; `grace_order != 0` while `grace == false` is representable; `finish.rs` sets `grace_order: u8::from(n.grace)`, so every imported drag loses its order.
- `Rest.up: bool` and `ScoreNote.stem_up: Option<bool>` are two encodings of "voice".
- `Grade::Pending` is a status, not a grade, yet it has `credit() == 0` and must be skipped explicitly everywhere (`summarize`, `worst_measure`, `played_notes`).
- `HitStatus` (render) duplicates `Grade` minus `Missed`/`Pending` plus `Extra`/`Neutral`, with two parallel colour functions (`grade_color`, `status_color`) and a lossy mapping with a wildcard (`session.rs:487`, `_ => HitStatus::Neutral`).

**Fix:**
- `enum Dynamic { Normal, Accent, Ghost }`.
- `enum Kind { Main, Grace { order: NonZeroU8 } }`.
- `enum Voice { Hands, Feet }`.
- `enum Outcome { Pending, Graded(Grade) }`.
- One `Judgement` type for both staves.

---

## D. Types, units and primitive obsession

### RV-20: Units carried in names, not types (S2, effort M to L)

`f64` is used for quarter notes (`q`, `start_q`, `len_q`), seconds (`t`, `t0`, `end_sec`, `beat_sec`, `GRACE_SPACING`), milliseconds (`dt_ms`, `latency_ms`, `window_ms`, `perfect_ms`, `MOVED_MS`) and `Duration` in the threads. `f32` is used for per cent (`tempo_pct`), BPM (`metro_bpm`, `rudiment_bpm`) and gains. There are 16 hand-written `* 1000.0` / `/ 1000.0` conversions in non-test code (`analysis.rs:188, 192, 261`, `results.rs:203, 211, 221`, `session.rs:466`, `audio_output.rs:80, 87`, `quantise.rs:157, 294`, `config.rs:436`, …) plus `/ 100.0` for per cent.

The core invariant from `CLAUDE.md` ("convert to seconds only via `TempoMap`") is unenforceable: nothing stops `n.q * beat_sec`.

**Fix:** newtypes `Quarters(f64)`, `Secs(f64)` (or `Duration` where possible), `Millis(f64)` and `Bpm(f64)`, with `TempoMap::to_secs(Quarters, scale) -> Secs` as the only bridge. Start with the core API (`Expected.t`, `Hit.t`, `ScoreNote.q`) and let the compiler find the rest.

### RV-21: Anonymous tuples as domain types (S3, effort S to M)

| Tuple | Location |
|---|---|
| `clicks: Vec<(f64, bool)>` (time, accent) | `Plan`, `Metronome::start`, `Score::click_track`, `playback::cues` |
| `pending: Option<((usize, usize, i32, u32), Instant)>` (clippy `type_complexity`) and `metro::Run.signature: (i32, u32, usize)` | `app.rs`, `metro.rs` |
| `rud.built: Option<(u8, usize, Lead)>`, read as `.0` | `rudiment.rs:28, 65` |
| candidate tuple `(f64, usize, usize, bool)` | `analysis.rs:233` |
| `Cymbal { cluster: (usize, f32, f32, f32, f32), wash: (f32, f32, f32, f32), wobble: (f32, f32), pings: &[(f32, f32, f32)] }` (unreadable at the call sites) | `synth.rs:122-134, 241-282` |
| `hands: Vec<(Hand, HandStats)>` plus `[Vec<f64>; 2]` indexed by `usize::from(hand == Hand::Left)` | `analysis.rs` |
| `TempoMap.points: Vec<(f64, f64)>`, `RawMeasure.tempos: Vec<(f64, f64)>` | score and import |
| `trigger_hint() -> Option<(String, bool)>`, `Controller::label() -> Option<(String, bool)>`, `recent_row -> (bool, bool)` | app |

**Fix:** named structs (`Click { at, accent }`, `ListenKey`, `BuiltRudiment`, `Partials { count, lo_hz, hi_hz, … }`). These are the cheapest readability wins in the codebase.

### RV-22: Boolean parameters (S3, effort S)

`stop_take(completed: bool)`, `install(.., again: bool)`, `begin_listen(count_in: bool)`, `choose_preset(name, keep_port: bool)`, `analyze_with(.., adaptive: bool)`, `Op::AddLight(Action, bool)`, `Op::ClearLight(Action, bool)`, `Op::AskDelete(bool)`, `Op::AskDeleteLayer(bool)`, `lights_of(.., ring: bool)`, `move_range(forward: bool)`, `controller.label(chosen: bool)`, `Score::click_track(.., beats: bool)`.

At the call sites (`stop_take(false)` appears 11 times in non-test code) the reader has to jump to the definition. **Fix:** small enums (`StopReason::{Aborted, Completed}`, `LightKind::{Button, Ring}`, `Window::{Fixed, Adaptive}`).

### RV-23: Stringly keys and sentinels (S3, effort S)

- `EditorState.kinds: BTreeMap<String, Reading>` keyed by `action.text()` instead of `Action` (`editor.rs:73, 101-107`).
- `Config.layouts: Vec<(String, RowLayout)>` and `recent_files: Vec<String>`: paths as lossy `String`s (`to_string_lossy`) and compared with a hand-written case-insensitive comparison on Windows only.
- The `"Untitled"` sentinel is written by two parsers (`musicxml.rs:35`, `midi.rs:304`) and checked by string comparison in `import/mod.rs:165`. Use `title: Option<String>` in `RawScore`.
- `Lane::family() -> u8` with `_ => 100 + self as u8` as a "no family" sentinel (`drums.rs:123-131`). Use `Option<Family>`.
- `NO_GROUP = u8::MAX` plus a `u32` bitmask built from lane indices (`playback.rs:choke_groups`). This breaks silently past 32 lanes, and it uses `Lane::ALL.iter().position()` linear searches at runtime.
- `group_lanes(i)`: `_ =>` maps every index ≥ 7 to the Ride group; `group_label` clamps. `Action::Mute(u8)` is unchecked. Use `enum Strip` or a checked index.
- `editor_layer()` returns `None` both for "the preset has no layers" and "the stored index is stale", and `None` means "edit the base bindings". A stale `view.layer` silently edits the wrong set.

### RV-24: Public fields everywhere in core (S3, effort M)

There are 367 `pub` fields in non-test code. `NoteMap.map` is public (and `config.rs` builds `NoteMap { map: Default::default() }` directly), and `Score.measures/notes/rests` are public, so no invariant (sorted notes, notes inside measures, non-empty measures) can be enforced (RV-5). `Report` exposes about 25 public fields, mixing raw matching data with derived statistics and English sentences.

---

## E. Magic numbers and several sources of truth

### RV-40: Ranges are defined in three or four places and already disagree (S2, effort M)

| Quantity | Config clamp (`config.rs` KEYS) | UI slider | Controller | Text |
|---|---|---|---|---|
| zoom | 0.3–4.0 | 0.5–3.0 (`score_tab.rs`) | — | — |
| count-in bars | 0–4 | 0–2 | `/ 2.0`, `* 2.0`, `clamp(0, 2)` (`apply.rs:127, 274, 303`) | — |
| bars per row | 0–16 (and `.min(16)` again in `layout` parsing) | 0–8 | `/ 8.0`, `clamp(0, 8)` | — |
| rudiment BPM | 20–300 (`config.rs:603, 607`) | 30–240 (`BPM_RANGE`) | `BPM_RANGE` | — |
| score tempo % | — | `25.0..=150.0` literal (`score_tab.rs:53`) | `TEMPO_RANGE` (`apply.rs:16`) | — |
| metronome BPM | 20–300 | `20.0..=300.0` literal (`metro.rs:151`) | `METRO_RANGE` | — |
| lead | — | `Lead::ALL` | `/ 2.0`, `* 2.0` (assumes 3 leads) | — |
| clean take | `CLEAN_TAKE = 85.0` | — | — | "85 or better" (`rudiment.rs:194`) |
| ladder step | `LADDER_STEP = 5.0` | — | — | "goes up by 5 BPM" (same line) |
| latency, tolerances, click volume | **unbounded** (`number(f64::MIN, f64::MAX)`, `config.rs:322-335`); negative tolerances and a click volume of 1e30 load fine | | | |
| monitor lines | `MONITOR_LINES = 14` (controls) | `truncate(14)` literal (`app.rs:407`) | | |
| restart debounce | `LISTEN_RESTART_DELAY` 300 ms | `metro::RESTART_DELAY` 300 ms | | |

**Fix:** one `Range` constant per setting, defined once (in core, next to the key), used by `KEYS`, by the slider and by the controller normalisation (`fn to_unit(v) / fn from_unit(x)`). Write hover texts with `format!` from the constants.

### RV-41: Persisted enum indices (S3, effort S)

`metro_sub: usize` is stored as an index into `Subdivision::ALL` (`config.rs`, `playback.rs:index/from_index`). Reordering or inserting a subdivision silently changes users' settings. Store the name, as `trigger_first` does with `LaneName`.

### RV-42: The grading model is inline arithmetic (S2, effort S)

`analysis.rs` mixes the algorithm with tuning constants:

- 0.040 s related-lane penalty
- 0.45 gap factor, 0.050 grace window
- 45 ms and 120 ms timing decays
- weights 0.55/0.25/0.10/0.10 vs 0.60/0.28/0.12
- `* 0.5` in the played-fraction correction
- velocity margins +10 / −15 and the median fallback of 80
- feedback thresholds 10, 15, 25, 4, 5, 2, 3, 0.6, 8, 10
- the letter thresholds

`HANDOFF.md` calls the weights "arbitrary first guesses; tune with real takes", but the code gives no single place to tune them and no test pins the formula as a whole. The `overall` expression at `analysis.rs:396-406` (`let overall` at 400) cannot be understood without a derivation.

**Fix:** a `ScoringModel` struct with documented fields and `Default`, passed in like `Tolerances`; one golden test per take fixture.

### RV-43: Renderer geometry magic (S3, effort S)

`render.rs` has named constants at the top (good), but the drawing code still uses dozens of literals: `14.0`, `28.0`, `18.0`, `22.0`, `9.0`, `4.5`, `7.0`, `16.0`, `420.0`, `+ 8.0` quarters after the end (`holds_playhead`), `0.08`, `0.7`, `0.25` alpha, `+ 2.0 * LS`, `LEFT - 22.0` / `- 18.0` / `- 20.0`. Moving any row breaks the others in ways that only show on screen.

---

## F. Duplication (DRY)

### RV-30: Two schedulers, still (S2, effort L; CD-5, open)

`metronome.rs::run` (takes, calibration) and `playback.rs::run`/`run_with` (Listen, the metronome tab, auditions) each:

- open the MIDI output (`connect_output`)
- clamp the channel (`clamp(1, 16) - 1`)
- create a `ClickBank`, call `new_run`
- wait `wait_ready(500 ms)` with near-identical error texts
- loop with look-ahead and send note-on/off
- sleep a tail and `cancel_run`

Since CD-5 was written down the **MIDI delay was implemented twice in two different designs**: `midi_delay_before_next` (blocking the click thread) vs `DeviceSink::held` (a queue). The ramp exists only in the player. Any timing fix now has to be made twice and verified twice on hardware.

**Fix (as CD-5 says):** takes use `Player` with a `Sink`. The fake-`Sink` tests already exist. Do it before the next timing feature, not after.

### RV-31: Restart-after-settle debounce, written twice (S3, effort S)

`update_listen` (`session.rs:247-270`) and `update_metro` (`metro.rs:103-127`) implement the same state machine ("signature changed → remember since when → restart after 300 ms") with different tuple types and constants.

**Fix:** `struct Debounce<K> { key: K, since: Instant }` with `fn poll(&mut self, now: K) -> bool`.

### RV-32: Other copies

- **MIDI message parsing:** `midi.rs:100-108` parses note-on and CC#4 by hand; `controller::parse` already parses MIDI messages.
- **Port opening:** list ports → names → `pick` → connect is written three times in `midi.rs` (`connect_input`, `connect_controller`, `connect_output`).
- **"Nothing audible" messages:** three variants (`session.rs:169-178`, `session.rs:326-336`, `metro.rs:71-77`).
- **"Connect the controller first (Refresh and reconnect, above)."** three times in `editor.rs:709, 717, 756`.
- **`self.take.as_ref().is_some_and(|t| t.running)`** written out **10 times**, although `take_running()` exists (`apply.rs:148`).
- **Stop everything:** `stop_take(false); stop_listen(); stop_metro();` is repeated as a block 9 times (`start_take`, `begin_listen`, `start_metro`, `start_calibration`, `activate`, `reimport`, `open_file`, shortcut `Esc`, `Action::Stop`; some without `stop_metro`). Make it `fn stop_all_sound(&mut self)`.
- **"Default gain is not stored":** `set_group_gain` (`apply.rs`) duplicates the mixer's rule; `(gain - 1.0).abs() < 0.01` vs the mixer's own check.
- **`median`:** a closure in `analysis.rs:435` (clones and sorts per call); the quantiser has its own median.
- **Test helper `noise()`:** written twice (`audio/handle.rs` tests and `audio/clock.rs` tests).
- **Two resamplers:** `load_wav` resamples to 44.1 kHz linearly, then `audio::resample` resamples again to the device rate, so a user's WAV is interpolated twice. `Kit` holds `Arc<Vec<f32>>` while the engine wants `Arc<[f32]>`, so every Listen start copies.
- **Two click velocity tables:** `ClickLevel::velocity` and `playback::velocity_for` (ghost 45, grace 70, normal 90, accent 120).
- **Docs contradicting code:** see RV-90.

---

## G. Error handling

### RV-50: CD-10 is half done (S3, effort S)

Typed errors were introduced, but `Result<_, String>` remains in:

- `audio::backend::Engine::open` / `open_device` / `build`
- `synth::load_wav`
- `rudiments::parse`
- `import::check_invariants`

Sound-thread problems travel as `ErrorLog(Vec<String>)`. The UI shows only the **first** error of each batch (`take_errors().into_iter().next()`, 6 places) and drops the rest without logging. There is no log file or `log`/`tracing` at all, so field problems on the owner's hardware cannot be diagnosed afterwards.

### RV-51: Silent failure as the default (S2, effort S)

There are 18 `let _ =` in non-test code. Some are fine (a dropped send on a closed channel). These hide real faults:

- `let _ = engine.play(...)` (`metronome.rs`, `playback.rs`, `DeviceSink`): `QueueFull` or `NotReady` means **a click or a note is silently not played**. The mixer counts its own drops, but drops at the handle are counted nowhere.
- `let _ = conn.send(...)` for MIDI clicks and notes.
- `Player::start` / `Metronome::start`: `thread::Builder::spawn(..).ok()`. If the thread cannot be spawned, nothing plays, `is_finished()` returns `true`, and the UI thinks playback ended normally.
- `let _ = Config::migrate(old, new)` (RV-2).

**Fix:** count handle-side failures in `Shared` next to the mixer counters and show them in the Sound readout. Turn a spawn failure into an error message.

---

## H. Performance and real-time behaviour

### RV-60: Per-frame recomputation of derived data (S2, effort M)

While recording or listening, the app repaints continuously, and every frame does:

| Work per frame | Where |
|---|---|
| `analyze_with` over the whole take: candidate generation, sort, greedy matching, `summarize`, `feedback` (English strings), `expected.to_vec()` | `session.rs:413-420` |
| `played_notes()` and `note_grades()` allocate fresh `Vec`s | `score_tab.rs:208-209` |
| `notation::analyse(score)`, `layout(...)`, `notes_by_measure`, `played_by_measure`, `stem_tips`, `rests_by_measure` | `render.rs:721-760` |
| `staff_for_lane` for 18 lanes, a linear scan of all notes each | `render.rs`, via `lane_staff` |
| `preset_being_edited()`: clones the preset, or **reads and parses the file** when disconnected (CD-14) | `editor.rs:191` |
| `Action::sections()` builds `Vec`s, `title()`/`help()` allocate | editor |
| `cfg != saved_cfg`: a deep compare of a ~60-field struct with maps, then `clone()` on change (RV-2) | `app.rs:720` |

For the owner's 990-note song this is still fast enough on a desktop, so `HANDOFF.md`'s "measure first" stance is reasonable for the editor. For the score view it is not a question of speed but of **structure**: derived data with no cache means no single place knows when the score changed. **Fix:** a `ScoreView` cache keyed by `score_gen` (which already exists) holding notation, buckets and layout per width/zoom; incremental grading (only hits since last frame).

### RV-61: Blocking work on the UI thread (S3, effort M; CD-13, open)

- `ensure_audio()` runs before **every** start (take, Listen, metronome, audition, calibration) and calls `default_output_device()`, a cpal host query, on the UI thread. `HANDOFF.md` measured listing outputs at 260–400 ms. The default query is cheaper, but it is unmeasured and sits on the path between pressing `R` and computing time zero.
- `watch_controller` enumerates MIDI ports on the UI thread every 2 s.
- `Metronome::stop()` / `Player::stop()` `join()` on the UI thread. That is bounded by the 80 ms click tail and 10–20 ms sleeps, but it is a hitch on every stop.
- `Kit::load` re-synthesises the kit (2 s cymbals with 40 partials) and re-reads the WAV files on **every** Listen start. It runs on the player thread, so it eats into the 0.5 s lead.

### RV-62: Allocation and freeing inside the audio callback (S4)

The mixer documents "never allocates, locks or frees", yet:

- `backend.rs`: `scratch.resize(data.len())` can allocate inside the callback when a block exceeds `SCRATCH`.
- `Mixer::bury` drops the `Arc` **on the callback thread** when the graveyard is full.
- `CancelRun` does `Vec::remove(i)` in a loop (O(n²) on 256 pending).

All are documented trade-offs, but the module's headline claim is stronger than the code. Reword it, or pre-size and handle the full graveyard by retrying later.

---

## I. UI-layer smells

### RV-70: The Manage window (`editor.rs`) is now the largest file (S2, effort M)

It is 1,257 lines (877 non-test). `assignments_body` is about 140 lines with closures nested 5–6 deep (`CollapsingHeader` → `Grid` → `for` → `horizontal` → `ComboBox` → `for`). It mixes file I/O (`std::fs::write`, `rename`, `remove_file`, `create_dir_all`, `unused_path`), preset domain operations, the learn state machine and widget code. `use super::*;` glob-imports the whole parent module. CD-28 is marked done, but the problem moved here.

**Fix:**
- Move preset file management (create, copy, rename, delete, unique name, name validation) to core as a `PresetStore` with `Paths`.
- Turn the learn and detect state machine into a pure struct with `fn feed(&mut self, Msg) -> LearnEvent`, tested without egui.
- Keep only drawing in `editor.rs`.

### RV-71: User data is stored in the settings file (S3, effort S)

`rudiment_best` (best clean tempo per rudiment) is **practice history**, stored next to the zoom level in a file that is overwritten at frame rate (RV-2). It is the only progress data the app persists (RUD-8 plans more). Keep progress in its own file with append or atomic semantics before RUD-8 builds on the wrong foundation.

### RV-72: UI conventions are enforced by prose (S3, effort S)

`HANDOFF.md` "UI conventions" says: always use `widgets::slider`, never put captioned sliders in `right_to_left`, use `widgets::hint`, and so on. None of it is enforced. `RichText::new(..).small().color(..)` is still written inline in many places (`score_tab.rs:195-199`, `editor.rs:521-563`). `widgets::slider_with_zero` takes 7 positional parameters, including `""` captions and `1.0` steps (CD-11 declined to fix it; this review disagrees, because positional `f64` plus `&str` plus `Option<&str>` is error-prone, so use a builder: `Slider::new("zoom", &mut v, 0.5..=3.0).step(0.05)`).

### RV-73: Colour palette in `render.rs` (S4)

`GREEN`, `AMBER`, `RED` and `ACCENT` are defined in the score renderer and imported by startup, controls, settings and results. Move them to a `theme` module, so a change to the renderer does not touch half the UI and so dark/light variants have a place to live.

### RV-74: Leftover scaffolding (S4)

- `app.rs:551-555`: three empty section banners (`// --- takes`, `// --- listen`, `// --- views`) with nothing under them, left by the CD-1 move.
- Double blank lines after `use` blocks (`score_tab.rs:2-3`, `controls.rs:14-15`, `playback.rs:397`).
- `app.rs` module doc lists "Score / Metronome / Results / Settings tabs", which misses Home and Rudiments.

---

## J. Tests

### RV-80: Timing tests use the wall clock (S2, effort M)

`src/playback/tests.rs` sleeps (`thread::sleep(260/150/330 ms)`) and measures real gaps. CI runs `cargo test --release` because "the timing tests are unreliable in a debug build" (`ci.yml`). That is a flaky test suite that has been accommodated rather than fixed. The `Sink` seam exists, but the **clock** is not injected (`Instant::now()` and `wait_until` are called directly).

**Fix:** a `Clock` trait (`now`, `wait_until`) with a manual clock in tests. The scheduler then becomes deterministic and fast in debug builds.

### RV-81: Tests through `DrumApp` (S3)

Most app-crate tests construct a whole `DrumApp` (with `Paths::none()`) and drive it through private fields (`app.range_from = 1`, `app.background_bytes = 0`, `app.view.tab = …`). This is a consequence of RV-13/RV-14: logic that should be unit-tested in core is only reachable through the god object. Tests also write to `std::env::temp_dir()` with pid-based names in 9 places and clean up manually, so a panicking test leaves files behind. Use `tempfile`, or a guard type that removes on drop.

### RV-82: Missing tests for the failure paths found here

There is no test for a background load across a tab switch (RV-1), for a corrupt or truncated config (RV-2), for a failed preset write during Learn (RV-3), or for invalid scores reaching the UI (RV-5). The suite is broad on happy paths and thin on failure paths.

---

## K. Tooling and process

### RV-85: No formatter, no lint gate (S3, effort S)

- `cargo fmt --check` shows 1,116 hunks. Either adopt rustfmt (with a `rustfmt.toml` setting `max_width = 140` if long lines are intended) or document why not. Without it every refactoring diff mixes in formatting noise.
- CI runs neither `cargo clippy -D warnings` nor `cargo fmt --check`. Default clippy is not clean on the current toolchain (16 warnings), which contradicts `HANDOFF.md`'s CD-22 note.
- The developer guide extracts code by matching "the first line that contains TEXT" (`{{code:PATH::TEXT}}`), and `build_manual.py` regex-parses `rud(...)` lines out of `rudiments.rs`. Both are documented, but it means **renaming or reformatting Rust breaks the release**. Running rustfmt (above) may trip them. Generate the data (`cargo run --example dump_rudiments` → JSON), or anchor on attributes or doc markers, not on source text.

---

## L. Documentation and process debt

### RV-90: `HANDOFF.md` is too large, contradicts itself, and is mandatory reading (S2, effort M)

It is 92 KB and 433 lines with paragraphs of 200+ words. `CLAUDE.md` makes it the entry point. Contradictions found:

- Line 233 says UI-3 is done (shortcuts work while a non-text widget has focus); lines 378 and 386 still say shortcuts are ignored whenever egui has a focused widget, via `wants_keyboard_input()`.
- Line 230 says FEAT-4 is done (`listen_count_in`); line 403 says "there is no separate listen without count-in".
- Line 404: "Restarting … opens the sound device again": stale since the persistent engine (0.8.0).
- Line 15: "now 0.8.0"; the version is 0.10.1. Line 25's test counts are from 0.7.3.
- The "Architecture" block lists `loader.rs` (removed by IMP-1), `controller.rs` (now a folder), and `app.rs` as holding "Score/Results/Settings tabs".
- Lines 247 and 290 report `app.rs` at 693 lines and `controls.rs` split as done; today `app.rs` is 885 and `editor.rs` 1,257.
- `core/src/import/mod.rs:34` (`Limits` doc): "Loading runs on the window's thread", stale since IMP-8.

The project also keeps planning artefacts in `docs/` (`resume.md` is a prompt for an AI session, `name-ideas.md`, two input-format briefs, an audio plan, a latency study) next to user and developer docs, with no index saying which are current.

**Fix:**
- Split `HANDOFF.md` into `STATUS.md` (what is true now, one screen), the GitHub issues (already the source of truth for open tasks, by the document's own rule), and `docs/history/` for the done logs.
- Generate the architecture table from the module docs (the guide builder already reads them).
- Move planning docs to `docs/archive/`.

### RV-91: Comment style raises the cognitive load (S3, ongoing)

The doc comments are plentiful and often explain *why*, which is good. But the house style avoids technical vocabulary, so the comments are harder to search and less precise:

| Comment says | Technical term |
|---|---|
| "let go of off the callback" | drop / free outside the callback |
| "a number for a run" | run ID |
| "the sound card" | output device or stream |
| "what it was started with" | start parameters |
| "is told about 30 ms ahead" | look-ahead |
| "the one in use" | active |
| "worked out once" | cached |

Many comments restate the code (`/// The state of the connection, in words.` above `message: String`). Some functions carry 6–10 lines of narrative where a precise sentence plus a link would do. Combined with RV-16's naming, a reader cannot grep for standard terms such as "drop", "cache", "debounce", "callback" or "lookahead" and find the relevant code.

**Fix:** keep the "why" comments; use standard terms; delete comments that restate a name; put long rationales in the developer guide and link them.

---

## Suggested order of work

1. **Now, small, high value:** RV-1 (fix and regression test), RV-2 (atomic write, debounced save, backup on parse failure), RV-3, RV-4, RV-5 (validate in release), RV-8, RV-9.
2. **Prerequisites for every later refactor:** RV-85 (rustfmt and a clippy gate, in a separate format-only commit; fix the guide anchors if they break), RV-80 (injected clock).
3. **Structure:** RV-10 (the `Practice` struct) → RV-11 (one tempo representation) → RV-12 (one command path, no wildcard arms) → RV-17/RV-18 (outcomes, intents applied after drawing).
4. **Types:** RV-20 (units, starting at the core API), RV-19, RV-21, RV-22, RV-23 (one at a time; the compiler does the work).
5. **Consolidation:** RV-40 (one range per setting), RV-42 (scoring model), RV-30 (CD-5, with the injected clock from step 2 as the safety net), RV-31, RV-32.
6. **Boundaries:** RV-14 (move domain code to core), RV-70 (`PresetStore`, learn state machine in core), RV-16 (renames).
7. **Docs:** RV-90, RV-91.

## What is good and should stay

- The core crate builds and tests without GUI, MIDI or audio crates, and the tests are fast (163 core tests in well under a second).
- The audio engine design: a frame-accurate start, the clock estimator with its drift tests, the wait-free queue, the graveyard for freeing off the callback, a `Sink` trait with a fake. These are the best-engineered parts of the codebase, with property-style tests (`sounds_start_where_the_program_clock_says_whatever_the_callbacks_do`).
- The import pipeline (`sniff → container → parse → finish`, neutral `RawScore`, `Limits`, snapshot tests, malformed-input tests).
- Table-driven `ACTIONS` and `KEYS` (CD-3, CD-4): keep them, but finish the job (RV-12 for exhaustiveness, RV-40 for ranges).
- Injected `Paths` (CD-15): no test can touch the owner's files.
- Test names that read as specifications.
- No `unsafe` and a single `unwrap()` in non-test code (`score_tab.rs:212`, which RV-5 would make unnecessary).
