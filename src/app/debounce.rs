//! "Do it once the change has settled": what Listen and the metronome use to restart with a
//! changed range, tempo or setting, a slider being dragged changing it on every frame.

use std::time::{Duration, Instant};

/// A change that is waited for: the value it changed to, and since when it has stayed that value.
#[derive(Clone, Copy, Debug)]
pub(super) struct Debounce<K> {
    pending: Option<(K, Instant)>,
}

impl<K> Default for Debounce<K> {
    fn default() -> Self {
        Debounce { pending: None }
    }
}

impl<K: PartialEq + Copy> Debounce<K> {
    /// Called every frame with the value the change is to, or `None` when there is none. `true`
    /// once the same value has been asked for during `delay`; the wait is over then, and begins again
    /// with the next change.
    pub(super) fn poll(&mut self, changed_to: Option<K>, now: Instant, delay: Duration) -> bool {
        let Some(key) = changed_to else {
            self.pending = None;
            return false;
        };
        match self.pending {
            Some((pending, since)) if pending == key => {
                let settled = now.saturating_duration_since(since) >= delay;
                if settled {
                    self.pending = None;
                }
                settled
            }
            _ => {
                self.pending = Some((key, now));
                false
            }
        }
    }

    /// Forgets a change that was waited for.
    pub(super) fn cancel(&mut self) {
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY: Duration = Duration::from_millis(300);

    #[test]
    fn a_change_fires_once_it_has_stayed_the_same_for_the_delay() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut d = Debounce::default();
        assert!(!d.poll(Some(1), at(0), DELAY), "the wait begins");
        assert!(!d.poll(Some(1), at(299), DELAY));
        assert!(d.poll(Some(1), at(300), DELAY));
        assert!(!d.poll(Some(1), at(301), DELAY), "and begins again for the next change");
    }

    #[test]
    fn another_value_or_no_change_starts_the_wait_over() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut d = Debounce::default();
        assert!(!d.poll(Some(1), at(0), DELAY));
        assert!(!d.poll(Some(2), at(200), DELAY), "a slider still moving");
        assert!(!d.poll(Some(2), at(400), DELAY), "300 ms are counted from the last change");
        assert!(d.poll(Some(2), at(500), DELAY));
        assert!(!d.poll(Some(3), at(600), DELAY));
        assert!(!d.poll(None, at(700), DELAY), "changed back: nothing to do");
        assert!(!d.poll(Some(3), at(900), DELAY), "and the old wait is forgotten");
    }
}
