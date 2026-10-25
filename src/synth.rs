//! A small software drum kit so the score can be played back without any
//! samples or hardware. Every lane is a short mono sample built from sine
//! sweeps, filtered noise and inharmonic partials. Rendering is deterministic.

use std::collections::BTreeMap;
use std::f32::consts::TAU;
use std::path::Path;
use std::sync::Arc;

use crate::drums::Lane;

pub const SAMPLE_RATE: u32 = 44_100;
const SR: f32 = SAMPLE_RATE as f32;

/// Peak of the loudest lane at full velocity. The rest of the headroom is for
/// notes that sound together (kick + hi-hat + crash).
const HEADROOM: f32 = 0.6;

/// xorshift noise in [-1, 1).
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / 2_147_483_648.0 - 1.0
    }
}

/// One-pole high-pass.
struct HighPass {
    a: f32,
    x1: f32,
    y1: f32,
}

impl HighPass {
    fn new(cutoff: f32) -> HighPass {
        let rc = 1.0 / (TAU * cutoff);
        HighPass { a: rc / (rc + 1.0 / SR), x1: 0.0, y1: 0.0 }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.a * (self.y1 + x - self.x1);
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// One-pole low-pass.
struct LowPass {
    a: f32,
    y: f32,
}

impl LowPass {
    fn new(cutoff: f32) -> LowPass {
        let dt = 1.0 / SR;
        LowPass { a: dt / (1.0 / (TAU * cutoff) + dt), y: 0.0 }
    }

    fn process(&mut self, x: f32) -> f32 {
        self.y += self.a * (x - self.y);
        self.y
    }
}

fn square(cycles: f32) -> f32 {
    if cycles.fract() < 0.5 {
        1.0
    } else {
        -1.0
    }
}

fn decay(t: f32, tau: f32) -> f32 {
    (-t / tau).exp()
}

/// `secs` of audio; `f` gets the time in seconds.
fn build(secs: f32, mut f: impl FnMut(f32) -> f32) -> Vec<f32> {
    (0..(secs * SR) as usize).map(|i| f(i as f32 / SR)).collect()
}

/// Scale so the loudest sample is `peak` and fade out the last 10 ms.
fn finish(mut v: Vec<f32>, peak: f32) -> Vec<f32> {
    let max = v.iter().fold(0.0_f32, |m, s| m.max(s.abs())).max(1e-6);
    let fade = (0.01 * SR) as usize;
    let n = v.len();
    for (i, s) in v.iter_mut().enumerate() {
        let tail = if i + fade > n { (n - i) as f32 / fade as f32 } else { 1.0 };
        *s *= peak / max * tail;
    }
    v
}

/// A pitched drum: a sine that falls from `base * (1 + sweep)` to `base`.
fn pitched(base: f32, sweep: f32, sweep_tau: f32, tau: f32, secs: f32, click: f32, noise: &mut Noise) -> Vec<f32> {
    let mut phase = 0.0;
    let mut hp = HighPass::new(2000.0);
    build(secs, |t| {
        phase += TAU * base * (1.0 + sweep * decay(t, sweep_tau)) / SR;
        phase.sin() * decay(t, tau) + hp.process(noise.next()) * decay(t, 0.008) * click
    })
}

/// Closed/open/pedal hi-hat: six detuned square waves plus noise, high-passed.
fn hat(secs: f32, tau: f32, cutoff: f32, noise: &mut Noise) -> Vec<f32> {
    const PARTIALS: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];
    let mut hp = HighPass::new(cutoff);
    build(secs, |t| {
        let metal: f32 = PARTIALS.iter().map(|f| square(t * f)).sum::<f32>() / 6.0;
        hp.process(0.6 * metal + 0.7 * noise.next()) * decay(t, tau)
    })
}

/// What a cymbal is made of. A real one is a dense, inharmonic wash that is
/// mostly above 2 kHz and rolls off toward the top, with a stick transient and,
/// for a ride, a few strong partials that ring (the "ping").
struct Cymbal {
    secs: f32,
    /// A cluster of inharmonic partials spread over `lo..hi` Hz: (count, lo, hi, gain, decay
    /// time in seconds at the low end). Higher partials die faster.
    cluster: (usize, f32, f32, f32, f32),
    /// Strong single partials: (Hz, gain, decay time).
    pings: &'static [(f32, f32, f32)],
    /// Filtered noise: (gain, high-pass Hz, low-pass Hz, decay time).
    wash: (f32, f32, f32, f32),
    /// Gain of the stick/mallet transient.
    stick: f32,
    /// A slow wobble of the whole sound (Hz, depth 0..1), for a trashy china.
    wobble: (f32, f32),
}

fn cymbal(c: &Cymbal, noise: &mut Noise) -> Vec<f32> {
    /// A sine kept as a rotating phasor, so a few dozen of them are cheap.
    struct Partial {
        re: f64,
        im: f64,
        cos: f64,
        sin: f64,
        amp: f64,
        decay: f64,
    }
    let partial = |hz: f32, amp: f32, tau: f32, phase: f32| {
        let w = TAU as f64 * hz as f64 / SR as f64;
        Partial { re: (phase as f64).cos(), im: (phase as f64).sin(), cos: w.cos(), sin: w.sin(), amp: amp as f64, decay: (-1.0 / (tau as f64 * SR as f64)).exp() }
    };
    let mut partials: Vec<Partial> = Vec::new();
    let (count, lo, hi, gain, tau) = c.cluster;
    for i in 0..count {
        // Log-spaced with a jitter, so no two cymbals share a grid of partials.
        let pos = ((i as f32 + 0.5 + 0.4 * noise.next()) / count as f32).clamp(0.0, 1.0);
        let hz = lo * (hi / lo).powf(pos);
        let amp = gain * (0.45 + 0.55 * (noise.next() * 0.5 + 0.5));
        partials.push(partial(hz, amp, tau * (1.15 - 0.85 * pos), noise.next() * std::f32::consts::PI));
    }
    let cluster_scale = 1.0 / (count.max(1) as f64).sqrt();
    for (hz, amp, ping_tau) in c.pings {
        partials.push(partial(*hz, *amp * (count.max(1) as f32).sqrt(), *ping_tau, noise.next() * std::f32::consts::PI));
    }

    let (wash_gain, wash_hp, wash_lp, wash_tau) = c.wash;
    // Two poles each: a steep enough band that the noise is not a white hiss.
    let (mut hp1, mut hp2) = (HighPass::new(wash_hp), HighPass::new(wash_hp));
    let (mut lp1, mut lp2) = (LowPass::new(wash_lp), LowPass::new(wash_lp));
    let mut tick_hp = HighPass::new(3500.0);
    build(c.secs, |t| {
        let mut sum = 0.0;
        for p in partials.iter_mut() {
            let (re, im) = (p.re * p.cos - p.im * p.sin, p.re * p.sin + p.im * p.cos);
            p.re = re;
            p.im = im;
            sum += p.im * p.amp;
            p.amp *= p.decay;
        }
        let attack = 1.0 - decay(t, 0.0015);
        let wash = lp2.process(lp1.process(hp2.process(hp1.process(noise.next())))) * wash_gain * decay(t, wash_tau);
        let tick = tick_hp.process(noise.next()) * decay(t, 0.004) * c.stick;
        let wobble = 1.0 - c.wobble.1 * 0.5 * (1.0 - (TAU * c.wobble.0 * t).cos());
        ((sum * cluster_scale) as f32 + wash) * attack * wobble + tick
    })
}

/// How loud each lane is relative to the others (before `HEADROOM`).
fn lane_gain(lane: Lane) -> f32 {
    match lane {
        Lane::Kick => 1.0,
        Lane::Snare => 0.9,
        Lane::CrossStick => 0.7,
        Lane::HiHatClosed | Lane::Ride | Lane::Splash => 0.45,
        Lane::HiHatOpen | Lane::RideBell | Lane::China | Lane::Cowbell | Lane::Crash1 | Lane::Crash2 => 0.5,
        Lane::HiHatPedal => 0.4,
        Lane::Tom1 | Lane::Tom2 | Lane::Tom3 | Lane::Tom4 => 0.85,
        Lane::Other => 0.6,
    }
}

/// The sample for one lane, peaking at `lane_gain * HEADROOM`.
pub fn render(lane: Lane) -> Vec<f32> {
    // A different, fixed seed per lane keeps hats and cymbals from sharing noise.
    let mut noise = Noise(0x9E37_79B9_u32.wrapping_mul(lane as u32 + 1) | 1);
    let n = &mut noise;
    let raw = match lane {
        Lane::Kick => {
            let mut phase = 0.0;
            let mut lp = LowPass::new(3000.0);
            build(0.42, |t| {
                phase += TAU * (48.0 + 120.0 * decay(t, 0.025)) / SR;
                let body = phase.sin() * decay(t, 0.14);
                let beater = lp.process(n.next()) * decay(t, 0.006) * 0.35;
                (1.6 * (body + beater)).tanh()
            })
        }
        Lane::Snare => {
            let mut phase = 0.0;
            let mut hp = HighPass::new(1200.0);
            build(0.32, |t| {
                phase += TAU * (180.0 + 60.0 * decay(t, 0.015)) / SR;
                phase.sin() * decay(t, 0.06) * 0.55 + hp.process(n.next()) * decay(t, 0.085) * 0.8
            })
        }
        Lane::CrossStick => {
            let mut hp = HighPass::new(1500.0);
            build(0.09, |t| {
                (TAU * 820.0 * t).sin() * decay(t, 0.012) * 0.6
                    + (TAU * 1650.0 * t).sin() * decay(t, 0.008) * 0.3
                    + hp.process(n.next()) * decay(t, 0.004) * 0.5
            })
        }
        Lane::HiHatClosed => hat(0.14, 0.022, 6500.0, n),
        Lane::HiHatOpen => hat(0.8, 0.22, 6000.0, n),
        Lane::HiHatPedal => hat(0.09, 0.014, 4000.0, n),
        Lane::Tom1 => pitched(200.0, 0.7, 0.035, 0.16, 0.5, 0.2, n),
        Lane::Tom2 => pitched(150.0, 0.7, 0.035, 0.18, 0.55, 0.2, n),
        Lane::Tom3 => pitched(110.0, 0.7, 0.04, 0.21, 0.65, 0.2, n),
        Lane::Tom4 => pitched(85.0, 0.7, 0.045, 0.24, 0.75, 0.2, n),
        Lane::Crash1 => cymbal(
            &Cymbal { secs: 2.0, cluster: (40, 1500.0, 11000.0, 1.0, 1.0), pings: &[], wash: (0.8, 1500.0, 9000.0, 0.55), stick: 0.6, wobble: (0.0, 0.0) },
            n,
        ),
        Lane::Crash2 => cymbal(
            &Cymbal { secs: 1.8, cluster: (36, 1700.0, 12000.0, 1.0, 0.8), pings: &[], wash: (0.8, 1700.0, 10000.0, 0.45), stick: 0.6, wobble: (0.0, 0.0) },
            n,
        ),
        Lane::China => cymbal(
            &Cymbal { secs: 1.4, cluster: (36, 2000.0, 12000.0, 1.0, 0.5), pings: &[], wash: (1.0, 2000.0, 11000.0, 0.3), stick: 0.7, wobble: (80.0, 0.5) },
            n,
        ),
        Lane::Splash => cymbal(
            &Cymbal { secs: 0.7, cluster: (28, 3000.0, 12000.0, 1.0, 0.18), pings: &[], wash: (0.8, 3000.0, 11000.0, 0.12), stick: 0.5, wobble: (0.0, 0.0) },
            n,
        ),
        Lane::Ride => cymbal(
            &Cymbal {
                secs: 1.4,
                cluster: (28, 2500.0, 9500.0, 0.45, 0.6),
                // The ping, and a little body underneath it.
                pings: &[(3150.0, 0.30, 0.8), (4230.0, 0.22, 0.7), (5440.0, 0.18, 0.6), (6900.0, 0.12, 0.45), (900.0, 0.10, 0.5), (1480.0, 0.08, 0.45)],
                wash: (0.18, 3000.0, 10000.0, 0.3),
                stick: 0.5,
                wobble: (0.0, 0.0),
            },
            n,
        ),
        Lane::RideBell => cymbal(
            &Cymbal {
                secs: 1.0,
                cluster: (16, 3000.0, 9000.0, 0.35, 0.3),
                pings: &[(1050.0, 0.40, 0.35), (1585.0, 0.25, 0.3), (2100.0, 0.20, 0.3), (2890.0, 0.15, 0.25), (4300.0, 0.10, 0.2)],
                wash: (0.1, 3000.0, 10000.0, 0.2),
                stick: 0.6,
                wobble: (0.0, 0.0),
            },
            n,
        ),
        Lane::Cowbell => {
            let mut lp = LowPass::new(5000.0);
            build(0.4, |t| lp.process(0.5 * (square(t * 587.0) + square(t * 845.0))) * decay(t, 0.07))
        }
        Lane::Other => build(0.2, |t| (TAU * 900.0 * t).sin() * decay(t, 0.03) + n.next() * decay(t, 0.002) * 0.3),
    };
    finish(raw, lane_gain(lane) * HEADROOM)
}

/// The longest user sound that is used; the rest is cut off.
const MAX_WAV_SECS: usize = 4;

/// Reads a WAV file (8, 16, 24 or 32-bit integer, or 32-bit float; any number
/// of channels) as mono samples at `SAMPLE_RATE`. Channels are averaged and
/// the rate is converted by linear interpolation. The level is not changed here.
pub fn load_wav(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("not a readable WAV file ({e})"))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let all: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => reader.samples::<f32>().collect::<Result<_, _>>(),
        (hound::SampleFormat::Int, bits @ (8 | 16 | 24 | 32)) => {
            let scale = 1.0 / (1_i64 << (bits - 1)) as f32;
            reader.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect::<Result<_, _>>()
        }
        (format, bits) => return Err(format!("{bits}-bit {format:?} WAV files are not supported")),
    }
    .map_err(|e| format!("the file is damaged ({e})"))?;

    let mono: Vec<f32> = all.chunks(channels).map(|frame| frame.iter().sum::<f32>() / frame.len() as f32).collect();
    if mono.is_empty() {
        return Err("the file has no sound in it".into());
    }
    let step = spec.sample_rate.max(1) as f64 / SAMPLE_RATE as f64;
    let n = ((mono.len() as f64 / step) as usize).min(MAX_WAV_SECS * SAMPLE_RATE as usize);
    let out: Vec<f32> = (0..n)
        .map(|i| {
            let pos = i as f64 * step;
            let (a, frac) = (pos as usize, (pos - pos.floor()) as f32);
            let b = (a + 1).min(mono.len() - 1);
            mono[a.min(mono.len() - 1)] * (1.0 - frac) + mono[b] * frac
        })
        .collect();
    if out.iter().fold(0.0_f32, |m, s| m.max(s.abs())) < 1e-4 {
        return Err("the file is silent".into());
    }
    Ok(out)
}

/// Gain for a MIDI velocity: soft notes stay audible, accents are not much
/// louder than normal strokes.
pub fn velocity_gain(velocity: u8) -> f32 {
    let x = velocity.clamp(1, 127) as f32 / 127.0;
    0.2 + 0.8 * x.powf(1.4)
}

/// Rendered samples for the lanes a piece uses.
pub struct Kit {
    samples: BTreeMap<Lane, Arc<Vec<f32>>>,
}

impl Kit {
    /// The kit for `lanes`: the built-in sound of each instrument, or the WAV
    /// file named after it (`Kick.wav`, see `Lane::name`) in `sounds_dir` when
    /// there is one. Also returns a message for every file that could not be
    /// used; that instrument keeps its built-in sound.
    pub fn load(lanes: impl IntoIterator<Item = Lane>, sounds_dir: Option<&Path>) -> (Kit, Vec<String>) {
        let mut samples = BTreeMap::new();
        let mut warnings = Vec::new();
        for lane in lanes {
            if samples.contains_key(&lane) {
                continue;
            }
            let file = sounds_dir.map(|d| d.join(format!("{}.wav", lane.name()))).filter(|p| p.is_file());
            let custom = file.as_ref().and_then(|path| match load_wav(path) {
                Ok(v) => Some(finish(v, lane_gain(lane) * HEADROOM)),
                Err(e) => {
                    warnings.push(format!(
                        "{}: {e}. The built-in sound is used for {}.",
                        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                        lane.label()
                    ));
                    None
                }
            });
            samples.insert(lane, Arc::new(custom.unwrap_or_else(|| render(lane))));
        }
        (Kit { samples }, warnings)
    }

    pub fn sample(&self, lane: Lane) -> Option<&Arc<Vec<f32>>> {
        self.samples.get(&lane)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_crossings(v: &[f32]) -> usize {
        v.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count()
    }

    #[test]
    fn every_lane_renders_a_clean_bounded_sample() {
        for lane in Lane::ALL {
            let v = render(lane);
            assert!(v.len() > 1000 && v.len() < 3 * SAMPLE_RATE as usize, "{lane:?}: {} samples", v.len());
            assert!(v.iter().all(|s| s.is_finite()), "{lane:?}: non-finite sample");
            let peak = v.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            let want = lane_gain(lane) * HEADROOM;
            assert!((peak - want).abs() < 1e-3, "{lane:?}: peak {peak}, want {want}");
            assert!(v.last().unwrap().abs() < 1e-3, "{lane:?} does not end in silence");
            // The sound dies away: the last tenth is much quieter than the loudest tenth.
            let tenth = v.len() / 10;
            let energy = |s: &[f32]| s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32;
            let loudest = v.chunks(tenth).map(energy).fold(0.0_f32, f32::max);
            assert!(energy(&v[v.len() - tenth..]) < loudest * 0.05, "{lane:?} does not decay");
        }
    }

    #[test]
    fn rendering_is_deterministic_and_lanes_differ() {
        assert_eq!(render(Lane::Snare), render(Lane::Snare));
        assert_ne!(render(Lane::HiHatClosed), render(Lane::HiHatPedal));
        assert_ne!(render(Lane::Crash1), render(Lane::Crash2));
    }

    #[test]
    fn kick_is_low_and_hat_is_bright() {
        let rate = |lane| {
            let v = render(lane);
            zero_crossings(&v) as f32 / v.len() as f32
        };
        // Zero crossings per sample track pitch: a kick thumps, a hat hisses.
        assert!(rate(Lane::Kick) * 10.0 < rate(Lane::HiHatClosed), "{} vs {}", rate(Lane::Kick), rate(Lane::HiHatClosed));
        assert!(rate(Lane::Tom4) < rate(Lane::Tom1));
        // Open rings longer than closed.
        assert!(render(Lane::HiHatOpen).len() > 3 * render(Lane::HiHatClosed).len());
    }

    /// In-place radix-2 FFT (the length must be a power of two).
    fn fft(re: &mut [f64], im: &mut [f64]) {
        let n = re.len();
        let mut j = 0;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let ang = -2.0 * std::f64::consts::PI / len as f64;
            for start in (0..n).step_by(len) {
                for k in 0..len / 2 {
                    let (wr, wi) = ((ang * k as f64).cos(), (ang * k as f64).sin());
                    let (a, b) = (start + k, start + k + len / 2);
                    let (tr, ti) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            len <<= 1;
        }
    }

    /// What a sample sounds like in numbers: where its energy sits.
    #[derive(Debug)]
    struct Spectrum {
        /// Power-weighted mean frequency.
        centroid_hz: f64,
        /// Share of the power below 2 kHz.
        below_2k: f64,
        /// Share of the power in the single strongest bin below 2 kHz (a lone low partial sounds like a bell or gong).
        low_peak: f64,
    }

    fn spectrum_of(sample: &[f32]) -> Spectrum {
        const N: usize = 16384;
        let mut re = vec![0.0_f64; N];
        let mut im = vec![0.0_f64; N];
        for (i, s) in sample.iter().take(N).enumerate() {
            let hann = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / N as f64).cos();
            re[i] = *s as f64 * hann;
        }
        fft(&mut re, &mut im);
        let bin_hz = SAMPLE_RATE as f64 / N as f64;
        let power: Vec<f64> = (0..N / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect();
        let total: f64 = power.iter().sum();
        let centroid_hz = power.iter().enumerate().map(|(k, p)| k as f64 * bin_hz * p).sum::<f64>() / total;
        let low_bins = (2000.0 / bin_hz) as usize;
        let below_2k = power[..low_bins].iter().sum::<f64>() / total;
        let low_peak = power[..low_bins].iter().cloned().fold(0.0, f64::max) / total;
        Spectrum { centroid_hz, below_2k, low_peak }
    }

    #[test]
    fn cymbals_are_bright_washes_not_bells_or_white_hiss() {
        // Measured before the redesign: the ride had 99 % of its energy below 2 kHz in a single
        // low partial (a bell), and the crashes sat near 10 kHz (white noise over a few sines).
        for lane in [Lane::Crash1, Lane::Crash2, Lane::China, Lane::Splash, Lane::Ride] {
            let s = spectrum_of(&render(lane));
            assert!((3500.0..9500.0).contains(&s.centroid_hz), "{lane:?}: energy centred at {:.0} Hz", s.centroid_hz);
            assert!(s.below_2k < 0.45, "{lane:?}: {:.0} % of the energy below 2 kHz", s.below_2k * 100.0);
            assert!(s.low_peak < 0.04, "{lane:?}: {:.1} % of the energy in one low partial", s.low_peak * 100.0);
        }
        // The bell of a ride may ring, but it has bright partials as well.
        let bell = spectrum_of(&render(Lane::RideBell));
        assert!(bell.centroid_hz > 1500.0 && bell.below_2k < 0.85 && bell.low_peak < 0.45, "{bell:?}");
    }

    #[test]
    fn a_splash_is_gone_sooner_than_a_crash() {
        let tail_energy = |lane: Lane| {
            let v = render(lane);
            let from = (0.4 * SAMPLE_RATE as f32) as usize;
            v[from..].iter().map(|s| s * s).sum::<f32>() / v.len() as f32
        };
        assert!(tail_energy(Lane::Crash1) > 4.0 * tail_energy(Lane::Splash), "after 0.4 s the splash has died away");
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("drum-tutor-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes a WAV file whose frame `i` has the sample values `f(i)` (one per channel).
    fn write_wav(path: &Path, rate: u32, bits: u16, float: bool, frames: usize, f: impl Fn(usize) -> Vec<f32>) {
        let channels = f(0).len() as u16;
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: bits,
            sample_format: if float { hound::SampleFormat::Float } else { hound::SampleFormat::Int },
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..frames {
            for v in f(i) {
                if float {
                    w.write_sample(v).unwrap();
                } else {
                    w.write_sample((v * ((1_i64 << (bits - 1)) - 1) as f32) as i32).unwrap();
                }
            }
        }
        w.finalize().unwrap();
    }

    fn peak(v: &[f32]) -> f32 {
        v.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn wav_files_become_mono_samples_at_the_kit_rate() {
        let dir = temp_dir("formats");
        let sine = |hz: f32, rate: u32| move |i: usize| (TAU * hz * i as f32 / rate as f32).sin() * 0.5;
        // 16-bit stereo at 22.05 kHz: the rate is doubled, the channels averaged.
        let p = dir.join("a.wav");
        write_wav(&p, 22_050, 16, false, 2205, |i| vec![sine(440.0, 22_050)(i), sine(440.0, 22_050)(i)]);
        let v = load_wav(&p).unwrap();
        assert!((v.len() as i64 - 4410).abs() <= 2, "{} samples", v.len());
        assert!((peak(&v) - 0.5).abs() < 0.01, "the level is kept: {}", peak(&v));
        // The same tone at the kit rate stays the same length.
        let p = dir.join("b.wav");
        write_wav(&p, SAMPLE_RATE, 32, true, 4410, |i| vec![sine(440.0, SAMPLE_RATE)(i)]);
        assert_eq!(load_wav(&p).unwrap().len(), 4410);
        for bits in [8, 16, 24, 32] {
            let p = dir.join(format!("int{bits}.wav"));
            write_wav(&p, SAMPLE_RATE, bits, false, 1000, |i| vec![sine(300.0, SAMPLE_RATE)(i)]);
            let v = load_wav(&p).unwrap();
            assert_eq!(v.len(), 1000, "{bits}-bit");
            assert!((peak(&v) - 0.5).abs() < 0.02, "{bits}-bit peak {}", peak(&v));
        }
        // Channels are averaged: 0.8 on the left and 0.2 on the right is 0.5.
        let p = dir.join("c.wav");
        write_wav(&p, SAMPLE_RATE, 16, false, 500, |_| vec![0.8, 0.2]);
        assert!(load_wav(&p).unwrap().iter().all(|s| (s - 0.5).abs() < 0.001));
        // Slower files are stretched, faster ones shortened.
        let p = dir.join("d.wav");
        write_wav(&p, 11_025, 16, false, 1000, |i| vec![sine(100.0, 11_025)(i)]);
        assert!((load_wav(&p).unwrap().len() as i64 - 4000).abs() <= 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unusable_wav_files_are_refused_with_a_reason() {
        let dir = temp_dir("bad");
        std::fs::write(dir.join("junk.wav"), b"this is not a wave file at all").unwrap();
        assert!(load_wav(&dir.join("junk.wav")).unwrap_err().contains("not a readable WAV"));
        write_wav(&dir.join("silent.wav"), SAMPLE_RATE, 16, false, 1000, |_| vec![0.0]);
        assert!(load_wav(&dir.join("silent.wav")).unwrap_err().contains("silent"));
        write_wav(&dir.join("empty.wav"), SAMPLE_RATE, 16, false, 0, |_| vec![0.0]);
        assert!(load_wav(&dir.join("empty.wav")).is_err());
        assert!(load_wav(&dir.join("missing.wav")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_kit_prefers_a_users_file_and_falls_back_with_a_warning() {
        let dir = temp_dir("kit");
        // Kick: a good file, quiet on purpose; Snare: damaged; HiHatClosed: a too long one.
        write_wav(&dir.join("Kick.wav"), SAMPLE_RATE, 16, false, 4410, |i| vec![(TAU * 60.0 * i as f32 / SAMPLE_RATE as f32).sin() * 0.1]);
        std::fs::write(dir.join("Snare.wav"), b"RIFF broken").unwrap();
        write_wav(&dir.join("HiHatClosed.wav"), 8000, 16, false, 8000 * 6, |i| vec![((i % 40) as f32 / 40.0 - 0.5) * 0.5]);
        let lanes = [Lane::Kick, Lane::Snare, Lane::HiHatClosed, Lane::Ride, Lane::Kick];
        let (kit, warnings) = Kit::load(lanes, Some(&dir));

        let kick = kit.sample(Lane::Kick).unwrap();
        assert_eq!(kick.len(), 4410, "the user's file, not the 18522-sample built-in one");
        assert!((peak(kick) - lane_gain(Lane::Kick) * HEADROOM).abs() < 1e-3, "levelled like the built-in sound: {}", peak(kick));
        assert_eq!(kit.sample(Lane::Snare).unwrap().len(), render(Lane::Snare).len(), "falls back to the built-in snare");
        assert_eq!(kit.sample(Lane::HiHatClosed).unwrap().len(), MAX_WAV_SECS * SAMPLE_RATE as usize, "cut to the longest allowed");
        assert_eq!(**kit.sample(Lane::Ride).unwrap(), render(Lane::Ride), "no file, built-in sound");

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("Snare.wav") && warnings[0].contains("built-in"), "{}", warnings[0]);

        // Without a folder, or with a folder that does not exist, everything is built in and quiet.
        for dir in [None, Some(dir.join("nope"))] {
            let (kit, warnings) = Kit::load([Lane::Kick], dir.as_deref());
            assert!(warnings.is_empty());
            assert_eq!(**kit.sample(Lane::Kick).unwrap(), render(Lane::Kick));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn velocity_is_monotonic_and_bounded() {
        let mut last = 0.0;
        for v in 0..=127u8 {
            let g = velocity_gain(v);
            assert!(g >= last && g > 0.0 && g <= 1.0 + 1e-6, "velocity {v}: {g}");
            last = g;
        }
        assert!((velocity_gain(127) - 1.0).abs() < 1e-6);
        assert!(velocity_gain(45) < velocity_gain(90));
    }

    #[test]
    fn kit_renders_each_lane_once() {
        let (kit, warnings) = Kit::load([Lane::Kick, Lane::Snare, Lane::Kick], None);
        assert!(warnings.is_empty());
        assert!(kit.sample(Lane::Kick).is_some() && kit.sample(Lane::Snare).is_some());
        assert!(kit.sample(Lane::Ride).is_none());
        assert_eq!(kit.samples.len(), 2);
    }
}
