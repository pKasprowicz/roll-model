# Audio driver latency: a study (2026-10-30)

Question: after the engine of 0.8.0 removed the jitter, **what could still lower the constant delay between the program and the speaker**, within the owner's decisions (WASAPI shared mode only; no exclusive mode, no ASIO, no WDM-KS)? This is a study, not a plan: it lists what is known, what was measured, what each option would buy and cost, and the one cheap experiment that could change the AUD-9 no-go. Background: `docs/audio-engine-plan.md` and chapter 6 of the developer guide.

## What is established

Measured on this project's two machines (2026-10-30), by throwaway programs outside the repository (the `wasapi` crate 0.26):

| Fact | Result | Where it came from |
|---|---|---|
| Shared-mode engine period (default / fundamental / min / max) | 480 / 480 / 480 / 480 frames = **10 ms** on all eight outputs seen (Realtek, a Bluetooth headset, a USB audio interface, AMD HD Audio over HDMI/DisplayPort, Steam's virtual devices) | `GetSharedModeEnginePeriod` |
| Endpoint buffer of a shared, event-driven stream opened as cpal opens it | **1056 frames = 22 ms** on all six outputs of the second machine | `initialize_client` with a default duration, then `GetBufferSize` |
| Asking for a smaller buffer | **Does not help**: requests of 5, 10, 15 and 20 ms all gave 22 ms; 30 ms gave 30 ms and 50 ms gave 50 ms. The floor is 22 ms; a larger request is honoured | the same call with `buffer_duration_hns` set |
| cpal's reported latency (the Sound page) | `playback - callback` = the free space of the block being filled, about one period | cpal 0.17.1 source, `output_timestamp` |
| cpal and `IAudioClient3` | cpal 0.17.1 does not use it (no mention in its source). cpal issue 401 added a buffer-size API in 2020 whose WASAPI side reports only the default size as both minimum and maximum | cpal source, GitHub issue 401 |
| Render thread priority | cpal 0.17.1 sets `THREAD_PRIORITY_TIME_CRITICAL` on its Windows render thread (this project does not enable its `audio_thread_priority` feature, which would use MMCSS instead) | cpal source, `boost_current_thread_priority` |

From Microsoft's low-latency audio documentation (updated 2025-03-26, [Low Latency Audio](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/low-latency-audio)):

- Since Windows 10 the audio engine's own latency is about **1.3 ms**; the buffer between the engine and the hardware is **chosen by the driver** (it was always about 10 ms before).
- An application can ask the driver for a smaller period only through `IAudioClient3` (or AudioGraph); **by default every application gets 10 ms buffers**. If one application asks for a small period, every application on that endpoint and mode switches to it until it exits.
- Whether a system offers small periods depends on the **driver**. The documentation says the inbox Microsoft HD Audio driver supports **128 to 480 frames** (2.67 to 10 ms at 48 kHz), and that third-party codec drivers may not. It describes how to switch to the inbox driver in Device Manager (*Update driver, Browse my computer, Let me pick, High Definition Audio Device*) and to switch back.
- Costs of small periods: more wake-ups and so more power, and the effects chain (APOs) adds its own latency, which varies with the effect. A stream may ask for the *raw* signal path (`AUDCLNT_STREAMOPTIONS_RAW`), which skips the OEM's effects and can lower the latency.

What that means for the program's delay: the data written in a callback waits behind what is already in the endpoint buffer (up to 22 ms deep here), then goes through the engine (about 1.3 ms) and the driver and device. Nothing in the numbers contradicts the plan's assumption of roughly 20-30 ms in all on a shared-mode output, but **no end-to-end measurement exists**: a loopback recording sees the mix before the driver.

## Options

| # | Option | Possible gain | Evidence | Cost and risk | Verdict |
|---|---|---|---|---|---|
| A | A smaller output buffer by asking for it | none | Measured: the floor is 22 ms, smaller requests are raised to it | none | **Closed.** (The plan's earlier statement was a reading of the source; it is now a measurement.) |
| B | A `cfg(windows)` backend on `IAudioClient3` asking the driver for its minimum period | period 10 ms to as low as 2.67 ms; likely a smaller buffer too | No output measured offers a range (eight outputs, five drivers) | A second backend (no `unsafe` in this repository if it goes through the `wasapi` crate, but new code); COM, MMCSS; low-latency mode is system-wide for the endpoint and costs power | **No-go as decided (AUD-9)**; E1 found no range on the inbox driver either |
| C | The inbox Microsoft HD Audio driver instead of the vendor's, on onboard codecs | the same as B, if the vendor driver was the reason for 480 | Microsoft documents 128-480 frames for the inbox driver; the first machine's Realtek reported 480 only, which fits a vendor driver (not checked which one it was) | None in the program; the user swaps a driver in Device Manager (reversible). Needs B to use the range | **Tested (E1): no gain**, the inbox driver reports 480 only |
| D | Windows' *audio enhancements* off for the output | a few ms and steadier timing, if effects add latency | Microsoft says APO latency varies; the first machine's loopback shows two sound starts per click, which fits an effects chain; not measured as latency | None in the program; a hint for the manual. Changing it is the user's choice | **Experiment E2**, then a manual tip |
| E | The raw signal path (`AUDCLNT_STREAMOPTIONS_RAW`) | the same as D without the user's setting | Microsoft: "the latency might be improved" | Needs `IAudioClient2::SetClientProperties`, which cpal does not expose: only in a backend like B | Fold into B if B is ever built |
| F | MMCSS "Pro Audio" for the render thread | steadier timing, not lower delay | cpal already uses time-critical priority; a 3-minute soak with 90 s of full CPU load showed no late sound | cpal's `audio_thread_priority` feature pulls in `unsafe` code of a dependency and changes nothing measured | **Not needed** |
| G | Render at the moment minus the latency (the plan's time model B) | lines up audio with MIDI and the playhead; not a lower delay | needs a trustworthy latency, and cpal's is a lower bound | Changes what Calibrate means; raises `ENGINE_GENERATION` | **A separate idea**, see "Beside the driver" |
| H | Smaller buffers on Linux and macOS (`BufferSize::Fixed` in cpal) | possibly a large gain there | cpal has `BufferSize::Fixed`; whether the Linux and macOS hosts honour it as hoped was not checked here | Needs a Linux and a macOS machine to try; the engine already accepts any block size | **Candidate**, unverified, needs the machines |
| I | Exclusive mode, ASIO, WDM-KS | the largest | Microsoft: exclusive mode bypasses the engine | Out of scope by the owner's decision | **Out of scope** |
| J | Send the click and the drums to the V31 over MIDI | removes the whole computer path (stage 5 and 6 of the guide's figure) | the README and manual already say so | none | **Already the recommended way for timing**; it needs no code |

## Experiments

**E1. Does the vendor driver hide a range? (decides whether to reopen AUD-9)** On a machine with an onboard Realtek (or similar) codec: run the throwaway query (the `wasapi_probe` crate described in the plan; the source is short and is recorded below), note the numbers, switch the output to the inbox *High Definition Audio Device* driver as Microsoft describes, reboot if asked, run the query again, and switch back if wanted. *Success:* a minimum below 480 frames. *If so:* AUD-9 changes from no-go to "worth a backend for onboard codecs", because onboard Realtek is the most common output among users; the backend would be option B (with E). *If not:* AUD-9 stays closed with one more datum. The first machine is the one to use (it had the Realtek); the second has no onboard codec in its list. Reversible, but it changes a driver, so it is the owner's to do.

**E1 result.** Measured 2026-10-30 on the first machine (Realtek codec, vendor driver 6.0.9075.1 before; Microsoft inbox *High Definition Audio Device* 10.0.26100.9711 after, confirmed in Device Manager): the shared-mode period range was 480 / 480 / 480 frames (10 ms) both times, and the endpoint buffer 1056 frames (22 ms) both times. **Result: negative. The vendor driver was not what hid the range.** AUD-9 stays closed.

**E2. Do enhancements matter? (a tip, maybe)** With `tools/audio_baseline.ps1`, record the output with the output's enhancements on and with *Disable all enhancements* ticked, on the machine that showed doubled sound starts. The loopback sees the mix before the driver but, as observed, after the effects, so it shows whether the effects change the click's shape and spacing; it does not measure latency. *Success:* the doubled starts disappear. Gain is not latency evidence by itself; if an end-to-end measurement (E3) is ever available, repeat it there.

**E3. A true end-to-end number.** Nothing so far measures the delay from the program to the air, and the Sound page's figure is a lower bound. Two ways, both with extra hardware: (1) a cable from the output of an interface with an input to that input, recorded with `audio_probe` on a capture device instead of the WASAPI loopback (a small change to `tools/audio_baseline.ps1`); (2) use **Calibrate** itself with a pad, which measures the sum of the output delay, the input delay and the player's habit, and is what grading needs anyway. (2) exists today; (1) would add the output delay alone, which matters only for options G and for knowing what a driver change buys.

**E3 result (method 1, 2026-10-31).** `examples/audio_latency.rs` runs the engine's own code on an output stream, places 60 clicks with `EngineHandle::play` at known instants, and records them from an input wired to the output (first machine: the Realtek codec's output cabled to its microphone input, Microsoft inbox driver installed at the time). The engine's `Instant`s and the input's capture stamps are tied together through the output callback, which reads both at once. Three runs: **median 45.16 ms from the `play` instant to the recorded click, std 0.01 ms, 45.13-45.17 ms at the extremes** (60 of 60 clicks found each time; the clock tie is good to well under 0.01 ms). The driver's reported playback delay at the same moments was 10.0 ms, so **the Sound page's figure understates the real delay by about 35 ms here**. The 45 ms is roughly the 22 ms endpoint buffer, two 10 ms periods and a few ms of converter and cable; the split was not measured. The number includes whatever part of the input path the input's capture stamp does not cover (small). It was not repeated with the Realtek vendor driver or on the USB interface.

Source of the query, for E1 (outside the repository, `wasapi = "0.26"`): initialise COM as MTA; `DeviceEnumerator::new()`; for each device of `get_device_collection(&Direction::Render)` take `get_iaudioclient()`, its `get_mixformat()`, and print `get_shared_mode_engine_period(&mix)` (default, fundamental, min, max in frames). Opening a stream like cpal (`initialize_client` with `StreamMode::EventsShared { autoconvert: true, buffer_duration_hns }`, then `get_buffer_size()`) gives the endpoint buffer.

## Beside the driver

Two things that would feel like lower latency without touching the driver:

- **The playhead runs ahead of the sound by the device's delay.** The calibrated `latency_ms` is a good estimate of that delay (it includes the input delay too, which is small). Drawing the playhead that much later, or sending a MIDI click that much later when audio and MIDI sound together, would line them up with what is heard. This needs no driver and no new measurement; it changes what the screen shows, not what is graded. A small code task.
- **Tell users what helps.** The manual says Bluetooth cannot be used and that the click can go to the V31. It could add: wired output; a vendor driver is not the reason for the 10 ms period (E1); enhancements off (after E2).

## Recommendation

1. **Keep AUD-9 closed.** E1 (task AUD-10) was run and did not change the verdict: the inbox driver offers no range either.
2. Do not build anything else for the driver. A buffer request cannot lower the 22 ms floor (measured), the thread priority is already raised, and the remaining options need hardware or are out of scope.
3. Consider the playhead shift as a small, separate task if the lag between the screen and the sound bothers in practice.

## Sources

- Microsoft, [Low Latency Audio](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/low-latency-audio) (Windows drivers documentation, updated 2025-03-26).
- Microsoft, [IAudioClient3::GetSharedModeEnginePeriod](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient3-getsharedmodeengineperiod).
- cpal, [issue 401](https://github.com/RustAudio/cpal/issues/401) (buffer sizes and `IAudioClient3`); cpal 0.17.1 source, `src/host/wasapi/stream.rs`.
- A Stack Overflow report (Windows 10 build 10586) of a minimum period of 128 frames after switching an onboard codec to the Microsoft *High Definition Audio Device* driver: [how to get below 10ms latency using wasapi shared mode](https://kiwix.lema.org/stackoverflow.com_en_all_2023-11/questions/37468283/how-to-get-below-10ms-latency-using-wasapi-shared-mode) (one anecdote, an old build).
- Mozilla cubeb's WASAPI backend and GStreamer's `wasapi` plugin both use `IAudioClient3` when it is available (found by search; not read in full).
