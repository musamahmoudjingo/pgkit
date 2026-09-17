use std::time::Duration;

use super::backoff_policy::BackoffPolicy;

/// Retry configuration: total attempts, the delay schedule between them,
/// and whether the operation is asserted idempotent.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    tries: u32,
    backoff: BackoffPolicy,
    base_delay: Duration,
    max_delay: Duration,
    idempotent: bool,
    jitter: bool,
}

impl RetryPolicy {
    pub fn new(
        tries: u32,
        backoff: BackoffPolicy,
        base_delay: Duration,
        max_delay: Duration,
    ) -> Self {
        Self {
            tries: tries.max(1),
            backoff,
            base_delay,
            max_delay,
            idempotent: false,
            jitter: false,
        }
    }

    /// Assert the operation is idempotent: safe to re-execute even when a
    /// failed attempt may have applied. Unlocks retrying mid-statement
    /// failures (see [`RetrySafety::IfIdempotent`]).
    ///
    /// [`RetrySafety::IfIdempotent`]: crate::errors::RetrySafety::IfIdempotent
    pub fn idempotent(mut self, idempotent: bool) -> Self {
        self.idempotent = idempotent;
        self
    }

    /// Randomize each delay by a factor in `[0.5, 1.5)`. De-synchronizes
    /// competing transactions that failed together (e.g. after a deadlock)
    /// so they don't retry in lockstep.
    pub fn jitter(mut self, jitter: bool) -> Self {
        self.jitter = jitter;
        self
    }

    /// Total number of attempts (the first execution counts as attempt 1).
    pub fn tries(&self) -> u32 {
        self.tries
    }

    /// Whether the operation was asserted idempotent.
    pub fn is_idempotent(&self) -> bool {
        self.idempotent
    }

    /// Delay to sleep after the given failed attempt (1-based), jittered if
    /// enabled, capped at the policy's max delay. Overflow saturates to the
    /// max delay.
    pub fn delay(&self, attempt: u32) -> Duration {
        let attempt = attempt.max(1);
        let delay = match self.backoff {
            BackoffPolicy::Fixed => Some(self.base_delay),
            BackoffPolicy::Linear => self.base_delay.checked_mul(attempt),
            BackoffPolicy::Exponential => 2u32
                .checked_pow(attempt - 1)
                .and_then(|factor| self.base_delay.checked_mul(factor)),
        };
        let delay = delay.unwrap_or(self.max_delay);
        let delay = if self.jitter { jittered(delay) } else { delay };
        delay.min(self.max_delay)
    }
}

/// Scale `delay` by a random factor in `[0.5, 1.5)`.
///
/// Randomness comes from std's `RandomState` (OS-seeded per instance); not
/// uniform-quality, but ample for de-synchronizing retry timing without
/// pulling in a rand dependency.
fn jittered(delay: Duration) -> Duration {
    use std::hash::{BuildHasher, Hasher, RandomState};

    let r = RandomState::new().build_hasher().finish();
    let factor = 0.5 + (r % 1_000) as f64 / 1_000.0;
    delay.mul_f64(factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn fixed_backoff_is_constant() {
        let p = RetryPolicy::new(5, BackoffPolicy::Fixed, 100 * MS, 10_000 * MS);
        assert_eq!(p.delay(1), 100 * MS);
        assert_eq!(p.delay(4), 100 * MS);
    }

    #[test]
    fn linear_backoff_grows_by_base() {
        let p = RetryPolicy::new(5, BackoffPolicy::Linear, 100 * MS, 10_000 * MS);
        assert_eq!(p.delay(1), 100 * MS);
        assert_eq!(p.delay(2), 200 * MS);
        assert_eq!(p.delay(3), 300 * MS);
    }

    #[test]
    fn exponential_backoff_doubles() {
        let p = RetryPolicy::new(5, BackoffPolicy::Exponential, 100 * MS, 10_000 * MS);
        assert_eq!(p.delay(1), 100 * MS);
        assert_eq!(p.delay(2), 200 * MS);
        assert_eq!(p.delay(3), 400 * MS);
        assert_eq!(p.delay(4), 800 * MS);
    }

    #[test]
    fn delay_is_capped_at_max() {
        let p = RetryPolicy::new(10, BackoffPolicy::Exponential, 100 * MS, 500 * MS);
        assert_eq!(p.delay(3), 400 * MS);
        assert_eq!(p.delay(4), 500 * MS);
        assert_eq!(p.delay(9), 500 * MS);
    }

    #[test]
    fn overflow_saturates_to_max() {
        let p = RetryPolicy::new(100, BackoffPolicy::Exponential, 100 * MS, 500 * MS);
        assert_eq!(p.delay(64), 500 * MS);
    }

    #[test]
    fn zero_tries_clamps_to_one() {
        let p = RetryPolicy::new(0, BackoffPolicy::Fixed, MS, MS);
        assert_eq!(p.tries(), 1);
    }

    #[test]
    fn idempotent_defaults_off_and_chains() {
        let p = RetryPolicy::new(3, BackoffPolicy::Fixed, MS, MS);
        assert!(!p.is_idempotent());
        assert!(p.idempotent(true).is_idempotent());
    }

    #[test]
    fn jittered_delay_stays_in_bounds() {
        let p = RetryPolicy::new(3, BackoffPolicy::Fixed, 1_000 * MS, 10_000 * MS).jitter(true);
        for _ in 0..50 {
            let d = p.delay(1);
            assert!(d >= 500 * MS && d < 1_500 * MS, "out of bounds: {d:?}");
        }
    }

    #[test]
    fn jittered_delay_respects_max_cap() {
        let p = RetryPolicy::new(3, BackoffPolicy::Fixed, 1_000 * MS, 1_100 * MS).jitter(true);
        for _ in 0..50 {
            assert!(p.delay(1) <= 1_100 * MS);
        }
    }
}
