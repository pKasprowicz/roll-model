//! Compares captured hits against the score and computes proficiency metrics.

use std::collections::BTreeMap;

use crate::drums::Lane;
use crate::score::Score;

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
                t -= 0.035; // grace notes / flams land just before the beat
            }
            Expected { note_idx: i, t, lane: n.lane, accent: n.accent, ghost: n.ghost, grace: n.grace }
        })
        .collect()
}

/// `elapsed` is how far into the take we are (seconds); notes later than
/// that are `Pending`. Pass `f64::INFINITY` for a finished take.
pub fn analyze(expected: &[Expected], hits: &[Hit], tol: &Tolerances, elapsed: f64) -> Report {
    let win = tol.window_ms / 1000.0;
    let mut order: Vec<usize> = (0..hits.len()).collect();
    order.sort_by(|a, b| hits[*a].t.total_cmp(&hits[*b].t));
    let sorted_t: Vec<f64> = order.iter().map(|&i| hits[i].t).collect();

    // Candidate pairs: (cost, expected idx, hit idx, wrong_pad)
    let mut cands: Vec<(f64, usize, usize, bool)> = Vec::new();
    for (ei, e) in expected.iter().enumerate() {
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
        if results[ei].hit.is_none() && e.t + win > elapsed {
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
        }
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
    if f.is_empty() && r.evaluated > 0 {
        f.push("Solid take — try a faster tempo or a harder section.".into());
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exp(t: f64, lane: Lane) -> Expected {
        Expected { note_idx: 0, t, lane, accent: false, ghost: false, grace: false }
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
}
