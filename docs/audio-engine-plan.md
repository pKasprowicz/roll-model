# Audio engine rework: plan

Status: **in progress on `feature/audio_rework`: AUD-1 to AUD-7 are done (the new engine plays), AUD-8 is left** (2026-10-30). Tasks AUD-1 to AUD-9 in `HANDOFF.md`.

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
- *Checked against the sources* (cpal and `wasapi-rs` read on GitHub, 2026-10-30):
  - cpal 0.15.3 (the lockfile's) uses plain `IAudioClient` in shared mode. `BufferSize::Fixed` only becomes a buffer duration; the shared engine period is fixed, so it can keep or raise the latency, never lower it. A buffer setting is pointless.
  - Its callback thread priority call passes a thread ID where a handle is needed (most likely a no-op) and there is no MMCSS. cpal master fixed the priority call (`GetCurrentThread`).
  - cpal's latest release is 0.18.2 (2026-08-16); master has no `IAudioClient3`. Once rodio is gone, cpal can be upgraded freely; its WASAPI output now accepts formats the built-in resampler can convert, so opening the device at 44.1 kHz may let Windows do the resampling (to be tried in AUD-4).
  - The `wasapi` crate (HEnquist/wasapi-rs 0.26.0, MIT, maintained) wraps `IAudioClient3` (`get_shared_mode_engine_period`, `InitializeSharedAudioStream`), so AUD-9 needs no hand-written `windows`-rs code.
  - cpal's WASAPI callback fills the free space of the buffer, so the block size varies: the mixer must accept any block size. Its callback timestamps come from the device clock (`IAudioClock`, QPC), steadier than `Instant::now()` inside the callback: use them for the frame clock.

## Decisions

1. **Time model, option A (recommended):** a cue is rendered at the frame that matches `zero + t`, and heard one device latency later, as today. Calibration absorbs the delay; the playhead and the MIDI alignment stay as they are in Stage 1.
2. **Option B (only after AUD-1/AUD-8 show that cpal's reported latency is trustworthy):** render at `zero + t - latency`, so the heard sound lands on the grid. That also fixes "the playhead runs ahead of the sound" and finding 4.
3. **Stale calibration:** store a signature (`backend|device|buffer`) next to `latency_ms` and warn in Settings -> Timing when it differs, instead of keeping a map per device.
4. **Engine lifetime:** open on first use and keep, with a latency readout in Settings; optionally pause after about 30 s idle. Never in `DrumApp::new` (the start-up invariant in `HANDOFF.md`).
5. **Settings, as few as possible:** `audio_output` stays. Stage 1 adds one key that records which engine the stored calibration was made with (it replaces the `backend|device|buffer` signature; the buffer is not selectable in shared mode). Old files load with defaults. Minor bump to 0.8.0 with a CHANGELOG entry, because calibration must be redone.
6. **Stage 2 is one checkbox** ("low-latency mode", Windows only) that picks the smallest period the driver reports, with automatic fallback to the cpal backend. No backend or period pickers.
7. **No legacy toggle.** The released v0.7.3 is the A/B baseline; rolling back is a revert.

Where the latency comes from (typical values, not measured): the wait for the next callback (0-10 ms, varying per hit; removed by look-ahead scheduling, which also takes about 5 ms off the mean), opening a stream per start (removed by the persistent engine), and the WASAPI shared period and buffer (about 20-30 ms in all; only `IAudioClient3` lowers it, to about 3-10 ms depending on the driver). Sending the click to the V31 over MIDI (README, Latency) avoids computer-audio latency altogether and needs no code.

Stage 1 removes the jitter, which is the goal. Stage 2 only lowers constant latency, depends on the driver (Realtek onboard often stays about 10 ms) and needs hardware: it is optional and goes behind a go/no-go measurement.

Least impact: the engine is a module of its own (`trigger`, `cancel_run`, `frame_at(Instant)`, latency). The worker threads get an `EngineHandle` through `PlayOptions` and `ClickOptions` in place of `open_audio`; the owner of the stream lives in `DrumApp` (UI thread), is created on first use and rebuilt when `audio_output` changes (as now: "applies the next time sound starts"). Touched: `metronome.rs`, `playback.rs`, the option builders in `session.rs` and `metro.rs`, the Sound page, `Cargo.toml`. `Metronome` and `Player` stay two threads, `core/` is untouched.

## Baseline (AUD-1, done 2026-10-30)

Measured with `tools/audio_baseline.ps1` (and `examples/audio_probe.rs`): the release build of 0.7.3 (rodio 0.19, cpal 0.15.3) with the free metronome at 240 BPM in sixteenths (a click every 62.5 ms, a bar of 1000 ms), 30 s recorded by WASAPI loopback on `Speakers (Realtek(R) Audio)`, 48 kHz, Windows 11. Run it again with `-Exe` pointing at another build to compare (AUD-8); it needs a quiet machine (no other sound), a free keyboard focus for about a minute, and no V31.

| | run 1 | run 2 |
|---|---|---|
| Sound starts found in a bar | 20 | 20 |
| Places in the bar that stay put (spread under 1.1 ms) | 15 | 17 |
| Places that jump between two positions | 5 | 3 |
| Size of the jump | **10.02 ms** | **10.02 ms** |
| Pooled standard deviation of the onsets | 1.09 ms | 0.72 ms |

The jump of exactly 10 ms is one callback period of the WASAPI shared engine: a click whose time falls near a callback boundary is played in the one before or the one after, which is finding 1 measured. The places that "stay put" are not exact either: they sit in the same callback every time, so the clicks come 60 and 70 ms apart instead of 62.5 ms (the places are at 880, 940, 1000, 1060, 1130 ms ... in the bar). The fold in `audio_probe` removes that fixed error, so **the standard deviation understates the problem**; after AUD-6 the places should lie exactly 62.5 ms apart and none should jump.

What the tool does and does not see:
- Loopback records the mix before the driver: it measures the callback timing, not the hardware latency.
- The sound card's effects change a click (the attack is a ramp, the 30 ms burst ends with a pop and rings on), so one click yields two sound starts in the recording; they move together, so the fold is not disturbed, but the report lists twenty places instead of sixteen.
- The default output of this machine was a Bluetooth headset until it was switched off; the measurement is the same, but the delay to the ears there is 100-200 ms and more.

## After the switch-over (AUD-4 to AUD-6, measured 2026-10-30)

The same script, the same machine, the new engine (cpal 0.17, look-ahead 30 ms), 30 s at 240 BPM in sixteenths:

| | v0.7.3 (rodio) | new engine |
|---|---|---|
| Widest spread of a sound start | 10.02 ms | **1.14 ms** (most places 0.04-0.06 ms) |
| Pooled standard deviation | 0.72-1.09 ms | **0.08 ms** |
| Places in the bar that jump | 3-5 of 20 | 0 (the two with a spread over 0.5 ms are ringing artefacts of the sound card's effects, not clicks) |
| Click spacing | 60 and 70 ms alternating | **exactly 62.5 ms** |

Listen on the sample score, recorded by loopback: the drums sound (peak 0.78), and after Esc the output is digitally silent within half a second. What the loopback cannot tell: how the kit sounds (nobody listened), the hardware latency, and the behaviour on a V31. Not tried yet: unplugging the output while it runs, a change of the default device, a long looping Listen with a ramp (AUD-8).

## Steps

| ID | Step | Needs |
|---|---|---|
| **AUD-1** | **Done (2026-10-30).** Baseline, no app change: `tools/audio_baseline.ps1` runs the release build with its own settings folder, records the output by WASAPI loopback and `examples/audio_probe.rs` lays the sound starts over one bar and reports the jitter (results above). Needs a Windows machine with an output device and no other sound, not a V31. The README and the manual already say that sending the click to the V31 over MIDI avoids computer-audio latency. | machine |
| **AUD-2** | **Done on `feature/audio_rework`.** Pure mixer `src/audio/mixer.rs` (no cpal): voice pool, pending list, choke groups, fade, run IDs and `CancelRun`, mono to N channels, sample-format conversion; fixed capacities and no allocation in `render`. Offline tests: onset at the exact frame, mid-block start, choke fade, pool and pending exhaustion with counters, cancel, no allocation. Port the offline mix tests of `playback/tests.rs`. | code |
| **AUD-3** | **Done on `feature/audio_rework`.** Clock and handle: shared counters (`frames`, `rate`, `latency_frames`, `overruns`), the smoothed `Instant <-> frame` estimator built on cpal's device-clock callback timestamps (tested on synthetic jittery times), `EngineHandle` (`Mutex<Producer>`) with `trigger(run, instant, sample, gain, group)`, `cancel_run`, `frame_at`. The stream owner stays on the UI thread. | code |
| **AUD-4** | **Done on `feature/audio_rework`, with cpal 0.17: 0.18 cannot be resolved next to eframe 0.29 (different `objc2-encode`).** cpal backend on **cpal 0.18** (rodio is going, so the upgrade is free; check the API changes since 0.15): open by device name with the existing fallback messages, errors to `ErrorLog`, reopen on the next start after a device loss, variable block sizes, `I16`/`U16` output; try opening the device at 44.1 kHz and letting Windows resample (decides AUD-5); keep the `audio` feature with a no-sound stub. No buffer-size setting. | code, device |
| **AUD-5** | **Done on `feature/audio_rework`; needed (cpal 0.17 has no Windows resampling flag).** Rate-aware kit and clicks, **only if AUD-4 shows that Windows cannot resample for us**: `Kit::load(lanes, sounds_dir, rate)` resamples the built-in sounds and WAVs with a proper interpolator (the linear one in `load_wav` is poor for cymbals); clicks generated at the rate; the kit lives in an engine-owned cache so the callback never frees a sample. Otherwise drop the task. | code |
| **AUD-6** | **Done on `feature/audio_rework`.** Switch-over, together with AUD-4: `Sink` gets `when`; `run_with` waits to `target - LOOKAHEAD` and keeps a due-queue for MIDI; `MAX_LATE` becomes an exact frame check; choke groups replace `Voice`, `play_voice` and `Ringing`; takes, calibration, audition and the metronome tab use the engine (this also ends the duplicated click synthesis of CD-5); the audio thread raises its priority from the first callback; drop rodio. `Metronome` and `Player` stay two threads (merging them is CD-5). | code |
| **AUD-7** | **Done on `feature/audio_rework`; the 0.8.0 bump is left to the release commit.** Settings and docs, trimmed: latency readout and overrun counter on Settings -> Sound; the one key that records which engine the calibration was made with, and an amber "recalibrate" warning in Settings -> Timing; CHANGELOG, 0.8.0 (`Cargo.toml`, `core/Cargo.toml`, `Cargo.lock`), `tools/build_notices.py` for rtrb; manual (Latency, Sound; Bluetooth output is unusable), developer guide (threads; run `tools/build_devguide.py`), README, HANDOFF. No `audio_backend` or `audio_buffer` keys. | code |
| **AUD-8** | Hardware validation: repeat AUD-1; unplug, change of the default device, a long looping Listen with a ramp, exact stop. Record the results in the test plan. | device |
| **AUD-9** | Go/no-go on the AUD-1/AUD-8 numbers; if go, a `cfg(windows)` low-latency backend with the `wasapi` crate (MIT) behind the same render callback: COM as MTA, the smallest period `get_shared_mode_engine_period` reports, MMCSS "Pro Audio", automatic fallback to cpal; a single "low-latency mode" checkbox. Its own release. | decision, device |

Order: AUD-1; then AUD-2, AUD-3 (and AUD-5 if it is needed) as tested code that nothing calls yet; then AUD-4 with AUD-6 as one switch-over; then AUD-7, AUD-8; AUD-9 last.

Docs to update with AUD-7: the manual (Latency and Sound sections), the developer guide threads section (`tools/build_devguide.py` fails if a shown item moves, so run it), `README.md` (Latency), `HANDOFF.md`.

## Progress notes

- **AUD-2/AUD-3** (`src/audio/mixer.rs`, `clock.rs`, `handle.rs`; nothing calls them yet, `mod audio` carries a temporary `allow(dead_code)` that AUD-6 removes). Choke groups are a bit number plus a mask in the trigger; a full pool or a full waiting list drops the *new* sound and counts it (`Counters`); a sound whose frame has passed starts at the first frame of the block and counts as late. The clock is a minimum filter over the last 128 callbacks of `wall - frame / rate` (`Estimator`), published through `Shared`; `Callback::render(out, channels, wall)` is the whole callback and is tested by simulating callbacks: with callbacks up to 4 ms late, 9 ms stalls and a sound card clock 80 ppm fast or slow, sounds start within 0.16 ms of the frame the program's clock names (the measured baseline jumps by 10 ms).
- **For AUD-4:** `wall` is whatever the backend reads from `Instant::now()` early in the callback; check whether cpal's `StreamInstant` (device clock) differences make a steadier `frame` side. The estimator needs no change for that, only what is fed to it. The backend also sets `Shared::set_latency_frames` and converts to `i16`/`u16` output if the device does not take `f32`. `rtrb` 0.4.1 (MIT OR Apache-2.0) is the queue.

- **AUD-7** (`ENGINE_GENERATION` in `src/audio.rs`, `Config::latency_engine` / `latency_is_stale`, `app/audio_output.rs`, the Timing and Sound pages): the stored latency is stale when it is not 0 and was set with another generation (1 = rodio, 2 = this engine); Calibrate and a move of the slider record the current one; a file from 0.7.x shows the note once. The owner listened to Listen on the new engine (2026-10-30): the sound is fine and the pause at the first sound, when the output opens, is acceptable, so the engine is not opened at start-up.

## Risks

- **CD-5 overlap.** AUD-6 touches the timing threads. Swap only their audio output; do not merge them by accident.
- **Only hardware proves the gain** (AUD-1, AUD-8); there is no automated test of the real output.
- **Live switches lag.** Click, notes and mixer changes act up to the look-ahead (about 30 ms) late.
- **Idle cost.** A persistent stream wakes every 10 ms and shows the app in the Windows mixer.
- **Licence.** Every new dependency must be permissive (the project is PolyForm Noncommercial); `rtrb` is MIT/Apache.
