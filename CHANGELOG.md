# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html); see "Versioning" in the README.

## [Unreleased]

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
