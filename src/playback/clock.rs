//! MIDI clock for a device that follows an external tempo: 24 timing clocks to the quarter note,
//! a Start before the first of them, and a Stop when the music ends (sent by the sink). The
//! messages are cues like the clicks and the notes, so they ride the same scheduler: a tempo
//! change of the score, the ramp and the practice tempo move them with the music.

use super::{Cue, Sound};
use crate::config::ClockStart;
use crate::score::TempoMap;
use crate::units::Quarters;

/// Timing clocks to a quarter note, as MIDI has it.
const TICKS_PER_QUARTER: f64 = 24.0;

/// The Start goes out this long before the first tick it belongs to, in the first pass only (a
/// time before zero is only in the first pass), so that it is not heard as part of the tick.
const START_LEAD: f64 = 0.001;

/// A MIDI position is counted in sixteenth notes, in 14 bits.
const MAX_POSITION: f64 = 16383.0;

/// What the clock says to the device.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClockMsg {
    Tick,
    Start,
    /// Go on from the position that came before it.
    Continue,
    /// Song Position Pointer: where the device is to go on from, in sixteenth notes.
    Position(u16),
}

/// The status byte of Stop; it is sent by the sink when the music ends or is stopped.
pub const STOP: u8 = 0xFC;

impl ClockMsg {
    /// The bytes to send.
    pub fn bytes(self) -> ([u8; 3], usize) {
        match self {
            ClockMsg::Tick => ([0xF8, 0, 0], 1),
            ClockMsg::Start => ([0xFA, 0, 0], 1),
            ClockMsg::Continue => ([0xFB, 0, 0], 1),
            ClockMsg::Position(p) => ([0xF2, (p & 0x7F) as u8, (p >> 7) as u8 & 0x7F], 3),
        }
    }

    /// Whether the device runs after it.
    pub fn starts(self) -> bool {
        matches!(self, ClockMsg::Start | ClockMsg::Continue)
    }
}

/// What the settings say about the clock of a run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockOptions {
    pub start: ClockStart,
    /// Start from the middle of the score by position and Continue, not by Start.
    pub position: bool,
}

fn cue(t: f64, msg: ClockMsg) -> Cue {
    Cue { t, sound: Sound::Clock(msg) }
}

/// The messages that begin a run: Start, or the position and Continue when the run starts from the
/// middle of the score and that is asked for. `t` is when the first tick falls.
fn beginning(t: f64, from: Quarters, opts: &ClockOptions) -> Vec<Cue> {
    let at = t - START_LEAD;
    if opts.position && from.0 > 0.0 {
        let sixteenths = (from.0 * 4.0).round().clamp(0.0, MAX_POSITION) as u16;
        vec![cue(at, ClockMsg::Position(sixteenths)), cue(at, ClockMsg::Continue)]
    } else {
        vec![cue(at, ClockMsg::Start)]
    }
}

/// The clock of the music from `from` to `to` (quarters of the score), at the tempo of `scale`
/// times the written one, in seconds from the first beat: the ticks of the count-in (`count_in`
/// quarters before it, at the tempo of the first bar), then those of the music. A tick is sent for
/// each 1/24 of a quarter, following the tempo changes of the score.
pub fn clock_cues(tempo: &TempoMap, from: Quarters, to: Quarters, scale: f64, count_in: Quarters, opts: &ClockOptions) -> Vec<Cue> {
    let t0 = tempo.q_to_sec(from, scale).0;
    let quarter_sec = 60.0 / (tempo.bpm_at(from) * scale.max(0.01));
    let count_in_secs = count_in.0 * quarter_sec;
    let first = match opts.start {
        ClockStart::CountIn => -count_in_secs,
        ClockStart::Downbeat => 0.0,
    };
    let mut cues = beginning(first, from, opts);
    let before = (count_in.0 * TICKS_PER_QUARTER).round() as i64;
    cues.extend((1..=before).rev().map(|k| cue(-(k as f64) * quarter_sec / TICKS_PER_QUARTER, ClockMsg::Tick)));
    let ticks = ((to.0 - from.0) * TICKS_PER_QUARTER - 1e-6).ceil().max(0.0) as i64;
    cues.extend((0..ticks).map(|n| {
        let q = Quarters(from.0 + n as f64 / TICKS_PER_QUARTER);
        cue(tempo.q_to_sec(q, scale).0 - t0, ClockMsg::Tick)
    }));
    cues
}

/// The clock of one bar of the free metronome (`beats` quarter notes at `bpm`), which repeats;
/// the Start goes out once, before the first tick.
pub fn metronome_clock(bpm: f64, beats: u32) -> Vec<Cue> {
    let tick_sec = 60.0 / bpm.max(1.0) / TICKS_PER_QUARTER;
    let ticks = beats.max(1) * TICKS_PER_QUARTER as u32;
    let mut cues = vec![cue(-START_LEAD, ClockMsg::Start)];
    cues.extend((0..ticks).map(|n| cue(n as f64 * tick_sec, ClockMsg::Tick)));
    cues
}

/// `cues` with the clock's cues among them, in the order of time (what falls on the same moment
/// keeps its order, the notes before the clock).
pub fn with_clock(mut cues: Vec<Cue>, clock: Vec<Cue>) -> Vec<Cue> {
    cues.extend(clock);
    cues.sort_by(|a, b| a.t.total_cmp(&b.t));
    cues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticks(cues: &[Cue]) -> Vec<f64> {
        cues.iter().filter(|c| c.sound == Sound::Clock(ClockMsg::Tick)).map(|c| c.t).collect()
    }

    fn others(cues: &[Cue]) -> Vec<(f64, ClockMsg)> {
        cues.iter()
            .filter_map(|c| match c.sound {
                Sound::Clock(m) if m != ClockMsg::Tick => Some((c.t, m)),
                _ => None,
            })
            .collect()
    }

    const OPTS: ClockOptions = ClockOptions { start: ClockStart::CountIn, position: false };

    #[test]
    fn a_constant_tempo_has_24_ticks_to_the_quarter_note_spaced_evenly() {
        let tempo = TempoMap::constant(120.0);
        let cues = clock_cues(&tempo, Quarters(0.0), Quarters(8.0), 1.0, Quarters(0.0), &OPTS);
        let t = ticks(&cues);
        assert_eq!(t.len(), 8 * 24);
        for (n, t) in t.iter().enumerate() {
            assert!((t - n as f64 * 0.5 / 24.0).abs() < 1e-9, "tick {n} at {t}");
        }
        assert_eq!(others(&cues).len(), 1);
    }

    #[test]
    fn the_ticks_follow_the_scale_and_a_tempo_change_of_the_score() {
        let tempo = TempoMap::from_points(vec![(Quarters(0.0), 60.0), (Quarters(2.0), 120.0)]);
        let t = ticks(&clock_cues(&tempo, Quarters(0.0), Quarters(4.0), 1.0, Quarters(0.0), &OPTS));
        assert_eq!(t.len(), 96);
        assert!((t[1] - t[0] - 1.0 / 24.0).abs() < 1e-9, "a quarter is a second at 60");
        assert!((t[49] - t[48] - 0.5 / 24.0).abs() < 1e-9, "and half a second at 120");
        let half = ticks(&clock_cues(&tempo, Quarters(0.0), Quarters(1.0), 0.5, Quarters(0.0), &OPTS));
        assert!((half[1] - half[0] - 2.0 / 24.0).abs() < 1e-9, "half the tempo, twice the time");
    }

    #[test]
    fn a_range_is_counted_from_its_own_first_beat() {
        let tempo = TempoMap::constant(120.0);
        let t = ticks(&clock_cues(&tempo, Quarters(4.0), Quarters(6.0), 1.0, Quarters(0.0), &OPTS));
        assert_eq!(t.len(), 48);
        assert!(t[0].abs() < 1e-9);
    }

    #[test]
    fn the_start_goes_with_the_count_in_or_just_before_the_first_beat() {
        let tempo = TempoMap::constant(120.0);
        let with = clock_cues(&tempo, Quarters(0.0), Quarters(4.0), 1.0, Quarters(4.0), &OPTS);
        assert_eq!(ticks(&with).len(), (4 + 4) * 24, "the count-in is clocked too");
        let (at, msg) = others(&with)[0];
        assert_eq!(msg, ClockMsg::Start);
        assert!((at - (-2.0 - START_LEAD)).abs() < 1e-9, "four quarters at 120 before the first beat: {at}");
        assert!(ticks(&with)[0] - (at + START_LEAD) < 1e-9, "the first tick is the first of the count-in");

        let downbeat = ClockOptions { start: ClockStart::Downbeat, ..OPTS };
        let late = clock_cues(&tempo, Quarters(0.0), Quarters(4.0), 1.0, Quarters(4.0), &downbeat);
        let (at, _) = others(&late)[0];
        assert!((at + START_LEAD).abs() < 1e-9, "a moment before the first beat: {at}");
        assert_eq!(ticks(&late).len(), (4 + 4) * 24);
    }

    #[test]
    fn a_range_from_the_middle_goes_on_from_its_position_when_that_is_asked_for() {
        let tempo = TempoMap::constant(100.0);
        let by_position = ClockOptions { position: true, ..OPTS };
        let from_middle = clock_cues(&tempo, Quarters(8.0), Quarters(12.0), 1.0, Quarters(0.0), &by_position);
        let messages: Vec<ClockMsg> = others(&from_middle).into_iter().map(|(_, m)| m).collect();
        assert_eq!(messages, [ClockMsg::Position(32), ClockMsg::Continue], "eight quarters are 32 sixteenths");
        let from_start = clock_cues(&tempo, Quarters(0.0), Quarters(4.0), 1.0, Quarters(0.0), &by_position);
        assert_eq!(
            others(&from_start).into_iter().map(|(_, m)| m).collect::<Vec<_>>(),
            [ClockMsg::Start],
            "the start of the score is Start"
        );
        let plain = clock_cues(&tempo, Quarters(8.0), Quarters(12.0), 1.0, Quarters(0.0), &OPTS);
        assert_eq!(others(&plain).into_iter().map(|(_, m)| m).collect::<Vec<_>>(), [ClockMsg::Start], "by default it is Start");
    }

    #[test]
    fn the_messages_are_the_midi_ones() {
        assert_eq!(ClockMsg::Tick.bytes(), ([0xF8, 0, 0], 1));
        assert_eq!(ClockMsg::Start.bytes().0[0], 0xFA);
        assert_eq!(ClockMsg::Continue.bytes().0[0], 0xFB);
        assert_eq!(STOP, 0xFC);
        assert_eq!(ClockMsg::Position(200).bytes(), ([0xF2, 200 & 0x7F, 200 >> 7], 3), "14 bits, low seven first");
        assert!(ClockMsg::Start.starts() && ClockMsg::Continue.starts() && !ClockMsg::Tick.starts());
    }

    #[test]
    fn a_metronome_bar_has_24_ticks_to_a_beat_and_the_start_comes_once_before_them() {
        let bar = metronome_clock(120.0, 3);
        assert_eq!(ticks(&bar).len(), 72);
        let (at, msg) = others(&bar)[0];
        assert!(msg == ClockMsg::Start && at < 0.0, "a time before zero belongs to the first pass only");
        assert!((ticks(&bar)[24] - 0.5).abs() < 1e-9, "a beat is half a second at 120");
    }

    #[test]
    fn the_clock_is_merged_with_the_cues_in_order_of_time() {
        let click = Cue { t: 0.0, sound: Sound::Click { level: crate::practice::ClickLevel::Beat } };
        let merged = with_clock(vec![click], vec![cue(0.5, ClockMsg::Tick), cue(0.0, ClockMsg::Tick), cue(-0.1, ClockMsg::Start)]);
        let times: Vec<f64> = merged.iter().map(|c| c.t).collect();
        assert_eq!(times, [-0.1, 0.0, 0.0, 0.5]);
        assert!(matches!(merged[1].sound, Sound::Click { .. }), "the click before the tick of the same moment");
    }
}
