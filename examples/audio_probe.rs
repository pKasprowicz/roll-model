//! Measures the onset jitter of the program's sound output (task AUD-1, AUD-8).
//!
//! `record <out.wav> <seconds>` records what the default output device plays (WASAPI loopback:
//! the mix before the driver, so the hardware latency is not in it, the timing of the callbacks
//! is). `analyze <in.wav> <bar-ms>` finds every start of a sound, lays the starts over one bar of
//! the repeating metronome pattern and prints how far each start wanders from its own typical place
//! in the bar. That does not depend on what the sound card makes of a click (it may ring on, or soften
//! the attack), only on the pattern repeating every `bar-ms`. `tools/audio_baseline.ps1` runs both
//! around the program. Close other programs that make sound first.
//!
//! Build: `cargo build --release --example audio_probe`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::cpal::{self, SampleFormat};

/// A sound starts when the envelope rises through this level...
const HIGH: f32 = 0.05;
/// ...after it has been below this level. What the sound card does to a click keeps the level between
/// clicks above digital silence, but well below `HIGH`.
const LOW: f32 = 0.02;
/// Starts of sounds closer than this are one start (a ripple at the beginning of a sound).
const MIN_GAP_SECS: f64 = 0.012;
/// Two starts further apart than this in the bar do not belong to the same place in it.
const CLUSTER_GAP_SECS: f64 = 0.012;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["record", out, secs] => secs.parse::<f64>().map_err(|e| e.to_string()).and_then(|s| record(out, s)),
        ["analyze", wav, bar_ms] => bar_ms.parse::<f64>().map_err(|e| e.to_string()).and_then(|p| analyze(wav, p)),
        _ => Err("usage: audio_probe record <out.wav> <seconds> | audio_probe analyze <in.wav> <bar-ms>".into()),
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
    // the gap is filled with silence so the starts keep their real distances.
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
        return Err("nothing was captured: was anything playing on the default output?".into());
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

/// Seconds at which a sound starts: the first sample of the first 0.5 ms block of the envelope that is
/// over `HIGH`, once the envelope has been under `LOW` and at least `MIN_GAP_SECS` have passed since
/// the previous start. A start is found a little after its true beginning, by about the same amount
/// each time, so the spread between starts is not affected.
fn onsets(samples: &[f32], rate: u32) -> Vec<f64> {
    let block = (rate as usize / 2000).max(1);
    let (mut found, mut armed, mut last) = (Vec::new(), true, f64::NEG_INFINITY);
    for (j, chunk) in samples.chunks(block).enumerate() {
        let level = chunk.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let t = (j * block) as f64 / rate as f64;
        if level < LOW {
            armed = true;
        } else if armed && level > HIGH && t - last >= MIN_GAP_SECS {
            let first = chunk.iter().position(|s| s.abs() > LOW).unwrap_or(0);
            let at = (j * block + first) as f64 / rate as f64;
            found.push(at);
            last = at;
            armed = false;
        }
    }
    found
}

fn analyze(path: &str, bar_ms: f64) -> Result<(), String> {
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
    println!("{path}\n  {:.1} s, peak {peak:.3}, {} sound starts", mono.len() as f64 / spec.sample_rate as f64, times.len());
    // The first and last seconds are the start and the cut of the recording.
    times.retain(|t| *t > 1.0 && *t < mono.len() as f64 / spec.sample_rate as f64 - 1.0);
    match fold(&times, bar_ms / 1000.0) {
        Some(f) => {
            println!("{}", f.report());
            Ok(())
        }
        None => Err(format!("too few sound starts ({}) or they do not repeat every {bar_ms} ms: was the metronome running and audible?", times.len())),
    }
}

/// The starts of the sounds, laid over one bar.
struct Fold {
    bars: usize,
    bar_ms: f64,
    /// (position in the bar in ms, starts seen, std in ms, widest spread in ms), in bar order.
    places: Vec<(f64, usize, f64, f64)>,
    pooled_std_ms: f64,
}

impl Fold {
    fn report(&self) -> String {
        let mut s = format!("  bar                {:.3} ms, {} bars, {} places in a bar\n", self.bar_ms, self.bars, self.places.len());
        for (pos, n, std, spread) in &self.places {
            s += &format!("    at {pos:7.1} ms   {n:4} starts   std {std:5.2} ms   spread {spread:5.2} ms\n");
        }
        s += &format!("  onset jitter       pooled std {:.2} ms, widest spread {:.2} ms", self.pooled_std_ms, self.places.iter().map(|p| p.3).fold(0.0, f64::max));
        s
    }
}

/// Groups starts by their place in a bar (each is `(time, place)`); the cut is at the widest empty
/// stretch so no group is split by the end of the bar.
fn group(ph: &[(f64, f64)], bar: f64) -> Option<Vec<Vec<(f64, f64)>>> {
        let mut sorted = ph.to_vec();
        sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
        let n = sorted.len();
        let cut = (0..n).max_by(|&i, &j| {
            let gap = |k: usize| (sorted[(k + 1) % n].1 - sorted[k].1).rem_euclid(bar);
            gap(i).total_cmp(&gap(j))
        })? + 1;
        sorted.rotate_left(cut % n);
        let mut groups: Vec<Vec<(f64, f64)>> = Vec::new();
        let mut prev: Option<f64> = None;
        for (t, p) in sorted {
            let unwrapped = prev.map_or(p, |q| q + (p - q).rem_euclid(bar));
            if prev.is_none() || unwrapped - prev.unwrap() > CLUSTER_GAP_SECS {
                groups.push(Vec::new());
            }
            groups.last_mut().unwrap().push((t, unwrapped));
            prev = Some(unwrapped);
        }
    Some(groups)
}

/// Lays the starts over bars of about `bar` seconds and groups them by their place in the bar. The
/// bar length is refined first (the sound card's clock is not exactly the computer's) by fitting the
/// most frequent place to the bar number. `None` when there are too few starts for it.
fn fold(times: &[f64], bar: f64) -> Option<Fold> {
    if times.len() < 24 {
        return None;
    }
    let t0 = times[0];
    let phases = |bar: f64| -> Vec<(f64, f64)> { times.iter().map(|t| (t - t0, (t - t0).rem_euclid(bar))).collect() };

    let groups = group(&phases(bar), bar)?;
    let biggest = groups.iter().max_by_key(|g| g.len())?;
    // Fit t = a + b * bar_number over the most frequent place.
    let (xs, ys): (Vec<f64>, Vec<f64>) = biggest.iter().map(|(t, _)| (((t - biggest[0].0) / bar).round(), *t)).unzip();
    let n = xs.len() as f64;
    let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
    let sxx: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
    let bar = if sxx > 0.0 { xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum::<f64>() / sxx } else { bar };
    if !(bar > 0.0) || (bar - bar.max(0.0)).abs() > 1.0 {
        return None;
    }

    let groups = group(&phases(bar), bar)?;
    let mut places = Vec::new();
    let (mut sq, mut count) = (0.0, 0usize);
    for g in groups.iter().filter(|g| g.len() >= 8) {
        let ps: Vec<f64> = g.iter().map(|(_, p)| *p).collect();
        let mean = ps.iter().sum::<f64>() / ps.len() as f64;
        let ss: f64 = ps.iter().map(|p| (p - mean) * (p - mean)).sum();
        let (lo, hi) = ps.iter().fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(*p), h.max(*p)));
        places.push((mean * 1000.0, ps.len(), (ss / ps.len() as f64).sqrt() * 1000.0, (hi - lo) * 1000.0));
        sq += ss;
        count += ps.len();
    }
    if places.is_empty() || count == 0 {
        return None;
    }
    places.sort_by(|a, b| a.0.total_cmp(&b.0));
    let bars = (times.last()? - t0) / bar;
    Some(Fold { bars: bars.round() as usize, bar_ms: bar * 1000.0, places, pooled_std_ms: (sq / count as f64).sqrt() * 1000.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Starts of `per_bar` sounds at fixed places in a bar, each moved by `jitter(n)` seconds.
    fn starts(bars: usize, bar: f64, places: &[f64], jitter: impl Fn(usize) -> f64) -> Vec<f64> {
        let mut v = Vec::new();
        for b in 0..bars {
            for (k, p) in places.iter().enumerate() {
                v.push(2.0 + b as f64 * bar + p + jitter(b * places.len() + k));
            }
        }
        v
    }

    #[test]
    fn a_pattern_that_repeats_exactly_has_no_jitter() {
        let t = starts(30, 1.0, &[0.0, 0.052, 0.122, 0.182, 0.2185], |_| 0.0);
        let f = fold(&t, 1.0).unwrap();
        assert_eq!(f.places.len(), 5);
        assert!(f.pooled_std_ms < 1e-6 && (f.bar_ms - 1000.0).abs() < 1e-6, "{}", f.report());
    }

    #[test]
    fn jitter_shows_up_per_place_and_pooled() {
        // The starts of every other bar come 4 ms late: a std of 2 ms.
        let t = starts(40, 1.0, &[0.0, 0.0625, 0.125], |i| if (i / 3) % 2 == 0 { 0.004 } else { 0.0 });
        let f = fold(&t, 1.0).unwrap();
        assert_eq!(f.places.len(), 3);
        assert!((f.pooled_std_ms - 2.0).abs() < 0.1, "{}", f.report());
        assert!(f.places.iter().all(|p| (p.3 - 4.0).abs() < 0.6), "{}", f.report());
    }

    #[test]
    fn a_clock_that_runs_a_little_fast_is_not_jitter() {
        // The sound card's bar is 1.0002 s long: 20 ms over 100 bars, which would swamp the jitter.
        let t = starts(100, 1.0002, &[0.0, 0.25, 0.5, 0.75], |_| 0.0);
        let f = fold(&t, 1.0).unwrap();
        assert!((f.bar_ms - 1000.2).abs() < 0.01 && f.pooled_std_ms < 0.01, "{}", f.report());
    }

    #[test]
    fn too_few_starts_are_refused() {
        assert!(fold(&starts(3, 1.0, &[0.0, 0.5], |_| 0.0), 1.0).is_none());
    }

    #[test]
    fn sounds_in_silence_are_found_once_each() {
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

    #[test]
    fn sounds_over_a_ringing_floor_are_found_once_each() {
        // What the sound card makes of a click: a ramped attack, a pop at the end and a tail, with the
        // level between sounds never quite zero.
        let rate = 48_000usize;
        let period = 3000; // 62.5 ms
        let mut v = vec![0.01f32; rate * 2];
        for k in 0..30 {
            let start = 4800 + k * period;
            for i in 0..1440 {
                let ramp = (i as f32 / 300.0).min(1.0);
                v[start + i] += 0.4 * ramp * ((i as f32) * 0.12).sin().abs().max(0.3);
            }
            for i in 0..1000 {
                v[start + 1440 + i] += 0.4 * (-(i as f32) / 250.0).exp();
            }
        }
        let got = onsets(&v, rate as u32);
        assert_eq!(got.len(), 30, "{got:?}");
    }
}
