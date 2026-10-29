//! Compares captured hits against the score and computes proficiency metrics.

use std::collections::BTreeMap;

use crate::drums::Lane;
use crate::score::{Hand, Score};

/// A captured hit, already mapped to a lane and expressed in score time
/// (seconds since the start of the practice range, latency-compensated).
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub t: f64,
    pub note: u8,
    pub velocity: u8,
    pub lane: Lane,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerances {
    pub perfect_ms: f64,
    pub good_ms: f64,
    /// Hits further than this from any expected note are "extra" hits.
    pub window_ms: f64,
}

impl Default for Tolerances {
    fn default() -> Self {
        Tolerances { perfect_ms: 20.0, good_ms: 45.0, window_ms: 100.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    Perfect,
    Good,
    Ok,
    /// Right time, related pad (e.g. open instead of closed hi-hat).
    WrongPad,
    Missed,
    /// Not yet reached (recording still running or stopped early).
    Pending,
}

impl Grade {
    pub fn credit(self) -> f64 {
        match self {
            Grade::Perfect => 1.0,
            Grade::Good => 0.85,
            Grade::Ok => 0.6,
            Grade::WrongPad => 0.3,
            Grade::Missed | Grade::Pending => 0.0,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Grade::Perfect => "Perfect",
            Grade::Good => "Good",
            Grade::Ok => "OK",
            Grade::WrongPad => "Wrong pad",
            Grade::Missed => "Missed",
            Grade::Pending => "-",
        }
    }
}

/// Expected note in the practice range, timed in seconds.
#[derive(Clone, Copy, Debug)]
pub struct Expected {
    /// Index into `Score::notes`.
    pub note_idx: usize,
    pub t: f64,
    pub lane: Lane,
    pub accent: bool,
    pub ghost: bool,
    pub grace: bool,
    pub hand: Option<Hand>,
}

#[derive(Clone, Copy, Debug)]
pub struct NoteResult {
    pub grade: Grade,
    pub hit: Option<usize>,
    /// Hit time minus expected time, ms (positive = late).
    pub dt_ms: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct LaneStats {
    pub expected: usize,
    pub hit: usize,
    pub missed: usize,
    pub wrong_pad: usize,
    pub extra: usize,
    pub mean_dt_ms: f64,
    pub std_dt_ms: f64,
    pub mean_velocity: f64,
}

/// What one hand did in a take of a score that says which hand plays what.
#[derive(Clone, Copy, Debug, Default)]
pub struct HandStats {
    /// Strokes of this hand that were matched with a hit.
    pub hit: usize,
    /// Mean offset of those, ms (positive = late).
    pub mean_dt_ms: f64,
    /// Strokes without an accent, matched with a hit; the evenness of the hands is read from them.
    pub plain: usize,
    pub mean_velocity: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Parallel to `expected`.
    pub results: Vec<NoteResult>,
    pub expected: Vec<Expected>,
    /// For every captured hit: matched expected index (into `expected`).
    pub hit_match: Vec<Option<usize>>,

    pub evaluated: usize,
    pub perfect: usize,
    pub good: usize,
    pub ok: usize,
    pub wrong_pad: usize,
    pub missed: usize,
    pub extra: usize,

    /// 0..1, credit-weighted note accuracy.
    pub accuracy: f64,
    pub mean_dt_ms: f64,
    pub mean_abs_dt_ms: f64,
    pub std_dt_ms: f64,
    /// Drift of timing over the take, ms per minute (positive = getting later).
    pub drift_ms_per_min: f64,
    pub timing_score: f64,
    /// None when the score has neither accents nor ghost notes.
    pub dynamics_score: Option<f64>,
    pub extra_penalty: f64,
    /// 0..100.
    pub overall: f64,
    pub lanes: BTreeMap<Lane, LaneStats>,
    /// Right, then left; empty unless the score names the hands.
    pub hands: Vec<(Hand, HandStats)>,
    pub feedback: Vec<String>,
}

impl Report {
    pub fn letter(&self) -> &'static str {
        match self.overall {
            x if x >= 95.0 => "A+",
            x if x >= 90.0 => "A",
            x if x >= 80.0 => "B",
            x if x >= 70.0 => "C",
            x if x >= 60.0 => "D",
            _ => "E",
        }
    }
}

/// How long before its main note a grace note is played, in seconds.
const GRACE_SPACING: f64 = 0.035;

/// Expected notes whose measures lie in `[from_measure, to_measure]`, timed
/// relative to the start of `from_measure`.
pub fn expected_notes(score: &Score, from_measure: usize, to_measure: usize, tempo_scale: f64) -> Vec<Expected> {
    let Some(first) = score.measures.get(from_measure) else { return Vec::new() };
    let t0 = score.tempo.q_to_sec(first.start_q, tempo_scale);
    score
        .notes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.measure >= from_measure && n.measure <= to_measure)
        .map(|(i, n)| {
            let mut t = score.tempo.q_to_sec(n.q, tempo_scale) - t0;
            if n.grace {
                // Grace notes / flams land just before the beat, the earlier ones of a drag earlier still.
                t -= GRACE_SPACING * f64::from(n.grace_order.max(1));
            }
            Expected { note_idx: i, t, lane: n.lane, accent: n.accent, ghost: n.ghost, grace: n.grace, hand: n.hand }
        })
        .collect()
}

/// How far from its time a hit may be to count for each expected note: the hit window,
/// or, if `adaptive`, no more than 45 % of the gap to the neighbouring notes (but not
/// less than the "perfect" tolerance), so that a hit is never within reach of two
/// strokes at once at a fast tempo. A grace note is only looked for shortly around its time.
fn windows(expected: &[Expected], tol: &Tolerances, adaptive: bool) -> Vec<f64> {
    let win = tol.window_ms / 1000.0;
    if !adaptive {
        return vec![win; expected.len()];
    }
    let floor = (tol.perfect_ms / 1000.0).min(win);
    let mut times: Vec<f64> = expected.iter().filter(|e| !e.grace).map(|e| e.t).collect();
    times.sort_by(f64::total_cmp);
    expected
        .iter()
        .map(|e| {
            if e.grace {
                return win.min(GRACE_WINDOW);
            }
            // Notes played together (other lanes) are not neighbours.
            let before = times.partition_point(|t| *t < e.t - 1e-6);
            let after = times.partition_point(|t| *t <= e.t + 1e-6);
            let prev = before.checked_sub(1).map(|i| e.t - times[i]);
            let next = times.get(after).map(|t| t - e.t);
            match prev.into_iter().chain(next).reduce(f64::min) {
                Some(gap) => (0.45 * gap).clamp(floor, win),
                None => win,
            }
        })
        .collect()
}

/// A grace note is matched with a hit at most this far from its time, seconds.
const GRACE_WINDOW: f64 = 0.050;

/// `analyze_with` with the fixed hit window: what a score is graded with.
pub fn analyze(expected: &[Expected], hits: &[Hit], tol: &Tolerances, elapsed: f64) -> Report {
    analyze_with(expected, hits, tol, elapsed, false)
}

/// `elapsed` is how far into the take we are (seconds); notes later than that are `Pending`.
/// Pass `f64::INFINITY` for a finished take. With `adaptive` the hit window is shrunk to the
/// spacing of the notes (for fast exercises such as rudiments, where the usual window spans
/// several strokes).
pub fn analyze_with(expected: &[Expected], hits: &[Hit], tol: &Tolerances, elapsed: f64, adaptive: bool) -> Report {
    let wins = windows(expected, tol, adaptive);
    let mut order: Vec<usize> = (0..hits.len()).collect();
    order.sort_by(|a, b| hits[*a].t.total_cmp(&hits[*b].t));
    let sorted_t: Vec<f64> = order.iter().map(|&i| hits[i].t).collect();

    // Candidate pairs: (cost, expected idx, hit idx, wrong_pad)
    let mut cands: Vec<(f64, usize, usize, bool)> = Vec::new();
    for (ei, e) in expected.iter().enumerate() {
        let win = wins[ei];
        let lo = sorted_t.partition_point(|t| *t < e.t - win);
        for &hi in &order[lo..] {
            let h = &hits[hi];
            if h.t > e.t + win {
                break;
            }
            let dt = (h.t - e.t).abs();
            if h.lane == e.lane {
                cands.push((dt, ei, hi, false));
            } else if h.lane.related(e.lane) {
                cands.push((dt + 0.040, ei, hi, true));
            }
        }
    }
    cands.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut results = vec![NoteResult { grade: Grade::Missed, hit: None, dt_ms: None }; expected.len()];
    let mut hit_match: Vec<Option<usize>> = vec![None; hits.len()];
    let mut e_taken = vec![false; expected.len()];
    for (_, ei, hi, wrong) in cands {
        if e_taken[ei] || hit_match[hi].is_some() {
            continue;
        }
        e_taken[ei] = true;
        hit_match[hi] = Some(ei);
        let dt_ms = (hits[hi].t - expected[ei].t) * 1000.0;
        let a = dt_ms.abs();
        let grade = if wrong {
            Grade::WrongPad
        } else if a <= tol.perfect_ms {
            Grade::Perfect
        } else if a <= tol.good_ms {
            Grade::Good
        } else {
            Grade::Ok
        };
        results[ei] = NoteResult { grade, hit: Some(hi), dt_ms: Some(dt_ms) };
    }
    for (ei, e) in expected.iter().enumerate() {
        if results[ei].hit.is_none() && e.t + wins[ei] > elapsed {
            results[ei].grade = Grade::Pending;
        }
    }

    let mut r = Report { results, expected: expected.to_vec(), hit_match, ..Default::default() };
    summarize(&mut r, hits);
    r
}

fn mean_std(v: &[f64]) -> (f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    let m = v.iter().sum::<f64>() / v.len() as f64;
    let var = v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / v.len() as f64;
    (m, var.sqrt())
}

fn summarize(r: &mut Report, hits: &[Hit]) {
    let mut dts = Vec::new();
    let mut dt_points = Vec::new(); // (t, dt)
    let mut credit = 0.0;
    let mut lane_dts: BTreeMap<Lane, Vec<f64>> = BTreeMap::new();
    let mut lane_vel: BTreeMap<Lane, Vec<f64>> = BTreeMap::new();
    // Per hand (right, left): offsets of all strokes, velocities of the unaccented ones.
    let mut hand_dts: [Vec<f64>; 2] = Default::default();
    let mut hand_vel: [Vec<f64>; 2] = Default::default();

    for (e, res) in r.expected.iter().zip(&r.results) {
        if res.grade == Grade::Pending {
            continue;
        }
        // Grace notes are optional: never counted as missed.
        if e.grace && res.hit.is_none() {
            continue;
        }
        r.evaluated += 1;
        let ls = r.lanes.entry(e.lane).or_default();
        ls.expected += 1;
        credit += res.grade.credit();
        match res.grade {
            Grade::Perfect => r.perfect += 1,
            Grade::Good => r.good += 1,
            Grade::Ok => r.ok += 1,
            Grade::WrongPad => {
                r.wrong_pad += 1;
                ls.wrong_pad += 1;
            }
            Grade::Missed => {
                r.missed += 1;
                ls.missed += 1;
            }
            Grade::Pending => {}
        }
        if let (Some(dt), Some(hi)) = (res.dt_ms, res.hit) {
            ls.hit += 1;
            dts.push(dt);
            dt_points.push((e.t, dt));
            lane_dts.entry(e.lane).or_default().push(dt);
            lane_vel.entry(e.lane).or_default().push(hits[hi].velocity as f64);
            if let (Some(hand), false) = (e.hand, e.grace) {
                let side = usize::from(hand == Hand::Left);
                hand_dts[side].push(dt);
                if !e.accent && !e.ghost {
                    hand_vel[side].push(hits[hi].velocity as f64);
                }
            }
        }
    }
    for (side, hand) in [Hand::Right, Hand::Left].into_iter().enumerate() {
        if !r.expected.iter().any(|e| e.hand == Some(hand) && !e.grace) {
            continue;
        }
        r.hands.push((
            hand,
            HandStats {
                hit: hand_dts[side].len(),
                mean_dt_ms: mean_std(&hand_dts[side]).0,
                plain: hand_vel[side].len(),
                mean_velocity: mean_std(&hand_vel[side]).0,
            },
        ));
    }
    let last_eval_t = r
        .expected
        .iter()
        .zip(&r.results)
        .filter(|(_, res)| res.grade != Grade::Pending)
        .map(|(e, _)| e.t)
        .fold(f64::NEG_INFINITY, f64::max);
    for (hi, m) in r.hit_match.iter().enumerate() {
        if m.is_none() && hits[hi].t <= last_eval_t + 0.2 {
            r.extra += 1;
            r.lanes.entry(hits[hi].lane).or_default().extra += 1;
        }
    }
    for (lane, ls) in r.lanes.iter_mut() {
        if let Some(v) = lane_dts.get(lane) {
            let (m, s) = mean_std(v);
            ls.mean_dt_ms = m;
            ls.std_dt_ms = s;
        }
        if let Some(v) = lane_vel.get(lane) {
            ls.mean_velocity = mean_std(v).0;
        }
    }

    if r.evaluated == 0 {
        return;
    }
    r.accuracy = credit / r.evaluated as f64;
    let (m, s) = mean_std(&dts);
    r.mean_dt_ms = m;
    r.std_dt_ms = s;
    r.mean_abs_dt_ms = if dts.is_empty() { 0.0 } else { dts.iter().map(|d| d.abs()).sum::<f64>() / dts.len() as f64 };
    r.drift_ms_per_min = regression_slope(&dt_points) * 60.0;
    r.timing_score = if dts.is_empty() { 0.0 } else { (-(r.mean_abs_dt_ms.max(0.0)) / 45.0).exp() * (-(r.std_dt_ms) / 120.0).exp() };
    r.extra_penalty = (r.extra as f64 / r.evaluated as f64).min(1.0);
    r.dynamics_score = dynamics_score(r, hits);

    let (w_acc, w_time, w_extra, w_dyn) = match r.dynamics_score {
        Some(_) => (0.55, 0.25, 0.10, 0.10),
        None => (0.60, 0.28, 0.12, 0.0),
    };
    let overall = w_acc * r.accuracy
        + w_time * r.timing_score
        + w_extra * (1.0 - r.extra_penalty)
        + w_dyn * r.dynamics_score.unwrap_or(0.0);
    // Timing credit only counts as far as notes were actually played.
    let played_fraction = (r.evaluated - r.missed) as f64 / r.evaluated as f64;
    r.overall = (100.0 * (overall - (1.0 - played_fraction) * (w_time + w_extra) * 0.5)).clamp(0.0, 100.0);
    r.feedback = feedback(r);
}

fn regression_slope(points: &[(f64, f64)]) -> f64 {
    if points.len() < 8 {
        return 0.0;
    }
    let n = points.len() as f64;
    let mx = points.iter().map(|p| p.0).sum::<f64>() / n;
    let my = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
    if sxx < 1e-9 {
        return 0.0;
    }
    points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / sxx
}

/// Accented notes should be louder, ghost notes softer, than plain notes on
/// the same lane.
fn dynamics_score(r: &Report, hits: &[Hit]) -> Option<f64> {
    let mut plain: BTreeMap<Lane, Vec<f64>> = BTreeMap::new();
    for (e, res) in r.expected.iter().zip(&r.results) {
        if let Some(hi) = res.hit {
            if !e.accent && !e.ghost && !e.grace {
                plain.entry(e.lane).or_default().push(hits[hi].velocity as f64);
            }
        }
    }
    let median = |v: &Vec<f64>| {
        let mut s = v.clone();
        s.sort_by(|a, b| a.total_cmp(b));
        s[s.len() / 2]
    };
    let mut checks = 0usize;
    let mut good = 0usize;
    for (e, res) in r.expected.iter().zip(&r.results) {
        let Some(hi) = res.hit else { continue };
        if !(e.accent || e.ghost) {
            continue;
        }
        let v = hits[hi].velocity as f64;
        let reference = plain.get(&e.lane).filter(|p| !p.is_empty()).map(median).unwrap_or(80.0);
        checks += 1;
        if (e.accent && v >= reference + 10.0) || (e.ghost && v <= reference - 15.0) {
            good += 1;
        }
    }
    (checks > 0).then(|| good as f64 / checks as f64)
}

fn feedback(r: &Report) -> Vec<String> {
    let mut f = Vec::new();
    if r.mean_dt_ms.abs() >= 10.0 && r.perfect + r.good + r.ok > 4 {
        f.push(format!(
            "Overall you play {:.0} ms {} the beat.",
            r.mean_dt_ms.abs(),
            if r.mean_dt_ms > 0.0 { "behind (dragging)" } else { "ahead of (rushing)" }
        ));
    }
    if r.drift_ms_per_min.abs() >= 15.0 {
        f.push(format!(
            "Your time drifts {} by about {:.0} ms per minute — practise with the metronome on.",
            if r.drift_ms_per_min > 0.0 { "later (slowing down)" } else { "earlier (speeding up)" },
            r.drift_ms_per_min.abs()
        ));
    }
    if r.std_dt_ms >= 25.0 {
        f.push(format!("Timing is uneven (spread ±{:.0} ms). Slow the tempo down and aim for consistency.", r.std_dt_ms));
    }
    for (lane, ls) in &r.lanes {
        if ls.expected >= 4 && ls.missed * 5 >= ls.expected {
            f.push(format!("{}: missed {} of {} notes.", lane.label(), ls.missed, ls.expected));
        }
        if ls.hit >= 4 && ls.mean_dt_ms.abs() >= 15.0 && (ls.mean_dt_ms - r.mean_dt_ms).abs() >= 10.0 {
            f.push(format!(
                "{} tends to be {:.0} ms {}.",
                lane.label(),
                ls.mean_dt_ms.abs(),
                if ls.mean_dt_ms > 0.0 { "late" } else { "early" }
            ));
        }
        if ls.wrong_pad >= 2 {
            f.push(format!("{}: {} hits on the wrong zone/articulation.", lane.label(), ls.wrong_pad));
        }
        if ls.extra >= 3 {
            f.push(format!("{}: {} extra hits that are not in the score.", lane.label(), ls.extra));
        }
    }
    if let Some(d) = r.dynamics_score {
        if d < 0.6 {
            f.push("Accents and ghost notes are not distinct enough in velocity.".into());
        }
    }
    hand_feedback(r, &mut f);
    if f.is_empty() && r.evaluated > 0 {
        f.push("Solid take — try a faster tempo or a harder section.".into());
    }
    f
}

/// Strokes needed from each hand before their difference is worth reporting.
const MIN_HAND_STROKES: usize = 4;

/// Compares the hands: how far apart in time and in loudness their plain strokes are.
fn hand_feedback(r: &Report, f: &mut Vec<String>) {
    let stats = |hand: Hand| r.hands.iter().find(|(h, _)| *h == hand).map(|(_, s)| *s);
    let (Some(right), Some(left)) = (stats(Hand::Right), stats(Hand::Left)) else { return };
    if right.hit >= MIN_HAND_STROKES && left.hit >= MIN_HAND_STROKES {
        let apart = left.mean_dt_ms - right.mean_dt_ms;
        if apart.abs() >= 8.0 {
            let (late, early) = if apart > 0.0 { (Hand::Left, Hand::Right) } else { (Hand::Right, Hand::Left) };
            f.push(format!("Your {} is {:.0} ms behind your {}.", late.label().to_lowercase(), apart.abs(), early.label().to_lowercase()));
        }
    }
    if right.plain >= MIN_HAND_STROKES && left.plain >= MIN_HAND_STROKES {
        let apart = left.mean_velocity - right.mean_velocity;
        if apart.abs() >= 10.0 {
            let (soft, loud) = if apart < 0.0 { (Hand::Left, Hand::Right) } else { (Hand::Right, Hand::Left) };
            f.push(format!(
                "Your {} plays {:.0} velocity points softer than your {}: aim for even strokes.",
                soft.label().to_lowercase(),
                apart.abs(),
                loud.label().to_lowercase()
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exp(t: f64, lane: Lane) -> Expected {
        Expected { note_idx: 0, t, lane, accent: false, ghost: false, grace: false, hand: None }
    }
    fn hit(t: f64, lane: Lane) -> Hit {
        Hit { t, note: 0, velocity: 90, lane }
    }

    #[test]
    fn perfect_take_scores_high() {
        let e: Vec<_> = (0..32).map(|i| exp(i as f64 * 0.3, if i % 2 == 0 { Lane::Kick } else { Lane::Snare })).collect();
        let h: Vec<_> = e.iter().map(|x| hit(x.t + 0.003, x.lane)).collect();
        let r = analyze(&e, &h, &Tolerances::default(), f64::INFINITY);
        assert_eq!(r.perfect, 32);
        assert_eq!(r.extra, 0);
        assert!(r.overall > 95.0, "overall {}", r.overall);
        assert_eq!(r.letter(), "A+");
    }

    #[test]
    fn grading_and_matching() {
        let e = vec![exp(0.0, Lane::Kick), exp(0.5, Lane::Snare), exp(1.0, Lane::HiHatClosed), exp(1.5, Lane::Tom1), exp(2.0, Lane::Kick)];
        let h = vec![
            hit(0.010, Lane::Kick),       // perfect
            hit(0.540, Lane::Snare),      // good
            hit(1.010, Lane::HiHatOpen),  // wrong pad
            // tom1 missed
            hit(2.090, Lane::Kick),       // ok
            hit(3.000, Lane::Snare),      // extra? beyond last eval + 0.2 -> ignored
            hit(1.2, Lane::Ride),         // extra
        ];
        let r = analyze(&e, &h, &Tolerances::default(), f64::INFINITY);
        let g: Vec<Grade> = r.results.iter().map(|x| x.grade).collect();
        assert_eq!(g, vec![Grade::Perfect, Grade::Good, Grade::WrongPad, Grade::Missed, Grade::Ok]);
        assert_eq!(r.extra, 1);
        assert!((r.results[1].dt_ms.unwrap() - 40.0).abs() < 1e-6);
    }

    #[test]
    fn closest_hit_wins_and_pending() {
        let e = vec![exp(1.0, Lane::Snare), exp(1.05, Lane::Snare), exp(5.0, Lane::Kick)];
        let h = vec![hit(1.049, Lane::Snare), hit(0.99, Lane::Snare)];
        let r = analyze(&e, &h, &Tolerances::default(), 2.0);
        assert_eq!(r.results[0].hit, Some(1));
        assert_eq!(r.results[1].hit, Some(0));
        assert_eq!(r.results[2].grade, Grade::Pending);
        assert_eq!(r.evaluated, 2);
    }

    #[test]
    fn dragging_feedback() {
        let e: Vec<_> = (0..40).map(|i| exp(i as f64 * 0.25, Lane::Snare)).collect();
        let h: Vec<_> = e.iter().map(|x| hit(x.t + 0.030, x.lane)).collect();
        let r = analyze(&e, &h, &Tolerances::default(), f64::INFINITY);
        assert!((r.mean_dt_ms - 30.0).abs() < 1e-6);
        assert!(r.feedback.iter().any(|s| s.contains("dragging")));
    }

    #[test]
    fn the_adaptive_window_follows_the_spacing_of_the_notes() {
        // 16ths at 150 BPM, 100 ms apart: the usual window (100 ms) reaches the neighbours.
        let e: Vec<_> = (0..8).map(|i| exp(i as f64 * 0.1, Lane::Snare)).collect();
        let tol = Tolerances::default();
        let w = windows(&e, &tol, true);
        assert!(w.iter().all(|x| (x - 0.045).abs() < 1e-9), "{w:?}");
        assert!(windows(&e, &tol, false).iter().all(|x| (x - 0.1).abs() < 1e-9));
        // Slow notes keep the whole window; very fast ones never go below "perfect".
        let slow: Vec<_> = (0..4).map(|i| exp(i as f64, Lane::Snare)).collect();
        assert!(windows(&slow, &tol, true).iter().all(|x| (x - 0.1).abs() < 1e-9));
        let fast: Vec<_> = (0..4).map(|i| exp(i as f64 * 0.03, Lane::Snare)).collect();
        assert!(windows(&fast, &tol, true).iter().all(|x| (x - 0.02).abs() < 1e-9));
        // Notes played together on two lanes are not neighbours of each other.
        let together = vec![exp(0.0, Lane::Snare), exp(0.0, Lane::Kick), exp(1.0, Lane::Snare)];
        assert!((windows(&together, &tol, true)[0] - 0.1).abs() < 1e-9);
        // A grace note is only looked for close to its time.
        let flam = Expected { grace: true, ..exp(-0.035, Lane::Snare) };
        assert!((windows(&[flam], &tol, true)[0] - GRACE_WINDOW).abs() < 1e-9);
    }

    #[test]
    fn with_the_adaptive_window_a_hit_far_from_its_stroke_is_not_matched_to_it() {
        // Strokes every 80 ms; one hit is 60 ms late: within the fixed 100 ms window of its
        // stroke, but out of reach of the adaptive window (36 ms).
        let e: Vec<_> = (0..6).map(|i| exp(i as f64 * 0.08, Lane::Snare)).collect();
        let mut h: Vec<_> = e.iter().map(|x| hit(x.t, x.lane)).collect();
        h[2].t += 0.06;
        let fixed = analyze(&e, &h, &Tolerances::default(), f64::INFINITY);
        let adaptive = analyze_with(&e, &h, &Tolerances::default(), f64::INFINITY, true);
        assert_eq!(fixed.missed, 0);
        assert_eq!(adaptive.missed, 1);
        assert_eq!(adaptive.extra, 1);
    }

    #[test]
    fn the_hands_are_compared_when_the_score_names_them() {
        // Alternating strokes; the left hand is 12 ms late and 25 velocity points softer.
        let expected: Vec<Expected> = (0..16)
            .map(|i| Expected { hand: Some(if i % 2 == 0 { Hand::Right } else { Hand::Left }), ..exp(i as f64 * 0.2, Lane::Snare) })
            .collect();
        let hits: Vec<Hit> = expected
            .iter()
            .map(|e| {
                let left = e.hand == Some(Hand::Left);
                Hit { t: e.t + if left { 0.012 } else { 0.0 }, note: 38, velocity: if left { 70 } else { 95 }, lane: Lane::Snare }
            })
            .collect();
        let r = analyze(&expected, &hits, &Tolerances::default(), f64::INFINITY);
        assert_eq!(r.hands.len(), 2);
        let (right, left) = (r.hands[0].1, r.hands[1].1);
        assert_eq!((right.hit, left.hit), (8, 8));
        assert!((left.mean_dt_ms - 12.0).abs() < 1e-6 && right.mean_dt_ms.abs() < 1e-6);
        assert!((right.mean_velocity - 95.0).abs() < 1e-9 && (left.mean_velocity - 70.0).abs() < 1e-9);
        assert!(r.feedback.iter().any(|s| s.contains("left hand is 12 ms behind your right hand")), "{:?}", r.feedback);
        assert!(r.feedback.iter().any(|s| s.contains("left hand plays 25 velocity points softer")), "{:?}", r.feedback);
        // Even hands say nothing about them; a score without hands has no hand statistics.
        let even: Vec<Hit> = expected.iter().map(|e| Hit { t: e.t, note: 38, velocity: 90, lane: Lane::Snare }).collect();
        let r = analyze(&expected, &even, &Tolerances::default(), f64::INFINITY);
        assert!(!r.feedback.iter().any(|s| s.contains("hand")), "{:?}", r.feedback);
        let plain: Vec<Expected> = (0..4).map(|i| exp(i as f64 * 0.2, Lane::Snare)).collect();
        assert!(analyze(&plain, &[], &Tolerances::default(), f64::INFINITY).hands.is_empty());
    }

    #[test]
    fn the_earlier_grace_notes_of_a_drag_are_expected_earlier() {
        use crate::score::{Measure, ScoreNote, TempoMap};
        let note = |q: f64, grace_order: u8| ScoreNote {
            q,
            measure: 0,
            lane: Lane::Snare,
            staff_step: 5,
            head: crate::drums::NoteHead::Normal,
            accent: false,
            ghost: false,
            grace: grace_order > 0,
            grace_order,
            hand: None,
            stem_up: None,
        };
        let score = Score {
            measures: vec![Measure { number: "1".into(), start_q: 0.0, len_q: 4.0, beats: 4, beat_type: 4 }],
            notes: vec![note(1.0, 2), note(1.0, 1), note(1.0, 0)],
            tempo: TempoMap::constant(60.0),
            ..Score::default()
        };
        let t: Vec<f64> = expected_notes(&score, 0, 0, 1.0).iter().map(|e| e.t).collect();
        assert!((t[2] - 1.0).abs() < 1e-9);
        assert!((t[1] - (1.0 - GRACE_SPACING)).abs() < 1e-9);
        assert!((t[0] - (1.0 - 2.0 * GRACE_SPACING)).abs() < 1e-9);
    }
}
