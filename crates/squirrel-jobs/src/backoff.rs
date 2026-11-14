//! Retry delays: 30 s × 4^(attempt − 1), capped at one hour, times a jitter factor.

use std::time::Duration;

const BASE_SECS: f64 = 30.0;
const FACTOR: f64 = 4.0;
const MAX_SECS: f64 = 3600.0;
/// Past this exponent the delay is capped anyway; keeps `powi` small.
const MAX_EXPONENT: i32 = 16;

/// Delay before retrying after failed attempt number `attempt` (1-based).
/// `jitter` is a factor, normally drawn from 0.8..=1.2 (see [`jitter`]).
pub fn retry_delay(attempt: i32, jitter: f64) -> Duration {
    let exponent = attempt.clamp(1, MAX_EXPONENT) - 1;
    let secs = (BASE_SECS * FACTOR.powi(exponent)).min(MAX_SECS) * jitter;
    Duration::from_secs_f64(secs.round().max(1.0))
}

/// A random factor in 0.8..=1.2, so retries of a burst of failures spread out.
pub fn jitter() -> f64 {
    rand::Rng::gen_range(&mut rand::thread_rng(), 0.8..=1.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_grow_fourfold_from_thirty_seconds_and_cap_at_an_hour() {
        let secs = |n| retry_delay(n, 1.0).as_secs();
        assert_eq!([secs(1), secs(2), secs(3), secs(4)], [30, 120, 480, 1920]);
        assert_eq!(secs(5), 3600);
        assert_eq!(secs(40), 3600, "no overflow");
        assert_eq!(secs(0), 30, "attempt 0 is treated as 1");
    }

    #[test]
    fn jitter_scales_the_delay() {
        assert_eq!(retry_delay(1, 0.8).as_secs(), 24);
        assert_eq!(retry_delay(1, 1.2).as_secs(), 36);
    }

    #[test]
    fn jitter_stays_within_twenty_percent() {
        for _ in 0..100 {
            assert!((0.8..=1.2).contains(&jitter()));
        }
    }
}
