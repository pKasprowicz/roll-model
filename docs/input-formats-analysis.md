# New score input formats: analysis and rework concept

Status: decisions taken 2026-11-01; plan awaiting the owner's go. Nothing is built.
Background and theory for each decision: [input-formats-decision-brief.md](input-formats-decision-brief.md) (written before the scope was narrowed; the Guitar Pro parts of it are now history).

## 0. Decisions taken

| # | Decision | By |
|---|---|---|
| 1 | The only new format is the **Standard MIDI file**. Files are meant, not devices. | owner |
| 2 | **Full rework of the import path** now, so that a further format later is one parser and nothing else. | owner |
| 3 | MIDI sources are **two populations**: programmed loops on a grid and human-played recordings. Both must work. | owner |
| 4 | The drum track is chosen **automatically**: all percussion merged, what is not a drum counted and reported. | owner |
| 5 | The MIDI decoding is done by the **`midly`** crate. | owner |
| 6 | Other formats (Guitar Pro, MuseScore, Sibelius, Dorico, Finale, ...) are reached **through the export to MusicXML that those programs provide**. A converter in the program is a later option, not part of this work. | owner |

Consequences already drawn: no Guitar Pro or ASCII-tab parser, no `guitarpro` crate, no part picker in the first version, and the quality of the MusicXML importer against other programs' exports becomes a deliverable of its own (section 6, IMP-5).

## 1. What exists

```
path --> loader::load_score --> parse_musicxml --> Score --> notation, grading, playback, click track, render
          (bytes, "PK" = .mxl)   (XML to RawMeasure,
                                  repeat unrolling,
                                  tempo map, sort, dedup)
```

`Score` (`core/src/score.rs`) is the single seam: a flat, time-ordered list of onsets (`ScoreNote`), the `Measure`s, the rests the file prints, a `TempoMap` and `warnings`. **Everything downstream reads only `Score`.** The program's own scores (rudiments, the metronome bar) already build one without a file.

Facts that matter:

- **A note has no duration.** `notation::analyse` reads beams, flags and tuplets from where onsets fall in the beat, so onsets alone are enough, provided they sit on exact positions.
- **A score is already unrolled**: repeats and endings are expanded while parsing (`playback_order` over `RawMeasure`).
- **Lane identification is format-neutral**: `drums::gm_lane`, `lane_from_name`, `lane_from_staff`, `Lane::default_staff`.
- **The invariants** (notes sorted by `q`, valid `measure` indices, tempo map from q = 0, near-duplicates removed, warnings for what was guessed) are enforced at the end of `parse_musicxml`, not in a shared place.
- `open_file` (`src/app.rs`) runs on the UI thread; the hardware controller can also open scores (`src/app/controls/apply.rs`), so an import cannot depend on a dialog.
- `examples/` holds one MusicXML file and `musicxml.rs` four unit tests.

## 2. Where the code is married to MusicXML

| Where | What |
|---|---|
| `loader::load_score` | calls `parse_musicxml` directly; "PK" means `.mxl` |
| `musicxml.rs` | `RawNote`, `RawRest`, `RawMeasure`, `playback_order` and the whole back half are private to it |
| `ParseOptions`, `ScoreError` | named and shaped for the XML parser (`Timewise`, `NotMusicXml`) |
| `drums::NoteHead::from_musicxml`, `staff_step(step, octave)` | MusicXML vocabulary in `drums.rs`; only MusicXML calls it |
| `app.rs` | dialog filter `["musicxml", "xml", "mxl"]`, button "Open MusicXML…", the drop handler opens whatever is dropped |
| `home.rs` hint, README, manual | text |
| 47 uses of `ParseOptions`, `ScoreError`, `LoadError`, `loader::`, `parse_musicxml` in 5 files | `core/src/musicxml.rs`, `core/src/loader.rs`, `core/tests/practice_session.rs`, `src/app.rs`, `src/playback/tests.rs` |

Not married to it and reusable as they are: lane mapping, `Score`, notation, grading, the settings keyed by path (layouts, recent files).

## 3. The rework

The import becomes four explicit stages. A format is a *parser* into a neutral intermediate form; everything else is shared.

```
bytes --> sniff --> unwrap --> parse --> finish --> Loaded { Score, Report }
          Format    container  per format  shared
```

```
core/src/import/
  mod.rs        ImportOptions, Limits, ImportError, Loaded, Report, load_score, EXTENSIONS
  sniff.rs      Format, Detected, sniff(bytes, name)
  container.rs  .mxl unzip, text decoding (moved from loader.rs)
  ir.rs         RawScore, PartInfo, RawMeasure, RawNote, RawRest      (neutral intermediate form)
  finish.rs     unroll repeats, build Score, tempo map, sort, dedup, invariants
  musicxml.rs   XML -> RawScore   (today's parser, front half only)
  midi.rs       SMF -> RawScore   (midly; drum track choice; velocity to accent and ghost)
  quantise.rs   pure: onsets in ticks -> exact positions in beats, with a report
core/src/loader.rs   removed (callers move to `import`)
```

### 3.1 Contracts

- **`sniff(bytes, name) -> Detected`** decides by content first, then extension: `MThd` is MIDI; a zip holding `META-INF/container.xml` is `.mxl`; text beginning `<` is MusicXML. `Detected::Known(Format)`, `Detected::Recognised(&'static str)` for formats we know but do not read (Guitar Pro, MuseScore, Sibelius, Dorico, Finale: magic numbers and extensions), `Detected::Unknown`.
- **`RawScore`** = `{ title, part_name, parts: Vec<PartInfo>, tempos, measures: Vec<RawMeasure>, warnings }`. `RawMeasure` is today's struct (number, length, time signature, notes, rests, repeat flags, endings). The parser fills it; nothing in it is format-specific.
- **`finish(RawScore, &ImportOptions) -> Score`** is today's back half moved intact: `playback_order` (repeats), `Measure` and `ScoreNote` creation, tempo map and the "no tempo, 120 BPM" warning, sort, dedup, "no drum notes". It ends with a check of the `Score` invariants (`debug_assert` in builds, a test helper everywhere else).
- **`ImportOptions`** (`#[non_exhaustive]`): `expand_repeats`, `part: Option<usize>` (unused by the UI at first, but in the API so a picker costs one control later), `limits: Limits`, `midi: MidiOptions`.
- **`Limits`**: maximum file size, events, notes, measures; enforced by each parser; exceeded gives `ImportError::TooLarge`. Reason: loading is synchronous on the UI thread and resource exhaustion is not prevented by Rust's memory safety.
- **`ImportError`** replaces `LoadError` and `ScoreError`: `Read`, `Unrecognised`, `Recognised { format, hint }` ("export it as MusicXML from …, see the manual"), `TooLarge`, `Container`, `Parse { format, reason }`, `NoDrumPart`, `NoMeasures`. The texts of today's MusicXML errors are kept word for word.
- **`Loaded { score, report }`**: `Report` is structured provenance for the status line and for the "adjust after load" control: format, parts found and used, and for MIDI the grid, the largest shift in ms and the number of notes moved by more than a set amount. `Score` itself is not extended, so rudiments and the metronome bar stay as they are.
- **`EXTENSIONS`** is the one list behind the dialog filter, the Home hint and the drop handler. The button reads "Open score…". An unknown dropped file shows a message; a *recognised* one shows its export hint.

### 3.2 How a later format fits

- A reader of another format (Guitar Pro, say) is one more `parse` that returns a `RawScore`. It does **not** have to emit MusicXML first, which is simpler and loses nothing. That is the "converter module" of decision 6, if it is ever built.
- A part picker needs only `ImportOptions::part` and `Report::parts`, both present from the start.
- Loading on a background thread needs only that the pipeline is a pure function of bytes and options (it is), plus a "loading" state in the app. It is a separate task (IMP-6).

### 3.3 What does not change

`Score`, `notation`, `analysis`, playback, the render, the settings keys. The public file formats the README promises keep working unchanged.

## 4. MIDI import design

### 4.1 Reading

- `midly` decodes the file; formats 0 and 1 are read, format 2 and SMPTE time division give a clear error. A note-on with velocity 0 is a note-off, not an onset. Note-offs are otherwise ignored (the score has no durations).
- **Tempo** from set-tempo events, **time signature** from time-signature events, kept in ticks; missing ones default to 120 BPM and 4/4 with a warning.
- **Drum events**: channel 10 notes, merged across tracks; if none, tracks named like a drum part (drum, perc, kit); if none, the channel with most notes in the General MIDI range 35 to 81; otherwise `NoDrumPart`. Keys map with `gm_lane` (not the user's module map, which is for input); unknown keys become `Lane::Other` and are counted in a warning.

### 4.2 Quantising (both populations)

Pure code in `quantise.rs`, working in integer ticks so that results are exact ratios, never accumulated floats.

1. **Offset.** For human-played files estimate one constant offset (the median distance of onsets from the best grid, for example a recording with a fixed latency) and remove it before snapping. A programmed loop gives zero.
2. **Per-beat grid.** For each beat choose the subdivision that fits all its onsets within tolerance, from the preference order straight 2, 4, then triplet 3, 6, then 8; ties go to straight. Per beat, not per file, so a 16th groove with a triplet fill in the last beat works; the notation module already detects tuplets per beat. A file-level **finest grid** option (1/8, 1/16, 1/32) caps the search.
3. **Tolerance** is the smaller of a fraction of the grid cell and an absolute time in ms, converted through the tempo map at that point.
4. **Simultaneous onsets**: onsets of different lanes closer than a small window snap to the same point (a hit with two pads).
5. **Report**: the grid used per beat (summarised), the largest shift in ms, the count of notes moved by more than the tolerance, and a **free-time warning** if shifts grow along the file (a linear trend of the residual against time means the tempo drifts, which the program does not repair).
6. **Not done**: tempo tracking and beat tracking (a recording made without a click); turning flams into grace notes (two onsets of one lane closer than the window collapse into one and are counted).

### 4.3 Accent and ghost from velocity

Relative to the lane, not absolute (a quiet file has no accents under fixed thresholds): a note at or above a set ratio of the lane's median velocity is an accent, at or below a lower ratio a ghost. Ratios are in `MidiOptions` with defaults to tune on real files.

### 4.4 Adjusting after load

The load is automatic. A small control beside **Layout** on the Score tab (grid: auto, 1/8, 1/16, triplets; "first bar starts at" shift in whole beats; velocity rule) re-runs the import on the same file with new options. The import is pure and takes milliseconds, so this needs no dialog and no state beyond the options kept with the loaded score. The controller's hands-free open path is unaffected.

**Determinism.** Layouts are stored per path; a change to the quantiser can change a file's bar count. The quantiser's output is a versioned contract: golden tests, and a CHANGELOG line when it changes.

## 5. Tests

- **Characterisation first (IMP-1a):** Debug snapshots of the `Score` from `basic_rock_beat.musicxml` and from a few small hand-written MusicXML files (repeats and endings, tempo change, voices and rests, tuplets, grace notes). They must be identical after the move. This also closes the thin coverage of the parser.
- **`assert_score_invariants(&Score)`**: sorted, valid measure indices, notes inside their bar, tempo from q = 0. Run on every format's fixtures and on the rudiment scores, which have no such check today.
- **Cross-format check:** `examples/basic_rock_beat.mid`, the same groove as the MusicXML sample, must import to the same lanes and positions.
- **Quantiser:** deterministic randomised tests (a fixed seed, no new dev-dependency): a grid pattern with added jitter comes back exact; no note moves by more than half a cell; the result is idempotent; a triplet beat is not read as sixteenths.
- **MIDI fixtures are built in the test code** by a small writer, so no binary files with unclear origin enter the repository; the example `.mid` is generated from the same data.
- **Malformed input:** truncated files, huge length fields, zero-length tracks, running status across meta events: error or limit, never a panic or a hang.

## 6. Tasks

| Task | What | Done when |
|---|---|---|
| IMP-1 | **Seam extraction.** (a) snapshot and invariant tests; (b) move the raw types and the back half to `import/ir.rs` and `finish.rs`; (c) `import/mod.rs` with the options, errors, `Loaded`, `Report`, container and `sniff`; migrate the five files that use the old names; drop `loader.rs` | All existing tests pass; the snapshots of (a) are identical; errors read as before |
| IMP-2 | **Detection and UI.** `EXTENSIONS`, "Open score…", the drop handler's messages, recognised-but-unsupported hints, README and manual text | An unknown file gives a clear message; a Guitar Pro or MuseScore file says to export MusicXML; `.musicxml` and `.mxl` still open |
| IMP-3 | **MIDI reader.** `midly` (licence entry in `THIRD_PARTY_NOTICES`), drum part choice, tempo and time signature, `Limits`, the malformed-input tests | A DAW groove opens with the right lanes, tempo and bars |
| IMP-4 | **Quantiser and velocity.** `quantise.rs`, the offset, the per-beat grid, the report, accents and ghosts, the `.mid` sample, the cross-format test | Programmed loops come back exact; a jittered file comes back on the grid with a report |
| IMP-5 | **Quality of the export route.** Collect our own exports of a drum part from MuseScore 4, Guitar Pro 8, Sibelius, Dorico and others the owner has; fix what fails in the MusicXML importer; list the tested programs in the manual | Each listed program's export opens with the right lanes, repeats and tempo; the list states which version was tried |
| IMP-6 | **Adjust after load** (the Layout-style control, options kept with the score) and, separately, background loading | The grid can be changed on an open MIDI score without reopening it |
| IMP-7 (parked) | A converter reading other formats directly into `RawScore` | Only if IMP-5 shows the export route failing for the owner's files |

Order: IMP-1, IMP-2, IMP-3, IMP-4, then IMP-6 and IMP-5 (IMP-5 can start any time after IMP-1, since it only touches the MusicXML front end and its tests). A release after IMP-4 and IMP-6 would be a minor version (0.10.0).

Housekeeping when it starts: `CLAUDE.md` lists the modules of the core crate (`loader`, `musicxml`, ...); update it. HANDOFF gets the tasks and each gets a GitHub issue.

## 7. Quantiser settings, decided 2026-11-01

These follow from decision 3 (both populations) and are settings of the quantiser rather than architecture:

1. **Click**: stay with detecting and warning only. A free-time recording (the tempo map does not match the playing) gets a warning; no beat tracking.
2. **Constant offset**: removed automatically.
3. **Flams**: collapse into one note and count them; grace notes may come later.
4. **Velocity rule**: relative to the lane's median.
5. **Count-in or pickup bars**: manual only. The adjust-after-load control gets a "first bar starts at" setting in whole-beat steps; it moves bar lines, not notes (the first bar becomes a short bar, like a MusicXML pickup). The shift is refused when a time-signature change follows. There is no automatic trimming of empty bars and no pickup hint in the report.
6. **Recognised-but-unsupported hints** (IMP-2): a dropped Guitar Pro, MuseScore or Sibelius file says "export as MusicXML" instead of "not a MusicXML score".

To check in IMP-1: `Score::click_track` (`core/src/score.rs`) emits `m.beats` clicks per measure, so a short first bar would get a full bar of clicks. Add it to the characterisation tests and fix it before the shift depends on it.

Not verified: how `midly` reacts to malformed files (the malformed-input tests in IMP-3 are what decides whether limits can be enforced around it or the reader must be hand-written); real MIDI drum files from the owner; the quality of other programs' MusicXML drum exports.
