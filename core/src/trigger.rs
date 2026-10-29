//! The start trigger: two drum hits in a row start a recording, so a drummer who is
//! holding sticks does not need the mouse. Pure: lanes and instants in, a yes or no
//! out; the application decides when the trigger may fire.

use std::time::{Duration, Instant};

use crate::drums::Lane;

/// The second hit must come at most this long after the first.
pub const WINDOW: Duration = Duration::from_millis(500);

/// The two instruments to hit, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sequence {
    pub first: Lane,
    pub second: Lane,
}

/// Which of the two a setting or a "learn" is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    First,
    Second,
}

impl Slot {
    pub fn label(self) -> &'static str {
        match self {
            Slot::First => "First",
            Slot::Second => "Second",
        }
    }
}

/// Watches the hits go by. The sequence must be exactly two hits in a row: a hit of another
/// instrument in between, or a late second hit, starts over.
#[derive(Debug, Default)]
pub struct Detector {
    /// When the first hit was made, while the trigger is waiting for the second.
    armed: Option<Instant>,
}

impl Detector {
    /// A hit of `lane` at `at`. True when it completes the sequence; the detector is then
    /// ready for the next one.
    pub fn feed(&mut self, sequence: Sequence, lane: Lane, at: Instant) -> bool {
        if let Some(first_at) = self.armed.take() {
            if lane == sequence.second && at.saturating_duration_since(first_at) <= WINDOW {
                return true;
            }
        }
        // Any hit of the first instrument (re)starts the wait, also one that failed to complete it.
        if lane == sequence.first {
            self.armed = Some(at);
        }
        false
    }

    /// Whether the first hit was made and the second may still come, for a display.
    pub fn armed(&self, now: Instant) -> bool {
        self.armed.is_some_and(|first_at| now.saturating_duration_since(first_at) <= WINDOW)
    }

    pub fn reset(&mut self) {
        self.armed = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEQ: Sequence = Sequence { first: Lane::Crash1, second: Lane::Tom1 };

    fn after(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn the_two_hits_in_order_complete_it() {
        let (mut d, t0) = (Detector::default(), Instant::now());
        assert!(!d.feed(SEQ, Lane::Crash1, t0));
        assert!(d.armed(after(t0, 100)));
        assert!(d.feed(SEQ, Lane::Tom1, after(t0, 200)));
        assert!(!d.armed(after(t0, 200)), "used up");
    }

    #[test]
    fn the_second_hit_may_come_up_to_half_a_second_later_and_no_later() {
        let (mut d, t0) = (Detector::default(), Instant::now());
        d.feed(SEQ, Lane::Crash1, t0);
        assert!(d.feed(SEQ, Lane::Tom1, after(t0, 500)), "exactly 0.5 s is in time");
        d.feed(SEQ, Lane::Crash1, t0);
        assert!(!d.feed(SEQ, Lane::Tom1, after(t0, 501)), "a millisecond too late");
        assert!(!d.armed(after(t0, 501)));
        assert!(!d.armed(after(t0, 10_000)));
    }

    #[test]
    fn the_wrong_order_or_a_stranger_in_between_does_not_count() {
        let (mut d, t0) = (Detector::default(), Instant::now());
        assert!(!d.feed(SEQ, Lane::Tom1, t0), "second first");
        assert!(!d.feed(SEQ, Lane::Tom1, after(t0, 50)));
        d.feed(SEQ, Lane::Crash1, after(t0, 100));
        assert!(!d.feed(SEQ, Lane::Snare, after(t0, 150)), "a hit of another instrument");
        assert!(!d.feed(SEQ, Lane::Tom1, after(t0, 200)), "which cancels the first one");
    }

    #[test]
    fn hitting_the_first_again_waits_for_the_second_from_the_newest_hit() {
        let (mut d, t0) = (Detector::default(), Instant::now());
        d.feed(SEQ, Lane::Crash1, t0);
        d.feed(SEQ, Lane::Crash1, after(t0, 400));
        assert!(d.feed(SEQ, Lane::Tom1, after(t0, 800)), "400 ms after the newest crash, 800 after the first");
    }

    #[test]
    fn one_instrument_twice_works_and_each_pair_fires_once() {
        let same = Sequence { first: Lane::Snare, second: Lane::Snare };
        let (mut d, t0) = (Detector::default(), Instant::now());
        assert!(!d.feed(same, Lane::Snare, t0));
        assert!(d.feed(same, Lane::Snare, after(t0, 120)));
        assert!(!d.feed(same, Lane::Snare, after(t0, 240)), "the third hit starts a new pair");
        assert!(d.feed(same, Lane::Snare, after(t0, 360)));
    }

    #[test]
    fn a_reset_forgets_the_first_hit() {
        let (mut d, t0) = (Detector::default(), Instant::now());
        d.feed(SEQ, Lane::Crash1, t0);
        d.reset();
        assert!(!d.feed(SEQ, Lane::Tom1, after(t0, 100)));
    }
}
