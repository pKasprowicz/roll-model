//! Units of time and music as types, so that quarter notes, seconds and milliseconds cannot be
//! added or compared by mistake. A [`Quarters`] position becomes [`Seconds`] only through the
//! [`TempoMap`](crate::score::TempoMap); [`Seconds`] and [`Millis`] convert into each other.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

macro_rules! quantity {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
        pub struct $name(pub f64);

        impl $name {
            pub const ZERO: $name = $name(0.0);
            pub fn abs(self) -> $name {
                $name(self.0.abs())
            }
            pub fn min(self, other: $name) -> $name {
                $name(self.0.min(other.0))
            }
            pub fn max(self, other: $name) -> $name {
                $name(self.0.max(other.0))
            }
            pub fn clamp(self, lo: $name, hi: $name) -> $name {
                $name(self.0.clamp(lo.0, hi.0))
            }
            pub fn total_cmp(&self, other: &$name) -> std::cmp::Ordering {
                self.0.total_cmp(&other.0)
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
        impl Add for $name {
            type Output = $name;
            fn add(self, o: $name) -> $name {
                $name(self.0 + o.0)
            }
        }
        impl Sub for $name {
            type Output = $name;
            fn sub(self, o: $name) -> $name {
                $name(self.0 - o.0)
            }
        }
        impl AddAssign for $name {
            fn add_assign(&mut self, o: $name) {
                self.0 += o.0;
            }
        }
        impl SubAssign for $name {
            fn sub_assign(&mut self, o: $name) {
                self.0 -= o.0;
            }
        }
        impl Neg for $name {
            type Output = $name;
            fn neg(self) -> $name {
                $name(-self.0)
            }
        }
        impl Mul<f64> for $name {
            type Output = $name;
            fn mul(self, k: f64) -> $name {
                $name(self.0 * k)
            }
        }
        impl Div<f64> for $name {
            type Output = $name;
            fn div(self, k: f64) -> $name {
                $name(self.0 / k)
            }
        }
        /// How many times one is the other.
        impl Div for $name {
            type Output = f64;
            fn div(self, o: $name) -> f64 {
                self.0 / o.0
            }
        }
    };
}

quantity!(
    /// A position or a length in quarter notes.
    Quarters
);
quantity!(
    /// Time in seconds.
    Seconds
);
quantity!(
    /// Time in milliseconds.
    Millis
);

impl Seconds {
    pub const INFINITY: Seconds = Seconds(f64::INFINITY);
    pub const NEG_INFINITY: Seconds = Seconds(f64::NEG_INFINITY);
    pub fn millis(self) -> Millis {
        Millis(self.0 * 1000.0)
    }
}

impl Millis {
    pub fn secs(self) -> Seconds {
        Seconds(self.0 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_and_milliseconds_convert_and_compute_in_their_own_unit() {
        assert_eq!(Seconds(0.25).millis(), Millis(250.0));
        assert_eq!(Millis(40.0).secs(), Seconds(0.04));
        assert_eq!(Seconds(1.0) - Seconds(0.75), Seconds(0.25));
        assert!(((Seconds(0.1) - Seconds(0.3)).abs() - Seconds(0.2)).abs() < Seconds(1e-12));
        assert_eq!(Seconds(1.0) / Seconds(4.0), 0.25);
        assert_eq!(-Millis(3.0) * 2.0, Millis(-6.0));
        assert!(Quarters(1.0) < Quarters(1.5));
        assert_eq!(Seconds(0.3).clamp(Seconds(0.0), Seconds(0.1)), Seconds(0.1));
    }
}
