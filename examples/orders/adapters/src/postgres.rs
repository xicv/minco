use async_trait::async_trait;
use minco_sqlx_postgres::{PostgresError, PostgresPoolConfig};
use orders_application::{OrderStore, PlaceOrderResult, PlaceOrderTransaction, StoreError};
use orders_domain::{CustomerReference, Order, OrderId, OrderLine, OrderStatus};
use sqlx::{PgPool, Row};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PostgresOrderStore {
    pool: PgPool,
}

impl PostgresOrderStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(config: &PostgresPoolConfig) -> Result<Self, PostgresError> {
        Ok(Self::new(minco_sqlx_postgres::connect(config).await?))
    }

    pub async fn migrate(&self, path: impl AsRef<Path>) -> Result<(), PostgresError> {
        minco_sqlx_postgres::migrate(&self.pool, path).await
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    async fn fetch_order(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        let row = sqlx::query(
            "SELECT id, customer_reference, lines, status, created_at FROM orders WHERE id = $1",
        )
        .bind(id.into_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;
        row.map(decode_row).transpose()
    }

    async fn replay(&self, key: &str, fingerprint: &str) -> Result<PlaceOrderResult, StoreError> {
        let record = sqlx::query(
            "SELECT request_fingerprint, order_id FROM order_idempotency WHERE idempotency_key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?
        .ok_or_else(|| StoreError::Internal("conflicting idempotency row disappeared".into()))?;
        let stored_fingerprint: String = record.try_get("request_fingerprint").map_err(database_error)?;
        if stored_fingerprint != fingerprint {
            return Err(StoreError::IdempotencyConflict);
        }
        let id: Uuid = record.try_get("order_id").map_err(database_error)?;
        let order = self
            .fetch_order(OrderId::from_uuid(id))
            .await?
            .ok_or_else(|| StoreError::Internal("idempotency record references a missing order".into()))?;
        Ok(PlaceOrderResult { order, replayed: true })
    }
}

#[async_trait]
impl OrderStore for PostgresOrderStore {
    async fn place_order(&self, transaction: PlaceOrderTransaction) -> Result<PlaceOrderResult, StoreError> {
        let mut db = self.pool.begin().await.map_err(database_error)?;
        let lines = serde_json::to_value(&transaction.order.lines)
            .map_err(|error| StoreError::Internal(format!("encode order lines: {error}")))?;
        sqlx::query(
            "INSERT INTO orders (id, customer_reference, lines, status, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(transaction.order.id.into_uuid())
        .bind(transaction.order.customer_reference.as_str())
        .bind(lines)
        .bind("accepted")
        .bind(transaction.order.created_at)
        .execute(&mut *db)
        .await
        .map_err(database_error)?;
        let inserted = sqlx::query(
            "INSERT INTO order_idempotency (idempotency_key, request_fingerprint, order_id) VALUES ($1, $2, $3) ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(&transaction.idempotency_key)
        .bind(&transaction.request_fingerprint)
        .bind(transaction.order.id.into_uuid())
        .execute(&mut *db)
        .await
        .map_err(database_error)?
        .rows_affected();
        if inserted == 1 {
            db.commit().await.map_err(database_error)?;
            return Ok(PlaceOrderResult { order: transaction.order, replayed: false });
        }
        db.rollback().await.map_err(database_error)?;
        self.replay(&transaction.idempotency_key, &transaction.request_fingerprint).await
    }

    async fn get_order(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        self.fetch_order(id).await
    }

    async fn ready(&self) -> bool {
        minco_sqlx_postgres::ready(&self.pool).await
    }
}

fn decode_row(row: sqlx::postgres::PgRow) -> Result<Order, StoreError> {
    let id: Uuid = row.try_get("id").map_err(database_error)?;
    let customer_reference: String = row.try_get("customer_reference").map_err(database_error)?;
    let lines: serde_json::Value = row.try_get("lines").map_err(database_error)?;
    let status: String = row.try_get("status").map_err(database_error)?;
    let created_at = row.try_get("created_at").map_err(database_error)?;
    if status != "accepted" {
        return Err(StoreError::Internal(format!("unsupported persisted order status {status}")));
    }
    let lines: Vec<OrderLine> = serde_json::from_value(lines)
        .map_err(|error| StoreError::Internal(format!("decode order lines: {error}")))?;
    Ok(Order {
        id: OrderId::from_uuid(id),
        customer_reference: CustomerReference::parse(customer_reference)
            .map_err(|error| StoreError::Internal(format!("invalid persisted customer reference: {error}")))?,
        lines,
        status: OrderStatus::Accepted,
        created_at,
    })
}

fn database_error(error: sqlx::Error) -> StoreError {
    StoreError::Unavailable(error.to_string())
}
