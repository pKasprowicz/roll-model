//! Compares captured hits against the score and computes proficiency metrics.

use std::collections::BTreeMap;

use crate::drums::Lane;
use crate::score::{Dynamic, Hand, Score};
use crate::stats::median;
use crate::units::{Millis, Seconds};

/// A captured hit, already mapped to a lane and expressed in score time
/// (seconds since the start of the practice range, latency-compensated).
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub t: Seconds,
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

impl Tolerances {
    pub fn perfect(&self) -> Millis {
        Millis(self.perfect_ms)
    }
    pub fn good(&self) -> Millis {
        Millis(self.good_ms)
    }
    pub fn window(&self) -> Millis {
        Millis(self.window_ms)
    }
}

impl Default for Tolerances {
    fn default() -> Self {
        Tolerances { perfect_ms: 20.0, good_ms: 45.0, window_ms: 100.0 }
    }
}

/// Every number that turns a take into a score and into advice, in one place to tune with real
/// takes. `Default` is the model the program ships with; `analyze_with_model` takes another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoringModel {
    /// Seconds added to the distance of a hit on a related pad, so that a right pad always wins.
    pub related_pad_penalty: Seconds,
    /// Adaptive window: this share of the gap to the neighbouring notes.
    pub gap_share: f64,
    /// A grace note is matched with a hit at most this far from its time, seconds.
    pub grace_window: Seconds,
    /// Timing score: `exp(-mean|dt| / this) * exp(-spread / that)`, both in ms.
    pub timing_decay_mean_ms: f64,
    pub timing_decay_spread_ms: f64,
    /// Weights of accuracy, timing, extra hits and dynamics when the score has accents or ghosts.
    pub weights_with_dynamics: Weights,
    /// The same without them (the dynamics weight is shared out).
    pub weights_without_dynamics: Weights,
    /// The share of the timing and extra-hit weights that is taken off for notes never played.
    pub unplayed_penalty: f64,
    /// An accent has to be this many velocity points above the plain notes, a ghost note this many below.
    pub accent_margin: f64,
    pub ghost_margin: f64,
    /// The plain velocity to compare with when a lane has no plain note.
    pub fallback_velocity: f64,
    /// Overall score from which each letter is given: A+, A, B, C, D (below the last: E).
    pub letters: [f64; 5],
    /// Advice appears from these deviations.
    pub advice: Advice,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weights {
    pub accuracy: f64,
    pub timing: f64,
    pub extra: f64,
    pub dynamics: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Advice {
    /// Mean offset, ms, and the number of graded hits needed for it to be worth saying.
    pub offset_ms: f64,
    pub offset_min_hits: usize,
    pub drift_ms_per_min: f64,
    pub spread_ms: f64,
    /// A lane's offset from the whole, ms: its own, and how far from the overall one.
    pub lane_offset_ms: f64,
    pub lane_offset_apart_ms: f64,
    /// Missed notes of a lane: one in this many.
    pub lane_missed_one_in: usize,
    pub lane_wrong_pad: usize,
    pub lane_extra: usize,
    /// Dynamics score under which accents and ghosts are called indistinct.
    pub dynamics_floor: f64,
    /// Difference between the hands, ms and velocity points.
    pub hands_ms: f64,
    pub hands_velocity: f64,
}

impl Default for ScoringModel {
    fn default() -> Self {
        ScoringModel {
            related_pad_penalty: Seconds(0.040),
            gap_share: 0.45,
            grace_window: Seconds(0.050),
            timing_decay_mean_ms: 45.0,
            timing_decay_spread_ms: 120.0,
            weights_with_dynamics: Weights { accuracy: 0.55, timing: 0.25, extra: 0.10, dynamics: 0.10 },
            weights_without_dynamics: Weights { accuracy: 0.60, timing: 0.28, extra: 0.12, dynamics: 0.0 },
            unplayed_penalty: 0.5,
            accent_margin: 10.0,
            ghost_margin: 15.0,
            fallback_velocity: 80.0,
            letters: [95.0, 90.0, 80.0, 70.0, 60.0],
            advice: Advice {
                offset_ms: 10.0,
                offset_min_hits: 4,
                drift_ms_per_min: 15.0,
                spread_ms: 25.0,
                lane_offset_ms: 15.0,
                lane_offset_apart_ms: 10.0,
                lane_missed_one_in: 5,
                lane_wrong_pad: 2,
                lane_extra: 3,
                dynamics_floor: 0.6,
                hands_ms: 8.0,
                hands_velocity: 10.0,
            },
        }
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
}

impl Grade {
    pub fn credit(self) -> f64 {
        match self {
            Grade::Perfect => 1.0,
            Grade::Good => 0.85,
            Grade::Ok => 0.6,
            Grade::WrongPad => 0.3,
            Grade::Missed => 0.0,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Grade::Perfect => "Perfect",
            Grade::Good => "Good",
            Grade::Ok => "OK",
            Grade::WrongPad => "Wrong pad",
            Grade::Missed => "Missed",
        }
    }
}

/// What became of an expected note: not reached yet (the recording still runs or stopped early),
/// or graded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pending,
    Graded(Grade),
}

impl Outcome {
    pub fn grade(self) -> Option<Grade> {
        match self {
            Outcome::Pending => None,
            Outcome::Graded(g) => Some(g),
        }
    }
    pub fn label(self) -> &'static str {
        self.grade().map_or("-", Grade::label)
    }
}

/// Expected note in the practice range, timed in seconds.
#[derive(Clone, Copy, Debug)]
pub struct Expected {
    /// Index into `Score::notes`.
    pub note_idx: usize,
    pub t: Seconds,
    pub lane: Lane,
    pub dynamic: Dynamic,
    pub grace: bool,
    pub hand: Option<Hand>,
}

#[derive(Clone, Copy, Debug)]
pub struct NoteResult {
    pub outcome: Outcome,
    pub hit: Option<usize>,
    /// Hit time minus expected time, ms (positive = late).
    pub dt: Option<Millis>,
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
        self.letter_in(&ScoringModel::default())
    }

    pub fn letter_in(&self, model: &ScoringModel) -> &'static str {
        const LETTERS: [&str; 5] = ["A+", "A", "B", "C", "D"];
        let at = model.letters.iter().position(|from| self.overall >= *from);
        at.map_or("E", |i| LETTERS[i])
    }
}

/// How long before its main note a grace note is played, in seconds.
const GRACE_SPACING: Seconds = Seconds(0.035);

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
            if n.is_grace() {
                // Grace notes / flams land just before the beat, the earlier ones of a drag earlier still.
                t -= GRACE_SPACING * f64::from(n.grace_order());
            }
            Expected { note_idx: i, t, lane: n.lane, dynamic: n.dynamic, grace: n.is_grace(), hand: n.hand }
        })
        .collect()
}

/// How far from its time a hit may be to count for each expected note: the hit window,
/// or, if `Window::Adaptive`, no more than 45 % of the gap to the neighbouring notes (but not
/// less than the "perfect" tolerance), so that a hit is never within reach of two
/// strokes at once at a fast tempo. A grace note is only looked for shortly around its time.
fn windows(expected: &[Expected], tol: &Tolerances, model: &ScoringModel, window: Window) -> Vec<Seconds> {
    let win = tol.window().secs();
    if window == Window::Fixed {
        return vec![win; expected.len()];
    }
    let floor = tol.perfect().secs().min(win);
    let mut times: Vec<Seconds> = expected.iter().filter(|e| !e.grace).map(|e| e.t).collect();
    times.sort_by(Seconds::total_cmp);
    expected
        .iter()
        .map(|e| {
            if e.grace {
                return win.min(model.grace_window);
            }
            // Notes played together (other lanes) are not neighbours.
            let before = times.partition_point(|t| *t < e.t - Seconds(1e-6));
            let after = times.partition_point(|t| *t <= e.t + Seconds(1e-6));
            let prev = before.checked_sub(1).map(|i| e.t - times[i]);
            let next = times.get(after).map(|t| *t - e.t);
            match prev.into_iter().chain(next).reduce(Seconds::min) {
                Some(gap) => (gap * model.gap_share).clamp(floor, win),
                None => win,
            }
        })
        .collect()
}

/// How far from its time a hit may be to count for a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// The tolerance's window, the same for every note.
    Fixed,
    /// No more than a share of the gap to the neighbouring notes: for fast exercises.
    Adaptive,
}

/// `analyze_with` with the fixed hit window: what a score is graded with.
pub fn analyze(expected: &[Expected], hits: &[Hit], tol: &Tolerances, elapsed: Seconds) -> Report {
    analyze_with(expected, hits, tol, elapsed, Window::Fixed)
}

/// `elapsed` is how far into the take we are (seconds); notes later than that are `Outcome::Pending`.
/// Pass `Seconds::INFINITY` for a finished take. With `Window::Adaptive` the hit window is shrunk to the
/// spacing of the notes (for fast exercises such as rudiments, where the usual window spans
/// several strokes).
pub fn analyze_with(expected: &[Expected], hits: &[Hit], tol: &Tolerances, elapsed: Seconds, window: Window) -> Report {
    analyze_with_model(expected, hits, tol, &ScoringModel::default(), elapsed, window)
}

/// `analyze_with` under another scoring model.
pub fn analyze_with_model(
    expected: &[Expected],
    hits: &[Hit],
    tol: &Tolerances,
    model: &ScoringModel,
    elapsed: Seconds,
    window: Window,
) -> Report {
    let wins = windows(expected, tol, model, window);
    let mut order: Vec<usize> = (0..hits.len()).collect();
    order.sort_by(|a, b| hits[*a].t.total_cmp(&hits[*b].t));
    let sorted_t: Vec<Seconds> = order.iter().map(|&i| hits[i].t).collect();

    // Candidate pairs: (cost, expected idx, hit idx, wrong_pad)
    let mut cands: Vec<(Seconds, usize, usize, bool)> = Vec::new();
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
                cands.push((dt + model.related_pad_penalty, ei, hi, true));
            }
        }
    }
    cands.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut results = vec![NoteResult { outcome: Outcome::Graded(Grade::Missed), hit: None, dt: None }; expected.len()];
    let mut hit_match: Vec<Option<usize>> = vec![None; hits.len()];
    let mut e_taken = vec![false; expected.len()];
    for (_, ei, hi, wrong) in cands {
        if e_taken[ei] || hit_match[hi].is_some() {
            continue;
        }
        e_taken[ei] = true;
        hit_match[hi] = Some(ei);
        let dt = (hits[hi].t - expected[ei].t).millis();
        let a = dt.abs();
        let grade = if wrong {
            Grade::WrongPad
        } else if a <= tol.perfect() {
            Grade::Perfect
        } else if a <= tol.good() {
            Grade::Good
        } else {
            Grade::Ok
        };
        results[ei] = NoteResult { outcome: Outcome::Graded(grade), hit: Some(hi), dt: Some(dt) };
    }
    for (ei, e) in expected.iter().enumerate() {
        if results[ei].hit.is_none() && e.t + wins[ei] > elapsed {
            results[ei].outcome = Outcome::Pending;
        }
    }

    let mut r = Report { results, expected: expected.to_vec(), hit_match, ..Default::default() };
    summarize(&mut r, hits, model);
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

fn summarize(r: &mut Report, hits: &[Hit], model: &ScoringModel) {
    let mut dts = Vec::new();
    let mut dt_points = Vec::new(); // (t, dt)
    let mut credit = 0.0;
    let mut lane_dts: BTreeMap<Lane, Vec<f64>> = BTreeMap::new();
    let mut lane_vel: BTreeMap<Lane, Vec<f64>> = BTreeMap::new();
    // Per hand (right, left): offsets of all strokes, velocities of the unaccented ones.
    let mut hand_dts: [Vec<f64>; 2] = Default::default();
    let mut hand_vel: [Vec<f64>; 2] = Default::default();

    for (e, res) in r.expected.iter().zip(&r.results) {
        let Outcome::Graded(grade) = res.outcome else { continue };
        // Grace notes are optional: never counted as missed.
        if e.grace && res.hit.is_none() {
            continue;
        }
        r.evaluated += 1;
        let ls = r.lanes.entry(e.lane).or_default();
        ls.expected += 1;
        credit += grade.credit();
        match grade {
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
        }
        if let (Some(dt), Some(hi)) = (res.dt.map(|d| d.0), res.hit) {
            ls.hit += 1;
            dts.push(dt);
            dt_points.push((e.t.0, dt));
            lane_dts.entry(e.lane).or_default().push(dt);
            lane_vel.entry(e.lane).or_default().push(hits[hi].velocity as f64);
            if let (Some(hand), false) = (e.hand, e.grace) {
                let side = usize::from(hand == Hand::Left);
                hand_dts[side].push(dt);
                if e.dynamic == Dynamic::Normal {
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
        .filter(|(_, res)| res.outcome != Outcome::Pending)
        .map(|(e, _)| e.t)
        .fold(Seconds::NEG_INFINITY, Seconds::max);
    for (hi, m) in r.hit_match.iter().enumerate() {
        if m.is_none() && hits[hi].t <= last_eval_t + Seconds(0.2) {
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
    r.timing_score = if dts.is_empty() {
        0.0
    } else {
        (-(r.mean_abs_dt_ms.max(0.0)) / model.timing_decay_mean_ms).exp() * (-(r.std_dt_ms) / model.timing_decay_spread_ms).exp()
    };
    r.extra_penalty = (r.extra as f64 / r.evaluated as f64).min(1.0);
    r.dynamics_score = dynamics_score(r, hits, model);

    let w = match r.dynamics_score {
        Some(_) => model.weights_with_dynamics,
        None => model.weights_without_dynamics,
    };
    let overall = w.accuracy * r.accuracy
        + w.timing * r.timing_score
        + w.extra * (1.0 - r.extra_penalty)
        + w.dynamics * r.dynamics_score.unwrap_or(0.0);
    // Timing credit only counts as far as notes were actually played.
    let played_fraction = (r.evaluated - r.missed) as f64 / r.evaluated as f64;
    r.overall = (100.0 * (overall - (1.0 - played_fraction) * (w.timing + w.extra) * model.unplayed_penalty)).clamp(0.0, 100.0);
    r.feedback = feedback(r, &model.advice);
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
fn dynamics_score(r: &Report, hits: &[Hit], model: &ScoringModel) -> Option<f64> {
    let mut plain: BTreeMap<Lane, Vec<f64>> = BTreeMap::new();
    for (e, res) in r.expected.iter().zip(&r.results) {
        if let Some(hi) = res.hit {
            if e.dynamic == Dynamic::Normal && !e.grace {
                plain.entry(e.lane).or_default().push(hits[hi].velocity as f64);
            }
        }
    }
    let mut checks = 0usize;
    let mut good = 0usize;
    for (e, res) in r.expected.iter().zip(&r.results) {
        let Some(hi) = res.hit else { continue };
        if e.dynamic == Dynamic::Normal {
            continue;
        }
        let v = hits[hi].velocity as f64;
        let reference = plain.get(&e.lane).filter(|p| !p.is_empty()).map(|v| median(&mut v.clone())).unwrap_or(model.fallback_velocity);
        checks += 1;
        if (e.dynamic == Dynamic::Accent && v >= reference + model.accent_margin)
            || (e.dynamic == Dynamic::Ghost && v <= reference - model.ghost_margin)
        {
            good += 1;
        }
    }
    (checks > 0).then(|| good as f64 / checks as f64)
}

fn feedback(r: &Report, a: &Advice) -> Vec<String> {
    let mut f = Vec::new();
    if r.mean_dt_ms.abs() >= a.offset_ms && r.perfect + r.good + r.ok > a.offset_min_hits {
        f.push(format!(
            "Overall you play {:.0} ms {} the beat.",
            r.mean_dt_ms.abs(),
            if r.mean_dt_ms > 0.0 { "behind (dragging)" } else { "ahead of (rushing)" }
        ));
    }
    if r.drift_ms_per_min.abs() >= a.drift_ms_per_min {
        f.push(format!(
            "Your time drifts {} by about {:.0} ms per minute — practise with the metronome on.",
            if r.drift_ms_per_min > 0.0 { "later (slowing down)" } else { "earlier (speeding up)" },
            r.drift_ms_per_min.abs()
        ));
    }
    if r.std_dt_ms >= a.spread_ms {
        f.push(format!("Timing is uneven (spread ±{:.0} ms). Slow the tempo down and aim for consistency.", r.std_dt_ms));
    }
    for (lane, ls) in &r.lanes {
        if ls.expected >= 4 && ls.missed * a.lane_missed_one_in >= ls.expected {
            f.push(format!("{}: missed {} of {} notes.", lane.label(), ls.missed, ls.expected));
        }
        if ls.hit >= 4 && ls.mean_dt_ms.abs() >= a.lane_offset_ms && (ls.mean_dt_ms - r.mean_dt_ms).abs() >= a.lane_offset_apart_ms {
            f.push(format!(
                "{} tends to be {:.0} ms {}.",
                lane.label(),
                ls.mean_dt_ms.abs(),
                if ls.mean_dt_ms > 0.0 { "late" } else { "early" }
            ));
        }
        if ls.wrong_pad >= a.lane_wrong_pad {
            f.push(format!("{}: {} hits on the wrong zone/articulation.", lane.label(), ls.wrong_pad));
        }
        if ls.extra >= a.lane_extra {
            f.push(format!("{}: {} extra hits that are not in the score.", lane.label(), ls.extra));
        }
    }
    if let Some(d) = r.dynamics_score {
        if d < a.dynamics_floor {
            f.push("Accents and ghost notes are not distinct enough in velocity.".into());
        }
    }
    hand_feedback(r, a, &mut f);
    if f.is_empty() && r.evaluated > 0 {
        f.push("Solid take — try a faster tempo or a harder section.".into());
    }
    f
}

/// Strokes needed from each hand before their difference is worth reporting.
const MIN_HAND_STROKES: usize = 4;

/// Compares the hands: how far apart in time and in loudness their plain strokes are.
fn hand_feedback(r: &Report, a: &Advice, f: &mut Vec<String>) {
    let stats = |hand: Hand| r.hands.iter().find(|(h, _)| *h == hand).map(|(_, s)| *s);
    let (Some(right), Some(left)) = (stats(Hand::Right), stats(Hand::Left)) else { return };
    if right.hit >= MIN_HAND_STROKES && left.hit >= MIN_HAND_STROKES {
        let apart = left.mean_dt_ms - right.mean_dt_ms;
        if apart.abs() >= a.hands_ms {
            let (late, early) = if apart > 0.0 { (Hand::Left, Hand::Right) } else { (Hand::Right, Hand::Left) };
            f.push(format!("Your {} is {:.0} ms behind your {}.", late.label().to_lowercase(), apart.abs(), early.label().to_lowercase()));
        }
    }
    if right.plain >= MIN_HAND_STROKES && left.plain >= MIN_HAND_STROKES {
        let apart = left.mean_velocity - right.mean_velocity;
        if apart.abs() >= a.hands_velocity {
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
mod tests;
