# Changelog

> **Made with AI.** This project was created with extensive use of AI (Anthropic's Claude), directed and reviewed by the author. Check anything that matters.

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html); see "Versioning" in the README.

## [Unreleased]

### Changed
- **The playhead, the beat lights and the count-in are drawn where the sound is heard** when it comes through the computer: the calibrated latency later than the click was scheduled (at most 150 ms; not when the latency is from an earlier engine or negative). Before, they ran about one output delay ahead of the sound. Grading is unchanged. New setting `sync_display` (Settings → Timing → Playhead), on by default.

### Added
- **`examples/audio_latency.rs`** measures the whole delay from the program's `play` instant to the sound at an input wired to the output (task AUD-10, experiment E3), with the program's own engine code. On the first machine: 45.2 ms, steady to 0.01 ms, against the 10 ms the driver reports.

## [0.8.0] - 2026-10-30

A new audio engine: the click and the built-in drums start at an exact moment of one output stream that stays open, instead of at the next audio callback, so their timing no longer wobbles by up to 10 ms. **Run Settings → Timing → Calibrate again after updating.** Covered by unit tests (271), and measured by loopback recording on two Windows machines. Unplugging the output, a missing named output, changing the Windows default output and a long looping Listen with a tempo ramp (up to 300 %) were tried by hand; Listen was listened to on the new engine (the cymbals and the WAV override were not judged). Not tried: a real V31 and controllers, and audio output on Linux and macOS.

### Changed
- **Clicks and the built-in drums start at an exact moment** (a new audio engine in place of rodio): sounds are handed to one output stream that stays open, about 30 ms ahead, and start at the very frame that matches their time instead of at the next audio callback. Measured by recording the output (WASAPI loopback): on the development machine (Windows 11, Realtek output) the onsets of a metronome at 240 BPM in sixteenths used to flip between two positions 10 ms apart and now stay put (0.04-0.06 ms per place in the bar, 1.1 ms at the very worst, which comes from the sound card's effects), and the clicks are spaced evenly (62.5 ms) where they used to alternate between 60 and 70 ms; on a second machine with a USB audio interface the widest spread is 0.02 ms. The average delay drops by about 5 ms (an estimate); the rest of the delay is the operating system's shared mode and is not in the program's hands. The sound output setting (`audio_output`) is unchanged; the first sound opens the output, later ones do not open it again. **Run Settings → Timing → Calibrate again**: the delay of the click changed with the engine.
- **Settings → Sound shows the open output** (its name, rate, the delay the driver reports, and how many sounds came too late, by how much at worst, or were dropped) and says that Bluetooth audio cannot be used for timing.
- **Settings → Timing says when the latency compensation is from an earlier audio engine** (new setting `latency_engine`, written by Calibrate and when the slider is moved), with a *Keep this value* button. A file from an earlier version has no engine recorded, so a calibrated latency shows the note once.
- **The sound output library is cpal 0.17** (rodio is gone); a stream that fails (for example an unplugged device) is opened again at the next start, and so is the output when *System default* is chosen and the default output in Windows has changed since the last start.

### Added
- **The developer guide has a chapter on the audio system**: where audio latency comes from (constant delay against jitter), the time model, the threads and the data between them, one sound's path through the engine, and how the engine is tested and measured; the data flow figure and a thread inventory are new.
- **`tools/audio_baseline.ps1` and `examples/audio_probe.rs`** measure the timing of the output from outside: they record the default output by loopback and report the spread of each sound start in a metronome bar.

## [0.7.3] - 2026-10-30

The score is drawn like a drum part: beams, flags, tuplet brackets and the rests a file prints. A bar that ends with a pause now ends its row. Covered by unit and integration tests (231); the notation, the rests, the row breaks and the zoom slider were looked at in the running app with the sample and with a real score. The manual and the developer guide are published online with this release (the first deployment of the GitHub Pages workflow). Nothing in the controller, tempo ramp and layer work of 0.7.2 has been tried on a device yet.

### Changed
- **The zoom slider is greyed out while *bars per row* is fixed**, with a hint on hover: with a fixed count the bars fill the width, so the zoom had no effect and the slider looked broken. Set *bars per row* to *auto* to use it.

### Added
- **A bar that ends with a pause ends its row** (Layout menu → *new row after a pause*, on by default; setting `break_at_pause`): when the last rest of a bar runs to the barline and no note in any voice starts during it, the score goes on in the next row. A rest under notes of another voice, like the feet's rest under the hands in the sample, is not a pause. The break wins over *bars per row*: after a pause a new row starts and the bars are counted from there (*start a row at bar* still applies to the part of the score it falls in). Covered by a test and looked at in the running app.
- **The user manual and the developer guide are published online** on GitHub Pages (<https://pkasprowicz.github.io/roll-model/>), behind a start page that links to both. They are the same files as before and are still in every release archive and attached to every release.
- **The score is drawn with beams, flags and tuplet brackets**: eighths and shorter notes of a beat are joined by beams (hands and feet separately, as two voices; a dotted quarter in 6/8, 9/8 and 12/8), a note alone on an off-beat has flags, and triplets, sextuplets and quintuplets have a bracket with its number. Everything is worked out from where the notes fall in the beat (`core/src/notation.rs`), because the score keeps no note lengths: a note counts as lasting until the next one of its voice. The metronome's rolling staff has none of this. Looked at in the running app (beams, flags, brackets).
- **Rests the file prints are drawn** on the Score tab: the hands' voice high and the feet's low when the score has both, else in the middle; a bar's rest is a whole rest in the middle of the bar; a rest that would sit on a note head moves away from the middle of the staff until it is clear; rests the file hides are not drawn. Looked at in the running app. Scores made by the program (rudiments) have none.

## [0.7.2] - 2026-10-30

Editing controller presets gets much further (layers, lights and rings, finding how an encoder counts), Listen and the metronome can ramp the tempo, each score remembers its row layout, and the playhead no longer jumps at a barline. Covered by unit and integration tests; the playhead fix was watched in the running app. The tempo ramp (not listened to), the layer, light and Detect editing (simulated messages only, no real controller) and the layouts were not tried on devices.

### Fixed
- **The playhead of the score no longer jumps over a barline**: each bar has a padding at both ends, so the first note of the next bar is about 24 pixels further on than the end of the bar before; the playhead used to cover that distance in one step at the barline. It now speeds up during the last stretch of the bar and arrives at the next bar's first note on time. (Only where the next bar is in the same row.)

### Added
- **The Manage window can find out how an endless encoder counts**: *Detect* next to the kind of a value (turn the encoder four clicks to the right, then four to the left) chooses *mackie*, *twos*, *offset* or *signbit2* by itself, or says that it could not tell.
- **Lights and rings can be added and edited in the Manage window**: the new *Lights and rings* section lists the actions that have a control; *Add light* / *Add ring* put one on that control, and the note or cc number and the on/off values can be changed (colours of a pad, for example).
- **Layers can be created and deleted in the Manage window**: *Add layer…* asks for a name and the button of the device that switches to it (a button that already does something is refused), *Delete layer* removes a layer with what is assigned on it.
- **A tempo ramp for Listen and the metronome**: with *loop* on, *ramp +* on the Score tab raises Listen's tempo by that many per cent of the written tempo every few passes; the Metronome tab's *faster by* does the same in BPM every few bars. Both stop at three times the starting tempo, are off by default, and take effect at the next start. The player squeezes the later passes in one go, without a gap; the playhead, the position label (which now shows the tempo in force), the beat lights and the rolling staff follow it.
- **Each score remembers its row layout** (bars per row and the bar that starts a row, in the Layout menu of the Score tab): open the score again and its rows are as you left them. The last 30 scores are kept; a score never laid out keeps the settings on screen.

## [0.7.1] - 2026-10-30

A small release: Listen can start without the count-in, the shortcuts work again after Tab or a click on a button, the splash shows how long each start-up step took, and the code was restructured (no change in behaviour): the part that needs no window, MIDI port or sound card is now a crate of its own, `core/`. Covered by unit and integration tests (209); the shortcut fix and the splash times were not tried with a real keyboard or devices.

### Added
- **Listen can start without the count-in**: the *in Listen* box next to *count-in bars* on the Score tab (setting `listen_count_in`, on by default). A recording always has the count-in.

### Fixed
- **Shortcuts work after Tab or a click on a button or slider**: `R`, `L`, `K`, `M`, `Esc` and the keyboard pads were ignored while any widget had the keyboard focus; now only typing in a text box (or a slider's value box) stops them.

### Changed
- **The splash shows how long each start-up step took** (for example `Found 3 MIDI inputs and 2 MIDI outputs · 41 ms`), so slow drivers and devices can be seen at a glance.
- **For developers: the code is split into two crates** in a Cargo workspace (`roll-model`, the application, and `roll-model-core` in `core/`, with the scores, grading, rudiments, controller presets and settings), the large files `app.rs`, `controller.rs` and `app/controls.rs` are folders of smaller modules, errors are typed, the settings keys and the controller actions are tables, and the settings folder is an injected value so no test can touch a user's files. `cargo build` and `cargo test` cover both crates. The developer guide is brought up to date.
- **A funding link**: the repository has a *Sponsor* button (`.github/FUNDING.yml`) and a README section, and the notes of a `vX.Y.Z` release carry the link too. The program itself is unchanged by it.

## [0.7.0] - 2026-10-29

A start-up splash, a start trigger, help pictures of controllers, presets for four more MIDI controllers, and a window for managing controller presets and teaching a controller by pressing its controls. The default controller is now the X-Touch Mini, and the presets of both X-Touch controllers changed (see below). Covered by unit tests (201); the splash, the help pictures and the Manage window were looked at in the running app. Not yet tried with a real V31, a real controller (Mackie or generic: the generic presets are from memory), or by ear.

### Changed
- **Presets are managed in one window**: the *Manage* button next to the preset selector on Settings → Controller opens a window with the list of all presets on the left (grouped as Mackie Control and Generic MIDI; a click chooses a preset), *Add*, *Copy* and *Delete…* under the list (Delete asks again; built-in presets can be neither renamed nor deleted), and the chosen preset's name (renaming) and assignments editor on the right. The Assignments and Your own presets sections of the page are gone.
- **Controller page**: the description is brought up to date (two kinds of preset, learning), every preset now says what it speaks, **Mackie Control** or **Generic MIDI** (new `kind` line in preset files; older files count as Mackie if they have layers or lights, otherwise as MIDI), the preset list is grouped by kind and the page and the help screen show it, and the assignments editor opens in a **window of its own** (*Edit assignments…*) instead of in the page. Rings of a MIDI preset show a plain value 0 to 127 unless `scale=` says otherwise.
- **X-Touch Mini, layer A**: the bottom row now does what is printed beside its buttons: rewind = range back, fast forward = range forward, LOOP = loop, stop = stop, play = listen, record = record (before, those buttons were range forward, tap tempo, tempo −, tempo +, discard take and Shift). The actions that had to move: Shift is the first button of the top row, Tempo −5 % and +5 % the second and third, Tap tempo the fifth, Loop worst bar and Discard take the first two of the bottom row; Click, Notes, Metronome and Next tab stay where they were. The lights moved with their buttons. A copy of the old preset in your `controllers` folder keeps working as before.
- **X-Touch Compact preset** reworked to fit the real device (seen on a photograph): Shift, Click, Notes, Metronome, Tap tempo, Loop worst bar, Discard take and Next tab are now the eight SELECT buttons under the faders (notes 24-31, left to right) instead of notes the Compact does not seem to have (84, 85, 87, 88, 89, 54, 47, 70), and the jog wheel line (cc 60) is gone, as the Compact has no jog wheel. The lights of Shift, Click, Notes and Metronome moved with them. A copy of the old preset in your `controllers` folder keeps working as before.
- The **Behringer X-Touch Mini** is the default controller: the program looks for it (and connects when it is plugged in) until you choose another preset or *— none —* on Settings → Controller. A settings file that already names a preset, or none, is not changed.

### Added
- **Help picture for every controller**: the controller help screen draws a picture of any preset that has no drawn layout (the generic built-in presets and the ones you teach), made from its controls: knobs and encoders on top, then faders, then buttons, each group in order of message number, with what every control does on the chosen layer. Layer buttons are included. It does not show where the controls are on the device.
- **Teach a controller by pressing its controls** (in the *Manage* window): *Add* makes an empty preset for the port, *Copy* an editable copy of any preset, and every action has a *Learn* button (press or turn the control on the device; the program can go on to the next action by itself). Knobs and faders can be absolute or relative with any of the encodings, a control can be limited to the MIDI channel it sends on, a control taken from another action is reported, and every change is written to the preset file in the `controllers` folder at once and takes effect without reconnecting. Built-in presets are never edited. Editing writes the preset afresh, so the comments of a copied preset are lost.
- **Controller presets can describe non-Mackie devices**: `ch N` limits a `bind` line to one MIDI channel (a line for the channel wins over one for any channel, and the same note on two channels is two controls); `rel` takes an encoding, `mackie` (default), `twos`, `offset` or `signbit2`, for endless encoders that count differently; a `led` line takes `on=` and `off=` values and `ch N` (colour pads, other channels) and a `ring` line `scale=N` for a plain value instead of Mackie ring values. Lines without the new words behave as before, so the existing presets did not change.
- **Presets for four generic MIDI controllers** (not Mackie Control): Novation Launch Control XL, Korg nanoKONTROL2 (in CC mode), Akai MPK Mini and Arturia BeatStep. Knobs and faders are absolute (they take over once they reach the current value) and buttons are plain notes or cc, mapped to the transport, tempo, measures, volumes and the mixer. They were written from memory of the devices' tables and are **not verified**: a web search confirmed only a few numbers. Copy one to the `controllers` folder and change it, or check it with the monitor. They have no layers, lights or picture (the help screen lists their controls).
- **Controller help screen**: click the controller indicator at the top right of the window (the green or red *⏺ controller* label) to open a picture of the controller with what each control does on the chosen layer, drawn as vectors: the eight encoders with their LED rings and pushes, the two rows of buttons (a green dot where the light follows the program), the fader and the LAYER buttons. Hover a control for what it does and which message it sends; choose the layer with A and B (or the drawn LAYER buttons). It also opens when the controller is not connected. There is a picture of the Behringer X-Touch Mini (drawn from a photograph, with its two layers, the fader, the MC MODE lamp, the LAYER buttons and the symbols printed beside the bottom row) and of the X-Touch Compact (also from a photograph: eight encoders over three rows of strip buttons, nine faders with a row of buttons under them, eight more encoders, six transport buttons and the two LAYER buttons); other controllers get the list of their controls. The Mini's picture is a little taller than the device, to leave room for the labels. Bindings of a preset that the picture has no place for are listed under it.
- **Start-up splash**: a small window of its own opens in the middle of the main window at once, with the name, version and copyright, and says what the program is doing while it starts (reading the settings, looking for MIDI ports, connecting the drum module and the controller, listing the sound outputs, opening a score given on the command line), listing what each step found. A drum module or controller that is not there shows in amber. The splash stays open when the program is ready, so that it can be read, and closes at the first key or mouse button pressed in the program (not in the first 0.6 s after the start-up, and the key that closes it is not used as a shortcut). Before, the main window stayed hidden until all of this was done, and listing the sound outputs alone took a third of a second or more.
- **Start trigger**: two drum hits in a row start a recording on the Score and Rudiments tabs, so you can begin a take with sticks in your hands. Switch it on and choose the first and the second instrument (or *Learn* them by hitting the pads) on Settings → MIDI input. The second hit must come within 0.5 s of the first with nothing played in between. The trigger hits are not part of the take, and the trigger is off while a take records and on other tabs. A hint under the score shows the pair and when it is waiting for the second hit. New settings keys `trigger_enabled`, `trigger_first`, `trigger_second`.

## [0.6.0] - 2026-10-26

Rudiments, a hardware controller, a new licence, and a user manual and developer guide in every release. Covered by unit tests (132); the Rudiments tab was looked at in the running app for two rudiments. Not yet tried with a real V31, a real controller, or by ear.

### Changed
- The grey helper texts (the small text style) are half as big again.
- Releases now include `LICENSE` and **`THIRD_PARTY_NOTICES.txt`**, the licence texts and copyright notices of all libraries and bundled fonts, as their licences require for binary copies. The README and manual say that the product names they mention are trademarks of their owners.
- The licence is now the **PolyForm Noncommercial License 1.0.0** (file `LICENSE`): free for any noncommercial purpose, no commercial use. Earlier releases stay under MIT.

### Added
- A **developer guide** (`roll-model-developer-guide.html`) in the same format as the user manual: architecture, concepts, threads and real time, the controller and rudiment subsystems, Rust constructs, extension points and seven tutorials. Its code excerpts are extracted from the source at build time. Built during the release and included in both archives and as a release file; build it locally with `python3 tools/build_devguide.py`.
- **Rudiments tab** between Score and Metronome: the 40 PAS international drum rudiments (rolls, diddles, flams, drags; the original NARD Standard 26 are marked). Pick one from the list (family groups, search, "original 26 only") or with Prev/Next, repeat it with the *repeats* slider instead of choosing measures, choose which hand leads (right, left, alternating), and Listen, Record, count in, click and loop as on the Score tab. The rudiment is drawn on the usual two staves with the sticking (R and L, the grace notes of flams and drags as small letters) under the notes. The tempo is in BPM and remembered per rudiment. The multiple bounce (buzz) roll can only be listened to. The catalogue and where it comes from are in `docs/rudiments.md`.
- Grading of rudiments: the hit window follows the spacing of the notes (45 % of the gap to the neighbours, not less than the "perfect" tolerance), the Results tab has a **Hands** table, and the feedback says when one hand is late or softer than the other. A drag's two grace notes are expected at different times before the main note.
- A take of a whole rudiment that scores 85 or better is a *clean take*: its tempo becomes the rudiment's **best clean tempo**, shown with the rudiment and in the list. A **ladder** switch raises the tempo by 5 BPM after each clean take.
- Controller: with a rudiment, encoder 2 sets the repeats, encoder 3 the lead, encoder 8 browses the rudiments, range back/forward step through them and the tempo controls are in BPM. No change to the presets is needed. New preset names: buttons `prev_rudiment`, `next_rudiment`; values `repeats`, `lead`; `rudiment` (only turned). Next tab includes the Rudiments tab.
- New settings keys `rudiment`, `rudiment_repeats`, `rudiment_nard_only`, `rudiment_ladder`, `rudiment_bpm.N`, `rudiment_best.N`.
- The Home tab has a button that opens the Rudiments tab.
- An extensive **user manual** (`roll-model-manual.html`, one self-contained page with contents, diagrams and a searchable reference of the 40 rudiments) is built during the release and included in both archives and as a release file. Build it locally with `python3 tools/build_manual.py`.
- Hardware controller support for Mackie Control surfaces, with built-in presets for the **Behringer X-Touch Mini** (MC mode, two layers switched by the program) and **X-Touch Compact**: record, listen, stop, click, loop, notes, metronome and tab switching, tap tempo, tempo and measure range on encoders and a fader, "loop the worst bar" of the last take, browsing and opening recent scores, and the instrument mixer (volume, mute, solo, play) on eight instrument groups. Lights and LED rings follow the application. Shift (held) makes encoder turns five times as large. A fader takes over only once it reaches the current value.
- **Settings → Controller**: choose the preset and port, see what each control is bound to, watch the incoming messages, and copy a built-in preset as a start for your own.
- A controller that is plugged in after the program started connects by itself, and one that is pulled out is let go of.
- Your own controller presets: text files in the `controllers` folder next to the settings file, listed with the built-in ones. New settings keys `controller_preset` and `controller_port`.
- The X-Touch Mini preset is written from the Mackie Control table as remembered and has not been tried on the device; the Compact preset follows the standard Mackie Control numbers and is untested.

## [0.5.0] - 2026-10-26

The application is renamed Roll Model, and gets a Home tab, a rolling score preview on the Metronome tab and hi-hat pedal (CC#4) support. Covered by unit tests; the new screens were not looked at in the running app and the V31 has still not been connected.

### Added
- The Metronome tab shows the loaded score as a rolling staff: the practice range of the Score tab repeats, its notes scroll past a fixed line at the metronome's tempo, and the first note arrives with the first beat. With no score loaded it shows the metronome's own bar (accent, beats, subdivisions in brackets). A switch hides it, and "Match score tempo and bar" sets the metronome to the score's tempo and bar length so the click lines up with the bars.
- Hi-hat pedal (CC#4) is read: a stick hit on the closed or open hi-hat note is closed when the pedal is at least three quarters of the way to the "value when closed" and open otherwise. The value is a setting (Settings → MIDI input, default 127) for pedals that never report the full 0-127 range. The monitor shows the pedal value next to each hit.

### Changed
- The application is now called **Roll Model** (window title, binary `roll-model`, release archives, MIDI client names). The settings folder moved from `DrumTutor` to `RollModel` (`drum-tutor` to `roll-model` on Linux): on first start the old folder is copied across, including the `sounds` folder, and is left in place.
- The **Score** tab is greyed out until a score is loaded, and the app starts on **Home**.
- A **Home** tab shows the welcome panel with the recent scores at any time, not only while no score is open.

### Fixed
- The first beat light on the Metronome tab is no longer clipped at its left edge.

## [0.4.1] - 2026-10-25

### Fixed
- The system default sound output is always listed in the output choices, even where the host (for example ALSA on Linux without a sound card) does not enumerate it.

## [0.4.0] - 2026-10-25

Live controls for playback, a click track and metronome, an audio output setting, a configurable score layout and an instrument mixer. Tested on Windows (`x86_64-pc-windows-gnu`, Rust 1.99.1) with unit tests and by measuring the app's sound output; the new cymbals have not been judged by ear and the V31 has still not been connected.

### Added
- The click can be switched on and off at any time, while Listen plays or a take records (checkbox or key `K`). The count-in always sounds.
- Listen has **notes** and **loop** switches: notes off plays a click track of the selected measures, loop repeats them until stopped. A label shows the bar and beat (or the count-in). Changing the range or tempo while it plays restarts it after a moment.
- Metronome tab: a click track without a score (tempo, beats per bar, subdivision, volume, one light per beat), started with `M`.
- Audio output device setting (Settings → Sound), with a fall back to the system default and a message when the chosen device is missing.
- Score layout: *Layout* button with bars per row (auto or 1–8) and a bar that must start a row. A fixed count gives every bar the same width; if that many do not fit, fewer are shown and a note says so.
- Instrument mixer (Settings → Sound): volume, mute and solo per instrument, live while playing, and a button that plays one instrument. WAV files named after an instrument (`Kick.wav`, …) in the `sounds` folder next to the settings file replace the built-in sound.
- An open hi-hat is cut off by a closed or pedal hi-hat.

### Changed
- The cymbals (crashes, china, splash, ride) are rebuilt as dense inharmonic washes. The old ride was a low bell and the crashes were close to white noise; tests now guard their spectrum. Not yet judged by ear.
- New settings keys: `audio_output`, `bars_per_row`, `layout_align_bar`, `gain.<Lane>`, `mute.<Lane>`, `metro_bpm`, `metro_beats`, `metro_sub`.

## [0.3.0] - 2026-10-24

A round of UI tidying. Tested on Windows (`x86_64-pc-windows-gnu`, Rust 1.99.1). The mouse-wheel tuning is covered by headless egui tests but has not been tried with a physical wheel.

### Added
- Welcome panel, shown while no score is open: an open button and the last ten scores opened (click to open, `×` to remove, "Clear list"). Missing files are greyed out. The list is kept in the settings file (`recent` lines).
- Every slider follows the mouse wheel: hover it and scroll to change the value by one step per notch. The wheel is not passed on, so a scrolling page stays put while a slider is tuned.

### Changed
- Settings are split into four pages: MIDI input, Timing, Sound and General. The MIDI output port now has its own section on the Sound page, shared by the metronome and Listen.
- The controls above the score are bigger (text, buttons, check boxes, slider rails and handles) and arranged in three rows: Record/Listen with zoom and follow, the measure range, and tempo/click/count-in.
- Slider captions come before the slider (`from [slider]`, `to [slider]`, `tempo [slider]`, …) everywhere, instead of after it. Combo boxes too.
- Percent and millisecond sliders show only the decimals their step needs ("100%", not "100.0%").

## [0.2.0] - 2026-10-24

Adds playback of the loaded score. Built and tested on Windows (`x86_64-pc-windows-gnu`, Rust 1.99.1); the synthesised drum sounds have not been judged by ear, and the V31 has still not been connected.

### Added
- **Listen** (button on the Score tab, key `L`): plays the selected measures at the practice tempo, with the count-in and click of a take, so the part can be heard before it is played. A playhead follows along; `L` or `Esc` stops it at once.
- Built-in software drum kit for Listen, so no samples or hardware are needed. Accents, ghost notes and grace notes are played with different loudness.
- Listen can also send the notes to a MIDI output (the V31's own sounds, or any GM synth) on a configurable channel, using the standard note of each instrument or the one mapped in Settings.
- New settings: `playback_audio`, `playback_volume`, `playback_midi`, `playback_channel` (Settings → Listening). Settings files from 0.1.0 load unchanged.

## [0.1.0] - 2026-10-24

First tagged version. Built and smoke-tested on Windows (`x86_64-pc-windows-gnu`, Rust 1.99.1); never run with a real Roland V31.

### Added
- MusicXML drum score import (`.musicxml`, `.xml`, `.mxl`): voices, chords, tempo and time-signature changes, repeats with voltas, ties, accents, ghost and grace notes.
- Score tab: score staff with a "You" staff under it on the same time axis, measure-range selection, practice tempo, count-in and metronome, per-note grade colours.
- MIDI input from a Roland V31 (midir) with a configurable and learnable pad map, a MIDI monitor, and an optional computer-keyboard pad input for testing.
- Grading: matching of played hits to score notes, accuracy, timing, extra/missed/wrong-pad counts, dynamics, feedback text, letter grade and overall score.
- Results tab: metrics, timing histogram, per-instrument table, session history and CSV export of a take.
- Settings: latency compensation with calibration, grading tolerances, metronome through the sound card (`audio` feature) and/or MIDI out; settings are saved automatically.
