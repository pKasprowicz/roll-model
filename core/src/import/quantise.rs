//! Putting onsets that were played (or programmed) at arbitrary ticks onto exact places in the
//! bar. Pure: ticks and bars in, bar numbers and exact positions out, plus a report.
//!
//! The method, for files that may be a programmed loop or a human recording:
//! 1. one constant offset (a recording with fixed latency) is estimated and removed;
//! 2. each beat gets the coarsest subdivision whose grid every onset of that beat fits within a
//!    tolerance (straight halves and quarters before triplets), so a sixteenth groove with a
//!    triplet fill in its last beat comes out right;
//! 3. every onset goes to the nearest point of its beat's grid; two onsets of one instrument on
//!    the same point are one note (a flam) and are counted;
//! 4. the report says what was done and warns when the notes do not sit on a regular grid.

use std::collections::{BTreeMap, HashSet};

use crate::drums::Lane;

/// How fine the grid may get.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Grid {
    /// Eighths and sixteenths, and triplet eighths and sixteenths where a beat needs them.
    #[default]
    Auto,
    Eighth,
    Sixteenth,
    /// Eighth and sixteenth triplets only.
    Triplet,
    /// As `Auto`, and thirty-seconds too.
    ThirtySecond,
}

impl Grid {
    /// The subdivisions of a beat to try, coarsest first.
    fn candidates(self) -> &'static [u32] {
        match self {
            Grid::Auto => &[1, 2, 4, 3, 6],
            Grid::Eighth => &[1, 2],
            Grid::Sixteenth => &[1, 2, 4],
            Grid::Triplet => &[1, 3, 6],
            Grid::ThirtySecond => &[1, 2, 4, 3, 6, 8],
        }
    }

    /// Where a beat goes when no candidate fits all its onsets: the finest straight grid allowed.
    fn fallback(self) -> u32 {
        match self {
            Grid::Auto | Grid::Sixteenth => 4,
            Grid::Eighth => 2,
            Grid::Triplet => 6,
            Grid::ThirtySecond => 8,
        }
    }
}

/// What the quantiser may be asked.
#[derive(Clone, Debug, PartialEq)]
pub struct MidiOptions {
    pub grid: Grid,
    /// Estimate and remove a constant offset of the whole performance.
    pub remove_offset: bool,
    /// A note at or above this multiple of its instrument's median velocity is an accent.
    pub accent_ratio: f64,
    /// A note at or below this multiple is a ghost note.
    pub ghost_ratio: f64,
    /// The music's first bar line is this many beats after the start of the file, so what comes
    /// before it is a short first bar (a pickup, or a count-in); 0 keeps the file's own bars.
    /// Refused when the time signature changes later in the file.
    pub first_bar_beats: u32,
}

impl Default for MidiOptions {
    fn default() -> Self {
        MidiOptions { grid: Grid::Auto, remove_offset: true, accent_ratio: 1.5, ghost_ratio: 0.45, first_bar_beats: 0 }
    }
}

/// A bar laid over the ticks.
#[derive(Clone, Copy, Debug)]
pub struct Bar {
    pub start: u64,
    pub len: u64,
    /// The length of one beat of the bar's time signature.
    pub beat: u64,
}

/// Where an onset ended up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Index of the onset in the input.
    pub index: usize,
    pub bar: usize,
    /// Quarter notes from the start of the bar.
    pub offset_q: f64,
}

/// What was done, for the status line and the adjust-after-load control.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MidiReport {
    /// Beats that got each subdivision of the beat (1, 2, 4 = sixteenths, 3 and 6 = triplets, 8).
    pub beats_by_subdivision: BTreeMap<u32, usize>,
    /// The constant offset that was removed, in milliseconds (positive: the playing was late).
    pub offset_ms: f64,
    /// The largest distance between an onset and where it was placed, in milliseconds.
    pub max_shift_ms: f64,
    /// Onsets placed more than [`MOVED_MS`] from where they were played.
    pub moved: usize,
    /// Onsets that were the same note as another of the same instrument and were merged into it.
    pub merged: usize,
    /// More than a fifth of the notes fall between grid points: the file was probably played without a
    /// click, and the placement is probably wrong in places.
    pub free_time: bool,
    pub notes: usize,
}

impl MidiReport {
    /// One line for the status bar: the grids chosen, the offset removed, the worst shift.
    pub fn summary(&self) -> String {
        let name = |n: u32| match n {
            1 => "quarters",
            2 => "eighths",
            4 => "sixteenths",
            3 => "eighth triplets",
            6 => "sixteenth triplets",
            _ => "thirty-seconds",
        };
        let mut grids: Vec<(u32, usize)> = self.beats_by_subdivision.iter().map(|(n, c)| (*n, *c)).collect();
        grids.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        let grids: Vec<String> = grids.iter().take(3).map(|(n, c)| format!("{} in {c} beats", name(*n))).collect();
        let mut parts = vec![grids.join(", ")];
        if self.offset_ms.abs() >= 1.0 {
            parts.push(format!("played {:.0} ms {}, corrected", self.offset_ms.abs(), if self.offset_ms > 0.0 { "late" } else { "early" }));
        }
        parts.push(format!("largest shift {:.0} ms", self.max_shift_ms));
        parts.join("; ")
    }
}

/// A note counts as moved when it was placed further than this from where it was played.
pub const MOVED_MS: f64 = 15.0;

/// The largest distance (milliseconds, and a fraction of a grid cell, whichever is smaller) at
/// which an onset still counts as sitting on a grid point.
const TOLERANCE_MS: f64 = 30.0;
const TOLERANCE_CELL: f64 = 0.35;
/// Triplets must fit tighter, because straight music is far more common than triplets.
const TRIPLET_FACTOR: f64 = 0.7;

struct Tempos<'a> {
    ppq: u64,
    points: &'a [(u64, u32)],
}

impl Tempos<'_> {
    /// Milliseconds per tick at `tick`.
    fn ms_per_tick(&self, tick: f64) -> f64 {
        let at = self.points.partition_point(|(t, _)| (*t as f64) <= tick);
        let us = if at == 0 { self.points.first().map_or(500_000, |p| p.1) } else { self.points[at - 1].1 };
        f64::from(us) / 1000.0 / self.ppq as f64
    }
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n % 2 == 1 { values[n / 2] } else { (values[n / 2 - 1] + values[n / 2]) / 2.0 }
}

/// The distance of `tick` from the nearest point of a grid with `cell` ticks between points.
fn residual(tick: f64, cell: f64) -> f64 {
    tick - (tick / cell).round() * cell
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Place `onsets` (ticks, sorted; the instrument of each) on `bars`. `bars` must reach one bar past
/// the last onset, so that a note that rounds up to the next bar line has a bar to go to.
pub fn quantise(ppq: u64, bars: &[Bar], tempo_points: &[(u64, u32)], onsets: &[(u64, Lane)], opts: &MidiOptions) -> (Vec<Placed>, MidiReport) {
    let tempos = Tempos { ppq, points: tempo_points };
    let mut report = MidiReport { notes: onsets.len(), ..Default::default() };
    if onsets.is_empty() || bars.is_empty() {
        return (Vec::new(), report);
    }
    let sixteenth = ppq as f64 / 4.0;

    // 1. The constant offset: the median distance from the nearest sixteenth, over the onsets that
    // are near one (a swung or triplet note is far from it and says nothing about the offset).
    let near = sixteenth * 0.25;
    let mut close: Vec<f64> = onsets.iter().map(|(t, _)| residual(*t as f64, sixteenth)).filter(|r| r.abs() <= near).collect();
    let offset = if opts.remove_offset && close.len() >= 8 && close.len() * 2 >= onsets.len() { median(&mut close) } else { 0.0 };
    let offset = if offset.abs() < 0.5 { 0.0 } else { offset };
    report.offset_ms = offset * tempos.ms_per_tick(0.0);

    // Onsets by beat, after the offset.
    let shifted = |tick: u64| (tick as f64 - offset).max(0.0);
    let locate = |t: f64| -> (usize, u64) {
        let bar = bars.partition_point(|b| (b.start as f64) <= t).saturating_sub(1);
        let b = &bars[bar];
        let beat = (((t - b.start as f64) / b.beat as f64).floor() as u64).min(b.len / b.beat.max(1) - 1);
        (bar, beat)
    };
    let mut beats: BTreeMap<(usize, u64), Vec<usize>> = BTreeMap::new();
    for (i, (tick, _)) in onsets.iter().enumerate() {
        beats.entry(locate(shifted(*tick))).or_default().push(i);
    }

    // 2. A subdivision for each beat.
    let candidates = opts.grid.candidates();
    let mut chosen: BTreeMap<(usize, u64), u32> = BTreeMap::new();
    for (&(bar, beat), members) in &beats {
        let b = &bars[bar];
        let from = (b.start + beat * b.beat) as f64;
        let ms = tempos.ms_per_tick(from);
        let fits = |n: u32| {
            let cell = b.beat as f64 / f64::from(n);
            let mut tol = (cell * TOLERANCE_CELL).min(TOLERANCE_MS / ms);
            if n.is_multiple_of(3) {
                tol *= TRIPLET_FACTOR;
            }
            members.iter().all(|&i| residual(shifted(onsets[i].0) - from, cell).abs() <= tol)
        };
        let n = candidates.iter().copied().find(|&n| fits(n)).unwrap_or(opts.grid.fallback());
        chosen.insert((bar, beat), n);
        *report.beats_by_subdivision.entry(n).or_default() += 1;
    }

    // 3. Each onset to the nearest point of its beat's grid.
    let mut placed = Vec::with_capacity(onsets.len());
    let mut seen: HashSet<(usize, u64, u32, u32, Lane)> = HashSet::new();
    let mut poor = 0usize;
    for (&(bar, beat), members) in &beats {
        let n = chosen[&(bar, beat)];
        let b = &bars[bar];
        let from = (b.start + beat * b.beat) as f64;
        let cell = b.beat as f64 / f64::from(n);
        for &i in members {
            let (tick, lane) = onsets[i];
            let t = shifted(tick);
            let k = ((t - from) / cell).round().max(0.0) as u32;
            let snapped = from + f64::from(k) * cell;
            let ms = tempos.ms_per_tick(from);
            report.max_shift_ms = report.max_shift_ms.max((tick as f64 - snapped).abs() * ms);
            if (tick as f64 - snapped).abs() * ms > MOVED_MS {
                report.moved += 1;
            }
            if (t - snapped).abs() > cell * TOLERANCE_CELL {
                poor += 1;
            }
            // k == n is the first point of the next beat (or bar).
            let (bar_at, beat_at, k_at, n_at) = if k >= n {
                let beats_in_bar = b.len / b.beat.max(1);
                if beat + 1 < beats_in_bar { (bar, beat + 1, 0, 1) } else { ((bar + 1).min(bars.len() - 1), 0, 0, 1) }
            } else {
                let g = gcd(k, n);
                (bar, beat, k / g, n / g)
            };
            if !seen.insert((bar_at, beat_at, k_at, n_at, lane)) {
                report.merged += 1;
                continue;
            }
            let bb = &bars[bar_at];
            let offset_q = (beat_at as f64 + f64::from(k_at) / f64::from(n_at)) * bb.beat as f64 / ppq as f64;
            placed.push(Placed { index: i, bar: bar_at, offset_q });
        }
    }
    placed.sort_by_key(|p| p.index);

    // 4. Does it look like playing against a grid at all? (A recording made without a click does not:
    // its notes fall between the points of every grid, and the tempo it wanders from is not repaired.)
    report.free_time = poor * 5 > onsets.len() && onsets.len() >= 16;
    (placed, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PPQ: u64 = 480;

    fn bars(n: usize) -> Vec<Bar> {
        (0..n).map(|i| Bar { start: i as u64 * 4 * PPQ, len: 4 * PPQ, beat: PPQ }).collect()
    }

    fn at_120() -> Vec<(u64, u32)> {
        vec![(0, 500_000)]
    }

    fn run(ticks: &[u64], opts: &MidiOptions) -> (Vec<Placed>, MidiReport) {
        let onsets: Vec<(u64, Lane)> = ticks.iter().map(|t| (*t, Lane::Snare)).collect();
        quantise(PPQ, &bars(8), &at_120(), &onsets, opts)
    }

    fn offsets(placed: &[Placed]) -> Vec<(usize, f64)> {
        placed.iter().map(|p| (p.bar, (p.offset_q * 1000.0).round() / 1000.0)).collect()
    }

    /// A small deterministic random sequence, so the tests need no extra crate.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) as f64 / (1u64 << 31) as f64
        }
    }

    #[test]
    fn the_report_reads_as_one_line() {
        let ticks: Vec<u64> = (0..32).map(|i| i * PPQ / 2 + 24).collect();
        let (_, report) = run(&ticks, &MidiOptions::default());
        assert_eq!(report.summary(), "eighths in 16 beats; played 25 ms late, corrected; largest shift 25 ms");
    }

    #[test]
    fn a_programmed_loop_comes_back_exact() {
        let ticks: Vec<u64> = (0..32).map(|i| i * PPQ / 2).chain([PPQ * 4 + PPQ / 4]).collect();
        let (placed, report) = run(&ticks, &MidiOptions::default());
        assert_eq!(report.max_shift_ms, 0.0);
        assert_eq!((report.moved, report.merged, report.offset_ms), (0, 0, 0.0));
        assert!(!report.free_time);
        for (p, t) in placed.iter().zip(&ticks) {
            let exact = (*t % (4 * PPQ)) as f64 / PPQ as f64;
            assert_eq!(p.offset_q, exact);
            assert_eq!(p.bar as u64, t / (4 * PPQ));
        }
    }

    #[test]
    fn a_jittered_sixteenth_groove_comes_back_on_the_grid_and_no_note_moves_more_than_half_a_cell() {
        let mut rng = Lcg(7);
        let mut ticks = Vec::new();
        for sixteenth in 0..64u64 {
            let jitter = ((rng.next() - 0.5) * 40.0) as i64; // up to +-20 ticks, about +-21 ms
            ticks.push((sixteenth * PPQ / 4).saturating_add_signed(jitter));
        }
        let (placed, report) = run(&ticks, &MidiOptions::default());
        for (i, p) in placed.iter().enumerate() {
            let grid = (p.bar as f64) * 4.0 + p.offset_q;
            let sixteenth = i as f64 / 4.0;
            assert!((grid - sixteenth).abs() < 1e-9, "note {i} landed at {grid}");
        }
        assert!(report.max_shift_ms < 30.0, "{}", report.max_shift_ms);
        assert!(report.beats_by_subdivision.contains_key(&4));
        assert!(!report.free_time);
    }

    #[test]
    fn a_triplet_beat_is_not_read_as_sixteenths_and_a_straight_beat_is_not_read_as_triplets() {
        // Beat 1 triplet eighths, beat 2 straight sixteenths.
        let ticks = [0, 160, 320, 480, 600, 720, 840];
        let (placed, report) = run(&ticks, &MidiOptions::default());
        let q: Vec<f64> = placed.iter().map(|p| (p.offset_q * 1e6).round() / 1e6).collect();
        assert_eq!(q, [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0, 1.25, 1.5, 1.75].map(|v: f64| (v * 1e6).round() / 1e6));
        assert_eq!(report.beats_by_subdivision.get(&3), Some(&1));
        assert_eq!(report.beats_by_subdivision.get(&4), Some(&1));
    }

    #[test]
    fn a_shuffle_is_read_as_triplets() {
        let ticks: Vec<u64> = (0..8).flat_map(|b| [b * PPQ, b * PPQ + 320]).collect();
        let (placed, report) = run(&ticks, &MidiOptions::default());
        assert_eq!(report.beats_by_subdivision.get(&3), Some(&8), "{:?}", report.beats_by_subdivision);
        assert!((placed[1].offset_q - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_constant_offset_is_removed_and_reported() {
        // Everything 24 ticks (25 ms at 120 BPM) late.
        let ticks: Vec<u64> = (0..32).map(|i| i * PPQ / 2 + 24).collect();
        let (placed, report) = run(&ticks, &MidiOptions::default());
        assert!((report.offset_ms - 25.0).abs() < 1e-9, "{}", report.offset_ms);
        for (i, p) in placed.iter().enumerate() {
            assert!((p.offset_q + (p.bar as f64) * 4.0 - i as f64 / 2.0).abs() < 1e-9);
        }
        let opts = MidiOptions { remove_offset: false, ..MidiOptions::default() };
        let (_, kept) = run(&ticks, &opts);
        assert_eq!(kept.offset_ms, 0.0);
        assert!(kept.max_shift_ms > 24.0);
    }

    #[test]
    fn the_grid_option_limits_how_fine_the_result_gets() {
        let ticks = [0, 120, 240, 360];
        let opts = MidiOptions { grid: Grid::Eighth, ..MidiOptions::default() };
        let (placed, _) = run(&ticks, &opts);
        assert_eq!(offsets(&placed).iter().map(|p| p.1).collect::<Vec<_>>(), [0.0, 0.5, 1.0], "an eighth grid merges the sixteenths");
    }

    #[test]
    fn two_onsets_of_one_instrument_on_the_same_point_are_one_note_and_are_counted() {
        let onsets = [(0, Lane::Snare), (12, Lane::Snare), (0, Lane::Kick)];
        let (placed, report) = quantise(PPQ, &bars(2), &at_120(), &onsets, &MidiOptions::default());
        assert_eq!(placed.len(), 2);
        assert_eq!(report.merged, 1, "the flam");
    }

    #[test]
    fn a_note_that_rounds_up_to_the_next_bar_goes_there() {
        let (placed, _) = run(&[4 * PPQ - 10, 5 * PPQ + 8, 8 * PPQ - 3], &MidiOptions::default());
        assert_eq!(offsets(&placed), [(1, 0.0), (1, 1.0), (2, 0.0)]);
    }

    #[test]
    fn the_result_is_idempotent() {
        let mut rng = Lcg(99);
        let ticks: Vec<u64> = (0..64u64).map(|i| (i * PPQ / 2).saturating_add_signed(((rng.next() - 0.5) * 30.0) as i64)).collect();
        let (first, _) = run(&ticks, &MidiOptions::default());
        let snapped: Vec<u64> = first.iter().map(|p| ((p.bar as f64 * 4.0 + p.offset_q) * PPQ as f64).round() as u64).collect();
        let (second, report) = run(&snapped, &MidiOptions::default());
        assert_eq!(first, second);
        assert_eq!((report.max_shift_ms, report.offset_ms), (0.0, 0.0));
    }

    #[test]
    fn playing_that_is_not_on_any_grid_is_called_free_time() {
        let mut rng = Lcg(3);
        let ticks: Vec<u64> = (0..256u64).scan(0u64, |t, _| {
            *t += 150 + (rng.next() * 300.0) as u64;
            Some(*t)
        }).collect();
        let onsets: Vec<(u64, Lane)> = ticks.iter().map(|t| (*t, Lane::Snare)).collect();
        let (_, report) = quantise(PPQ, &bars(64), &at_120(), &onsets, &MidiOptions::default());
        assert!(report.free_time);
    }

}
