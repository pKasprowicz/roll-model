# New score input formats: decision brief

Companion to [input-formats-analysis.md](input-formats-analysis.md) (what exists, the rework, the order of work). That document says *what* to build; this one gives the background for the five decisions in its section 6. Draft, 2026-10-31.

**Status 2026-11-01:** the owner has decided the scope (MIDI is the only new format; other formats go through their programs' export to MusicXML; `midly` for decoding; automatic drum track; full rework of the import path). The decisions and the resulting plan are in the analysis document. This brief is kept as background; the Guitar Pro material in it (decision 4, the `guitarpro` crate, the Guitar Pro spike) no longer applies.

Conventions: **Fact** is checked against the code or against crates.io on 2026-10-31. **Claim** is what a README or a paper says and I have not tested. **Judgement** is mine.

## 0. The one idea behind all five decisions

A score file is a *description of music*, and the program needs a much smaller thing: a list of onsets with a bar grid and a tempo map (`Score`). Each format sits at a different distance from that:

| Family | Examples | What the file stores | Distance to `Score` |
|---|---|---|---|
| Notation | MusicXML, Guitar Pro, MuseScore | notes with written durations, voices, bars, repeats, tempo marks | **Short.** Throw information away (durations). No guessing. |
| Performance | Standard MIDI file | events in time (ticks), a tempo track, maybe a time signature | **Long.** Information must be *invented*: where the bars and beats are, which grid the notes belong to, which are accents. |
| Grid / tab | ASCII drum tab, Guitar Pro's drum lanes | symbols at grid positions | Short, but the grid is implicit in the text columns. |

This gives a rule that decides most of what follows: **the cost of a format is parser cost plus inference cost, and the two are inversely related.** MIDI is cheap to read and expensive to interpret. Guitar Pro is expensive to read and cheap to interpret. Notation files are lossless for our purpose; a MIDI import is a *model* of the performance, and every model has a failure mode to decide about in advance.

Two facts about the program raise the stakes for inference (Fact):

1. **The score is the reference for three things at once**: what Listen plays, what the notation shows, and what a take is graded against. A wrongly quantised note is therefore heard, shown and graded wrong, consistently, with nothing to tell the player that the file said otherwise.
2. **Scores can be opened without the mouse**: the hardware controller can step through a list of scores and open one with an encoder push (`src/app/controls/apply.rs`). And `open_file` runs on the UI thread. So an import must be non-interactive by default and must be fast and bounded.

---

## Decision 1. Which formats, in what order

### Background

**What a drummer practising from a file wants** differs by source:

- *Tab collections* (Guitar Pro `.gp3` to `.gp8`, the bulk of what is shared online for songs): written by hand by fans, durations and repeats present, drum kit mapped by MIDI key. Highest "I want to play that song" value. Today the README sends these users through an export to MusicXML, which works but is a barrier (the owner needs Guitar Pro or MuseScore to convert).
- *Groove and fill packs* (MIDI, often made for DAWs): hundreds of short patterns, mostly programmed on a grid. Highest value for *pattern* practice, which is what a rudiment and groove trainer does.
- *Engraved scores* (MusicXML from MuseScore, Sibelius, Dorico): already supported.

**Why not "everything"?** Each format becomes part of the public interface (the README lists "the MusicXML features listed above" as interface and SemVer applies). A format that opens "most files" is a promise that gets bug reports for every dialect. So the real cost of a format is its *tail* of edge cases, not its first working version.

**Reversibility (Judgement).** Adding a format is cheap to start and cheap to drop *if* it is isolated behind the import seam and behind a cargo feature. Removing a format users rely on is not cheap (a breaking change, a minor bump while version is 0.x). So order by *value divided by dialect risk*, and put the most uncertain format behind a feature flag.

### Options

| Option | Content | Effect |
|---|---|---|
| A | Seam only (IMP-1, IMP-2), no new format | Pure refactor; users see "Open score…" and clearer errors; nothing new to open |
| B (recommended) | A, then MIDI | Largest value for the lowest dialect risk (SMF is frozen since 1988) |
| C | B, then Guitar Pro 3 to 5 and 7/8 | Unlocks the tab world; the biggest effort and the biggest tail |
| D | C plus ASCII tab | Quick pasted grooves; arguably a typing feature, not a file format |

### What would change my mind

- If you rarely have tab files and mostly practise grooves: B is enough, skip C.
- If you mostly learn songs: C moves first (but B's quantiser is not needed for it; GP has real durations).
- Evidence cheap to get: take ten files you would actually want to open and sort them by format. The ranking falls out.

---

## Decision 2. Quantisation: automatic, dialog, or something in between

### Theory: what quantisation is

A MIDI file gives onset times t_i in ticks. The program needs positions in exact quarter-note fractions. **Quantising** means choosing a grid (a period g, possibly several families such as straight and triplet) and replacing each t_i by the nearest grid point. Three things make it a real problem, not a rounding step:

1. **Model selection.** A finer grid always fits better: a 1/64 grid explains *every* human performance and produces unreadable notation. So the choice needs a penalty for grid complexity: the standard answer is "the coarsest grid that explains the data within tolerance" (the same idea as minimum description length: prefer the simplest model that fits). This is the approach in the analysis document.
2. **Ambiguity between grids.** At 120 BPM a quarter is 500 ms. A note at 1/4 of a beat is at 125 ms after the beat; a triplet note at 1/3 is at 167 ms. They differ by about 42 ms, which is of the order of human timing scatter (a good drummer is commonly quoted at some 10 to 30 ms standard deviation; Claim, from memory). So a *single* note cannot be classified as 16th or triplet. The evidence has to be collected over a whole beat: all onsets inside one beat must belong to one subdivision family. The program already thinks this way: `notation::analyse` groups by beat and detects tuplets per beat.
3. **Swing and feel.** Swung eighths sit near 2:1 (triplet) but swung sixteenths and "laid back" playing sit between grids. Writing them as straight notes is the usual convention (the feel is a performance attribute); writing them as triplets is right only for true shuffles. A file cannot tell you which. This is the one place where any automatic choice will be *wrong for some files*, and it should be a documented, overridable default (straight unless a beat clearly fits triplets).

Related literature (Claim, cited from memory, not re-read for this brief): Longuet-Higgins's work on tempo tracking and quantisation (*Mental Processes*, 1987); Desain and Honing, "The quantization of musical time: a connectionist approach" (Computer Music Journal, 1989); Cemgil and Kappen, "Monte Carlo methods for tempo tracking and rhythm quantization" (JAIR, 2003). They all treat tempo drift and quantisation as one joint problem. **Judgement: we should not implement tempo tracking.** The sources a practice program meets are either programmed on a grid (no drift) or recorded against a DAW click (the tempo map is right). Free-time recordings are out of scope; the importer should *detect* them (large residuals) and say so, not try to repair them.

### What is cheap to compute and gives the user evidence

Two numbers, shown in the load message with no dialog:

- the **grid chosen** ("quantised to sixteenths"),
- the **largest shift** in milliseconds and how many notes moved by more than a fraction of the grid ("max shift 9 ms; 2 notes moved more than 25 ms").

A residual that is large relative to the grid is the signal for "this file is not on a grid; the result may be poor". That is an honest uncertainty indicator for the price of two floats.

### Options for the control

| Option | How | For | Against |
|---|---|---|---|
| A. Automatic, warn | Pick the grid, show it in the status and warnings | Works with drag-drop and with the controller's hands-free open; deterministic; least code | A wrong guess is only visible if the player reads the message |
| B. Import dialog | Modal: grid, track, velocity thresholds, with a preview | Every choice is explicit | Cannot be driven from the encoder; interrupts drag-drop; needs UI, state and tests; every load of a simple file pays the interruption |
| C (recommended). Automatic, then adjustable after load | Auto at open; a small control next to Layout ("Grid: auto / 1/8 / 1/16 / triplets") re-imports in place | Non-modal; same pattern as the existing Layout control; the import is pure and takes milliseconds, so re-running it on a parameter change is cheap; the hands-free path is unaffected | Needs the load options kept with the score (small) and a place in the UI |

**Why not B (Fact-based).** The controller-driven open and drag-drop are existing, tested ways in. A dialog would be a second, interactive path that those cannot use. C gives the dialog's power after the fact with none of its blocking.

### Consequences to accept

- **The quantised version is the reference.** For *pattern practice* that is what you want. If the aim is to reproduce a recording's feel, a quantised reference is stiffer than the source; the program has no feature for that and should not pretend to. State it in the manual.
- **Determinism.** Layouts are stored per file path (`bars_per_row`). If a later version changes the quantiser, the bar count of an old file can change and the stored layout can mismatch. Treat the quantiser's output as a versioned contract: golden tests, and a changelog line when it changes.

### What would change my mind

- If most of your MIDI files are perfectly quantised DAW loops, the control in C is rarely needed and A suffices at first.
- If your files are mostly human recordings, the quantiser carries the whole feature and deserves more design (and C becomes mandatory, not optional).
- Cheap evidence: run a throwaway script over ten real MIDI drum files and print the chosen grid and the residuals. If nine fit a 16th grid within 5 ms, A is enough.

---

## Decision 3. Which track or part to use

### Theory: how a MIDI file says "this is the drums"

Standard MIDI Files hold events in tracks; format 0 has one track with all channels, format 1 has several that play together. General MIDI reserves **channel 10** (index 9) for percussion, and defines keys 35 to 81 (General MIDI level 2 extends the range to 27 to 87). That is a convention, not a guarantee; a drum part can sit on another channel with a percussion bank (GS and XG bank-select messages), or a "drum track" can be named only in a text event. A reasonable detection order (Judgement):

1. notes on channel 10;
2. else a track whose name contains drum, perc or kit;
3. else the channel whose notes fall mostly in 35 to 81 with the highest density;
4. else a clear error ("no drum track").

Two situations are genuinely ambiguous, and the same two occur in Guitar Pro:

- **Several percussion tracks** (kick, snare and hats on separate tracks, as in stem packs; or a drum kit plus a shaker/tambourine track). *Merge* is right for the first case and wrong for the second.
- **A multi-part MusicXML file.** The existing parser already picks one part (the one with unpitched notes or a percussion clef) and emits a warning "File has N parts; using X". **Fact: today's behaviour is automatic selection with a warning and no way to change it.**

### Options

| Option | Behaviour | Note |
|---|---|---|
| A (recommended to start). Automatic, merge all percussion, count what is `Other` | Consistent with MusicXML; works hands-free | A shaker merged into the kit shows as `Other`; the warning counts them |
| B. Ask at open | Dialog with the parts | Same objections as in decision 2 |
| C. Automatic now, a part picker later for all formats | `ImportOptions.part: Option<usize>`; the loader returns the list of parts it found | Costs almost nothing *if IMP-1 designs for it* (return the part list, accept a part index); adds a UI control only when someone needs it |

**Judgement:** A with the hook of C. The cost of the hook is two fields in the import result and options; adding it later means touching every parser.

### What would change my mind

If your files commonly have separate stems you would want to practise one at a time (hi-hat only), then the picker (C) becomes the feature.

---

## Decision 4. Parse by hand or use crates

### Theory: what a library saves and what it does not

A format library saves the **byte-level decoding**: variable-length integers and running status in MIDI; version-dependent record layouts in Guitar Pro 3 to 5; XML schemas in GPIF. It does **not** save the **semantics**: what a Guitar Pro "percussion track" means, which MIDI key becomes which `Lane`, how a tuplet's written duration becomes `q`, how a repeat with alternate endings unrolls, what a dead note or a ghost note becomes. That mapping is most of the work for Guitar Pro whichever route you take. So the real question per format is: *is the decoding the risky part, or the mapping?*

| Format | Decoding risk | Mapping effort | So |
|---|---|---|---|
| MIDI | low: a frozen, 1988 specification; a careful parser is some 200 to 300 lines | low: `gm_lane` exists | a crate is optional |
| GP7/8 (GPIF in a zip) | low: XML, and `xml.rs` plus `zip` are already in `core` | high | hand-written is reasonable |
| GP3 to 5 (binary) | **high**: many versions and quirks, little documentation | high | a crate is attractive here |

### Safety of untrusted input (Fact about the program)

`open_file` is synchronous on the UI thread. A malformed file with a huge length field or a giant delta-time that makes a parser allocate gigabytes or loop long would freeze or kill the program. Rust's memory safety removes the *corruption* risk, not the *resource* risk. Whichever route: apply limits (file size, track length, event count, note count) and test with malformed files. A hand-written parser lets you set the limits at the source; with a crate you can only check the file's size and count the output afterwards, unless the crate offers limits (unknown).

### What I checked on crates.io (Fact, 2026-10-31)

| Crate | Version, date | Licence | Use | Notes |
|---|---|---|---|---|
| `midly` | 0.5.3, 2023-01-01 | Unlicense | MIDI decode and encode | about 546 000 downloads, about 203 000 recent, 40 KB. Last release is old, but the format is frozen, so little is left to change |
| `guitarpro` | 0.4.4, 2026-10-22 | MIT | Guitar Pro 3 to 5; GP6/7 (GPIF); MuseScore `.mscz`; MusicXML | about 5 600 downloads; repository on Codeberg; versions 0.1.0 (2022) then a burst 0.3.0 to 0.4.4 between May and October 2026. Depends on `encoding_rs`, `fraction`, `quick-xml`, `serde`, `thiserror` and **`zip` 9** |
| `score-tab` | 0.2.0, 2025-08-31 | MIT | "Guitar Pro and MuseScore files" | about 390 downloads |
| `acorde-io` | 1.3.7, 2026-10-31 | MIT or Apache-2.0 | MusicXML, MIDI, ABC, MSCZ | needs Rust 1.88; about 2 000 downloads |
| `ruxguitar` | 0.13.0, 2026-08-23 | Apache-2.0 | a Guitar Pro *player* (an application) | not a library to depend on |
| `musicxml` | 1.1.2, 2024-11-05 | MIT | MusicXML | we already parse MusicXML ourselves |

**Claim** (the `guitarpro` README, not tested by me): "safe, modular, high-performance", "100 % safe Rust", "high-fidelity" reading of GP3 to 5, and reading of GP6/7+ through GPIF. Nothing here says how it treats drum tracks, and the API is `0.x` with a recent history of rapid change.

**Licence compatibility (Judgement).** The project is PolyForm Noncommercial. MIT, Apache-2.0 and the Unlicense impose no conflict, and `tools/build_notices.py` already generates `THIRD_PARTY_NOTICES.txt` for the release archives. So licences are not an obstacle; the risks are maintenance and size.

### Risks of the `guitarpro` crate specifically

- **Young and fast-moving**: a 0.x API with seven releases in five months can break between versions; pin an exact version (`=0.4.4`).
- **Small user base**: about 5 600 downloads means few people have run it on odd files. Bugs in less common GP versions are likelier to be found by *you*.
- **Dependency weight**: `serde`, `quick-xml`, `fraction` and a second major version of `zip` (the project uses `zip` 2 for `.mxl`). Two copies of a library for one purpose is avoidable cost.
- **Scope creep**: it also reads MuseScore and MusicXML, which we do not want from it. Using only the Guitar Pro parts is possible but the crate's model (`Song`) is big.

### Options

| Option | Content |
|---|---|
| A | Hand-write everything (MIDI, GPIF, and GP3 to 5 if wanted) |
| B (recommended) | `midly` for MIDI; GP7/8 by hand on `xml.rs`; evaluate `guitarpro` for GP3 to 5 behind a spike before committing |
| C | Crates for every format |

### How to contain a crate if you take one (Judgement)

- **An adapter layer.** The crate's types never leave `import/guitarpro.rs`; the adapter converts its `Song` into `RawMeasure`s. Swapping or dropping the crate then touches one file (the "anti-corruption layer" idea from domain-driven design).
- **A cargo feature**, as `audio` already is (`--no-default-features` builds without it): the default build stays small and a broken crate cannot block a release.
- **Pin and vendor-check**: exact version, read the `unsafe` count and the parse limits once.
- **Our own fixtures only**: no copyrighted tabs in the repository.

### What would change my mind

- If the spike shows `guitarpro` reads your real GP3 to 5 drum tracks cleanly, take it (feature-gated). If it needs patching, the cost of owning a binary GP3 to 5 parser is probably higher than patching or forking.
- If `midly` panics on a malformed file in the spike, hand-write MIDI (it is small) with limits.

---

## Decision 5. Files or devices

The brief assumes **score files**. "Input" could also mean new *MIDI devices* (other e-kits, other controllers), which is a different subsystem: `trigger`, `controller`, `controller_layout`, and the note map with *Learn*. Nothing in this brief applies to it. If you meant devices, say so and I will write a separate analysis.

---

## A cheap way to decide: two spikes, about an hour each, no repository changes

Spikes are throwaway programs that cost nothing to discard and turn guesses into facts. Both can be `examples/` files that are not committed.

1. **MIDI spike.** Read ten drum MIDI files you pick with `midly`; print the track chosen, the tempo and time-signature events, the best grid and the residuals in milliseconds. *Answers:* is a grid-only quantiser enough, how often is the file not on a grid, are the heuristics for the drum track right.
2. **Guitar Pro spike.** Read five of your GP3 to 8 files with the `guitarpro` crate; print every percussion track, its notes with MIDI keys and durations, and the repeat structure. *Answers:* does the crate work on your files, how it marks drum tracks, how much mapping is left.

## Summary of recommendations

| # | Question | Recommendation | Reversible? |
|---|---|---|---|
| 1 | Formats and order | IMP-1 and IMP-2, then MIDI; Guitar Pro after the spike | yes, isolated behind the seam |
| 2 | Quantiser control | Automatic with the grid and the largest shift reported; adjustable after load (not a modal dialog) | yes |
| 3 | Track or part | Automatic, merge all percussion, count `Other`; design the part list and index into the import API now | yes, but cheapest at IMP-1 |
| 4 | Crates | `midly`; hand-written GPIF; a feature-gated adapter for `guitarpro` if the spike passes | yes with the adapter |
| 5 | Scope | Files, not devices (please confirm) | n/a |

**No-regret step:** IMP-1 (the seam extraction) is worth doing under every option: it simplifies `musicxml.rs`, adds the tests the parser lacks, and costs nothing if no format is ever added.
