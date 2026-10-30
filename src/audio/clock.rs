//! Where a moment of the program's clock falls in the frames of the output stream.
//!
//! The scheduler threads know when a sound should start as an `Instant`; the mixer starts sounds at
//! frames. Frame `f` of the stream is rendered at about `intercept + f / rate` seconds after the
//! engine's origin. The callback knows when it ran and which frame it started, but it runs late by
//! a varying amount (it is woken by the driver, which is woken by the system), never early, so the
//! smallest `wall - frame / rate` over the last callbacks is the best estimate of the intercept:
//! the late ones fall away.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::mixer::Counters;

/// How many callbacks the estimate looks back on: about a second and a third at 10 ms. A longer
/// window shrugs off longer stalls but follows a drifting sound card clock more slowly (at 100 parts
/// per million a second and a third is 0.13 ms).
pub const WINDOW: usize = 128;

/// Kept by the audio callback; allocation-free.
pub struct Estimator {
    rate: f64,
    recent: [f64; WINDOW],
    filled: usize,
    next: usize,
}

impl Estimator {
    pub fn new(rate: u32) -> Estimator {
        Estimator { rate: rate as f64, recent: [0.0; WINDOW], filled: 0, next: 0 }
    }

    /// A callback ran `wall` seconds after the origin and rendered `frame` as its first frame. Returns
    /// the new estimate of the intercept in seconds.
    pub fn observe(&mut self, wall: f64, frame: u64) -> f64 {
        self.recent[self.next] = wall - frame as f64 / self.rate;
        self.next = (self.next + 1) % WINDOW;
        self.filled = (self.filled + 1).min(WINDOW);
        self.recent[..self.filled].iter().copied().fold(f64::INFINITY, f64::min)
    }
}

/// What the callback tells the rest of the program; every field is an atomic.
pub struct Shared {
    origin: Instant,
    rate: u32,
    intercept: AtomicU64,
    ready: AtomicBool,
    latency_frames: AtomicU32,
    late: AtomicU64,
    dropped_pending: AtomicU64,
    dropped_voices: AtomicU64,
    next_run: AtomicU32,
}

impl Shared {
    /// `origin` is the instant the callback's `wall` seconds count from.
    pub fn new(origin: Instant, rate: u32) -> Shared {
        Shared {
            origin,
            rate,
            intercept: AtomicU64::new(0),
            ready: AtomicBool::new(false),
            latency_frames: AtomicU32::new(0),
            late: AtomicU64::new(0),
            dropped_pending: AtomicU64::new(0),
            dropped_voices: AtomicU64::new(0),
            next_run: AtomicU32::new(1),
        }
    }

    /// A number for a run of sounds, different from every other this engine has given out.
    pub fn new_run(&self) -> u32 {
        self.next_run.fetch_add(1, Ordering::Relaxed)
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// From the callback: the new estimate, and the counters of the mixer.
    pub fn publish(&self, intercept: f64, counters: Counters) {
        self.intercept.store(intercept.to_bits(), Ordering::Relaxed);
        self.ready.store(true, Ordering::Release);
        self.late.store(counters.late, Ordering::Relaxed);
        self.dropped_pending.store(counters.dropped_pending, Ordering::Relaxed);
        self.dropped_voices.store(counters.dropped_voices, Ordering::Relaxed);
    }

    /// Whether the callback has run, so that frames can be told.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub fn counters(&self) -> Counters {
        Counters {
            late: self.late.load(Ordering::Relaxed),
            dropped_pending: self.dropped_pending.load(Ordering::Relaxed),
            dropped_voices: self.dropped_voices.load(Ordering::Relaxed),
        }
    }

    /// From the backend: how long a frame takes from the callback to the air, as it reports.
    pub fn set_latency_frames(&self, frames: u32) {
        self.latency_frames.store(frames, Ordering::Relaxed);
    }

    pub fn latency(&self) -> Duration {
        Duration::from_secs_f64(self.latency_frames.load(Ordering::Relaxed) as f64 / self.rate.max(1) as f64)
    }

    fn intercept(&self) -> f64 {
        f64::from_bits(self.intercept.load(Ordering::Relaxed))
    }

    /// The frame that is rendered at `at`; `None` until the callback has run. Before the first frame: 0.
    pub fn frame_at(&self, at: Instant) -> Option<u64> {
        if !self.is_ready() {
            return None;
        }
        let wall = at.saturating_duration_since(self.origin).as_secs_f64();
        Some(((wall - self.intercept()) * self.rate as f64).round().max(0.0) as u64)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small deterministic noise in 0..1.
    fn noise(i: u64) -> f64 {
        let x = i.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((x >> 33) as f64) / (1u64 << 31) as f64
    }

    #[test]
    fn the_estimate_ignores_late_callbacks() {
        let mut e = Estimator::new(48_000);
        let mut last = 0.0;
        for i in 0..400u64 {
            let late = noise(i) * 0.004 + if i % 37 == 0 { 0.008 } else { 0.0 };
            last = e.observe(5.0 + i as f64 * 0.01 + late, i * 480);
        }
        assert!((last - 5.0).abs() < 0.0002, "off by {} ms", (last - 5.0) * 1000.0);
    }

    #[test]
    fn the_estimate_follows_a_sound_card_clock_that_runs_fast_or_slow() {
        for drift in [-100e-6, 100e-6] {
            let mut e = Estimator::new(48_000);
            let mut worst = 0.0f64;
            for i in 0..2000u64 {
                // Frame f is rendered at 5 s + f / (rate * (1 + drift)).
                let true_start = 5.0 + (i * 480) as f64 / (48_000.0 * (1.0 + drift));
                let intercept = e.observe(true_start + noise(i) * 0.003, i * 480);
                if i > 200 {
                    // The estimate is for the frame of the latest callback: compare there.
                    let est = intercept + (i * 480) as f64 / 48_000.0;
                    worst = worst.max((est - true_start).abs());
                }
            }
            assert!(worst < 0.0004, "drift {drift}: off by {} ms", worst * 1000.0);
        }
    }

    #[test]
    fn frames_are_told_from_instants_once_the_callback_has_run() {
        let origin = Instant::now();
        let shared = Shared::new(origin, 48_000);
        assert_eq!(shared.frame_at(origin), None);
        shared.publish(2.0, Counters::default());
        assert_eq!(shared.frame_at(origin + Duration::from_secs(3)), Some(48_000));
        assert_eq!(shared.frame_at(origin + Duration::from_secs(1)), Some(0), "before the first frame");
    }

    #[test]
    fn counters_and_latency_are_passed_on() {
        let shared = Shared::new(Instant::now(), 48_000);
        shared.publish(0.0, Counters { late: 3, dropped_pending: 2, dropped_voices: 1 });
        assert_eq!(shared.counters(), Counters { late: 3, dropped_pending: 2, dropped_voices: 1 });
        shared.set_latency_frames(960);
        assert_eq!(shared.latency(), Duration::from_millis(20));
    }
}
