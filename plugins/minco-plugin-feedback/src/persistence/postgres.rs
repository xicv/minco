use super::{
    MIGRATION_HISTORY_TABLE, decode_thread, encode_thread, revision_from_i64, revision_to_i64,
};
use crate::{
    FeedbackId, FeedbackListFilter, FeedbackStore, FeedbackStoreError, FeedbackSummary,
    FeedbackThread,
};
use async_trait::async_trait;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct PostgresFeedbackStore {
    pool: PgPool,
}

impl PostgresFeedbackStore {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<(), FeedbackStoreError> {
        let mut migrator = sqlx::migrate!("migrations/postgres");
        migrator.dangerous_set_table_name(MIGRATION_HISTORY_TABLE);
        migrator
            .run(&self.pool)
            .await
            .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))
    }
}

#[async_trait]
impl FeedbackStore for PostgresFeedbackStore {
    async fn create(
        &self,
        thread: FeedbackThread,
        client_token_hash: String,
    ) -> Result<(), FeedbackStoreError> {
        let document = encode_thread(&thread)?;
        let result = sqlx::query(
            r"
            INSERT INTO minco_feedback_threads (
                id, client_token_hash, project_id, status, document,
                revision, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ",
        )
        .bind(thread.id.0)
        .bind(client_token_hash)
        .bind(&thread.project_id)
        .bind(thread.status.to_string())
        .bind(document)
        .bind(revision_to_i64(thread.revision)?)
        .bind(thread.created_at)
        .bind(thread.updated_at)
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
        let row = sqlx::query("SELECT document FROM minco_feedback_threads WHERE id = $1")
            .bind(id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;
        row.map(|row| row.try_get::<serde_json::Value, _>("document"))
            .transpose()
            .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?
            .map(decode_thread)
            .transpose()
    }

    async fn get_for_client(
        &self,
        id: FeedbackId,
        client_token_hash: &str,
    ) -> Result<Option<FeedbackThread>, FeedbackStoreError> {
        let row = sqlx::query(
            "SELECT document FROM minco_feedback_threads WHERE id = $1 AND client_token_hash = $2",
        )
        .bind(id.0)
        .bind(client_token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;
        row.map(|row| row.try_get::<serde_json::Value, _>("document"))
            .transpose()
            .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?
            .map(decode_thread)
            .transpose()
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
            WHERE ($1::text IS NULL OR status = $1)
              AND ($2::text IS NULL OR project_id = $2)
            ORDER BY updated_at DESC, id DESC
            LIMIT $3
            ",
        )
        .bind(status)
        .bind(filter.project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;

        rows.into_iter()
            .map(|row| {
                let value = row
                    .try_get::<serde_json::Value, _>("document")
                    .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;
                decode_thread(value).map(|thread| FeedbackSummary::from(&thread))
            })
            .collect()
    }

    async fn ready(&self) -> Result<(), FeedbackStoreError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
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
        let result = sqlx::query(
            r"
            UPDATE minco_feedback_threads
            SET project_id = $2,
                status = $3,
                document = $4,
                revision = $5,
                updated_at = $6
            WHERE id = $1 AND revision = $7
            ",
        )
        .bind(thread.id.0)
        .bind(&thread.project_id)
        .bind(thread.status.to_string())
        .bind(encode_thread(&thread)?)
        .bind(revision_to_i64(thread.revision)?)
        .bind(thread.updated_at)
        .bind(revision_to_i64(expected_revision)?)
        .execute(&self.pool)
        .await
        .map_err(|error| FeedbackStoreError::Infrastructure(error.to_string()))?;

        if result.rows_affected() == 1 {
            return Ok(());
        }
        let actual = sqlx::query("SELECT revision FROM minco_feedback_threads WHERE id = $1")
            .bind(thread.id.0)
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
