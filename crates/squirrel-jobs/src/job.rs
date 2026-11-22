//! What handlers see and return, and how jobs are described when enqueued.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;

/// Job priorities: higher runs first.
pub mod priority {
    /// Processing a file the user just uploaded.
    pub const INTERACTIVE: i16 = 100;
    pub const THUMBNAILS: i16 = 80;
    pub const METADATA: i16 = 70;
    pub const TEXT: i16 = 40;
    pub const GEOCODE: i16 = 30;
    pub const BACKFILL: i16 = 0;
    /// Grouping a user's near-identical photos (SQL only, debounced): after
    /// every media step, before machine learning, which needs no models.
    pub const DUPLICATES: i16 = 10;
    /// Machine learning runs after every other step, fresh or backfilled.
    pub const CLIP: i16 = -25;
    /// Computing the label bank (once per labels version): after fresh CLIP
    /// embeddings, before faces; its re-tags follow at `background(CLIP)`.
    pub const LABEL_BANK: i16 = -26;
    pub const FACES: i16 = -30;
    pub const CLUSTER: i16 = -35;
    /// Subtracted from work that comes from backfill rather than a fresh upload.
    pub const BACKGROUND_PENALTY: i16 = 50;

    pub const fn background(priority: i16) -> i16 {
        priority - BACKGROUND_PENALTY
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewJob {
    pub kind: &'static str,
    pub payload: Value,
    pub priority: i16,
    pub dedupe_key: Option<String>,
    /// Not before (default: now).
    pub delay: Option<Duration>,
}

impl NewJob {
    pub fn new(kind: &'static str, payload: Value) -> Self {
        Self {
            kind,
            payload,
            priority: 0,
            dedupe_key: None,
            delay: None,
        }
    }

    pub fn priority(self, priority: i16) -> Self {
        Self { priority, ..self }
    }

    pub fn dedupe(self, key: impl Into<String>) -> Self {
        Self {
            dedupe_key: Some(key.into()),
            ..self
        }
    }

    pub fn delay(self, delay: Duration) -> Self {
        Self {
            delay: Some(delay),
            ..self
        }
    }
}

/// How a handler run ended, when it did not succeed.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    /// Transient: retried with backoff, `failed` after the last attempt.
    #[error("{0:#}")]
    Retry(anyhow::Error),
    /// Retrying cannot help (bad payload, bug): `failed` at once.
    #[error("{0:#}")]
    Permanent(anyhow::Error),
    /// A dependency is not ready (e.g. no geodata): run again after the delay
    /// without using an attempt.
    #[error("not ready yet, rescheduled in {0:?}")]
    Reschedule(Duration),
}

impl JobError {
    pub fn retry(err: impl Into<anyhow::Error>) -> Self {
        Self::Retry(err.into())
    }

    pub fn permanent(err: impl Into<anyhow::Error>) -> Self {
        Self::Permanent(err.into())
    }
}

/// One claimed job, as handed to its handler.
#[derive(Debug, Clone)]
pub struct JobContext {
    pub id: i64,
    pub kind: String,
    pub payload: Value,
    /// As enqueued: tells a handler whether it serves a fresh upload or backfill.
    pub priority: i16,
    /// 1-based number of this attempt.
    pub attempt: i32,
    pub max_attempts: i32,
}

impl JobContext {
    pub fn payload<T: DeserializeOwned>(&self) -> Result<T, JobError> {
        T::deserialize(&self.payload).map_err(JobError::permanent)
    }

    /// The next `Retry` would mark the job failed: write terminal states now.
    pub fn is_last_attempt(&self) -> bool {
        self.attempt >= self.max_attempts
    }

    #[doc(hidden)]
    pub fn for_tests(kind: &str, payload: Value) -> Self {
        Self {
            id: 0,
            kind: kind.to_owned(),
            payload,
            priority: 0,
            attempt: 1,
            max_attempts: 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builder_sets_priority_dedupe_and_payload() {
        let job = NewJob::new("thumbnails", json!({"sha256": "ab"}))
            .priority(priority::THUMBNAILS)
            .dedupe("thumbnails:ab");
        assert_eq!(job.kind, "thumbnails");
        assert_eq!(job.priority, 80);
        assert_eq!(job.dedupe_key.as_deref(), Some("thumbnails:ab"));
        assert_eq!(job.payload, json!({"sha256": "ab"}));
    }

    #[test]
    fn background_work_runs_after_interactive_work() {
        use priority::*;
        for fresh in [INTERACTIVE, THUMBNAILS, METADATA, TEXT, GEOCODE] {
            assert!(background(fresh) < fresh, "{fresh}");
        }
        assert!(background(THUMBNAILS) <= GEOCODE);
        assert!(background(INTERACTIVE) > BACKFILL);
    }

    #[test]
    fn machine_learning_runs_after_every_media_step() {
        use priority::*;
        let lowest_media = background(GEOCODE);
        for media in [INTERACTIVE, THUMBNAILS, METADATA, TEXT, GEOCODE] {
            assert!(background(media) >= lowest_media, "{media}");
        }
        // fresh media > backfilled media > fresh CLIP > fresh faces > cluster > backfilled ML
        assert!(lowest_media > CLIP);
        const {
            assert!(CLIP > LABEL_BANK && LABEL_BANK > FACES && FACES > CLUSTER);
            assert!(CLUSTER > background(CLIP));
            assert!(background(CLIP) > background(FACES));
        }
    }

    #[test]
    fn payloads_decode_into_their_type_or_fail_permanently() {
        #[derive(serde::Deserialize)]
        struct P {
            sha256: String,
        }
        let ctx = JobContext::for_tests("thumbnails", json!({"sha256": "ab"}));
        assert_eq!(ctx.payload::<P>().unwrap().sha256, "ab");
        let bad = JobContext::for_tests("thumbnails", json!({"nope": 1}));
        assert!(matches!(bad.payload::<P>(), Err(JobError::Permanent(_))));
    }

    #[test]
    fn the_last_attempt_is_known() {
        let mut ctx = JobContext::for_tests("x", json!({}));
        assert!(!ctx.is_last_attempt());
        ctx.attempt = ctx.max_attempts;
        assert!(ctx.is_last_attempt());
    }
}
