//! A small software drum kit so the score can be played back without any
//! samples or hardware. Every lane is a short mono sample built from sine
//! sweeps, filtered noise and inharmonic partials. Rendering is deterministic.

use std::collections::BTreeMap;
use std::f32::consts::TAU;
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

/// Crash-like cymbal: noise over inharmonic sine partials with a fast attack.
fn cymbal(secs: f32, tau: f32, pitch: f32, shimmer: f32, noise: &mut Noise) -> Vec<f32> {
    const RATIOS: [f32; 7] = [1.0, 1.51, 1.93, 2.48, 3.07, 3.71, 4.45];
    let mut hp = HighPass::new(3500.0);
    build(secs, |t| {
        let partials: f32 = RATIOS
            .iter()
            .enumerate()
            .map(|(i, r)| (TAU * 410.0 * pitch * r * t + i as f32).sin())
            .sum::<f32>()
            / RATIOS.len() as f32;
        (hp.process(noise.next()) * 0.7 + partials * shimmer) * (1.0 - decay(t, 0.002)) * decay(t, tau)
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
        Lane::Crash1 => cymbal(2.0, 0.55, 1.0, 0.6, n),
        Lane::Crash2 => cymbal(1.8, 0.45, 1.18, 0.6, n),
        Lane::China => cymbal(1.4, 0.3, 1.6, 0.9, n),
        Lane::Splash => cymbal(0.7, 0.12, 1.5, 0.5, n),
        Lane::Ride => {
            let mut hp = HighPass::new(4000.0);
            const RATIOS: [f32; 5] = [1.0, 1.58, 2.14, 2.65, 3.62];
            build(1.4, |t| {
                let ping: f32 = RATIOS.iter().map(|r| (TAU * 520.0 * r * t).sin()).sum::<f32>() / 5.0;
                ping * decay(t, 0.4) + hp.process(n.next()) * (decay(t, 0.004) * 0.5 + decay(t, 0.25) * 0.08)
            })
        }
        Lane::RideBell => {
            const PARTIALS: [(f32, f32); 5] = [(1.0, 1.0), (1.51, 0.6), (2.0, 0.5), (2.75, 0.35), (4.1, 0.2)];
            let mut hp = HighPass::new(3000.0);
            build(1.0, |t| {
                let bell: f32 = PARTIALS.iter().map(|(r, a)| a * (TAU * 1050.0 * r * t).sin()).sum();
                bell * decay(t, 0.35) + hp.process(n.next()) * decay(t, 0.003) * 0.5
            })
        }
        Lane::Cowbell => {
            let mut lp = LowPass::new(5000.0);
            build(0.4, |t| lp.process(0.5 * (square(t * 587.0) + square(t * 845.0))) * decay(t, 0.07))
        }
        Lane::Other => build(0.2, |t| (TAU * 900.0 * t).sin() * decay(t, 0.03) + n.next() * decay(t, 0.002) * 0.3),
    };
    finish(raw, lane_gain(lane) * HEADROOM)
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
    pub fn for_lanes(lanes: impl IntoIterator<Item = Lane>) -> Kit {
        let mut samples = BTreeMap::new();
        for lane in lanes {
            samples.entry(lane).or_insert_with(|| Arc::new(render(lane)));
        }
        Kit { samples }
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
        let kit = Kit::for_lanes([Lane::Kick, Lane::Snare, Lane::Kick]);
        assert!(kit.sample(Lane::Kick).is_some() && kit.sample(Lane::Snare).is_some());
        assert!(kit.sample(Lane::Ride).is_none());
        assert_eq!(kit.samples.len(), 2);
    }
}
