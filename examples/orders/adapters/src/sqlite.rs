use async_trait::async_trait;
use minco_sqlx_sqlite::{SqliteError, SqlitePoolConfig};
use orders_application::{OrderStore, PlaceOrderResult, PlaceOrderTransaction, StoreError};
use orders_domain::{CustomerReference, Order, OrderId, OrderLine, OrderStatus};
use sqlx::{Row, SqlitePool};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteOrderStore {
    pool: SqlitePool,
}

impl SqliteOrderStore {
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn connect(config: &SqlitePoolConfig) -> Result<Self, SqliteError> {
        Ok(Self::new(minco_sqlx_sqlite::connect(config).await?))
    }

    pub async fn migrate(&self, path: impl AsRef<Path>) -> Result<(), SqliteError> {
        minco_sqlx_sqlite::migrate_with_history_table(&self.pool, path, "_minco_orders_migrations")
            .await
    }

    #[must_use]
    pub const fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn fetch_order(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        let row = sqlx::query(
            "SELECT id, customer_reference, lines, status, created_at FROM orders WHERE id = ?1",
        )
        .bind(id.into_uuid().to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| database_error(&error))?;
        row.as_ref().map(decode_row).transpose()
    }

    async fn replay(&self, key: &str, fingerprint: &str) -> Result<PlaceOrderResult, StoreError> {
        let record = sqlx::query(
            "SELECT request_fingerprint, order_id FROM order_idempotency WHERE idempotency_key = ?1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| database_error(&error))?
        .ok_or_else(|| StoreError::Internal("conflicting idempotency row disappeared".into()))?;
        let stored_fingerprint: String = record
            .try_get("request_fingerprint")
            .map_err(|error| database_error(&error))?;
        if stored_fingerprint != fingerprint {
            return Err(StoreError::IdempotencyConflict);
        }
        let id: String = record
            .try_get("order_id")
            .map_err(|error| database_error(&error))?;
        let id = Uuid::parse_str(&id)
            .map_err(|error| StoreError::Internal(format!("invalid stored order ID: {error}")))?;
        let order = self
            .fetch_order(OrderId::from_uuid(id))
            .await?
            .ok_or_else(|| {
                StoreError::Internal("idempotency record references a missing order".into())
            })?;
        Ok(PlaceOrderResult {
            order,
            replayed: true,
        })
    }
}

#[async_trait]
impl OrderStore for SqliteOrderStore {
    async fn place_order(
        &self,
        transaction: PlaceOrderTransaction,
    ) -> Result<PlaceOrderResult, StoreError> {
        let mut db = self
            .pool
            .begin()
            .await
            .map_err(|error| database_error(&error))?;
        let lines = serde_json::to_string(&transaction.order.lines)
            .map_err(|error| StoreError::Internal(format!("encode order lines: {error}")))?;
        sqlx::query(
            "INSERT INTO orders (id, customer_reference, lines, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(transaction.order.id.into_uuid().to_string())
        .bind(transaction.order.customer_reference.as_str())
        .bind(lines)
        .bind("accepted")
        .bind(transaction.order.created_at.to_rfc3339())
        .execute(&mut *db)
        .await
        .map_err(|error| database_error(&error))?;
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO order_idempotency (idempotency_key, request_fingerprint, order_id) VALUES (?1, ?2, ?3)",
        )
        .bind(&transaction.idempotency_key)
        .bind(&transaction.request_fingerprint)
        .bind(transaction.order.id.into_uuid().to_string())
        .execute(&mut *db)
        .await
        .map_err(|error| database_error(&error))?
        .rows_affected();
        if inserted == 1 {
            db.commit().await.map_err(|error| database_error(&error))?;
            return Ok(PlaceOrderResult {
                order: transaction.order,
                replayed: false,
            });
        }
        db.rollback()
            .await
            .map_err(|error| database_error(&error))?;
        self.replay(
            &transaction.idempotency_key,
            &transaction.request_fingerprint,
        )
        .await
    }

    async fn get_order(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        self.fetch_order(id).await
    }

    async fn ready(&self) -> bool {
        minco_sqlx_sqlite::ready(&self.pool).await
    }
}

fn decode_row(row: &sqlx::sqlite::SqliteRow) -> Result<Order, StoreError> {
    let id: String = row.try_get("id").map_err(|error| database_error(&error))?;
    let id = Uuid::parse_str(&id)
        .map_err(|error| StoreError::Internal(format!("invalid stored order ID: {error}")))?;
    let customer_reference: String = row
        .try_get("customer_reference")
        .map_err(|error| database_error(&error))?;
    let lines: String = row
        .try_get("lines")
        .map_err(|error| database_error(&error))?;
    let status: String = row
        .try_get("status")
        .map_err(|error| database_error(&error))?;
    let created_at: String = row
        .try_get("created_at")
        .map_err(|error| database_error(&error))?;
    if status != "accepted" {
        return Err(StoreError::Internal(format!(
            "unsupported persisted order status {status}"
        )));
    }
    let lines: Vec<OrderLine> = serde_json::from_str(&lines)
        .map_err(|error| StoreError::Internal(format!("decode order lines: {error}")))?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at)
        .map_err(|error| StoreError::Internal(format!("invalid persisted timestamp: {error}")))?
        .with_timezone(&chrono::Utc);
    Ok(Order {
        id: OrderId::from_uuid(id),
        customer_reference: CustomerReference::parse(customer_reference).map_err(|error| {
            StoreError::Internal(format!("invalid persisted customer reference: {error}"))
        })?,
        lines,
        status: OrderStatus::Accepted,
        created_at,
    })
}

fn database_error(error: &sqlx::Error) -> StoreError {
    StoreError::Unavailable(error.to_string())
}
