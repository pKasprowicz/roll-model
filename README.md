# Roll Model

> **Made with AI.** Roll Model was created with extensive use of AI: most of its code, tests, documentation and rudiment transcriptions were written by AI models (Anthropic's Claude), directed and reviewed by the author. It has not been independently audited, and parts have never been tried on real hardware (the Roland V31, the controller presets); expect mistakes and check anything that matters.

A Rust desktop app (egui) for practising drum parts against a score:

1. **Loads MusicXML drum scores** (`.musicxml`, `.xml`, compressed `.mxl`) and draws them on a percussion staff in the **Score** tab.
2. **Practises the 40 drum rudiments** on the **Rudiments** tab: pick one, repeat it as often as you like, hear it, play it along with the click and get graded, hand by hand.
3. **Captures MIDI from a Roland V31** (or any V-Drums / e-kit) and draws every hit as a note on a second staff **directly under the score**, on the same time axis — early hits sit left of the written note, late hits to the right.
4. **Compares the performance with the score** and grades it: note accuracy, timing offset and consistency, tempo drift, wrong pads, missed and extra notes, and dynamics (accents and ghost notes).

## User manual

An extensive manual (setup, every tab, the controller, grading, troubleshooting, and a reference of the 40 rudiments) is one HTML file, `roll-model-manual.html`, included in every release archive and attached to the release. It is built from `docs/manual/manual.template.html` and the rudiment data in `src/rudiments.rs`:

```sh
python3 tools/build_manual.py      # writes target/manual/roll-model-manual.html (standard library only)
```

Edit the template when behaviour changes; the rudiment tables regenerate themselves.

## Developer guide

For anyone who wants to change the program: `roll-model-developer-guide.html`, also in every release archive. It explains the architecture and the ideas behind it (layers, time in quarter notes, takes and grading, the real-time threads, the controller and rudiment subsystems), the Rust constructs the design leans on, where the program can be extended, and seven step-by-step tutorials. It is built from `docs/developer/developer.template.html`, and its code excerpts, module map and statistics are generated from the source, so they cannot go stale:

```sh
python3 tools/build_devguide.py    # writes target/devguide/roll-model-developer-guide.html
```

If a refactoring renames an item the guide shows, that command fails and names the snippet to update.

## Build and run

Install a recent Rust from <https://rustup.rs> (tested with 1.99; 1.82 is too old for the dependencies), then:

```sh
cargo run --release                                 # start empty
cargo run --release -- examples/basic_rock_beat.musicxml
```

The binary ends up in `target/release/roll-model(.exe)`.

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

**Start a recording with the sticks in your hands.** On **Settings → MIDI input → Start trigger** switch on *Start recording with the two hits* and choose a first and a second instrument (or press *Learn* and hit the pad). On the Score and Rudiments tabs, hitting the first and then the second within half a second, with nothing played in between, starts a take just as `R` does, with its count-in. The trigger hits are not part of the take, it is off while a take records, and it is off by default (a crash followed by a tom is the suggested pair).

No kit at hand? Enable **Settings → General → Computer keyboard plays pads** (Z kick, X snare, C/V/B hi-hat closed/open/pedal, A S D F toms, Q W E cymbals).

## Rudiments

The **Rudiments** tab (between Score and Metronome) teaches the **PAS international drum rudiments**: all 40, grouped as rolls, diddles (paradiddles), flams and drags, with the original NARD Standard 26 marked `*`. The notation, sticking and how each rudiment was transcribed are in [docs/rudiments.md](docs/rudiments.md).

- **Choosing.** *Prev* and *Next* step through the list; the button with the name opens it by family, with a search box and a switch for only the original 26. Each rudiment is drawn on the same two staves as a score, with the sticking (`R` and `L`, the grace notes of flams and drags as small letters) under the notes, and your hits below, coloured as on the Score tab.
- **Repeats instead of measures.** The *repeats* slider says how many times the pattern is written one after the other, one bar each; that is the whole exercise. *Lead* chooses the hand that starts: right, left, or alternating (right then left, and so on). Each rudiment starts with the lead that makes its repetitions flow without playing a hand twice in a row. Clicking a bar still narrows the range, as on the Score tab.
- **Tempo in BPM.** The tempo slider is the tempo in quarter-note BPM (30–240) and each rudiment starts at a tempo of about six strokes a second. The tempo is remembered per rudiment. Rolls are written on a 16th-note grid (the PAS writes them in 32nds), so a 16th at 120 BPM is a 32nd at 60 BPM.
- **Listen, Record, count-in, click, loop** work as on the Score tab (`L`, `R`, `K`, `Esc`), also the notes switch and the loop. The multiple bounce (buzz) roll is one held note and can only be listened to.
- **Grading** is as for a score, with two differences made for fast, repeated patterns: the hit window shrinks to 45 % of the distance between neighbouring notes (but never below the "perfect" tolerance), so a hit can never be within reach of two strokes, and the **Results** tab adds a **Hands** table. The drum module cannot tell which hand played, but the expected hand of every stroke is known, so the table and the feedback compare the hands: how far apart in time they are and how much softer one plays than the other. Flams and drags are graded as grace notes: playing them is never required and never counts as extra.
- **Progress.** A take of the whole exercise that scores 85 or better is a *clean take*: its tempo is remembered as the **best clean tempo** of that rudiment (shown next to the rudiment and in the list). With **ladder** on, the tempo then goes up by 5 BPM, so you climb as long as you play it cleanly. The PAS advises practising open to close to open (slow, fast, slow).
- **Controller.** While the Rudiments tab's practice is in use, the controls that choose measures choose repetitions and rudiment instead, with no change to the presets; see below.

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

The **Metronome** tab is a click track that needs no score: tempo, beats per bar, subdivision (eighths, triplets, sixteenths) and volume, with one light per beat. `M` starts and stops it. The first beat of each bar is accented and subdivisions are softer. Its notes roll by below the lights: the click pattern of the bar, or with a score loaded its practice range below the lights as a staff, repeating at the metronome's tempo so you can play the notes along with the click; **Match score tempo and bar** sets the tempo and beats per bar to the score's. Changes while it runs take effect after a moment. It plays through the click output chosen on **Settings → Sound** (the sound card and/or MIDI).

## Controller (Mackie Control, X-Touch)

A control surface that speaks the Mackie Control protocol runs the program from the stool, with sticks in your hands. Built-in presets: **Behringer X-Touch Mini** (set it to *MC mode*) and **X-Touch Compact**. Generic (non-Mackie) presets for the Novation Launch Control XL, Korg nanoKONTROL2 (CC mode), Akai MPK Mini and Arturia BeatStep use absolute knobs and faders and plain buttons; they are written from memory and untested, so check them with the monitor and copy and change them. The X-Touch Mini is the default; choose another (or none) on **Settings → Controller**; the program finds its MIDI port by name (or pick the port there) and remembers the choice. The controller has its own MIDI port, separate from the drum module, so its buttons never count as hits. The page also has a monitor that shows every message the controller sends and what it is bound to. The presets have not been tried on a device yet: if a number in one is wrong, the monitor shows the real one.

**X-Touch Mini.** The LAYER A / LAYER B buttons switch between two sets of controls (the program does this itself). Click the controller indicator at the top right of the window for a picture of the Mini that shows what every control does on each layer (hover a control for details); the X-Touch Compact has one too.

| Layer A: practice | |
|---|---|
| Top row | **Shift** (hold: encoders turn five times as far) · Tempo −5 % · Tempo +5 % · Click · Tap tempo · Notes · Metronome tab · Next tab |
| Bottom row | Loop the worst bar of the last take · Discard take · then what is printed beside the buttons: ◀◀ Range back · ▶▶ Range forward (by the length of the range) · LOOP · ■ Stop · ▶ Listen · ● Record |
| Encoders 1–8 | Tempo (push: 100 %) · First measure · Last measure · Click volume · Music volume · Count-in bars · Bars per row · Recent scores (push: open) |
| Fader | Tempo (it takes over once it reaches the current value, so nothing jumps) |

| Layer B: mixer | |
|---|---|
| Top row / bottom row | Mute / solo of eight instrument groups: Kick, Snare (with cross-stick), Hi-hat, Tom 1, Tom 2, Tom 3 and 4, Crash (all crashes, china, splash), Ride (with bell and cowbell) |
| Encoders 1–8 | Volume of the group (push: play it) |
| Fader | Music volume |

**With a rudiment.** While the Rudiments tab's practice is in use (the Rudiments tab, or any tab opened after it until the Score tab is shown again), the same controls mean: encoder 2 (*first measure*) = repeats, encoder 3 (*last measure*) = lead (right, left, alternate), encoder 8 (*recent scores*) = browse the rudiments, *range back* / *range forward* = previous / next rudiment, tempo (encoder 1, the fader, tap tempo, ±5 %) = BPM, and the push of encoder 1 = the rudiment's starting tempo. The rings of encoders 2 and 3 show the repeats and the lead. Everything else (record, listen, stop, click, loop, notes, mixer) is unchanged.

Tempo controls act on the practice tempo, or on the metronome while the Metronome tab is open. Tempo, measures and the score are locked while a take records. The lights show what is on (recording, listening, click, loop, notes, metronome, mute, solo, shift) and the rings around the encoders show their values.

**Teaching a controller.** On Settings → Controller, *New preset by learning* makes an empty preset for the controller's port, and *Copy the chosen preset to edit* makes an editable copy of any preset (the built-in ones cannot be changed). Under *Assignments* every action has a *Learn* button: click it, then press or turn the control on the device. By default the program then waits for the next action in the list; untick that to learn one at a time. A control that was bound to another action is taken from it. A knob or fader sends its position (*absolute*); an endless encoder needs *relative* and the way it counts (*mackie*, *twos*, *offset* or *signbit2*, which the monitor helps to find out). Every change is saved to the preset file at once.

**Your own preset.** A preset is a text file ending in `.txt` in the `controllers` folder next to the settings file (the Controller page shows the path); it then shows up in the list. The page can copy a built-in preset there to start from. Each line is `key = value`, `#` starts a comment:

```
name  = My surface
port  = X-TOUCH MINI                  # part of the MIDI port's name
layer = A note 84                     # this message switches to layer A (optional)
layer = B note 85
bind  = A note 89 -> record           # a button; without "A" it works on every layer
bind  = A cc 16 rel -> tempo          # an endless encoder (Mackie relative values)
bind  = bend 1 -> tempo               # a fader (pitch bend on MIDI channel 1)
led   = A record -> note 89           # a button light that follows the state
ring  = A tempo -> cc 48              # an LED ring that shows a value
bind  = cc 14 ch 2 rel twos -> tempo    # only on MIDI channel 2; rel mackie | twos | offset | signbit2
bind  = cc 13 abs -> tempo            # a knob that sends its position
led   = record -> note 36 ch 10 on=21 off=0   # another channel and values (a pad's colour)
ring  = tempo -> cc 20 scale=127      # a plain value 0 to 127 instead of Mackie ring values
```

*Buttons:* `record listen stop click loop notes metronome next_tab loop_worst_bar range_back range_forward tap_tempo tempo_down tempo_up discard_take tempo_reset open_score prev_rudiment next_rudiment shift`, and `mute N`, `solo N`, `audition N` for the mixer strips 1–8. *Values* (an encoder with `rel`, or a fader): `tempo range_from range_to click_volume music_volume count_in bars_per_row repeats lead`, `gain N`; `score` (recent scores) and `rudiment` (the rudiments) are only turned. *Lights:* `record listen stop click loop notes metronome shift`, `mute N`, `solo N`; *rings* take any of the values (`repeats` and `lead` too). A line that cannot be used is reported on the Controller page and the rest still works.

## Settings

The Settings tab has five pages:

| Page | What is on it |
|---|---|
| **MIDI input** | Input port, a monitor of incoming notes, the start trigger, the pad-to-instrument mapping (with *Learn*) |
| **Controller** | The Mackie Control surface: preset, port, monitor, your own presets |
| **Timing** | Latency compensation and calibration, the perfect/good/hit-window tolerances |
| **Sound** | Audio output device, MIDI output port, the metronome click, Listen (built-in kit and MIDI), the instrument mixer |
| **General** | Repeat expansion, the recent-scores list, keyboard test input |

Settings are saved automatically. Sliders show their caption in front of them, and the mouse wheel tunes any slider under the pointer.

**Audio output device.** By default sound goes to the system's default output. Pick another one on the Sound page (for example a USB interface) and the click, Listen and the calibration clicks follow it. If the chosen device is not connected, the default is used and a message says so.

**Score layout.** The **Layout** button on the Score tab sets *bars per row* (auto fits as many as the width allows) and *start a row at bar*. With a fixed number every bar has the same width, so barlines line up from row to row, and the first row may be shorter: with 4 bars per row and a row starting at bar 2 you get `[1] [2–5] [6–9] …`. If that many bars do not fit the window, fewer are shown and a note says so. The bar number is the position in the score as shown (the same numbering as the Measures sliders); with repeats expanded the numbers printed above the bars can repeat, and the button shows the printed number of the chosen bar.

## Latency

Every audio and MIDI path adds some delay. Use **Settings → Timing → Calibrate**: play any pad exactly with 16 clicks; the median offset becomes the latency compensation. Sending the click to the V31 (Settings → Sound → Metronome, channel 10, an unmapped note such as 76) puts it in your headphones without computer-audio latency.

## How grading works

(For rudiments, see "Rudiments" above: the hit window follows the note spacing and the hands are compared.)

- Each written note gets at most one hit and each hit at most one note. Pairs are matched closest-first within the hit window (default ±100 ms). A hit on a related pad (snare/x-stick, hi-hat closed/open, ride/bell, crashes) counts as *wrong pad* with partial credit.
- Grades: perfect ≤ 20 ms (credit 1.0), good ≤ 45 ms (0.85), OK inside the window (0.6), wrong pad (0.3), missed (0). All thresholds are adjustable.
- **Overall score (0–100)** = 55 % note accuracy + 25 % timing (mean |offset| and spread) + 10 % no extra hits + 10 % dynamics. Without accents or ghost notes in the score, the dynamics share goes to the other three. Grades: A+ ≥ 95, A ≥ 90, B ≥ 80, C ≥ 70, D ≥ 60.
- Positive offsets mean *behind the beat* (dragging), negative mean *ahead* (rushing). Drift is the trend of offsets over the take in ms per minute.
- Grace notes and flams are optional: playing them never counts as extra, and skipping them is not a miss.

## MusicXML support

Partwise MusicXML 2.0–4.0: multiple voices (`backup`/`forward`), chords, tempo changes (`sound tempo` or metronome marks), time-signature changes, pickup bars, repeats with 1st/2nd endings (expanded by default), ties, accents, ghost notes (parenthesised noteheads), grace notes, notehead shapes. Instruments are identified from `midi-unpitched` (General MIDI), then the instrument name, then staff position and notehead. Not supported: D.S./D.C./coda jumps and `score-timewise` files.

## License

Roll Model is released under the [PolyForm Noncommercial License 1.0.0](LICENSE): anyone may use, study, change and share it for any noncommercial purpose (personal use, hobby, learning, teaching without charge, research, charities, schools), and nobody may use it to make money. It is source-available rather than open source in the OSI sense, because the licence excludes commercial use. Versions published before this change (up to 0.5.0) were released under the MIT licence and stay under it.

The program is built from open-source libraries and fonts under their own licences; their texts are in `THIRD_PARTY_NOTICES.txt`, included in every release archive and generated by `python3 tools/build_notices.py`. Roland, V-Drums, V31, Behringer, X-Touch, Mackie Control, MuseScore, Guitar Pro, Sibelius, Dorico, MIDI and Microsoft are trademarks of their respective owners. Roll Model is an independent project, not affiliated with or endorsed by any of them; the names only say what it works with.

## Versioning

The project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The version lives in `Cargo.toml`, each release is tagged `vMAJOR.MINOR.PATCH`, and every change that users can see is listed in [CHANGELOG.md](CHANGELOG.md). Moving the tag `preview` onto a commit of `develop` (`git tag -f preview && git push -f origin preview`) builds a *preview* release from it: a GitHub pre-release, replaced each time, that may be unfinished.

What counts as the public interface: the settings file keys, the CSV export columns, and the MusicXML features listed above. While the major version is 0, a minor bump (`0.x.0`) may break any of them and a patch bump (`0.1.x`) is for fixes only. From 1.0.0, breaking one of them needs a major bump.

## Source layout

| File | Purpose |
|---|---|
| `src/xml.rs` | Small dependency-free XML parser |
| `src/musicxml.rs` | MusicXML → score (measures, notes, tempo map, repeats) |
| `src/drums.rs` | Drum instruments, General MIDI and Roland note maps, staff positions |
| `src/score.rs` | Score model and tempo map (beats ↔ seconds) |
| `src/rudiments.rs` | The 40 PAS rudiments as stroke patterns and the builder that turns one into a score (no GUI or MIDI) |
| `src/analysis.rs` | Matching hits to notes and all metrics |
| `src/midi.rs` | MIDI input capture and output (midir), and the control surface's port |
| `src/controller.rs` | Control surfaces: messages, presets, which control does what, lights (no MIDI port, no GUI) |
| `presets/` | The built-in controller presets (text files) |
| `src/metronome.rs` | Count-in/metronome thread (audio and/or MIDI click) |
| `src/playback.rs` | "Listen" and the free metronome: plays notes and clicks on one thread (sound card and/or MIDI) |
| `src/control.rs` | Settings shared with the sound threads so they can change while playing (click, notes, loop, volumes) |
| `src/synth.rs` | The built-in software drum kit (one synthesised sample per instrument) and the WAV override loader |
| `src/render.rs` | Score and played-notes staves |
| `src/app.rs` | UI: welcome panel, Score, Results and Settings tabs; `src/app/metro.rs` is the Metronome tab, `src/app/rudiment.rs` the Rudiments tab, `src/app/mixer.rs` the instrument mixer, `src/app/controls.rs` what the controller does and its Settings page |
| `src/widgets.rs` | Shared UI helpers: the wheel-aware slider and the larger Score-tab style |
| `tools/build_notices.py` | Generates `THIRD_PARTY_NOTICES.txt`, the licence texts of all dependencies, for the release archives |
| `docs/developer/`, `tools/build_devguide.py` | The developer guide (HTML template with code excerpts pulled from the source) and its build script |
| `docs/style.css` | The stylesheet shared by the manual and the developer guide |
| `docs/manual/`, `tools/build_manual.py` | The user manual (HTML template) and the script that builds it for releases |
| `src/config.rs` | Settings file (`%APPDATA%\RollModel\config.txt` on Windows) |
