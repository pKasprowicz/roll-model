//! Sample rate conversion for the sounds of the kit and the clicks: they are made at
//! `synth::SAMPLE_RATE` and the sound card may run at another rate (48 kHz is usual). A windowed
//! sinc interpolator, so the cymbals keep their top and nothing folds back below the new Nyquist
//! frequency when the rate goes down. Done once per run, off the audio thread.

use std::f64::consts::PI;

/// Input samples on each side of a point that take part in an output sample, at the narrower of the
/// two rates (so a conversion to a lower rate looks further out, the filter being longer there).
const HALF_WIDTH: f64 = 16.0;

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

/// `input` (at `from` Hz) as it sounds at `to` Hz.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || to == 0 || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    // Going down, everything above the new Nyquist frequency is cut off first.
    let cutoff = ratio.min(1.0);
    let half = (HALF_WIDTH / cutoff).ceil();
    let out_len = (input.len() as f64 * ratio).round() as usize;
    let last = input.len() as isize - 1;
    (0..out_len)
        .map(|n| {
            let pos = n as f64 / ratio;
            let centre = pos.floor() as isize;
            let (lo, hi) = ((centre - half as isize + 1).max(0), (centre + half as isize).min(last));
            let mut acc = 0.0f64;
            for k in lo..=hi {
                let d = pos - k as f64;
                let window = 0.5 * (1.0 + (PI * d / half).cos());
                acc += input[k as usize] as f64 * sinc(d * cutoff) * cutoff * window;
            }
            acc as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: u32, frames: usize) -> Vec<f32> {
        (0..frames).map(|i| (2.0 * PI * freq * i as f64 / rate as f64).sin() as f32).collect()
    }

    /// The largest difference from the ideal sine, leaving out the ends where the window is cut.
    fn worst_error(got: &[f32], freq: f64, rate: u32) -> f64 {
        let edge = 40;
        (edge..got.len() - edge)
            .map(|i| (got[i] as f64 - (2.0 * PI * freq * i as f64 / rate as f64).sin()).abs())
            .fold(0.0, f64::max)
    }

    #[test]
    fn the_same_rate_changes_nothing() {
        let s = sine(440.0, 44_100, 1000);
        assert_eq!(resample(&s, 44_100, 44_100), s);
        assert!(resample(&[], 44_100, 48_000).is_empty());
    }

    #[test]
    fn going_up_keeps_pitch_and_level() {
        let out = resample(&sine(1000.0, 44_100, 44_100), 44_100, 48_000);
        assert_eq!(out.len(), 48_000);
        assert!(worst_error(&out, 1000.0, 48_000) < 0.002, "{}", worst_error(&out, 1000.0, 48_000));
    }

    #[test]
    fn a_high_tone_survives_going_up() {
        // 15 kHz is where a linear interpolator would lose a third of the level.
        let out = resample(&sine(15_000.0, 44_100, 22_050), 44_100, 48_000);
        assert!(worst_error(&out, 15_000.0, 48_000) < 0.02, "{}", worst_error(&out, 15_000.0, 48_000));
    }

    #[test]
    fn going_down_keeps_pitch_and_level() {
        let out = resample(&sine(1000.0, 48_000, 48_000), 48_000, 44_100);
        assert_eq!(out.len(), 44_100);
        assert!(worst_error(&out, 1000.0, 44_100) < 0.002, "{}", worst_error(&out, 1000.0, 44_100));
    }

    #[test]
    fn going_down_does_not_fold_what_the_new_rate_cannot_hold_back_into_the_sound() {
        // 30 kHz at 96 kHz is above the Nyquist frequency of 44.1 kHz (22.05 kHz).
        let out = resample(&sine(30_000.0, 96_000, 96_000), 96_000, 44_100);
        let peak = out[100..out.len() - 100].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak < 0.02, "{peak}");
    }

    #[test]
    fn a_dc_level_stays_where_it_is() {
        let out = resample(&vec![0.5; 2000], 44_100, 48_000);
        assert!(out[100..out.len() - 100].iter().all(|s| (s - 0.5).abs() < 0.002));
    }
}
