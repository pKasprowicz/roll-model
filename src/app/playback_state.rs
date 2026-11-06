//! Listen, and the instrument played from the mixer: what is playing, the switches the sound
//! threads read and the restart that follows a change. It knows nothing of the rest of the
//! application, so its lifecycle can be tested without a `DrumApp`.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::control::SoundSwitches;
use crate::drums::Lane;
use crate::playback::Player;
use crate::practice::Ramp;
use crate::units::Seconds;

use super::debounce::Debounce;

/// The score being played back to the user ("Listen"), for the playhead.
pub(super) struct Listen {
    /// Score time zero = start of the first measure of the range.
    pub(super) zero: Instant,
    pub(super) tempo_scale: f64,
    /// Seconds from score start to the first measure of the range.
    pub(super) t0: Seconds,
    pub(super) end_sec: Seconds,
    /// Seconds per beat of the first measure, and the beats of the count-in.
    pub(super) beat_sec: Seconds,
    pub(super) count_in_beats: u32,
    /// The tempo ramp it plays with (none: `every` is 0).
    pub(super) ramp: Ramp,
    /// What it was started with; a change of it restarts Listen.
    pub(super) key: ListenKey,
}

/// What Listen plays: the bars, the tempo (rounded, in the unit of the practice tempo) and which
/// score (`score_gen`). A change of any restarts a running Listen once it has settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ListenKey {
    pub(super) from: usize,
    pub(super) to: usize,
    pub(super) tempo: i32,
    pub(super) score_gen: u32,
}

/// What a look at Listen found.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Polled {
    /// What was changed has settled: Listen is to start again.
    pub(super) restart: bool,
    /// Problems the sound thread reported.
    pub(super) errors: Vec<String>,
    /// The music came to its end (the last pass was played), as against being stopped.
    pub(super) finished: bool,
}

pub(super) struct Playback {
    /// Set while the playhead runs; the player may outlive it while the last notes ring.
    listen: Option<Listen>,
    player: Option<Player>,
    /// Live switches of Listen (the click switch is `cfg.metronome_during_take`).
    notes: bool,
    /// Range and tempo changed while listening, and since when: restarts once it settles.
    restart: Debounce<ListenKey>,
    /// Shared with the sound threads of Listen and of takes.
    control: Arc<SoundSwitches>,
    /// Instruments soloed in the mixer (not saved).
    solo: BTreeSet<Lane>,
    /// One instrument played from the mixer; it has its own control so it never disturbs Listen.
    audition: Option<Player>,
    audition_control: Arc<SoundSwitches>,
    /// The clicks of a take: the click switch and volume follow the settings, one pass whatever `passes` says.
    take_control: Arc<SoundSwitches>,
}

impl Playback {
    pub(super) fn new() -> Playback {
        Playback {
            listen: None,
            player: None,
            notes: true,
            restart: Debounce::default(),
            control: SoundSwitches::new(),
            solo: BTreeSet::new(),
            audition: None,
            audition_control: SoundSwitches::new(),
            take_control: SoundSwitches::new(),
        }
    }

    pub(super) fn listen(&self) -> Option<&Listen> {
        self.listen.as_ref()
    }

    pub(super) fn is_listening(&self) -> bool {
        self.listen.is_some()
    }

    pub(super) fn notes(&self) -> bool {
        self.notes
    }

    pub(super) fn toggle_notes(&mut self) {
        self.notes = !self.notes;
    }

    /// The switches the sound threads of Listen and of takes read.
    pub(super) fn control(&self) -> &Arc<SoundSwitches> {
        &self.control
    }

    /// The switches of the instrument played from the mixer.
    /// The switches the clicks of a take read.
    pub(super) fn take_control(&self) -> &Arc<SoundSwitches> {
        &self.take_control
    }

    pub(super) fn audition_control(&self) -> &Arc<SoundSwitches> {
        &self.audition_control
    }

    pub(super) fn solo(&self) -> &BTreeSet<Lane> {
        &self.solo
    }

    pub(super) fn is_soloed(&self, lane: Lane) -> bool {
        self.solo.contains(&lane)
    }

    pub(super) fn set_soloed(&mut self, lane: Lane, on: bool) {
        if on {
            self.solo.insert(lane);
        } else {
            self.solo.remove(&lane);
        }
    }

    pub(super) fn clear_solo(&mut self) {
        self.solo.clear();
    }

    /// Listen has started: its thread, and what it plays.
    pub(super) fn begin(&mut self, player: Player, listen: Listen) {
        self.restart.cancel();
        self.player = Some(player);
        self.listen = Some(listen);
    }

    /// Forgets a change that was waited for (Listen is being started anyway).
    pub(super) fn cancel_restart(&mut self) {
        self.restart.cancel();
    }

    /// Stops at once, also cutting off cymbals that are still ringing.
    pub(super) fn stop(&mut self) {
        if let Some(mut p) = self.player.take() {
            p.stop();
        }
        self.listen = None;
    }

    /// One look at Listen, every frame. `signature` is what Listen would play now: when it differs
    /// from what runs, and has for `delay`, `restart` says so.
    pub(super) fn poll(&mut self, signature: Option<ListenKey>, now: Instant, delay: Duration) -> Polled {
        let finished = self.listen.is_some() && self.control.music_done();
        if finished {
            self.listen = None; // the music is over; the last notes may still ring
        }
        // A change of the range or the tempo restarts it, without the count-in, once it has settled
        // (a slider being dragged changes it on every frame).
        let wanted = match (self.listen.as_ref(), signature) {
            (Some(l), Some(sig)) if l.key != sig => Some(sig),
            _ => None,
        };
        if self.restart.poll(wanted, now, delay) {
            return Polled { restart: true, errors: Vec::new(), finished };
        }
        let Some(player) = self.player.as_ref() else { return Polled { finished, ..Polled::default() } };
        let errors = player.take_errors();
        if player.is_finished() {
            self.listen = None;
            self.player = None;
        }
        Polled { restart: false, errors, finished }
    }

    /// An instrument is played from the mixer; the one before it is cut off.
    pub(super) fn begin_audition(&mut self, player: Player) {
        self.stop_audition();
        self.audition = Some(player);
    }

    /// Stops the instrument played from the mixer, if one is.
    pub(super) fn stop_audition(&mut self) {
        if let Some(mut p) = self.audition.take() {
            p.stop();
        }
    }

    /// The problems the audition reported; it is forgotten when it is over.
    pub(super) fn poll_audition(&mut self) -> Vec<String> {
        let Some(p) = self.audition.as_ref() else { return Vec::new() };
        let errors = p.take_errors();
        if p.is_finished() {
            self.audition = None;
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listen(key: ListenKey) -> Listen {
        Listen {
            zero: Instant::now(),
            tempo_scale: 1.0,
            t0: Seconds::ZERO,
            end_sec: Seconds(4.0),
            beat_sec: Seconds(0.5),
            count_in_beats: 4,
            ramp: Ramp::default(),
            key,
        }
    }

    fn key(to: usize) -> ListenKey {
        ListenKey { from: 0, to, tempo: 100, score_gen: 1 }
    }

    const DELAY: Duration = Duration::from_millis(300);

    #[test]
    fn nothing_is_asked_while_nothing_plays() {
        let mut p = Playback::new();
        assert_eq!(p.poll(Some(key(1)), Instant::now(), DELAY), Polled::default());
        assert!(!p.is_listening());
    }

    #[test]
    fn a_changed_range_restarts_listen_once_it_has_settled_and_not_before() {
        let mut p = Playback::new();
        p.listen = Some(listen(key(1)));
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        assert!(!p.poll(Some(key(1)), at(0), DELAY).restart, "the same: nothing to do");
        assert!(!p.poll(Some(key(2)), at(10), DELAY).restart, "changed: the wait begins");
        assert!(!p.poll(Some(key(3)), at(200), DELAY).restart, "changed again: it begins again");
        assert!(!p.poll(Some(key(3)), at(400), DELAY).restart);
        assert!(p.poll(Some(key(3)), at(500), DELAY).restart, "300 ms after the last change");
    }

    #[test]
    fn a_change_that_is_undone_is_forgotten() {
        let mut p = Playback::new();
        p.listen = Some(listen(key(1)));
        let t0 = Instant::now();
        assert!(!p.poll(Some(key(2)), t0, DELAY).restart);
        assert!(!p.poll(Some(key(1)), t0 + Duration::from_millis(100), DELAY).restart);
        assert!(!p.poll(Some(key(2)), t0 + Duration::from_millis(350), DELAY).restart, "the old wait is gone");
    }

    #[test]
    fn the_end_of_the_music_ends_the_playhead() {
        let mut p = Playback::new();
        p.listen = Some(listen(key(1)));
        assert!(p.is_listening());
        p.control.set_music_done(true);
        assert!(p.poll(None, Instant::now(), DELAY).finished, "it says the music came to its end");
        assert!(!p.is_listening());
    }

    #[test]
    fn the_switches_and_the_solo_set_are_kept_here() {
        let mut p = Playback::new();
        assert!(p.notes());
        p.toggle_notes();
        assert!(!p.notes());
        p.set_soloed(Lane::Ride, true);
        assert!(p.is_soloed(Lane::Ride) && !p.is_soloed(Lane::Kick));
        p.clear_solo();
        assert!(p.solo().is_empty());
    }
}
