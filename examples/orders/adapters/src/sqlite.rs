use async_trait::async_trait;
use minco_sqlx_sqlite::{SqliteError, SqlitePoolConfig};
use orders_application::{
    ConditionalResult, DeleteOrderPort, GetOrderPort, ListOrdersPort, ListOrdersQuery, OrderCursor,
    OrderPage, OrderReadiness, OrderSortField, OrderSortTerm, PlaceOrderPort, PlaceOrderResult,
    PlaceOrderTransaction, SortDirection, StoreError, UpdateOrderPort,
};
use orders_domain::{CustomerReference, Order, OrderId, OrderLine, OrderStatus};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
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
            "SELECT id, customer_reference, lines, status, created_at, updated_at, revision FROM orders WHERE id = ?1 AND deleted_at IS NULL",
        )
        .bind(id.into_uuid().to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| database_error(&error))?;
        row.as_ref().map(decode_row).transpose()
    }

    async fn replay(&self, key: &str, fingerprint: &str) -> Result<PlaceOrderResult, StoreError> {
        let record = sqlx::query(
            "SELECT request_fingerprint, response_snapshot FROM order_idempotency WHERE idempotency_key = ?1",
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
        let snapshot: String = record
            .try_get("response_snapshot")
            .map_err(|error| database_error(&error))?;
        let order = serde_json::from_str(&snapshot)
            .map_err(|error| StoreError::Internal(format!("decode idempotency result: {error}")))?;
        Ok(PlaceOrderResult {
            order,
            replayed: true,
        })
    }
}

#[async_trait]
impl PlaceOrderPort for SqliteOrderStore {
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
        let response_snapshot = serde_json::to_string(&transaction.order)
            .map_err(|error| StoreError::Internal(format!("encode idempotency result: {error}")))?;
        sqlx::query(
            "INSERT INTO orders (id, customer_reference, lines, status, created_at, updated_at, revision) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(transaction.order.id.into_uuid().to_string())
        .bind(transaction.order.customer_reference.as_str())
        .bind(lines)
        .bind("accepted")
        .bind(transaction.order.created_at.to_rfc3339())
        .bind(transaction.order.updated_at.to_rfc3339())
        .bind(i64::try_from(transaction.order.revision).map_err(|_| {
            StoreError::Internal("order revision cannot be represented by SQLite".into())
        })?)
        .execute(&mut *db)
        .await
        .map_err(|error| database_error(&error))?;
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO order_idempotency (idempotency_key, request_fingerprint, order_id, response_snapshot) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(&transaction.idempotency_key)
        .bind(&transaction.request_fingerprint)
        .bind(transaction.order.id.into_uuid().to_string())
        .bind(response_snapshot)
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
}

#[async_trait]
impl GetOrderPort for SqliteOrderStore {
    async fn get_order(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        self.fetch_order(id).await
    }
}

#[async_trait]
impl ListOrdersPort for SqliteOrderStore {
    async fn list_orders(&self, query: ListOrdersQuery) -> Result<OrderPage, StoreError> {
        let sort = normalized_sort(&query.sort);
        let mut builder = QueryBuilder::<Sqlite>::new(
            "SELECT id, customer_reference, lines, status, created_at, updated_at, revision FROM orders WHERE deleted_at IS NULL",
        );
        if let Some(cursor) = query.after {
            push_sqlite_cursor(&mut builder, &sort, cursor);
        }
        builder.push(" ORDER BY ");
        for (index, term) in sort.iter().enumerate() {
            if index > 0 {
                builder.push(", ");
            }
            push_sqlite_field(&mut builder, term.field);
            builder.push(match term.direction {
                SortDirection::Ascending => " ASC",
                SortDirection::Descending => " DESC",
            });
        }
        builder.push(" LIMIT ");
        builder.push_bind(i64::from(query.limit) + 1);
        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|error| database_error(&error))?;
        let mut orders = rows.iter().map(decode_row).collect::<Result<Vec<_>, _>>()?;
        let limit = usize::from(query.limit);
        let has_more = orders.len() > limit;
        orders.truncate(limit);
        let next_cursor = has_more
            .then(|| orders.last())
            .flatten()
            .map(|order| OrderCursor {
                created_at: order.created_at,
                id: order.id,
            });
        Ok(OrderPage {
            orders,
            next_cursor,
        })
    }
}

#[async_trait]
impl UpdateOrderPort for SqliteOrderStore {
    async fn get_order_for_update(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        self.fetch_order(id).await
    }

    async fn save_order(
        &self,
        order: Order,
        expected_revision: u64,
    ) -> Result<ConditionalResult<Order>, StoreError> {
        let lines = serde_json::to_string(&order.lines)
            .map_err(|error| StoreError::Internal(format!("encode order lines: {error}")))?;
        let rows = sqlx::query(
            "UPDATE orders SET customer_reference = ?1, lines = ?2, status = ?3, updated_at = ?4, revision = ?5 WHERE id = ?6 AND revision = ?7 AND deleted_at IS NULL",
        )
        .bind(order.customer_reference.as_str())
        .bind(lines)
        .bind("accepted")
        .bind(order.updated_at.to_rfc3339())
        .bind(to_sqlite_revision(order.revision)?)
        .bind(order.id.into_uuid().to_string())
        .bind(to_sqlite_revision(expected_revision)?)
        .execute(&self.pool)
        .await
        .map_err(|error| database_error(&error))?
        .rows_affected();
        if rows == 1 {
            return Ok(ConditionalResult::Applied(order));
        }
        Ok(conditional_miss(
            self.fetch_order(order.id).await?.is_some(),
        ))
    }
}

#[async_trait]
impl DeleteOrderPort for SqliteOrderStore {
    async fn delete_order(
        &self,
        id: OrderId,
        expected_revision: u64,
        deleted_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<ConditionalResult<()>, StoreError> {
        let timestamp = deleted_at.to_rfc3339();
        let rows = sqlx::query(
            "UPDATE orders SET deleted_at = ?1, updated_at = ?1, revision = revision + 1 WHERE id = ?2 AND revision = ?3 AND deleted_at IS NULL",
        )
        .bind(timestamp)
        .bind(id.into_uuid().to_string())
        .bind(to_sqlite_revision(expected_revision)?)
        .execute(&self.pool)
        .await
        .map_err(|error| database_error(&error))?
        .rows_affected();
        if rows == 1 {
            return Ok(ConditionalResult::Applied(()));
        }
        Ok(conditional_miss(self.fetch_order(id).await?.is_some()))
    }
}

#[async_trait]
impl OrderReadiness for SqliteOrderStore {
    async fn ready(&self) -> bool {
        minco_sqlx_sqlite::ready(&self.pool).await
    }
}

const fn conditional_miss<T>(active: bool) -> ConditionalResult<T> {
    if active {
        ConditionalResult::PreconditionFailed
    } else {
        ConditionalResult::NotFound
    }
}

fn to_sqlite_revision(revision: u64) -> Result<i64, StoreError> {
    i64::try_from(revision)
        .map_err(|_| StoreError::Internal("order revision cannot be represented by SQLite".into()))
}

fn normalized_sort(sort: &[OrderSortTerm]) -> Vec<OrderSortTerm> {
    let mut normalized = sort.to_vec();
    if !normalized
        .iter()
        .any(|term| term.field == OrderSortField::Id)
    {
        normalized.push(OrderSortTerm {
            field: OrderSortField::Id,
            direction: sort
                .last()
                .map_or(SortDirection::Descending, |term| term.direction),
        });
    }
    normalized
}

fn push_sqlite_field(builder: &mut QueryBuilder<Sqlite>, field: OrderSortField) {
    builder.push(match field {
        OrderSortField::CreatedAt => "julianday(created_at)",
        OrderSortField::Id => "id",
    });
}

fn push_sqlite_value(
    builder: &mut QueryBuilder<Sqlite>,
    field: OrderSortField,
    cursor: OrderCursor,
) {
    match field {
        OrderSortField::CreatedAt => {
            builder.push("julianday(");
            builder.push_bind(cursor.created_at.to_rfc3339());
            builder.push(")");
        }
        OrderSortField::Id => {
            builder.push_bind(cursor.id.into_uuid().to_string());
        }
    }
}

fn push_sqlite_cursor(
    builder: &mut QueryBuilder<Sqlite>,
    sort: &[OrderSortTerm],
    cursor: OrderCursor,
) {
    builder.push(" AND (");
    for index in 0..sort.len() {
        if index > 0 {
            builder.push(" OR ");
        }
        builder.push("(");
        for previous in &sort[..index] {
            push_sqlite_field(builder, previous.field);
            builder.push(" = ");
            push_sqlite_value(builder, previous.field, cursor);
            builder.push(" AND ");
        }
        let term = sort[index];
        push_sqlite_field(builder, term.field);
        builder.push(match term.direction {
            SortDirection::Ascending => " > ",
            SortDirection::Descending => " < ",
        });
        push_sqlite_value(builder, term.field, cursor);
        builder.push(")");
    }
    builder.push(")");
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
    let updated_at: String = row
        .try_get("updated_at")
        .map_err(|error| database_error(&error))?;
    let revision: i64 = row
        .try_get("revision")
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
    let updated_at = chrono::DateTime::parse_from_rfc3339(&updated_at)
        .map_err(|error| StoreError::Internal(format!("invalid persisted timestamp: {error}")))?
        .with_timezone(&chrono::Utc);
    let revision = u64::try_from(revision)
        .ok()
        .filter(|revision| *revision > 0)
        .ok_or_else(|| StoreError::Internal("invalid persisted order revision".into()))?;
    Ok(Order {
        id: OrderId::from_uuid(id),
        customer_reference: CustomerReference::parse(customer_reference).map_err(|error| {
            StoreError::Internal(format!("invalid persisted customer reference: {error}"))
        })?,
        lines,
        status: OrderStatus::Accepted,
        created_at,
        updated_at,
        revision,
    })
}

fn database_error(error: &sqlx::Error) -> StoreError {
    StoreError::Unavailable(error.to_string())
}
