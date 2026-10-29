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
