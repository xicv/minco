use async_trait::async_trait;
use minco_plugin_audit::AuditJournalEntry;
use minco_sqlx_postgres::{
    PostgresError, PostgresPoolConfig, audit_v2::PostgresAuditJournal, jobs::PostgresJobStore,
};
use orders_application::{
    ConditionalResult, DeleteOrderPort, GetOrderPort, ListOrdersPort, ListOrdersQuery,
    OrderAuditIntent, OrderCursor, OrderPage, OrderReadiness, OrderSortField, OrderSortTerm,
    PlaceOrderPort, PlaceOrderResult, PlaceOrderTransaction, SortDirection, StoreError,
    UpdateOrderPort,
};
use orders_domain::{CustomerReference, Order, OrderId, OrderLine, OrderStatus};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PostgresOrderStore {
    pool: PgPool,
    audit_journal: PostgresAuditJournal,
    job_store: PostgresJobStore,
}

impl PostgresOrderStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            audit_journal: PostgresAuditJournal::new(pool.clone()),
            job_store: PostgresJobStore::new(pool.clone()),
            pool,
        }
    }

    pub async fn connect(config: &PostgresPoolConfig) -> Result<Self, PostgresError> {
        Ok(Self::new(minco_sqlx_postgres::connect(config).await?))
    }

    pub async fn migrate(&self, path: impl AsRef<Path>) -> Result<(), PostgresError> {
        minco_sqlx_postgres::migrate_with_history_table(
            &self.pool,
            path,
            "_minco_orders_migrations",
        )
        .await
    }

    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    async fn fetch_order(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        let row = sqlx::query(
            "SELECT id, customer_reference, lines, status, created_at, updated_at, revision FROM orders WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id.into_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| database_error(&error))?;
        row.as_ref().map(decode_row).transpose()
    }

    async fn replay(&self, key: &str, fingerprint: &str) -> Result<PlaceOrderResult, StoreError> {
        let record = sqlx::query(
            "SELECT request_fingerprint, response_snapshot FROM order_idempotency WHERE idempotency_key = $1",
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
        let snapshot: serde_json::Value = record
            .try_get("response_snapshot")
            .map_err(|error| database_error(&error))?;
        let order = serde_json::from_value(snapshot)
            .map_err(|error| StoreError::Internal(format!("decode idempotency result: {error}")))?;
        Ok(PlaceOrderResult {
            order,
            replayed: true,
        })
    }
}

#[async_trait]
impl PlaceOrderPort for PostgresOrderStore {
    async fn place_order(
        &self,
        transaction: PlaceOrderTransaction,
    ) -> Result<PlaceOrderResult, StoreError> {
        let audit = transaction
            .audit
            .as_ref()
            .map(crate::audit::audit_record)
            .transpose()?
            .map(AuditJournalEntry::pending)
            .transpose()
            .map_err(crate::audit::audit_error)?;
        let mut db = self
            .pool
            .begin()
            .await
            .map_err(|error| database_error(&error))?;
        let lines = serde_json::to_value(&transaction.order.lines)
            .map_err(|error| StoreError::Internal(format!("encode order lines: {error}")))?;
        let response_snapshot = serde_json::to_value(&transaction.order)
            .map_err(|error| StoreError::Internal(format!("encode idempotency result: {error}")))?;
        sqlx::query(
            "INSERT INTO orders (id, customer_reference, lines, status, created_at, updated_at, revision) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(transaction.order.id.into_uuid())
        .bind(transaction.order.customer_reference.as_str())
        .bind(lines)
        .bind("accepted")
        .bind(transaction.order.created_at)
        .bind(transaction.order.updated_at)
        .bind(to_postgres_revision(transaction.order.revision)?)
        .execute(&mut *db)
        .await
        .map_err(|error| database_error(&error))?;
        let inserted = sqlx::query(
            "INSERT INTO order_idempotency (idempotency_key, request_fingerprint, order_id, response_snapshot) VALUES ($1, $2, $3, $4) ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(&transaction.idempotency_key)
        .bind(&transaction.request_fingerprint)
        .bind(transaction.order.id.into_uuid())
        .bind(response_snapshot)
        .execute(&mut *db)
        .await
        .map_err(|error| database_error(&error))?
        .rows_affected();
        if inserted == 1 {
            if let Some(audit) = audit {
                self.audit_journal
                    .enqueue_in(&mut db, audit)
                    .await
                    .map_err(crate::audit::audit_error)?;
            }
            if let Some(confirmation) = &transaction.confirmation_job {
                let envelope = crate::jobs::confirmation_envelope(confirmation)
                    .map_err(|error| StoreError::Internal(error.to_string()))?;
                let record = minco_plugin_jobs::pending_record(envelope);
                self.job_store
                    .enqueue_in(&mut db, record)
                    .await
                    .map_err(|error| StoreError::Internal(error.to_string()))?;
            }
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
impl GetOrderPort for PostgresOrderStore {
    async fn get_order(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        self.fetch_order(id).await
    }
}

#[async_trait]
impl ListOrdersPort for PostgresOrderStore {
    async fn list_orders(&self, query: ListOrdersQuery) -> Result<OrderPage, StoreError> {
        let sort = normalized_sort(&query.sort);
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, customer_reference, lines, status, created_at, updated_at, revision FROM orders WHERE deleted_at IS NULL",
        );
        if let Some(cursor) = query.after {
            push_postgres_cursor(&mut builder, &sort, cursor);
        }
        builder.push(" ORDER BY ");
        for (index, term) in sort.iter().enumerate() {
            if index > 0 {
                builder.push(", ");
            }
            push_postgres_field(&mut builder, term.field);
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
impl UpdateOrderPort for PostgresOrderStore {
    async fn get_order_for_update(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        self.fetch_order(id).await
    }

    async fn save_order(
        &self,
        order: Order,
        expected_revision: u64,
    ) -> Result<ConditionalResult<Order>, StoreError> {
        let lines = serde_json::to_value(&order.lines)
            .map_err(|error| StoreError::Internal(format!("encode order lines: {error}")))?;
        let rows = sqlx::query(
            "UPDATE orders SET customer_reference = $1, lines = $2, status = $3, updated_at = $4, revision = $5 WHERE id = $6 AND revision = $7 AND deleted_at IS NULL",
        )
        .bind(order.customer_reference.as_str())
        .bind(lines)
        .bind("accepted")
        .bind(order.updated_at)
        .bind(to_postgres_revision(order.revision)?)
        .bind(order.id.into_uuid())
        .bind(to_postgres_revision(expected_revision)?)
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

    async fn save_order_with_audit(
        &self,
        order: Order,
        expected_revision: u64,
        audit: OrderAuditIntent,
    ) -> Result<ConditionalResult<Order>, StoreError> {
        let audit = AuditJournalEntry::pending(crate::audit::audit_record(&audit)?)
            .map_err(crate::audit::audit_error)?;
        let lines = serde_json::to_value(&order.lines)
            .map_err(|error| StoreError::Internal(format!("encode order lines: {error}")))?;
        let mut db = self
            .pool
            .begin()
            .await
            .map_err(|error| database_error(&error))?;
        let rows = sqlx::query(
            "UPDATE orders SET customer_reference = $1, lines = $2, status = $3, updated_at = $4, revision = $5 WHERE id = $6 AND revision = $7 AND deleted_at IS NULL",
        )
        .bind(order.customer_reference.as_str())
        .bind(lines)
        .bind("accepted")
        .bind(order.updated_at)
        .bind(to_postgres_revision(order.revision)?)
        .bind(order.id.into_uuid())
        .bind(to_postgres_revision(expected_revision)?)
        .execute(&mut *db)
        .await
        .map_err(|error| database_error(&error))?
        .rows_affected();
        if rows == 1 {
            self.audit_journal
                .enqueue_in(&mut db, audit)
                .await
                .map_err(crate::audit::audit_error)?;
            db.commit().await.map_err(|error| database_error(&error))?;
            return Ok(ConditionalResult::Applied(order));
        }
        db.rollback()
            .await
            .map_err(|error| database_error(&error))?;
        Ok(conditional_miss(
            self.fetch_order(order.id).await?.is_some(),
        ))
    }
}

#[async_trait]
impl DeleteOrderPort for PostgresOrderStore {
    async fn delete_order(
        &self,
        id: OrderId,
        expected_revision: u64,
        deleted_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<ConditionalResult<()>, StoreError> {
        let rows = sqlx::query(
            "UPDATE orders SET deleted_at = $1, updated_at = $1, revision = revision + 1 WHERE id = $2 AND revision = $3 AND deleted_at IS NULL",
        )
        .bind(deleted_at)
        .bind(id.into_uuid())
        .bind(to_postgres_revision(expected_revision)?)
        .execute(&self.pool)
        .await
        .map_err(|error| database_error(&error))?
        .rows_affected();
        if rows == 1 {
            return Ok(ConditionalResult::Applied(()));
        }
        Ok(conditional_miss(self.fetch_order(id).await?.is_some()))
    }

    async fn delete_order_with_audit(
        &self,
        id: OrderId,
        expected_revision: u64,
        deleted_at: chrono::DateTime<chrono::Utc>,
        audit: OrderAuditIntent,
    ) -> Result<ConditionalResult<()>, StoreError> {
        let audit = AuditJournalEntry::pending(crate::audit::audit_record(&audit)?)
            .map_err(crate::audit::audit_error)?;
        let mut db = self
            .pool
            .begin()
            .await
            .map_err(|error| database_error(&error))?;
        let rows = sqlx::query(
            "UPDATE orders SET deleted_at = $1, updated_at = $1, revision = revision + 1 WHERE id = $2 AND revision = $3 AND deleted_at IS NULL",
        )
        .bind(deleted_at)
        .bind(id.into_uuid())
        .bind(to_postgres_revision(expected_revision)?)
        .execute(&mut *db)
        .await
        .map_err(|error| database_error(&error))?
        .rows_affected();
        if rows == 1 {
            self.audit_journal
                .enqueue_in(&mut db, audit)
                .await
                .map_err(crate::audit::audit_error)?;
            db.commit().await.map_err(|error| database_error(&error))?;
            return Ok(ConditionalResult::Applied(()));
        }
        db.rollback()
            .await
            .map_err(|error| database_error(&error))?;
        Ok(conditional_miss(self.fetch_order(id).await?.is_some()))
    }
}

#[async_trait]
impl OrderReadiness for PostgresOrderStore {
    async fn ready(&self) -> bool {
        minco_sqlx_postgres::ready(&self.pool).await
    }
}

const fn conditional_miss<T>(active: bool) -> ConditionalResult<T> {
    if active {
        ConditionalResult::PreconditionFailed
    } else {
        ConditionalResult::NotFound
    }
}

fn to_postgres_revision(revision: u64) -> Result<i64, StoreError> {
    i64::try_from(revision).map_err(|_| {
        StoreError::Internal("order revision cannot be represented by PostgreSQL".into())
    })
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

fn push_postgres_field(builder: &mut QueryBuilder<Postgres>, field: OrderSortField) {
    builder.push(match field {
        OrderSortField::CreatedAt => "created_at",
        OrderSortField::Id => "id",
    });
}

fn push_postgres_value(
    builder: &mut QueryBuilder<Postgres>,
    field: OrderSortField,
    cursor: OrderCursor,
) {
    match field {
        OrderSortField::CreatedAt => {
            builder.push_bind(cursor.created_at);
        }
        OrderSortField::Id => {
            builder.push_bind(cursor.id.into_uuid());
        }
    }
}

fn push_postgres_cursor(
    builder: &mut QueryBuilder<Postgres>,
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
            push_postgres_field(builder, previous.field);
            builder.push(" = ");
            push_postgres_value(builder, previous.field, cursor);
            builder.push(" AND ");
        }
        let term = sort[index];
        push_postgres_field(builder, term.field);
        builder.push(match term.direction {
            SortDirection::Ascending => " > ",
            SortDirection::Descending => " < ",
        });
        push_postgres_value(builder, term.field, cursor);
        builder.push(")");
    }
    builder.push(")");
}

fn decode_row(row: &sqlx::postgres::PgRow) -> Result<Order, StoreError> {
    let id: Uuid = row.try_get("id").map_err(|error| database_error(&error))?;
    let customer_reference: String = row
        .try_get("customer_reference")
        .map_err(|error| database_error(&error))?;
    let lines: serde_json::Value = row
        .try_get("lines")
        .map_err(|error| database_error(&error))?;
    let status: String = row
        .try_get("status")
        .map_err(|error| database_error(&error))?;
    let created_at = row
        .try_get("created_at")
        .map_err(|error| database_error(&error))?;
    let updated_at = row
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
    let lines: Vec<OrderLine> = serde_json::from_value(lines)
        .map_err(|error| StoreError::Internal(format!("decode order lines: {error}")))?;
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
