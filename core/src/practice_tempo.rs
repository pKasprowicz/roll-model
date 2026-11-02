//! How fast the user practises: one value with its unit, so that "80" cannot mean per cent in
//! one place and beats per minute in another.
//!
//! A score is played at a *percentage* of the tempo it is written in. A rudiment has no tempo of
//! its own worth keeping the percentage of, so it is practised at a *tempo in BPM*; the score the
//! program builds for it is written at some tempo, and the factor to play it at the chosen BPM is
//! worked out from that tempo, not assumed.

/// The range of the percentage, for a score.
pub const PERCENT_RANGE: (f32, f32) = (25.0, 150.0);
/// The range of the tempo in BPM, for a rudiment.
pub const BPM_RANGE: (f32, f32) = (30.0, 240.0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PracticeTempo {
    /// Per cent of the tempo the score is written in.
    Percent(f32),
    /// Quarter notes per minute.
    Bpm(f32),
}

impl Default for PracticeTempo {
    fn default() -> Self {
        PracticeTempo::Percent(100.0)
    }
}

impl PracticeTempo {
    /// The number the user sees and sets, in the unit of `unit()`.
    pub fn value(self) -> f32 {
        match self {
            PracticeTempo::Percent(v) | PracticeTempo::Bpm(v) => v,
        }
    }

    /// The same unit with another value, kept inside the range.
    pub fn with_value(self, value: f32) -> PracticeTempo {
        let (lo, hi) = self.range();
        let value = value.clamp(lo, hi);
        match self {
            PracticeTempo::Percent(_) => PracticeTempo::Percent(value),
            PracticeTempo::Bpm(_) => PracticeTempo::Bpm(value),
        }
    }

    pub fn range(self) -> (f32, f32) {
        match self {
            PracticeTempo::Percent(_) => PERCENT_RANGE,
            PracticeTempo::Bpm(_) => BPM_RANGE,
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            PracticeTempo::Percent(_) => "%",
            PracticeTempo::Bpm(_) => "BPM",
        }
    }

    /// "80 %" or "90 BPM".
    pub fn text(self) -> String {
        format!("{:.0} {}", self.value(), self.unit())
    }

    /// The factor to play a score written at `written_bpm` at this tempo.
    pub fn scale(self, written_bpm: f64) -> f64 {
        match self {
            PracticeTempo::Percent(p) => f64::from(p) / 100.0,
            PracticeTempo::Bpm(b) => f64::from(b) / written_bpm.max(1.0),
        }
    }

    /// The tempo in BPM it makes of a score written at `written_bpm`.
    pub fn bpm(self, written_bpm: f64) -> f64 {
        written_bpm * self.scale(written_bpm)
    }

    /// This tempo's unit holding `bpm`, for a score written at `written_bpm` (tap tempo).
    pub fn from_bpm(self, bpm: f64, written_bpm: f64) -> PracticeTempo {
        let value = match self {
            PracticeTempo::Percent(_) => bpm / written_bpm.max(1.0) * 100.0,
            PracticeTempo::Bpm(_) => bpm,
        };
        self.with_value(value.round() as f32)
    }

    /// `steps` units faster (or slower, if negative), from the rounded value.
    pub fn stepped(self, steps: i32) -> PracticeTempo {
        self.with_value(self.value().round() + steps as f32)
    }

    /// Where the value lies in its range, from 0 to 1 (for a fader).
    pub fn position(self) -> f32 {
        let (lo, hi) = self.range();
        (self.value() - lo) / (hi - lo)
    }

    /// The tempo at `x` (0 to 1) of the range, rounded (a fader moved).
    pub fn at_position(self, x: f32) -> PracticeTempo {
        let (lo, hi) = self.range();
        self.with_value((lo + x * (hi - lo)).round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_percentage_scales_by_itself_and_a_bpm_by_the_written_tempo() {
        assert_eq!(PracticeTempo::Percent(80.0).scale(120.0), 0.8);
        assert_eq!(PracticeTempo::Percent(80.0).scale(60.0), 0.8, "the written tempo does not matter");
        assert_eq!(PracticeTempo::Bpm(90.0).scale(100.0), 0.9);
        // Not 0.9 because the rudiment happens to be written at 100: at 120 the factor is 0.75.
        assert_eq!(PracticeTempo::Bpm(90.0).scale(120.0), 0.75);
        assert_eq!(PracticeTempo::Bpm(90.0).bpm(120.0), 90.0);
        assert_eq!(PracticeTempo::Percent(50.0).bpm(120.0), 60.0);
    }

    #[test]
    fn values_stay_inside_the_range_of_their_unit() {
        assert_eq!(PracticeTempo::Percent(100.0).with_value(500.0).value(), PERCENT_RANGE.1);
        assert_eq!(PracticeTempo::Bpm(100.0).with_value(5.0).value(), BPM_RANGE.0);
        assert_eq!(PracticeTempo::Bpm(100.0).with_value(200.0), PracticeTempo::Bpm(200.0), "200 is a fine BPM and not a percentage");
        assert_eq!(PracticeTempo::Percent(100.0).with_value(200.0), PracticeTempo::Percent(150.0));
    }

    #[test]
    fn steps_and_faders_keep_the_unit() {
        assert_eq!(PracticeTempo::Percent(80.4).stepped(5), PracticeTempo::Percent(85.0));
        assert_eq!(PracticeTempo::Bpm(238.0).stepped(5), PracticeTempo::Bpm(240.0));
        assert_eq!(PracticeTempo::Percent(25.0).stepped(-5), PracticeTempo::Percent(25.0));
        for t in [PracticeTempo::Percent(0.0), PracticeTempo::Bpm(0.0)] {
            assert_eq!(t.at_position(0.0).position(), 0.0);
            assert_eq!(t.at_position(1.0).position(), 1.0);
            assert!(matches!(
                (t, t.at_position(0.5)),
                (PracticeTempo::Percent(_), PracticeTempo::Percent(_)) | (PracticeTempo::Bpm(_), PracticeTempo::Bpm(_))
            ));
        }
    }

    #[test]
    fn a_tapped_tempo_is_put_in_the_unit_of_the_practice() {
        assert_eq!(PracticeTempo::Percent(100.0).from_bpm(90.0, 120.0), PracticeTempo::Percent(75.0));
        assert_eq!(PracticeTempo::Bpm(100.0).from_bpm(90.0, 120.0), PracticeTempo::Bpm(90.0));
        assert_eq!(PracticeTempo::Percent(100.0).from_bpm(400.0, 120.0), PracticeTempo::Percent(150.0), "kept in the range");
    }

    #[test]
    fn the_text_has_the_unit() {
        assert_eq!(PracticeTempo::Percent(80.0).text(), "80 %");
        assert_eq!(PracticeTempo::Bpm(90.0).text(), "90 BPM");
    }
}
