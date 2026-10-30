# Roll Model

Rust (eframe/egui 0.29) desktop app: MusicXML drum score + Roland V31 MIDI capture + performance grading.

- Start with `HANDOFF.md`: current status, the todo list for the next session, invariants, known gaps.
- User docs: `README.md`.
- Build/test: `cargo build`, `cargo test` (Linux needs `libasound2-dev pkg-config`).
- Versioning is SemVer (see `README.md`): version in `Cargo.toml`, release tags `vX.Y.Z`, user-visible changes go under `[Unreleased]` in `CHANGELOG.md`. Bump `Cargo.toml`, `core/Cargo.toml` and `Cargo.lock` together.
- Score positions are quarter notes (`q`); convert to seconds only via `TempoMap`. The GUI/MIDI-free part (`analysis`, `musicxml`, `notation`, `score`, `drums`, `xml`, `loader`, `rudiments`, `trigger`, `controller`, `controller_layout`, `config`) is the crate `core/` (`roll-model-core`); it must not depend on the app crate or on eframe, midir or cpal.
