# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html); see "Versioning" in the README.

## [Unreleased]

## [0.1.0] - 2026-10-24

First tagged version. Built and smoke-tested on Windows (`x86_64-pc-windows-gnu`, Rust 1.99.1); never run with a real Roland V31.

### Added
- MusicXML drum score import (`.musicxml`, `.xml`, `.mxl`): voices, chords, tempo and time-signature changes, repeats with voltas, ties, accents, ghost and grace notes.
- Score tab: score staff with a "You" staff under it on the same time axis, measure-range selection, practice tempo, count-in and metronome, per-note grade colours.
- MIDI input from a Roland V31 (midir) with a configurable and learnable pad map, a MIDI monitor, and an optional computer-keyboard pad input for testing.
- Grading: matching of played hits to score notes, accuracy, timing, extra/missed/wrong-pad counts, dynamics, feedback text, letter grade and overall score.
- Results tab: metrics, timing histogram, per-instrument table, session history and CSV export of a take.
- Settings: latency compensation with calibration, grading tolerances, metronome through the sound card (`audio` feature) and/or MIDI out; settings are saved automatically.
