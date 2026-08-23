//! Retry policies with overflow-safe backoff and a deterministic clock port.

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::JobError;
use crate::envelope::MAX_JOB_ATTEMPTS;

/// Hard ceiling for a single backoff delay.
pub const MAX_BACKOFF_DELAY: TimeDelta = TimeDelta::seconds(86_400);

/// How the delay before the next attempt grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackoffMode {
    /// A constant delay for every retry.
    Fixed,
    /// `base << (attempt - 1)`, saturating at `max_delay`.
    Exponential,
}

/// A bounded, serializable retry policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryPolicy {
    pub maximum_attempts: u32,
    pub mode: BackoffMode,
    pub base_delay_seconds: u64,
    pub max_delay_seconds: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            maximum_attempts: 5,
            mode: BackoffMode::Exponential,
            base_delay_seconds: 30,
            max_delay_seconds: 900,
        }
    }
}

impl RetryPolicy {
    #[must_use]
    pub const fn fixed(maximum_attempts: u32, delay_seconds: u64) -> Self {
        Self {
            maximum_attempts,
            mode: BackoffMode::Fixed,
            base_delay_seconds: delay_seconds,
            max_delay_seconds: delay_seconds,
        }
    }

    #[must_use]
    pub const fn exponential(maximum_attempts: u32, base_seconds: u64, max_seconds: u64) -> Self {
        Self {
            maximum_attempts,
            mode: BackoffMode::Exponential,
            base_delay_seconds: base_seconds,
            max_delay_seconds: max_seconds,
        }
    }

    /// Validate counts and delays. Delays are bounded to one day so retry
    /// state cannot pin a job indefinitely.
    pub fn validate(&self) -> Result<(), JobError> {
        if self.maximum_attempts == 0 || self.maximum_attempts > MAX_JOB_ATTEMPTS {
            return Err(JobError::InvalidJob(format!(
                "maximum attempts must be between 1 and {MAX_JOB_ATTEMPTS}"
            )));
        }
        let ceiling = u64::try_from(MAX_BACKOFF_DELAY.num_seconds()).unwrap_or(u64::MAX);
        if self.base_delay_seconds == 0 || self.base_delay_seconds > ceiling {
            return Err(JobError::InvalidJob(format!(
                "base delay must be between 1 and {ceiling} seconds"
            )));
        }
        if self.max_delay_seconds < self.base_delay_seconds || self.max_delay_seconds > ceiling {
            return Err(JobError::InvalidJob(
                "max delay must be at least the base delay and at most one day".into(),
            ));
        }
        Ok(())
    }

    /// The delay after the given attempt number (1-based). Arithmetic is
    /// saturating: exponential growth can never overflow into panic.
    #[must_use]
    pub fn delay_for_attempt(&self, attempt: u32) -> TimeDelta {
        const CEILING_SECONDS: u64 = 86_400;
        let shift = attempt.saturating_sub(1).min(31);
        let seconds = match self.mode {
            BackoffMode::Fixed => self.base_delay_seconds,
            BackoffMode::Exponential => self
                .base_delay_seconds
                .saturating_mul(1_u64 << shift)
                .min(self.max_delay_seconds),
        };
        TimeDelta::seconds(i64::try_from(seconds.min(CEILING_SECONDS)).unwrap_or(86_400))
    }
}

/// Readable clock port so retry arithmetic and leases are testable.
pub trait JobClock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> DateTime<Utc>;
}

/// The real system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemJobClock;

impl JobClock for SystemJobClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A deterministic clock for tests, advancing only when told to.
#[derive(Debug, Default)]
pub struct FakeJobClock {
    now: std::sync::RwLock<DateTime<Utc>>,
}

impl FakeJobClock {
    #[must_use]
    pub const fn starting(now: DateTime<Utc>) -> Self {
        Self {
            now: std::sync::RwLock::new(now),
        }
    }

    pub fn advance(&self, delta: TimeDelta) {
        let mut guard = self.now.write().expect("fake clock lock");
        *guard += delta;
    }

    pub fn set(&self, now: DateTime<Utc>) {
        let mut guard = self.now.write().expect("fake clock lock");
        *guard = now;
    }
}

impl JobClock for FakeJobClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.read().expect("fake clock lock")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy_arithmetic_is_overflow_safe_and_bounded() {
        let policy = RetryPolicy::exponential(u32::MAX, 30, 900);
        assert_eq!(policy.delay_for_attempt(1), TimeDelta::seconds(30));
        assert_eq!(policy.delay_for_attempt(2), TimeDelta::seconds(60));
        assert_eq!(policy.delay_for_attempt(6), TimeDelta::seconds(900));
        assert_eq!(
            policy.delay_for_attempt(u32::MAX),
            TimeDelta::seconds(900),
            "extreme attempts saturate at max delay"
        );
        let fixed = RetryPolicy::fixed(3, 45);
        assert_eq!(fixed.delay_for_attempt(1), TimeDelta::seconds(45));
        assert_eq!(fixed.delay_for_attempt(9), TimeDelta::seconds(45));
    }

    #[test]
    fn invalid_retry_policies_are_rejected() {
        assert!(RetryPolicy::fixed(0, 5).validate().is_err());
        assert!(
            RetryPolicy::fixed(MAX_JOB_ATTEMPTS + 1, 5)
                .validate()
                .is_err()
        );
        assert!(RetryPolicy::fixed(3, 0).validate().is_err());
        assert!(RetryPolicy::exponential(3, 60, 30).validate().is_err());
        assert!(RetryPolicy::exponential(3, 30, 86_401).validate().is_err());
    }

    #[test]
    fn fake_clock_advances_deterministically() {
        let start = Utc::now();
        let clock = FakeJobClock::starting(start);
        assert_eq!(clock.now(), start);
        clock.advance(TimeDelta::seconds(900));
        assert_eq!(clock.now(), start + TimeDelta::seconds(900));
    }
}
