//! Measures how long after the program asks for a sound it comes out of the sound output (task AUD-10, experiment E3).
//!
//! Needs the output wired back to an input (a cable from an interface's output to its input, or a
//! device with a hardware loopback). The tool runs the program's own engine code (`src/audio/`:
//! the clock estimate, the queue and the mixer) on an output stream, places a click at a known
//! instant with `EngineHandle::play`, records the input, finds when each click arrives and prints
//! how late it is.
//!
//! `list` prints the outputs and inputs. `run <output> <input> [clicks]` measures; `<output>` and
//! `<input>` are parts of the names `list` prints ("USB Audio Box" matches both the speakers and the
//! microphone of that device). Turn the loopback level so the recorded clicks are clear but do not
//! clip; the tool prints the peak.
//!
//! What is measured: from the instant given to `play` to the instant the input driver stamps on the
//! block that holds the click's start. That is the output path (engine period, endpoint buffer,
//! driver, converter, cable) plus the part of the input path the stamp does not already cover. The
//! two clocks (the engine's `Instant`s and the input's stamps) are tied together through the output
//! callback, which reads both at the same moment.
//!
//! Build: `cargo build --release --example audio_latency`.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamInstant};

#[path = "audio_latency/engine.rs"]
mod audio;

use audio::handle::channel;

/// Seconds between two clicks. Not a multiple of the 10 ms period, so the clicks meet every phase of it.
const SPACING: f64 = 0.4137;
/// How far ahead of its instant a click is handed to the engine (the program's `AUDIO_LOOKAHEAD`).
const LOOKAHEAD: f64 = 0.030;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["list"] => list(),
        ["run", out, inp] => run(out, inp, 60),
        ["run", out, inp, n] => n.parse().map_err(|e| format!("{e}")).and_then(|n| run(out, inp, n)),
        _ => Err("usage: audio_latency list | audio_latency run <output> <input> [clicks]".into()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn name_of(device: &cpal::Device) -> String {
    let Ok(d) = device.description() else { return String::new() };
    d.extended().first().cloned().unwrap_or_else(|| d.name().to_string())
}

fn list() -> Result<(), String> {
    let host = cpal::default_host();
    println!("outputs:");
    for d in host.output_devices().map_err(|e| e.to_string())? {
        println!("  {}", name_of(&d));
    }
    println!("inputs:");
    for d in host.input_devices().map_err(|e| e.to_string())? {
        println!("  {}", name_of(&d));
    }
    Ok(())
}

fn find(devices: impl Iterator<Item = cpal::Device>, part: &str, what: &str) -> Result<cpal::Device, String> {
    let all: Vec<cpal::Device> = devices.filter(|d| name_of(d).to_lowercase().contains(&part.to_lowercase())).collect();
    match all.len() {
        1 => Ok(all.into_iter().next().unwrap()),
        0 => Err(format!("no {what} has \"{part}\" in its name (try `list`)")),
        _ => Err(format!("several {what}s match \"{part}\": {}", all.iter().map(name_of).collect::<Vec<_>>().join(" | "))),
    }
}

/// Seconds from `b` to `a` (negative when `a` is earlier).
fn signed(a: &StreamInstant, b: &StreamInstant) -> f64 {
    match a.duration_since(b) {
        Some(d) => d.as_secs_f64(),
        None => -b.duration_since(a).map_or(0.0, |d| d.as_secs_f64()),
    }
}

fn run(output: &str, input: &str, clicks: usize) -> Result<(), String> {
    let host = cpal::default_host();
    let out_dev = find(host.output_devices().map_err(|e| e.to_string())?, output, "output")?;
    let in_dev = find(host.input_devices().map_err(|e| e.to_string())?, input, "input")?;
    let (out_name, in_name) = (name_of(&out_dev), name_of(&in_dev));

    let out_cfg = out_dev.default_output_config().map_err(|e| e.to_string())?;
    let in_cfg = in_dev.default_input_config().map_err(|e| e.to_string())?;
    if out_cfg.sample_format() != SampleFormat::F32 || in_cfg.sample_format() != SampleFormat::F32 {
        return Err(format!("formats are {:?} / {:?}; this tool reads f32 only", out_cfg.sample_format(), in_cfg.sample_format()));
    }
    let (rate, out_ch) = (out_cfg.sample_rate(), out_cfg.channels() as usize);
    let (in_rate, in_ch) = (in_cfg.sample_rate(), in_cfg.channels() as usize);
    if rate != in_rate {
        return Err(format!("the output runs at {rate} Hz and the input at {in_rate} Hz; set both to one rate in the Sound settings"));
    }
    let out_config: cpal::StreamConfig = out_cfg.into();
    let in_config: cpal::StreamConfig = in_cfg.into();

    // The engine on an output stream, as the program runs it.
    let origin = Instant::now();
    let (handle, mut callback) = channel(rate, origin);
    // (seconds after `origin` when the callback ran, the stream's stamp of the same moment, reported playback delay).
    type Tick = (f64, StreamInstant, f64);
    let ticks: Arc<Mutex<Vec<Tick>>> = Arc::default();
    let tick_sink = ticks.clone();
    let mut scratch: Vec<f32> = Vec::new();
    let out_stream = out_dev
        .build_output_stream(
            &out_config,
            move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                let wall = origin.elapsed().as_secs_f64();
                let ts = info.timestamp();
                let ahead = ts.playback.duration_since(&ts.callback).map_or(0.0, |d| d.as_secs_f64());
                if let Ok(mut t) = tick_sink.lock() {
                    t.push((wall, ts.callback, ahead));
                }
                scratch.resize(data.len(), 0.0);
                callback.render(&mut scratch, out_ch, wall);
                data.copy_from_slice(&scratch);
            },
            |e| eprintln!("output error: {e}"),
            None,
        )
        .map_err(|e| format!("cannot open \"{out_name}\": {e}"))?;

    // The input: (the stream's stamp of the block's first frame, channel 0).
    let blocks: Arc<Mutex<Vec<(StreamInstant, Vec<f32>)>>> = Arc::default();
    let block_sink = blocks.clone();
    let in_stream = in_dev
        .build_input_stream(
            &in_config,
            move |data: &[f32], info: &cpal::InputCallbackInfo| {
                let mono: Vec<f32> = data.chunks(in_ch).map(|f| f[0]).collect();
                if let Ok(mut b) = block_sink.lock() {
                    b.push((info.timestamp().capture, mono));
                }
            },
            |e| eprintln!("input error: {e}"),
            None,
        )
        .map_err(|e| format!("cannot capture from \"{in_name}\": {e}"))?;
    in_stream.play().map_err(|e| e.to_string())?;
    out_stream.play().map_err(|e| e.to_string())?;
    if !handle.wait_ready(Duration::from_secs(3)) {
        return Err("the output stream did not start".into());
    }
    std::thread::sleep(Duration::from_millis(1500)); // the clock estimate settles

    // A click: a 2 kHz burst of 3 ms with a 0.3 ms attack, loud enough to stand out of the loopback's noise.
    let burst: Arc<[f32]> = (0..(rate as f64 * 0.003) as usize)
        .map(|i| {
            let t = i as f64 / rate as f64;
            ((std::f64::consts::TAU * 2000.0 * t).sin() * (t / 0.0003).min(1.0) * 0.7) as f32
        })
        .collect::<Vec<_>>()
        .into();
    let run_id = handle.new_run();
    let first_at = origin.elapsed().as_secs_f64() + 1.0;
    let mut targets = Vec::with_capacity(clicks);
    let mut refused = 0usize;
    for k in 0..clicks {
        let t = first_at + k as f64 * SPACING;
        let when = origin + Duration::from_secs_f64(t);
        if let Some(wait) = when.checked_duration_since(Instant::now() + Duration::from_secs_f64(LOOKAHEAD)) {
            std::thread::sleep(wait);
        }
        match handle.play(run_id, when, &burst, 1.0, 0, 0) {
            Ok(()) => targets.push(t),
            Err(e) => {
                refused += 1;
                eprintln!("click {k} refused: {e:?}");
            }
        }
    }
    std::thread::sleep(Duration::from_millis(800));
    drop(in_stream);
    drop(out_stream);

    // Tie the clocks: at each output callback, `wall` and the stream's stamp are read together, so
    // wall - stamp is a constant plus a small delay; its smallest value is the constant.
    let ticks = ticks.lock().map_err(|_| "lock")?;
    let blocks = blocks.lock().map_err(|_| "lock")?;
    let (_, t0, _) = *ticks.first().ok_or("no output callbacks ran")?;
    let offset = ticks.iter().map(|(w, s, _)| w - signed(s, &t0)).fold(f64::MAX, f64::min);
    let spread = ticks.iter().map(|(w, s, _)| w - signed(s, &t0) - offset).fold(0.0, f64::max);
    let reported = ticks.iter().map(|t| t.2).sum::<f64>() / ticks.len() as f64;

    // The recording on the engine's time axis: (seconds after origin, sample).
    let mut times: Vec<f64> = Vec::new();
    let mut samples: Vec<f32> = Vec::new();
    for (cap, mono) in blocks.iter() {
        let start = signed(cap, &t0) + offset;
        for (i, s) in mono.iter().enumerate() {
            times.push(start + i as f64 / rate as f64);
            samples.push(*s);
        }
    }
    if samples.is_empty() {
        return Err("nothing was captured".into());
    }
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let quiet_end = times.partition_point(|t| *t < first_at - 0.2);
    let noise = samples[..quiet_end].iter().fold(0.0f32, |m, s| m.max(s.abs()));
    println!("output \"{out_name}\"  input \"{in_name}\"  {rate} Hz");
    println!("recorded peak {peak:.3}, noise before the first click {noise:.4}");
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    let low = samples.iter().fold(0.0f32, |m, s| m.min(*s));
    println!("level: mean {mean:.3}, lowest {low:.3}");
    if peak < 0.05 || peak < noise * 8.0 {
        return Err("the clicks are not clear of the noise: is the loopback connected and its level up?".into());
    }
    let threshold = (peak * 0.15).max(noise * 4.0);

    let mut delays = Vec::new();
    for t in &targets {
        let lo = times.partition_point(|x| *x < t - 0.02);
        let hi = times.partition_point(|x| *x < t + 0.25);
        if let Some(j) = samples[lo..hi].iter().position(|s| s.abs() > threshold) {
            delays.push((times[lo + j] - t) * 1000.0);
        }
    }
    if delays.len() < targets.len() / 2 {
        return Err(format!("only {} of {} clicks were found", delays.len(), targets.len()));
    }
    delays.sort_by(f64::total_cmp);
    let n = delays.len() as f64;
    let mean = delays.iter().sum::<f64>() / n;
    let std = (delays.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / n).sqrt();
    println!("clicks found {} of {} ({} refused)", delays.len(), targets.len() + refused, refused);
    println!("clock tie: {} output callbacks, estimated constant uncertain by up to {:.2} ms", ticks.len(), spread * 1000.0);
    println!(
        "delay from `play` instant to the recorded click: median {:.2} ms, mean {:.2} ms, std {:.2} ms, min {:.2} ms, max {:.2} ms",
        delays[delays.len() / 2],
        mean,
        std,
        delays[0],
        delays[delays.len() - 1]
    );
    println!("the driver's reported playback delay (the Sound page's figure): {:.2} ms", reported * 1000.0);
    Ok(())
}
