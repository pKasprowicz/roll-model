# Resume prompt: the audio latency work

Paste everything below the line into a fresh Claude Code session started in a checkout of this repository on the other machine. It is written for a reader with no memory of the earlier sessions. Check it against the repository: the plan and `HANDOFF.md` are the record, this is the entry point.

---

You are resuming work on **Roll Model** (Rust, eframe/egui 0.29, `https://github.com/pKasprowicz/roll-model`): a MusicXML drum-score trainer that captures a Roland V31 over MIDI and grades the playing. The task in hand is **lower and steadier computer-audio output latency on Windows** (the click, Listen with the built-in kit, calibration clicks). It is almost finished.

## First, read these, in this order

1. `CLAUDE.md`, then `HANDOFF.md` (the project's own handoff: invariants, task index, known gaps).
2. `docs/audio-engine-plan.md`: the plan, the corrections to the original proposal, **all measurements** (baseline, after the switch-over, long run and load, what the drivers offer, the estimated gain) and the progress notes.
3. `git log --oneline develop..feature/audio_rework` and `CHANGELOG.md` `[Unreleased]`.

## Where things stand

- Branch: **`feature/audio_rework`** (cut from `develop`; pushed to `origin`). `develop`/`main` have none of this. Nothing is released; the version in `Cargo.toml` is still 0.7.3.
- GitHub issues **#38 to #46** are the tasks AUD-1 to AUD-9 (labelled `sound`; #33 is the related CD-5). They are commented with results and **stay open until the branch is merged**.
- **Done:** AUD-1 (baseline measurement), AUD-2/3 (pure mixer, frame clock, engine handle: `src/audio/mixer.rs`, `clock.rs`, `handle.rs`), AUD-4/5/6 (cpal backend, resampling, rewiring of takes, Listen, metronome tab, calibration and auditions; rodio removed), AUD-7 (Settings → Sound readout, Settings → Timing warning when the stored latency is from another engine), AUD-8 (load and soak runs; they found and fixed a late first click per looped pass).
- **Result in one line:** on the development machine (Windows 11, Realtek, 48 kHz, WASAPI shared) the widest onset spread of a 240 BPM sixteenths metronome fell from **10.02 ms to about 1 ms** (per place in the bar 0.04-0.06 ms), the click spacing from 60/70 ms alternating to **exactly 62.5 ms**, and the estimated average latency gain is **about 5 ms** (0-10 ms per sound; derivation in the plan). Late sounds in a 3-minute soak with 90 s of full CPU load: 0.
- **Design in one paragraph:** one persistent cpal output stream. A scheduler thread hands each sound to the engine about 30 ms ahead (`AUDIO_LOOKAHEAD`) with the `Instant` it should start at; the callback keeps a minimum-filter estimate of which frame it renders at that instant (`clock.rs`); the mixer (`mixer.rs`, fixed pools, no allocation, locks or frees in the callback, finished samples go back through a graveyard queue) starts the sound at exactly that frame. `Sink` methods carry `when: Instant`; MIDI is held back to its exact time (`DeviceSink`). `DrumApp.audio` (`src/app/audio_output.rs`) keeps one engine open and `ensure_audio()` runs before every start. WASAPI shared mode only: **exclusive mode and ASIO are out of scope by the owner's decision, do not add them.**

## What is left

1. **AUD-8, the checks that need a person at the machine** (they cannot be scripted): unplug the output while it plays and plug it back; change the default output device in Windows while the program runs (the stream should be opened again at the next start; `Engine::has_failed`, `AudioOutput::ensure`); a long **looping Listen with a tempo ramp** (looping is a session-only checkbox on the Score tab, and so is the tempo ramp: the *ramp +* and *every* sliders next to it; settings `ramp_percent`, `ramp_passes`). Read the Sound page afterwards: it must show no "too late" line. Write the results into the plan and tick #45.
2. **AUD-9 decision (the owner's):** evidence says **no-go on the development machine**: the Realtek and Bluetooth drivers both report a shared-mode period range of exactly 480 frames (10 ms), so `IAudioClient3` has nothing smaller to ask for. If you are on a machine with another audio interface, re-run the query (a throwaway crate with `wasapi = "0.26"`: `get_shared_mode_engine_period` on the mix format of each render device) and add the numbers to the plan. Only if some driver offers a range is a `cfg(windows)` backend with the `wasapi` crate worth building, as one more backend behind the same callback and a single "low-latency mode" checkbox. Otherwise close #46.
3. **Release:** the **0.8.0 bump is deliberately not done** (it belongs in the release commit: `Cargo.toml`, `core/Cargo.toml` and `Cargo.lock` together, `CHANGELOG.md` `[Unreleased]` → `[0.8.0]`, per `CLAUDE.md`). Merge to `develop` first; the issues #38-#46 close when the work reaches `main`. Users must run **Calibrate** again after the update (the Timing page says so).
4. Not asked for, but known: SND-1/SND-3 (listening to the cymbals and the WAV override) are still open; the owner listened to Listen on the new engine and found it fine.

## Build, test, measure

```
cargo build --release
cargo test                      # expect 153 (app) + 110 (core) + 3 passing
cargo build --release --example audio_probe
powershell -NoProfile -ExecutionPolicy Bypass -File tools\audio_baseline.ps1 -Seconds 30
powershell -NoProfile -ExecutionPolicy Bypass -File tools\audio_baseline.ps1 -Seconds 30 -LoadThreads <logical processors>   # busy machine
```

`tools/audio_baseline.ps1` runs the release build with its own settings folder, starts the metronome (240 BPM, sixteenths) by sending keys, records the default output by WASAPI loopback and prints the jitter per place in the bar (`examples/audio_probe.rs`). It needs a Windows machine with an output device and **no other sound playing**, and it takes the keyboard focus for about a minute; it sends keys only while the program is the foreground window. To compare with the old engine, build the `v0.7.3` tag in a separate worktree and pass its exe with `-Exe`. The counters of the engine (sounds too late, dropped) are on Settings → Sound once a sound has played. The documentation builds (`tools/build_manual.py`, `tools/build_devguide.py`, `tools/build_site.py`, `tools/build_notices.py`, standard-library Python) must keep working: the developer guide fails if an item it shows is renamed or moved.

## Things that will bite you

- **Windows with `autocrlf`: working files may be CRLF.** `perl -0pi` patterns with `\n` then match nothing, silently. Edit with the Edit tool or a small Python script that normalises `\r\n`, and check the edit applied. After `git stash pop` files are rewritten as CRLF.
- Python may not be on `PATH` in a session started before it was installed (the Microsoft Store stub answers instead); call the interpreter by its full path.
- `gh` may not be on `PATH`: `C:\Program Files\GitHub CLI\gh.exe`. Give multi-line text with `--body-file`.
- cpal **0.18 cannot be resolved next to eframe 0.29** (different `objc2-encode`), hence cpal **0.17**. cpal 0.17's `name()` on WASAPI is the short description; `name_of` in `src/audio/backend.rs` keeps the friendly name that settings files hold. Do not "upgrade" either without reading `HANDOFF.md`'s note.
- The repository contains **no `unsafe`**. Raising the scheduler thread's priority was considered and was not needed; adding the first `unsafe` is the owner's call.
- The loopback tap sees the mix before the driver, after the sound card's effects: a click leaves two sound starts in the recording (the burst and the pop at its end), so the report lists twenty places for sixteen clicks per bar. That is expected. A "deviation from the ideal grid" metric is confounded by those effects; use the per-place spread and the click spacing.
- Do not start real audio in a unit test (set `cfg.click_audio`/`playback_audio` to false); the playback core is tested with a fake `Sink` and with simulated callbacks.
- Repository rules: pull requests are not accepted, the author makes every change; commits end with the attribution line given by the session's instructions; every commit message that closes nothing says `Refs #N`; keep `HANDOFF.md`, the plan and `CHANGELOG.md` in step with the code; check any new dependency's licence (PolyForm Noncommercial project) and keep `tools/build_notices.py` working.

## How to work

Start by reading, then run `cargo test` and one `audio_baseline.ps1` run to confirm the numbers in the plan on this machine (they depend on the machine; compare runs on the same one). Then do the remaining items above that need hardware with the owner, record each result in `docs/audio-engine-plan.md` and `HANDOFF.md`, comment on the issue, commit on `feature/audio_rework`, and ask before pushing or merging.
