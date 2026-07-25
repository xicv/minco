use super::{
    MIGRATION_HISTORY_TABLE, decode_thread, encode_thread, revision_from_i64, revision_to_i64,
};
use crate::{
    FeedbackId, FeedbackListFilter, FeedbackStore, FeedbackStoreError, FeedbackSummary,
    FeedbackThread,
};
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone)]
pub struct SqliteFeedbackStore {
    pool: SqlitePool,
}

impl SqliteFeedbackStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub const fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<(), FeedbackStoreError> {
        let mut migrator = sqlx::migrate!("migrations/sqlite");
        migrator.dangerous_set_table_name(MIGRATION_HISTORY_TABLE);
        migrator
            .run(&self.pool)
            .await
            .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))
    }
}

#[async_trait]
impl FeedbackStore for SqliteFeedbackStore {
    async fn create(
        &self,
        thread: FeedbackThread,
        client_token_hash: String,
    ) -> Result<(), FeedbackStoreError> {
        let document = serde_json::to_string(&encode_thread(&thread)?)
            .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;
        let result = sqlx::query(
            r"
            INSERT INTO minco_feedback_threads (
                id, client_token_hash, project_id, status, document,
                revision, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
        )
        .bind(thread.id.to_string())
        .bind(client_token_hash)
        .bind(&thread.project_id)
        .bind(thread.status.to_string())
        .bind(document)
        .bind(revision_to_i64(thread.revision)?)
        .bind(thread.created_at.to_rfc3339())
        .bind(thread.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(error)
                if error
                    .as_database_error()
                    .is_some_and(sqlx::error::DatabaseError::is_unique_violation) =>
            {
                Err(FeedbackStoreError::AlreadyExists(thread.id))
            }
            Err(error) => Err(FeedbackStoreError::Infrastructure(error.to_string())),
        }
    }

    async fn get(&self, id: FeedbackId) -> Result<Option<FeedbackThread>, FeedbackStoreError> {
        let row = sqlx::query("SELECT document FROM minco_feedback_threads WHERE id = ?1")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;
        row.map(|row| decode_row(&row)).transpose()
    }

    async fn get_for_client(
        &self,
        id: FeedbackId,
        client_token_hash: &str,
    ) -> Result<Option<FeedbackThread>, FeedbackStoreError> {
        let row = sqlx::query(
            "SELECT document FROM minco_feedback_threads WHERE id = ?1 AND client_token_hash = ?2",
        )
        .bind(id.to_string())
        .bind(client_token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;
        row.map(|row| decode_row(&row)).transpose()
    }

    async fn list(
        &self,
        filter: FeedbackListFilter,
    ) -> Result<Vec<FeedbackSummary>, FeedbackStoreError> {
        let status = filter.status.map(|value| value.to_string());
        let limit = i64::try_from(filter.limit.clamp(1, 200))
            .map_err(|_| FeedbackStoreError::Infrastructure("invalid list limit".into()))?;
        let rows = sqlx::query(
            r"
            SELECT document
            FROM minco_feedback_threads
            WHERE (?1 IS NULL OR status = ?1)
              AND (?2 IS NULL OR project_id = ?2)
            ORDER BY updated_at DESC, id DESC
            LIMIT ?3
            ",
        )
        .bind(status)
        .bind(filter.project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;

        rows.into_iter()
            .map(|row| decode_row(&row).map(|thread| FeedbackSummary::from(&thread)))
            .collect()
    }

    async fn ready(&self) -> Result<(), FeedbackStoreError> {
        sqlx::query_scalar::<_, i64>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map(|_| ())
            .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))
    }

    async fn save(
        &self,
        thread: FeedbackThread,
        expected_revision: u64,
    ) -> Result<(), FeedbackStoreError> {
        let document = serde_json::to_string(&encode_thread(&thread)?)
            .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;
        let result = sqlx::query(
            r"
            UPDATE minco_feedback_threads
            SET project_id = ?2,
                status = ?3,
                document = ?4,
                revision = ?5,
                updated_at = ?6
            WHERE id = ?1 AND revision = ?7
            ",
        )
        .bind(thread.id.to_string())
        .bind(&thread.project_id)
        .bind(thread.status.to_string())
        .bind(document)
        .bind(revision_to_i64(thread.revision)?)
        .bind(thread.updated_at.to_rfc3339())
        .bind(revision_to_i64(expected_revision)?)
        .execute(&self.pool)
        .await
        .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;

        if result.rows_affected() == 1 {
            return Ok(());
        }
        let actual = sqlx::query("SELECT revision FROM minco_feedback_threads WHERE id = ?1")
            .bind(thread.id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;
        match actual {
            None => Err(FeedbackStoreError::NotFound(thread.id)),
            Some(row) => Err(FeedbackStoreError::ConcurrentModification {
                id: thread.id,
                expected_revision,
                actual_revision: revision_from_i64(
                    row.try_get::<i64, _>("revision")
                        .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?,
                )?,
            }),
        }
    }
}

fn decode_row(row: &sqlx::sqlite::SqliteRow) -> Result<FeedbackThread, FeedbackStoreError> {
    let document = row
        .try_get::<String, _>("document")
        .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;
    let value = serde_json::from_str(&document)
        .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;
    decode_thread(value)
}
