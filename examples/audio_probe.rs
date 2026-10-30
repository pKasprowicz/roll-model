//! Measures the onset jitter of the program's sound output (task AUD-1, AUD-8).
//!
//! `record <out.wav> <seconds>` records what the default output device plays (WASAPI loopback:
//! the mix before the driver, so the hardware latency is not in it, the timing of the callbacks
//! is). `analyze <in.wav> <period-ms>` finds the click onsets, fits a perfect grid of that period
//! to them and prints how far the onsets fall from it. `tools/audio_baseline.ps1` runs both
//! around the program. Close other programs that make sound first.
//!
//! Build: `cargo build --release --example audio_probe`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::cpal::{self, SampleFormat};

/// A sample louder than this starts a click (the loopback of an idle engine is digital silence).
const THRESHOLD: f32 = 1e-3;
/// Quiet for this long re-arms the detector; a 30 ms click has a zero crossing every ~0.5 ms.
const REARM_SECS: f64 = 0.008;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["record", out, secs] => secs.parse::<f64>().map_err(|e| e.to_string()).and_then(|s| record(out, s)),
        ["analyze", wav, period_ms] => period_ms.parse::<f64>().map_err(|e| e.to_string()).and_then(|p| analyze(wav, p)),
        _ => Err("usage: audio_probe record <out.wav> <seconds> | audio_probe analyze <in.wav> <period-ms>".into()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn record(out: &str, seconds: f64) -> Result<(), String> {
    let device = cpal::default_host().default_output_device().ok_or("no default sound output")?;
    let name = device.name().unwrap_or_default();
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    if supported.sample_format() != SampleFormat::F32 {
        return Err(format!("the output format is {:?}; this tool reads f32 only", supported.sample_format()));
    }
    let (rate, channels) = (supported.sample_rate().0, supported.channels() as usize);
    let config: cpal::StreamConfig = supported.into();

    // (seconds since the first block, channel 0 of the block); a tool, so a lock in the callback is fine.
    let blocks: Arc<Mutex<Vec<(f64, Vec<f32>)>>> = Arc::default();
    let sink = blocks.clone();
    let mut first: Option<cpal::StreamInstant> = None;
    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], info: &cpal::InputCallbackInfo| {
                let at = info.timestamp().capture;
                let origin = *first.get_or_insert(at);
                let t = at.duration_since(&origin).map_or(0.0, |d| d.as_secs_f64());
                let mono: Vec<f32> = data.chunks(channels).map(|f| f[0]).collect();
                if let Ok(mut b) = sink.lock() {
                    b.push((t, mono));
                }
            },
            |e| eprintln!("capture error: {e}"),
            None,
        )
        .map_err(|e| format!("cannot capture from \"{name}\": {e}"))?;
    stream.play().map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_secs_f64(seconds));
    drop(stream);

    // Put the blocks on a time axis: when the device delivered nothing for a while (nothing played),
    // the gap is filled with silence so the onsets keep their real distances.
    let blocks = blocks.lock().map_err(|_| "capture lock poisoned")?;
    let mut samples: Vec<f32> = Vec::new();
    for (t, mono) in blocks.iter() {
        let at = (t * rate as f64).round() as usize;
        if at > samples.len() + (rate as f64 * 0.005) as usize {
            samples.resize(at, 0.0);
        }
        samples.extend_from_slice(mono);
    }
    if samples.is_empty() {
        return Err("nothing was captured".into());
    }
    let spec = hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut writer = hound::WavWriter::create(out, spec).map_err(|e| e.to_string())?;
    for s in &samples {
        writer.write_sample(*s).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    println!("recorded {:.1} s from \"{name}\" at {rate} Hz into {out}", samples.len() as f64 / rate as f64);
    Ok(())
}

/// Seconds at which each click starts.
fn onsets(samples: &[f32], rate: u32) -> Vec<f64> {
    let rearm = (REARM_SECS * rate as f64) as usize;
    let (mut found, mut armed, mut quiet) = (Vec::new(), true, 0usize);
    for (i, s) in samples.iter().enumerate() {
        if s.abs() > THRESHOLD {
            if armed {
                found.push(i as f64 / rate as f64);
                armed = false;
            }
            quiet = 0;
        } else {
            quiet += 1;
            if quiet >= rearm {
                armed = true;
            }
        }
    }
    found
}

fn analyze(path: &str, period_ms: f64) -> Result<(), String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let all: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => reader.samples::<f32>().collect::<Result<_, _>>(),
        (hound::SampleFormat::Int, bits @ (16 | 24 | 32)) => {
            let scale = 1.0 / (1_i64 << (bits - 1)) as f32;
            reader.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect::<Result<_, _>>()
        }
        (f, b) => return Err(format!("{b}-bit {f:?} WAV files are not supported")),
    }
    .map_err(|e| e.to_string())?;
    let mono: Vec<f32> = all.chunks(channels).map(|f| f[0]).collect();

    let peak = mono.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let mut times = onsets(&mono, spec.sample_rate);
    println!("{:.1} s, peak {peak:.3}, {} raw onsets", mono.len() as f64 / spec.sample_rate as f64, times.len());
    // The first and last clicks are the start and the cut of the recording.
    if times.len() > 10 {
        times.drain(..4);
        times.truncate(times.len() - 4);
    }
    if times.len() < 8 {
        let win = (spec.sample_rate as usize / 100).max(1);
        let blocks = mono.chunks(win).count().max(1);
        let silent = mono.chunks(win).filter(|w| w.iter().all(|s| s.abs() <= THRESHOLD)).count() as f64 / blocks as f64;
        if silent < 0.25 {
            return Err(format!("only {:.0}% of the recording is silent (a metronome leaves about half): other sound is playing on this output. Pause the media, close calls and browsers with sound, and record again.", silent * 100.0));
        }
        return Err(format!("only {} clicks found: was the metronome running and audible?", times.len()));
    }
    match stats(&times, period_ms / 1000.0) {
        Some(s) => {
            println!("{}", s.report(path, period_ms));
            Ok(())
        }
        None => Err("the clicks do not fall on a grid of that period: wrong period?".into()),
    }
}

struct Stats {
    clicks: usize,
    missing: usize,
    period_ms: f64,
    std_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}

impl Stats {
    fn report(&self, path: &str, nominal_ms: f64) -> String {
        format!(
            "{path}\n  clicks used        {}  ({} grid slots without a click)\n  period             {:.3} ms (asked for {nominal_ms:.3})\n  onset jitter       std {:.2} ms, 95% within {:.2} ms, worst {:.2} ms",
            self.clicks, self.missing, self.period_ms, self.std_ms, self.p95_ms, self.max_ms
        )
    }
}

/// Fits `t = a + b * k` to the onsets, `k` being the grid slot nearest each onset, and reports the
/// residuals. `None` when the slots do not make sense (a click would be more than a quarter
/// period from its slot).
fn stats(times: &[f64], period: f64) -> Option<Stats> {
    let t0 = times[0];
    let slots: Vec<f64> = times.iter().map(|t| ((t - t0) / period).round()).collect();
    if times.iter().zip(&slots).any(|(t, k)| ((t - t0) - k * period).abs() > period / 4.0) {
        return None;
    }
    let n = times.len() as f64;
    let (mk, mt) = (slots.iter().sum::<f64>() / n, times.iter().sum::<f64>() / n);
    let sxx: f64 = slots.iter().map(|k| (k - mk) * (k - mk)).sum();
    let sxy: f64 = slots.iter().zip(times).map(|(k, t)| (k - mk) * (t - mt)).sum();
    let b = sxy / sxx;
    let a = mt - b * mk;
    let mut resid: Vec<f64> = slots.iter().zip(times).map(|(k, t)| (t - (a + b * k)).abs() * 1000.0).collect();
    let std = (resid.iter().map(|r| r * r).sum::<f64>() / n).sqrt();
    resid.sort_by(|x, y| x.total_cmp(y));
    let last_slot = *slots.last()? as usize;
    Some(Stats {
        clicks: times.len(),
        missing: last_slot + 1 - times.len(),
        period_ms: b * 1000.0,
        std_ms: std,
        p95_ms: resid[((resid.len() as f64 * 0.95) as usize).min(resid.len() - 1)],
        max_ms: *resid.last()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_perfect_grid_has_no_jitter() {
        let times: Vec<f64> = (0..40).map(|k| 1.0 + k as f64 * 0.0625).collect();
        let s = stats(&times, 0.0625).unwrap();
        assert!(s.max_ms < 1e-6 && (s.period_ms - 62.5).abs() < 1e-6 && s.missing == 0);
    }

    #[test]
    fn a_wrong_period_is_refused_and_a_missing_click_is_counted() {
        let times: Vec<f64> = (0..40).filter(|k| *k != 10).map(|k| k as f64 * 0.0625).collect();
        assert_eq!(stats(&times, 0.0625).unwrap().missing, 1);
        assert!(stats(&times, 0.09).is_none());
    }

    #[test]
    fn jitter_shows_up_in_the_residuals() {
        let times: Vec<f64> = (0..40).map(|k| k as f64 * 0.0625 + if k % 2 == 0 { 0.004 } else { 0.0 }).collect();
        let s = stats(&times, 0.0625).unwrap();
        assert!(s.std_ms > 1.5 && s.std_ms < 2.2 && s.max_ms > 1.9 && s.max_ms < 2.3, "{} {}", s.std_ms, s.max_ms);
    }

    #[test]
    fn clicks_in_silence_are_found_once_each() {
        let rate = 48_000;
        let mut v = vec![0.0f32; rate];
        for start in [4800usize, 12_000, 30_000] {
            for i in 0..1440 {
                v[start + i] = ((i as f32) * 0.12).sin() * 0.5;
            }
        }
        let got = onsets(&v, rate as u32);
        assert_eq!(got.len(), 3, "{got:?}");
        assert!((got[1] - 0.25).abs() < 0.001);
    }
}
