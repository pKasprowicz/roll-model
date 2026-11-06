//! Time for the scheduler: a moment from a time in seconds relative to a zero, and a wait that is
//! exact to a fraction of a millisecond.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The moment `t` seconds from `zero` (before it when `t` is negative), if it can be told.
pub(super) fn at(zero: Instant, t: f64) -> Option<Instant> {
    if t >= 0.0 {
        zero.checked_add(Duration::from_secs_f64(t))
    } else {
        zero.checked_sub(Duration::from_secs_f64(-t))
    }
}

/// Coarse sleep, then spin for the last ~1.5 ms. `false` if `stop` was set first.
pub(super) fn wait_until(target: Instant, stop: &AtomicBool) -> bool {
    loop {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let now = Instant::now();
        if now >= target {
            return true;
        }
        let rem = target - now;
        if rem > Duration::from_micros(2500) {
            std::thread::sleep((rem - Duration::from_micros(1500)).min(Duration::from_millis(10)));
        } else {
            std::hint::spin_loop();
        }
    }
}

/// How far ahead of its time a sound is handed to the audio engine: longer than a callback period
/// plus the lateness of a scheduling thread, so the engine can place it at its exact frame.
pub(super) const AUDIO_LOOKAHEAD: Duration = Duration::from_millis(30);
