# Audio engine rework: plan

Status: **proposed, nothing implemented** (2026-10-30). Tasks AUD-1 to AUD-9 in `HANDOFF.md`.

Origin: a handover note ("low-latency audio rework", written the same day from a read of the repository on GitHub, nothing compiled or measured). This plan checks that note against the source and corrects it. All latency figures are typical values, not measurements of this app, until AUD-1 has run.

## Goal and scope

Make the computer-audio output (click, Listen with the built-in kit, calibration clicks, audition, the metronome tab) low-latency and above all **timing-stable** on Windows x64. A constant delay is removed by Calibrate; random jitter is not. Grading uses perfect <= 20 ms and good <= 45 ms; a 16th at 240 BPM is 62 ms.

Decided by the owner: **WASAPI shared mode is enough.** Exclusive mode, ASIO and WDM-KS are out of scope; do not add them. Linux and macOS must keep building (the cpal backend stays the portable one).

## What the code does today (verified)

- `Cargo.toml`: feature `audio = ["dep:rodio"]`, rodio 0.19 without default features; `Cargo.lock` has cpal 0.15.3.
- `metronome.rs`: `open_audio`, `click_audio` (a 30 ms `SineWave` through `play_raw`), `output_devices`, `default_output_device`, `at`, `wait_until`. `Metronome` (takes, calibration) and `Player` (Listen, metronome tab, audition) are two threads; each opens its own stream after `zero` is fixed.
- `playback.rs`: `run_with` waits to each cue's `Instant`, then `dispatch` calls a `Sink`; drums are `Voice`s (rodio sources) with a per-hit `Arc<AtomicBool>` choke flag. Cues later than `MAX_LATE` (250 ms) are dropped.
- `synth.rs`: every sample is mono at the fixed `SAMPLE_RATE` (44,100 Hz); `load_wav` resamples linearly. Rodio resamples to the device rate on the fly today.
- `latency_ms` is applied in one place: `record_take_hit` (`src/app.rs`). Calibration (`session.rs`) only sets that number.

## Findings

1. **Onsets are not sample-accurate.** `play_raw` queues into rodio's mixer; the sound starts at the next audio callback, so onsets are quantized to the callback period (about 10 ms in shared mode) and vary per hit.
2. **A stream is opened per start, on the worker thread**, and cues later than 250 ms are dropped, so early count-in clicks can vanish on a slow device.
3. **The buffer size is not controllable** through rodio 0.19.
4. **MIDI and audio click are not aligned** when both are on: MIDI goes out at once, audio arrives one device latency later.
5. **No thread priority** (MMCSS "Pro Audio") on the scheduler thread; unknown for cpal's callback thread.
6. **Calibration is not a pure latency measurement** (it includes the player's anticipation) and becomes invalid when backend, device or buffer changes.

## Corrections to the handover note

- *"Keep the `Sink` trait; the test fakes keep working unchanged."* Only partly. Audio must be queued about 30-40 ms ahead while MIDI must still go out at the exact moment, so `Sink` methods take a `when: Instant` and `run_with` changes. `dispatch` and `playback/tests.rs` need small signature changes; their assertions stay.
- *"`zero_frame(lead_ms)`."* The app picks `zero: Instant` itself (now + 0.5 s + count-in) and takes, the playhead and grading all use it. The engine needs a smoothed `Instant <-> frame` mapping, estimated from the callbacks. Without smoothing the callback's own wake-up jitter comes back as onset jitter. This is the most important design item.
- *Frees in the callback.* The sketch's per-hit `Arc<AtomicBool>` is last dropped by the callback, once the scheduler's `ringing` list lets go; so is an `Arc` sample if the `Kit` is dropped while a voice plays. Triggers carry a choke group and a choke mask instead, and the engine owns a long-lived sample cache.
- *Stop and cancel.* The stream now outlives runs, so `Player::stop` and Listen's restart need a `CancelRun(id)` command that clears pending triggers and fades out that run's voices. Dropping the stream no longer does it.
- *Several producers.* An `rtrb` queue is single-producer; Player, Metronome, audition and the metronome tab can overlap. Put the producer behind a `Mutex` (only the callback has to be lock-free).
- *The stream may not be `Send`.* Split the engine into an owner on the UI thread (holds the `cpal::Stream`) and a cloneable `EngineHandle` (queue, shared counters, kit cache) for the threads.
- Smaller: handle `I16`/`U16` output, not only `f32`; report errors through `ErrorLog` (the repo has no `eprintln!`); count overruns in the shared state and show them in Settings.
- *Unmeasured.* Whether `BufferSize::Fixed` changes the WASAPI shared period in cpal 0.15.3 (believed: it only sets the buffer, the period stays about 10 ms); whether cpal sets MMCSS priority; whether a newer cpal already does better (once rodio is gone, cpal can be upgraded freely). Stage 2 is therefore gated on a measurement.

## Decisions

1. **Time model, option A (recommended):** a cue is rendered at the frame that matches `zero + t`, and heard one device latency later, as today. Calibration absorbs the delay; the playhead and the MIDI alignment stay as they are in Stage 1.
2. **Option B (only after AUD-1/AUD-8 show that cpal's reported latency is trustworthy):** render at `zero + t - latency`, so the heard sound lands on the grid. That also fixes "the playhead runs ahead of the sound" and finding 4.
3. **Stale calibration:** store a signature (`backend|device|buffer`) next to `latency_ms` and warn in Settings -> Timing when it differs, instead of keeping a map per device.
4. **Engine lifetime:** open on first use and keep, with a latency readout in Settings; optionally pause after about 30 s idle. Never in `DrumApp::new` (the start-up invariant in `HANDOFF.md`).
5. **Settings:** `audio_output` stays; add `audio_backend`, `audio_buffer` and the calibration signature. Old files load with defaults. Minor bump to 0.8.0 with a CHANGELOG entry, because calibration must be redone.

Stage 1 removes the jitter, which is the goal. Stage 2 (`IAudioClient3`) only lowers constant latency, depends on the driver (Realtek onboard often stays about 10 ms) and needs hardware: it is optional and goes behind a go/no-go measurement.

## Steps

| ID | Step | Needs |
|---|---|---|
| **AUD-1** | Baseline: record a loopback or microphone capture of the current engine playing 16th clicks at 240 BPM and measure the onset spread. Add the procedure to `docs/hardware-test-plan.md`. | device |
| **AUD-2** | Pure mixer `src/audio/mixer.rs` (no cpal): voice pool, pending list, choke groups, fade, run IDs and `CancelRun`, mono to N channels, sample-format conversion; fixed capacities and no allocation in `render`. Offline tests: onset at the exact frame, mid-block start, choke fade, pool and pending exhaustion with counters, cancel, no allocation. Port the offline mix tests of `playback/tests.rs`. | code |
| **AUD-3** | Clock and handle: shared counters (`frames`, `rate`, `latency_frames`, `overruns`), the smoothed `Instant <-> frame` estimator (tested on synthetic jittery callback times), `EngineHandle` (`Mutex<Producer>`) and `trigger(run, instant, sample, gain, group)`. | code |
| **AUD-4** | cpal backend: open by device name with the existing fallback messages, request a fixed buffer and read back what was granted, error callback to `ErrorLog`, reopen on device loss; keep the `audio` feature with a no-sound stub. | code, device |
| **AUD-5** | Rate-aware kit: `Kit::load(lanes, sounds_dir, rate)` resamples the built-in sounds and WAVs to the device rate with a proper interpolator (the linear one in `load_wav` is poor for cymbals); clicks generated at the rate; the kit lives in an engine-owned cache. | code |
| **AUD-6** | Rewire: `Sink` gets `when`; `run_with` waits to `target - LOOKAHEAD` and keeps a due-queue for MIDI; `MAX_LATE` becomes an exact frame check; choke groups replace `Voice`, `play_voice` and `Ringing`; `Metronome`, audition, the metronome tab and calibration use the engine; remove rodio. `Metronome` and `Player` stay two threads (merging them is CD-5). | code |
| **AUD-7** | Settings and calibration: latency readout and overrun counter on Settings -> Sound, the calibration signature and its warning, new keys, CHANGELOG, 0.8.0 (`Cargo.toml`, `core/Cargo.toml`, `Cargo.lock`), `tools/build_notices.py` for `rtrb`, a note in the manual that Bluetooth output is unusable (100-200 ms and more). | code |
| **AUD-8** | Hardware validation: repeat AUD-1; unplug, change of the default device, a long looping Listen with a ramp, exact stop. Record the results in the test plan. | device |
| **AUD-9** | Stage 2 decision on the AUD-1/AUD-8 numbers; if go, a `cfg(windows)` `IAudioClient3` backend behind the same trait (COM as MTA, MMCSS "Pro Audio", fall back to cpal), its own release. First check what cpal and the `wasapi` crate already offer. | decision, device |

Docs to update with AUD-7: the manual (Latency and Sound sections), the developer guide threads section (`tools/build_devguide.py` fails if a shown item moves, so run it), `README.md` (Latency), `HANDOFF.md`.

## Risks

- **CD-5 overlap.** AUD-6 touches the timing threads. Swap only their audio output; do not merge them by accident.
- **Only hardware proves the gain** (AUD-1, AUD-8); there is no automated test of the real output.
- **Live switches lag.** Click, notes and mixer changes act up to the look-ahead (about 30 ms) late.
- **Idle cost.** A persistent stream wakes every 10 ms and shows the app in the Windows mixer.
- **Licence.** Every new dependency must be permissive (the project is PolyForm Noncommercial); `rtrb` is MIT/Apache.
