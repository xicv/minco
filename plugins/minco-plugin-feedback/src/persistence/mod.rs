#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;

#[cfg(feature = "postgres")]
pub use postgres::PostgresFeedbackStore;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteFeedbackStore;

use crate::{FeedbackStoreError, FeedbackThread};

const MIGRATION_HISTORY_TABLE: &str = "_minco_feedback_migrations";

fn encode_thread(thread: &FeedbackThread) -> Result<serde_json::Value, FeedbackStoreError> {
    serde_json::to_value(thread)
        .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))
}

fn decode_thread(value: serde_json::Value) -> Result<FeedbackThread, FeedbackStoreError> {
    serde_json::from_value(value)
        .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))
}

fn revision_to_i64(revision: u64) -> Result<i64, FeedbackStoreError> {
    i64::try_from(revision).map_err(|_| {
        FeedbackStoreError::Infrastructure("feedback revision exceeds i64 range".into())
    })
}

fn revision_from_i64(revision: i64) -> Result<u64, FeedbackStoreError> {
    u64::try_from(revision).map_err(|_| {
        FeedbackStoreError::Infrastructure("database returned a negative revision".into())
    })
}
