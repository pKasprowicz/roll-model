# Defect catalogue

The classes of defect the first review of Roll Model found (`docs/code-review-2026-11-01.md`, RV-1 to RV-91), as questions to put to *any* codebase of this kind: a Rust desktop app with a GUI thread, device I/O, real-time audio and a separate pure core crate. For each class: what to look for, how to find it, what it looked like here, and the fix that worked. "Scan" names the check of `scripts/scan.py` that finds candidates; every candidate is read by a person before it becomes a finding.

Severity: **S1** wrong behaviour now or on the next change · **S2** structural debt that makes changes costly or risky · **S3** cognitive load, readability · **S4** polish. Effort: **S** hours, **M** a day or two, **L** several days.

## A. Defects (look for these first; they are the S1s)

| Class | Look for | Find it by | Seen as | Fix that worked |
|---|---|---|---|---|
| **Wrong-slot state** | State that belongs to one mode written while another mode is active; a swap of fields done by hand | Trace what every `poll_*`/`install`/`apply` writes to and *when* relative to the swap. Write a throwaway test that reproduces the order and **run it** | RV-1: a background file load landed in the rudiment's slot | Model the state (a struct per mode, swapped whole: RV-10); activate the right mode first; regression test |
| **Write storms and torn files** | A persist call every frame or every drag step; `fs::write` straight onto the target; a failed read that falls back to defaults silently | Scan: `fs::write`. Read the save path, the load path and what a parse failure does | RV-2, RV-4: settings written 60 times a second, truncated file reset everything | Write a temp file and rename; save on settle (debounce, `drag_stopped`); keep a `.bad` copy when a file cannot be read; say so on screen |
| **Success reported after failure** | A flow that moves on although the step before returned an error or was ignored (`let _ =`) | Scan: `let _ =` on I/O. Follow each editor/learn flow's error path to the screen | RV-3: Learn went to the next control after the preset failed to save | Return a `Result`, stop the flow, say why |
| **Invariants only in debug** | `debug_assert!`; checks behind `cfg!(debug_assertions)`; UI that indexes as if invariants held | Scan: debug-only checks, `unwrap`/`expect`, indexing `[i]` on data from files | RV-5 | Validate at the boundary in release builds (`from_parts` + `check_invariants`); no indexing without a guarantee |
| **Type confusion at a border** | A channel used as a note, per cent as BPM, an index as an ID | Read every conversion between two meanings of the same primitive | RV-6, RV-11 | One representation; make the other impossible to construct |
| **Unbounded queues** | `Arc<Mutex<Vec<_>>>` filled by a device thread, drained by a frame | Scan: shared Vec queues; check each for a cap and for what happens when the UI stops repainting | RV-7 | Cap it (drop the newest), count what was dropped |
| **Inconsistent lock policy** | Poisoned mutex handled differently per site (`unwrap`, `into_inner`, mapped to an unrelated error) | Scan: lock sites by policy | RV-8 | One helper (`sync::locked`) |
| **Hand-rolled formats** | CSV/JSON/text written by `format!` without quoting or escaping | Grep exporters | RV-9 | A quoting function with a test of hostile input |

## B. Architecture

- **God object.** One struct with dozens of fields touched by every file; "split" by moving `impl` blocks into files, not by responsibility. *Scan:* largest files, field counts. *Fix:* extract state types with private fields, methods and their own tests (`Practice`, `Playback`, `MetroState`, `MidiState`, `Take`); the app keeps handles to them. (RV-13)
- **Two command paths.** Keyboard and controller (or menu) each with their own rules; `_ => {}` arms; match guards with side effects (`if self.unlocked()`). *Scan:* wildcard arms, calls in guards. *Fix:* one `press/turn/set`, the lock asked once, no wildcard arms. (RV-12)
- **Domain logic in the GUI crate; an impure core.** Logic that could be tested without a window sitting next to egui; the "pure" crate pulling in `Instant::now`, threads, device crates. *Scan:* impure things in the core directory. *Fix:* move it down, with its tests. (RV-14, RV-15)
- **Names that mislead.** The same word for unrelated things (`State`, `Control`, `Report`, `Source`/`Src`); one thing with three names. *Find by:* grep the type names; list the pairs a newcomer would confuse. *Fix:* rename (the compiler does it) and fix the doc anchors. (RV-16)

## C. State, semantics, hidden coupling

- **Return channel by side effect.** A function that "returns" by writing a status line or error field. *Scan:* writes to `status`/`error`/`notice`. *Fix:* return an outcome (`TakeOutcome`, `Result<Connected, _>`); the caller decides what to show. (RV-17)
- **Drawing that mutates the domain.** `draw_*(&mut self)` that changes the model while painting. *Scan:* drawing functions taking `&mut self`. *Fix:* collect intents (`Op`) while drawing, apply them once after. (RV-18)
- **Boolean and illegal-state soup.** Flags and `Option`s that must agree (`running`, `finished`, `count_in`...). *Find by:* a struct whose constructor needs a comment about which fields go together. *Fix:* an enum of states, `Outcome::{Pending, Graded}`, `Kind::Grace { order }`. (RV-19)

## D. Types and units

- **Units in names, not types** (`dt_ms: f64`, `t_sec`, `tempo_pct`). *Scan:* unit-suffixed primitives. *Fix:* `Quarters`, `Seconds`, `Millis` with arithmetic only within a unit and one bridge (`TempoMap`). Start at the core API. (RV-20)
- **Anonymous tuples as domain types** (three or more fields). *Scan.* *Fix:* a named struct where it is read. (RV-21)
- **Boolean parameters; 6+ parameters.** `f(x, true, false)` at call sites. *Scan.* *Fix:* a two-variant enum, a parameter struct. (RV-22)
- **Stringly keys and sentinels** (`100 + lane`, `usize::MAX`, `""` for none). *Fix:* `Option`, enums. (RV-23)
- **Public fields everywhere.** Anyone can break an invariant. *Scan:* `pub` fields. *Fix:* private fields, accessors, a validating constructor, doc-hidden `*_mut()` for tests. (RV-24)

## E. Magic numbers and several sources of truth

- **A range defined in three or four places** (config clamp, slider range, controller normalisation, hover text) that already disagree. *Scan:* `clamp(` literals next to slider ranges. *Fix:* one `Span` per setting that every layer reads. (RV-40)
- **Persisted enum indices.** A settings file that stores the position in a list; reordering breaks old files. *Scan:* lookup by position, `as usize` on enums. *Fix:* store the name; read the old number too. (RV-41)
- **Inline arithmetic for a model** (grading weights, penalties). *Fix:* a model struct with documented fields and `Default`, one golden test per fixture. (RV-42)
- **Geometry and layout literals** in drawing code. *Scan:* literals by file. *Fix:* named constants for the ones that carry meaning; accept the rest and say so. (RV-43)

## F. Duplication

- Two schedulers/loops/openers doing the same job with different bugs (RV-30); a debounce or retry written twice (RV-31); copies of a helper (median, gain rule, connect text) (RV-32). *Scan:* repeated blocks. *Fix:* one implementation behind a small trait with a fake for tests; do the big merge only with a safety net (injected clock) and the hardware checks it needs.

## G. Error handling

- **`Result<_, String>`** where the caller wants to match; **silent failure as the default** (a thread that cannot start, a device that cannot open, an error counted nowhere). *Scan.* *Fix:* typed errors; count failures where the readout is; log to a file and say how many more there were. (RV-50, RV-51)

## H. Performance and real-time

- **Per-frame recomputation** of pure derived data (grading the whole take, layout, notation analysis, a file read and parse inside the draw). *Scan:* allocation-heavy per-frame functions; read `update`/`draw` for I/O. *Fix:* cache keyed by a generation counter; do it on change, not every frame. (RV-60)
- **Blocking work on the UI thread** (device enumeration, `join`, file reads). *Scan:* blocking-looking calls in UI directories. *Fix:* a worker with a channel. (RV-61)
- **Allocation, locking or freeing inside the audio callback.** *Scan:* allocation in the audio directories; then read the callback. *Fix:* pre-allocate, wait-free queue, free off the callback (a graveyard). Needs the hardware to verify. (RV-62)

## I. UI-layer smells

- The largest file is a window or an editor; state machines inside draw code (Learn/Detect) (RV-70). User data in the settings file (RV-71). Conventions kept by prose, not by a test (a lint test that fails when a slider is built by hand) (RV-72). A colour palette scattered through the renderer (RV-73). Leftover scaffolding, `allow(dead_code)` (RV-74).

## J. Tests

- **Wall clock in timing tests** (flaky, slow): inject a clock (`ManualClock`). (RV-80)
- **Tests that must build the whole app object**: a sign that the logic is not separable. (RV-81)
- **Missing tests for the failure paths** the review found; write the failing test first, for every S1. (RV-82)

## K. Tooling and process

- No formatter, no lint gate, 1,000+ diff hunks, clippy warnings that the docs claim are zero. Make `fmt --check` and `clippy -D warnings` CI gates, in a format-only commit first. (RV-85)

## L. Documentation and process debt

- The mandatory-reading task file is huge and contradicts itself (versions, sizes, removed modules). Split into *status* (what is true now), the issue tracker (what is open) and *history* (what is done); generate what can be generated. (RV-90)
- **Comment style.** Comments that restate the name; house vocabulary where the standard term (drop, cache, debounce, callback, look-ahead) would be searchable. Keep the *why*; delete the *what*. (RV-91)

## What was good (keep a list like this in every review)

Core builds and tests without GUI/MIDI/audio crates; the audio engine's design with a fake sink and drift tests; the import pipeline with snapshot tests; table-driven actions and keys; injected paths so no test touches the owner's files; test names that read as specifications; no `unsafe`. A review that lists only faults gets the good parts "fixed".
