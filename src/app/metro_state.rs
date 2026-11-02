//! The free metronome's run: its sound thread, the switches that thread shares, and the restart
//! that follows a change of tempo, beats or subdivision. It knows nothing of the rest of the
//! application, so it can be tested without a `DrumApp`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::Subdivision;
use crate::control::SoundSwitches;
use crate::playback::Player;
use crate::practice::Ramp;

use super::debounce::Debounce;

/// What the running metronome plays; a change of it restarts it once it has settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MetroKey {
    pub(super) tempo_tenths: i32,
    pub(super) beats: u32,
    pub(super) subdivision: Subdivision,
}

/// A running metronome and what it was started with.
pub(super) struct Run {
    zero: Instant,
    beat_sec: f64,
    /// Seconds of one bar at the starting tempo, and the tempo ramp it runs with.
    bar_sec: f64,
    ramp: Ramp,
    /// What it was started with: a change restarts it.
    key: MetroKey,
    /// A changed key, waited for until it has settled.
    restart: Debounce<MetroKey>,
}

impl Run {
    pub(super) fn new(zero: Instant, beat_sec: f64, bar_sec: f64, ramp: Ramp, key: MetroKey) -> Run {
        Run { zero, beat_sec, bar_sec, ramp, key, restart: Debounce::default() }
    }

    /// How many beats of the starting tempo have gone by at `now`: negative before the start.
    fn beats_at(&self, now: Instant) -> f64 {
        let elapsed = match now.checked_duration_since(self.zero) {
            Some(d) => d.as_secs_f64(),
            None => -self.zero.duration_since(now).as_secs_f64(),
        };
        self.ramp.base_seconds(self.bar_sec, elapsed) / self.beat_sec
    }
}

/// What a look at the metronome found.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Polled {
    /// What was changed has settled: the metronome is to start again.
    pub(super) restart: bool,
    pub(super) errors: Vec<String>,
}

pub(super) struct MetroState {
    run: Option<Run>,
    player: Option<Player>,
    control: Arc<SoundSwitches>,
}

impl MetroState {
    pub(super) fn new() -> MetroState {
        MetroState { run: None, player: None, control: SoundSwitches::new() }
    }

    pub(super) fn is_running(&self) -> bool {
        self.run.is_some()
    }

    /// The switches the metronome's thread reads.
    pub(super) fn control(&self) -> &Arc<SoundSwitches> {
        &self.control
    }

    pub(super) fn begin(&mut self, player: Player, run: Run) {
        self.player = Some(player);
        self.run = Some(run);
    }

    pub(super) fn stop(&mut self) {
        if let Some(mut p) = self.player.take() {
            p.stop();
        }
        self.run = None;
    }

    /// The beat the metronome is in at `now` (the whole part), and how far through it (0 to 1),
    /// once it has started.
    pub(super) fn beat_at(&self, now: Instant) -> Option<(f64, f32)> {
        let beat = self.run.as_ref()?.beats_at(now);
        (beat >= 0.0).then(|| (beat.floor(), beat.fract() as f32))
    }

    /// The beats of the starting tempo gone by at `now`, negative before the start; `None` when it
    /// does not run.
    pub(super) fn beats_at(&self, now: Instant) -> Option<f64> {
        self.run.as_ref().map(|r| r.beats_at(now))
    }

    /// One look, every frame: `key` is what the metronome would play now.
    pub(super) fn poll(&mut self, key: MetroKey, now: Instant, delay: Duration) -> Polled {
        let restart = self.run.as_mut().is_some_and(|run| {
            let changed = (run.key != key).then_some(key);
            run.restart.poll(changed, now, delay)
        });
        if restart {
            return Polled { restart: true, errors: Vec::new() };
        }
        let Some(player) = self.player.as_ref() else { return Polled::default() };
        let errors = player.take_errors();
        if player.is_finished() {
            self.player = None;
            self.run = None;
        }
        Polled { restart: false, errors }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(beats: u32) -> MetroKey {
        MetroKey { tempo_tenths: 1000, beats, subdivision: Subdivision::None }
    }

    fn running(zero: Instant) -> MetroState {
        let mut m = MetroState::new();
        // 120 BPM: half a second a beat, four beats a bar.
        m.run = Some(Run::new(zero, 0.5, 2.0, Ramp::default(), key(4)));
        m
    }

    #[test]
    fn it_does_not_run_until_started_and_has_no_beat_before_its_start() {
        let t0 = Instant::now();
        assert!(!MetroState::new().is_running());
        assert_eq!(MetroState::new().beat_at(t0), None);
        let m = running(t0 + Duration::from_millis(500));
        assert!(m.is_running());
        assert_eq!(m.beat_at(t0), None, "the lead before the first click");
        assert!(m.beats_at(t0).unwrap() < 0.0);
    }

    #[test]
    fn the_beat_is_counted_from_the_start_in_the_starting_tempo() {
        let t0 = Instant::now();
        let m = running(t0);
        let (beat, into) = m.beat_at(t0 + Duration::from_millis(1250)).unwrap();
        assert_eq!(beat, 2.0);
        assert!((into - 0.5).abs() < 1e-3, "{into}");
    }

    #[test]
    fn a_changed_setting_restarts_it_once_it_has_settled() {
        let t0 = Instant::now();
        let delay = Duration::from_millis(300);
        let mut m = running(t0);
        assert!(!m.poll(key(4), t0, delay).restart, "nothing changed");
        assert!(!m.poll(key(3), t0, delay).restart, "changed: the wait begins");
        assert!(!m.poll(key(3), t0 + Duration::from_millis(200), delay).restart);
        assert!(m.poll(key(3), t0 + Duration::from_millis(300), delay).restart);
        assert!(!MetroState::new().poll(key(3), t0, delay).restart, "a stopped metronome has nothing to restart");
    }
}
