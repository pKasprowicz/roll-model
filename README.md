# Drum Tutor

A Rust desktop app (egui) for practising drum parts against a score:

1. **Loads MusicXML drum scores** (`.musicxml`, `.xml`, compressed `.mxl`) and draws them on a percussion staff in the **Score** tab.
2. **Captures MIDI from a Roland V31** (or any V-Drums / e-kit) and draws every hit as a note on a second staff **directly under the score**, on the same time axis — early hits sit left of the written note, late hits to the right.
3. **Compares the performance with the score** and grades it: note accuracy, timing offset and consistency, tempo drift, wrong pads, missed and extra notes, and dynamics (accents and ghost notes).

## Build and run

Install a recent Rust from <https://rustup.rs> (tested with 1.99; 1.82 is too old for the dependencies), then:

```sh
cargo run --release                                 # start empty
cargo run --release -- examples/basic_rock_beat.musicxml
```

The binary ends up in `target/release/drum-tutor(.exe)`.

| Platform | Extra requirements |
|---|---|
| Windows | none (WinMM MIDI, WASAPI audio) |
| macOS | none (CoreMIDI, CoreAudio) |
| Linux | `sudo apt install libasound2-dev pkg-config` (ALSA, for MIDI and audio) |

Build without the sound-card metronome (drops `rodio`/`cpal`): `cargo build --release --no-default-features`. The click can still be sent to the V31 over MIDI.

Run the unit tests (XML parser, MusicXML import, repeats, tempo maps, scoring): `cargo test`.

## Connecting the V31

- Connect the module by USB (or a MIDI interface on MIDI OUT). The app auto-connects to the first input whose name contains `V31`; otherwise pick the port in **Settings → MIDI input**.
- On Windows a MIDI port can be opened by only one program at a time — close your DAW or other drum software first.
- The monitor on **Settings → MIDI input** shows each incoming note, velocity and the instrument it maps to.
- The default note map is the Roland V-Drums factory map (kick 36, snare 38/rim 40/x-stick 37, toms 48/50, 45/47, 43/58, 41/39, hi-hat 42/22 closed, 46/26 open, 44 pedal, crash 49/55 and 57/52, ride 51/59, bell 53). If your kit is set up differently, click **Learn** next to an instrument and hit the pad/zone.
- The hi-hat pedal (MIDI CC#4) decides between closed and open for hits on the closed or open hi-hat note: closed when the pedal is at least three quarters of the way down. A pedal that never reports up to 127 needs **Settings → MIDI input → Hi-hat pedal value when closed** lowered: close the pedal, hit the hi-hat and read the "pedal" value in the monitor. Until a pedal message arrives, the note number alone decides.

## Practising

1. **Open MusicXML…** (or drop a file on the window). Export the drum part from MuseScore, Guitar Pro, Sibelius, Dorico, etc. The welcome panel lists the scores you opened before (the last ten); click one to open it, `×` removes it from the list. It is shown on the **Home** tab, so it is always one click away,.
2. Pick the measures to practise (sliders, or click a measure to start there and shift+click to set the end), and the tempo (25–150 %). Every slider also follows the mouse wheel: hover it and scroll to change the value one step per notch.
3. Press **⏵ Listen** (or `L`) to hear the selected measures at the chosen tempo, with a moving playhead and the current bar and beat next to the button. Press it again, or `Esc`, to stop. See "Listening to the score" below.
4. Press **⏺ Record** (or `R`). After a count-in (0–2 bars, with click) play along. The take stops by itself at the end of the range, or press `Esc`. The click can be switched off and on while you play (`K`).
5. Notes are coloured as you play: green = perfect, light green = good, amber = OK, orange = right time but wrong pad/zone (e.g. open instead of closed hi-hat), red = missed (score) or extra (your staff). Velocity is shown as bars under your staff; hover a hit for its exact offset.
6. The **Results** tab has the full report, feedback, timing histogram, per-instrument table, session history and CSV export.

No kit at hand? Enable **Settings → General → Computer keyboard plays pads** (Z kick, X snare, C/V/B hi-hat closed/open/pedal, A S D F toms, Q W E cymbals).

## Listening to the score

**Listen** plays exactly what a take would ask of you: the selected measures, the practice tempo, and the same count-in and click. Accents are louder than normal strokes, ghost notes softer, and grace notes land just before the beat.

- **Computer sound output** (default): a small built-in drum kit, synthesised on the fly. No sample files and no hardware needed. Volume is in **Settings → Sound → Listening**.
- **MIDI output**: tick *Send the notes to the MIDI output* and the module plays the notes with its own sounds. Each instrument is sent as its standard General MIDI/Roland note (or the note you mapped to it), on the chosen channel (default 10), to the output port chosen under **Settings → Sound → MIDI output**. This works with the V31 or with Windows' built-in *Microsoft GS Wavetable Synth*.
- Both outputs can be on at the same time. The click follows the Metronome settings.
- The playhead follows the clock, not the sound: a sound card with a large buffer plays a few tens of milliseconds after it.

Next to the tempo on the Score tab there are three switches that work **while it plays**:

- **click (`K`)**: the metronome on or off, in Listen and in a take. The count-in always sounds.
- **notes**: the drums of the score on or off. With notes off, Listen is a **click track** of the selected measures.
- **loop**: repeat the selected measures until you stop it. Unticking it lets the current round finish. Changing the measures or the tempo while it plays restarts it after a moment.

**Instruments.** On **Settings → Sound → Instruments** every instrument has a volume, a mute and a solo (solo is not saved), live while Listen plays, and a ▶ button that plays it on its own. To use your own sound, put a WAV file named after the instrument in the `sounds` folder next to the settings file (the page shows the path), for example `Kick.wav`, `Snare.wav`, `HiHatClosed.wav`, `Crash1.wav`, `Ride.wav`. Any instrument without a file keeps its built-in sound, and a file that cannot be read is reported and replaced by the built-in sound. An open hi-hat is cut off when the hi-hat is closed.

## Metronome

The **Metronome** tab is a click track that needs no score: tempo, beats per bar, subdivision (eighths, triplets, sixteenths) and volume, with one light per beat. `M` starts and stops it. The first beat of each bar is accented and subdivisions are softer. With a score loaded, its practice range rolls by below the lights as a staff, repeating at the metronome's tempo so you can play the notes along with the click; **Match score tempo and bar** sets the tempo and beats per bar to the score's. Changes while it runs take effect after a moment. It plays through the click output chosen on **Settings → Sound** (the sound card and/or MIDI).

## Settings

The Settings tab has four pages:

| Page | What is on it |
|---|---|
| **MIDI input** | Input port, a monitor of incoming notes, the pad-to-instrument mapping (with *Learn*) |
| **Timing** | Latency compensation and calibration, the perfect/good/hit-window tolerances |
| **Sound** | Audio output device, MIDI output port, the metronome click, Listen (built-in kit and MIDI), the instrument mixer |
| **General** | Repeat expansion, the recent-scores list, keyboard test input |

Settings are saved automatically. Sliders show their caption in front of them, and the mouse wheel tunes any slider under the pointer.

**Audio output device.** By default sound goes to the system's default output. Pick another one on the Sound page (for example a USB interface) and the click, Listen and the calibration clicks follow it. If the chosen device is not connected, the default is used and a message says so.

**Score layout.** The **Layout** button on the Score tab sets *bars per row* (auto fits as many as the width allows) and *start a row at bar*. With a fixed number every bar has the same width, so barlines line up from row to row, and the first row may be shorter: with 4 bars per row and a row starting at bar 2 you get `[1] [2–5] [6–9] …`. If that many bars do not fit the window, fewer are shown and a note says so. The bar number is the position in the score as shown (the same numbering as the Measures sliders); with repeats expanded the numbers printed above the bars can repeat, and the button shows the printed number of the chosen bar.

## Latency

Every audio and MIDI path adds some delay. Use **Settings → Timing → Calibrate**: play any pad exactly with 16 clicks; the median offset becomes the latency compensation. Sending the click to the V31 (Settings → Sound → Metronome, channel 10, an unmapped note such as 76) puts it in your headphones without computer-audio latency.

## How grading works

- Each written note gets at most one hit and each hit at most one note. Pairs are matched closest-first within the hit window (default ±100 ms). A hit on a related pad (snare/x-stick, hi-hat closed/open, ride/bell, crashes) counts as *wrong pad* with partial credit.
- Grades: perfect ≤ 20 ms (credit 1.0), good ≤ 45 ms (0.85), OK inside the window (0.6), wrong pad (0.3), missed (0). All thresholds are adjustable.
- **Overall score (0–100)** = 55 % note accuracy + 25 % timing (mean |offset| and spread) + 10 % no extra hits + 10 % dynamics. Without accents or ghost notes in the score, the dynamics share goes to the other three. Grades: A+ ≥ 95, A ≥ 90, B ≥ 80, C ≥ 70, D ≥ 60.
- Positive offsets mean *behind the beat* (dragging), negative mean *ahead* (rushing). Drift is the trend of offsets over the take in ms per minute.
- Grace notes and flams are optional: playing them never counts as extra, and skipping them is not a miss.

## MusicXML support

Partwise MusicXML 2.0–4.0: multiple voices (`backup`/`forward`), chords, tempo changes (`sound tempo` or metronome marks), time-signature changes, pickup bars, repeats with 1st/2nd endings (expanded by default), ties, accents, ghost notes (parenthesised noteheads), grace notes, notehead shapes. Instruments are identified from `midi-unpitched` (General MIDI), then the instrument name, then staff position and notehead. Not supported: D.S./D.C./coda jumps and `score-timewise` files.

## Versioning

The project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The version lives in `Cargo.toml`, each release is tagged `vMAJOR.MINOR.PATCH`, and every change that users can see is listed in [CHANGELOG.md](CHANGELOG.md).

What counts as the public interface: the settings file keys, the CSV export columns, and the MusicXML features listed above. While the major version is 0, a minor bump (`0.x.0`) may break any of them and a patch bump (`0.1.x`) is for fixes only. From 1.0.0, breaking one of them needs a major bump.

## Source layout

| File | Purpose |
|---|---|
| `src/xml.rs` | Small dependency-free XML parser |
| `src/musicxml.rs` | MusicXML → score (measures, notes, tempo map, repeats) |
| `src/drums.rs` | Drum instruments, General MIDI and Roland note maps, staff positions |
| `src/score.rs` | Score model and tempo map (beats ↔ seconds) |
| `src/analysis.rs` | Matching hits to notes and all metrics |
| `src/midi.rs` | MIDI input capture and output (midir) |
| `src/metronome.rs` | Count-in/metronome thread (audio and/or MIDI click) |
| `src/playback.rs` | "Listen" and the free metronome: plays notes and clicks on one thread (sound card and/or MIDI) |
| `src/control.rs` | Settings shared with the sound threads so they can change while playing (click, notes, loop, volumes) |
| `src/synth.rs` | The built-in software drum kit (one synthesised sample per instrument) and the WAV override loader |
| `src/render.rs` | Score and played-notes staves |
| `src/app.rs` | UI: welcome panel, Score, Results and Settings tabs; `src/app/metro.rs` is the Metronome tab, `src/app/mixer.rs` the instrument mixer |
| `src/widgets.rs` | Shared UI helpers: the wheel-aware slider and the larger Score-tab style |
| `src/config.rs` | Settings file (`%APPDATA%\DrumTutor\config.txt` on Windows) |
