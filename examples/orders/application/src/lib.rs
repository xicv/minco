//! Use cases and use-case-shaped ports for the orders reference application.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use orders_domain::{
    CustomerReference, DomainError, Order, OrderId, OrderLine, OrderStatus, Quantity, Sku,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub subject: String,
    pub permissions: BTreeSet<String>,
}

impl Actor {
    #[must_use]
    pub fn service(
        subject: impl Into<String>,
        permissions: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            permissions: permissions.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceOrderLine {
    pub sku: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceOrderCommand {
    pub customer_reference: String,
    pub lines: Vec<PlaceOrderLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceOrderResult {
    pub order: Order,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceOrderTransaction {
    pub order: Order,
    pub idempotency_key: String,
    pub request_fingerprint: String,
    pub audit: Option<OrderAuditIntent>,
}

#[async_trait]
pub trait PlaceOrderPort: Send + Sync {
    async fn place_order(
        &self,
        transaction: PlaceOrderTransaction,
    ) -> Result<PlaceOrderResult, StoreError>;
}

#[async_trait]
pub trait GetOrderPort: Send + Sync {
    async fn get_order(&self, id: OrderId) -> Result<Option<Order>, StoreError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderCursor {
    pub created_at: DateTime<Utc>,
    pub id: OrderId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OrderSortField {
    CreatedAt,
    Id,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderSortTerm {
    pub field: OrderSortField,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListOrdersQuery {
    pub limit: u16,
    pub after: Option<OrderCursor>,
    pub sort: Vec<OrderSortTerm>,
    pub status: Option<OrderStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderPage {
    pub orders: Vec<Order>,
    pub next_cursor: Option<OrderCursor>,
}

#[async_trait]
pub trait ListOrdersPort: Send + Sync {
    async fn list_orders(&self, query: ListOrdersQuery) -> Result<OrderPage, StoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionalResult<T> {
    Applied(T),
    NotFound,
    PreconditionFailed,
}

#[async_trait]
pub trait UpdateOrderPort: Send + Sync {
    async fn get_order_for_update(&self, id: OrderId) -> Result<Option<Order>, StoreError>;
    async fn save_order(
        &self,
        order: Order,
        expected_revision: u64,
    ) -> Result<ConditionalResult<Order>, StoreError>;

    async fn save_order_with_audit(
        &self,
        order: Order,
        expected_revision: u64,
        audit: OrderAuditIntent,
    ) -> Result<ConditionalResult<Order>, StoreError> {
        let _ = (order, expected_revision, audit);
        Err(StoreError::Internal(
            "the selected order adapter does not support transactional auditing".into(),
        ))
    }
}

#[async_trait]
pub trait DeleteOrderPort: Send + Sync {
    async fn delete_order(
        &self,
        id: OrderId,
        expected_revision: u64,
        deleted_at: DateTime<Utc>,
    ) -> Result<ConditionalResult<()>, StoreError>;

    async fn delete_order_with_audit(
        &self,
        id: OrderId,
        expected_revision: u64,
        deleted_at: DateTime<Utc>,
        audit: OrderAuditIntent,
    ) -> Result<ConditionalResult<()>, StoreError> {
        let _ = (id, expected_revision, deleted_at, audit);
        Err(StoreError::Internal(
            "the selected order adapter does not support transactional auditing".into(),
        ))
    }
}

pub const ORDERS_AUDIT_TENANT_SCOPE: &str = "orders-reference";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderAuditValue {
    Literal(String),
    Digest(String),
    Redacted,
    Omitted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderAuditChange {
    pub before: Option<OrderAuditValue>,
    pub after: Option<OrderAuditValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderAuditIntent {
    pub event_id: Uuid,
    pub action: String,
    pub order_id: OrderId,
    pub resource_revision: u64,
    pub actor_subject: String,
    pub operation_id: String,
    pub correlation_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub idempotency_key_digest: Option<String>,
    pub changes: BTreeMap<String, OrderAuditChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderAuditActorKind {
    Human,
    Service,
    System,
    Migration,
    DatabasePrincipal,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderAuditEvent {
    pub event_id: Uuid,
    pub action: String,
    pub resource_revision: Option<u64>,
    pub actor_kind: OrderAuditActorKind,
    pub actor_subject: Option<String>,
    pub operation_id: String,
    pub correlation_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub changes: BTreeMap<String, OrderAuditChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderAuditCursor {
    pub occurred_at: DateTime<Utc>,
    pub event_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListOrderAuditHistoryQuery {
    pub order_id: OrderId,
    pub limit: u16,
    pub after: Option<OrderAuditCursor>,
    pub direction: OrderAuditSortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderAuditSortDirection {
    OldestFirst,
    NewestFirst,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderAuditPage {
    pub events: Vec<OrderAuditEvent>,
    pub next_cursor: Option<OrderAuditCursor>,
}

#[async_trait]
pub trait ListOrderAuditHistoryPort: Send + Sync {
    async fn list_order_audit_history(
        &self,
        query: ListOrderAuditHistoryQuery,
    ) -> Result<OrderAuditPage, StoreError>;
}

#[async_trait]
pub trait OrderReadiness: Send + Sync {
    async fn ready(&self) -> bool;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug)]
pub struct PlaceOrder<S: ?Sized, C: ?Sized> {
    store: Arc<S>,
    clock: Arc<C>,
}

impl<S, C> PlaceOrder<S, C>
where
    S: PlaceOrderPort + ?Sized,
    C: Clock + ?Sized,
{
    #[must_use]
    pub const fn new(store: Arc<S>, clock: Arc<C>) -> Self {
        Self { store, clock }
    }

    pub async fn execute(
        &self,
        actor: &Actor,
        command: PlaceOrderCommand,
        idempotency_key: &str,
    ) -> Result<PlaceOrderResult, ApplicationError> {
        self.execute_correlated(actor, command, idempotency_key, Uuid::now_v7())
            .await
    }

    pub async fn execute_correlated(
        &self,
        actor: &Actor,
        command: PlaceOrderCommand,
        idempotency_key: &str,
        correlation_id: Uuid,
    ) -> Result<PlaceOrderResult, ApplicationError> {
        if !actor.has_permission("orders.create") {
            return Err(ApplicationError::Forbidden);
        }
        validate_correlation_id(correlation_id)?;
        let idempotency_key = validate_idempotency_key(idempotency_key)?;
        let customer_reference = CustomerReference::parse(command.customer_reference.clone())?;
        let lines = parse_lines(&command.lines)?;
        let fingerprint = request_fingerprint(actor, &command)?;
        let order = Order::new(customer_reference, lines, self.clock.now())?;
        let audit = place_order_audit(actor, &order, &idempotency_key, correlation_id)?;
        self.store
            .place_order(PlaceOrderTransaction {
                order,
                idempotency_key,
                request_fingerprint: fingerprint,
                audit: Some(audit),
            })
            .await
            .map_err(ApplicationError::from)
    }
}

#[derive(Debug)]
pub struct GetOrder<S: ?Sized> {
    store: Arc<S>,
}

impl<S> GetOrder<S>
where
    S: GetOrderPort + ?Sized,
{
    #[must_use]
    pub const fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, actor: &Actor, id: OrderId) -> Result<Order, ApplicationError> {
        if !actor.has_permission("orders.read") {
            return Err(ApplicationError::Forbidden);
        }
        self.store
            .get_order(id)
            .await?
            .ok_or(ApplicationError::NotFound)
    }
}

#[derive(Debug)]
pub struct ListOrders<S: ?Sized> {
    store: Arc<S>,
}

impl<S> ListOrders<S>
where
    S: ListOrdersPort + ?Sized,
{
    #[must_use]
    pub const fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn execute(
        &self,
        actor: &Actor,
        query: ListOrdersQuery,
    ) -> Result<OrderPage, ApplicationError> {
        if !actor.has_permission("orders.read") {
            return Err(ApplicationError::Forbidden);
        }
        validate_list_query(&query)?;
        self.store
            .list_orders(query)
            .await
            .map_err(ApplicationError::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateOrderCommand {
    pub customer_reference: Option<String>,
    pub lines: Option<Vec<PlaceOrderLine>>,
}

#[derive(Debug)]
pub struct UpdateOrder<S: ?Sized, C: ?Sized> {
    store: Arc<S>,
    clock: Arc<C>,
}

impl<S, C> UpdateOrder<S, C>
where
    S: UpdateOrderPort + ?Sized,
    C: Clock + ?Sized,
{
    #[must_use]
    pub const fn new(store: Arc<S>, clock: Arc<C>) -> Self {
        Self { store, clock }
    }

    pub async fn execute(
        &self,
        actor: &Actor,
        id: OrderId,
        expected_revision: u64,
        command: UpdateOrderCommand,
    ) -> Result<Order, ApplicationError> {
        self.execute_correlated(actor, id, expected_revision, command, Uuid::now_v7())
            .await
    }

    pub async fn execute_correlated(
        &self,
        actor: &Actor,
        id: OrderId,
        expected_revision: u64,
        command: UpdateOrderCommand,
        correlation_id: Uuid,
    ) -> Result<Order, ApplicationError> {
        if !actor.has_permission("orders.update") {
            return Err(ApplicationError::Forbidden);
        }
        validate_correlation_id(correlation_id)?;
        if command.customer_reference.is_none() && command.lines.is_none() {
            return Err(ApplicationError::Validation(
                "an order update must change at least one mutable field".into(),
            ));
        }
        let customer_reference = command
            .customer_reference
            .map(CustomerReference::parse)
            .transpose()?;
        let lines = command.lines.as_deref().map(parse_lines).transpose()?;
        let mut order = self
            .store
            .get_order_for_update(id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        if order.revision != expected_revision {
            return Err(ApplicationError::PreconditionFailed);
        }
        let before = order.clone();
        order.update(customer_reference, lines, self.clock.now())?;
        let audit = update_order_audit(actor, &before, &order, correlation_id)?;
        match self
            .store
            .save_order_with_audit(order, expected_revision, audit)
            .await?
        {
            ConditionalResult::Applied(order) => Ok(order),
            ConditionalResult::NotFound => Err(ApplicationError::NotFound),
            ConditionalResult::PreconditionFailed => Err(ApplicationError::PreconditionFailed),
        }
    }
}

#[derive(Debug)]
pub struct DeleteOrder<S: ?Sized, C: ?Sized> {
    store: Arc<S>,
    clock: Arc<C>,
}

impl<S, C> DeleteOrder<S, C>
where
    S: DeleteOrderPort + ?Sized,
    C: Clock + ?Sized,
{
    #[must_use]
    pub const fn new(store: Arc<S>, clock: Arc<C>) -> Self {
        Self { store, clock }
    }

    pub async fn execute(
        &self,
        actor: &Actor,
        id: OrderId,
        expected_revision: u64,
    ) -> Result<(), ApplicationError> {
        self.execute_correlated(actor, id, expected_revision, Uuid::now_v7())
            .await
    }

    pub async fn execute_correlated(
        &self,
        actor: &Actor,
        id: OrderId,
        expected_revision: u64,
        correlation_id: Uuid,
    ) -> Result<(), ApplicationError> {
        if !actor.has_permission("orders.delete") {
            return Err(ApplicationError::Forbidden);
        }
        validate_correlation_id(correlation_id)?;
        let deleted_at = self.clock.now();
        let audit = delete_order_audit(actor, id, expected_revision, deleted_at, correlation_id)?;
        match self
            .store
            .delete_order_with_audit(id, expected_revision, deleted_at, audit)
            .await?
        {
            ConditionalResult::Applied(()) => Ok(()),
            ConditionalResult::NotFound => Err(ApplicationError::NotFound),
            ConditionalResult::PreconditionFailed => Err(ApplicationError::PreconditionFailed),
        }
    }
}

#[derive(Debug)]
pub struct ListOrderAuditHistory<S: ?Sized> {
    store: Arc<S>,
}

impl<S> ListOrderAuditHistory<S>
where
    S: ListOrderAuditHistoryPort + ?Sized,
{
    #[must_use]
    pub const fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn execute(
        &self,
        actor: &Actor,
        query: ListOrderAuditHistoryQuery,
    ) -> Result<OrderAuditPage, ApplicationError> {
        if !actor.has_permission("orders.audit.read") {
            return Err(ApplicationError::Forbidden);
        }
        if query.limit == 0 || query.limit > 100 {
            return Err(ApplicationError::Validation(
                "order audit history limit must be between 1 and 100".into(),
            ));
        }
        self.store
            .list_order_audit_history(query)
            .await
            .map_err(ApplicationError::from)
    }
}

fn place_order_audit(
    actor: &Actor,
    order: &Order,
    idempotency_key: &str,
    correlation_id: Uuid,
) -> Result<OrderAuditIntent, ApplicationError> {
    let mut changes = BTreeMap::new();
    changes.insert(
        "customer_reference".into(),
        OrderAuditChange {
            before: None,
            after: Some(OrderAuditValue::Digest(sha256_text(
                order.customer_reference.as_str(),
            ))),
        },
    );
    changes.insert(
        "lines".into(),
        OrderAuditChange {
            before: None,
            after: Some(OrderAuditValue::Digest(digest_json(&order.lines)?)),
        },
    );
    changes.insert(
        "status".into(),
        OrderAuditChange {
            before: None,
            after: Some(OrderAuditValue::Literal(
                order_status_name(order.status).into(),
            )),
        },
    );
    Ok(OrderAuditIntent {
        event_id: audit_event_id(correlation_id, "order.created", order.id, order.revision),
        action: "order.created".into(),
        order_id: order.id,
        resource_revision: order.revision,
        actor_subject: actor.subject.clone(),
        operation_id: "placeOrder".into(),
        correlation_id,
        occurred_at: order.created_at,
        idempotency_key_digest: Some(sha256_text(idempotency_key)),
        changes,
    })
}

fn update_order_audit(
    actor: &Actor,
    before: &Order,
    after: &Order,
    correlation_id: Uuid,
) -> Result<OrderAuditIntent, ApplicationError> {
    let mut changes = BTreeMap::new();
    if before.customer_reference != after.customer_reference {
        changes.insert(
            "customer_reference".into(),
            OrderAuditChange {
                before: Some(OrderAuditValue::Digest(sha256_text(
                    before.customer_reference.as_str(),
                ))),
                after: Some(OrderAuditValue::Digest(sha256_text(
                    after.customer_reference.as_str(),
                ))),
            },
        );
    }
    if before.lines != after.lines {
        changes.insert(
            "lines".into(),
            OrderAuditChange {
                before: Some(OrderAuditValue::Digest(digest_json(&before.lines)?)),
                after: Some(OrderAuditValue::Digest(digest_json(&after.lines)?)),
            },
        );
    }
    changes.insert(
        "revision".into(),
        OrderAuditChange {
            before: Some(OrderAuditValue::Literal(before.revision.to_string())),
            after: Some(OrderAuditValue::Literal(after.revision.to_string())),
        },
    );
    Ok(OrderAuditIntent {
        event_id: audit_event_id(correlation_id, "order.updated", after.id, after.revision),
        action: "order.updated".into(),
        order_id: after.id,
        resource_revision: after.revision,
        actor_subject: actor.subject.clone(),
        operation_id: "updateOrder".into(),
        correlation_id,
        occurred_at: after.updated_at,
        idempotency_key_digest: None,
        changes,
    })
}

fn delete_order_audit(
    actor: &Actor,
    id: OrderId,
    expected_revision: u64,
    deleted_at: DateTime<Utc>,
    correlation_id: Uuid,
) -> Result<OrderAuditIntent, ApplicationError> {
    let resource_revision = expected_revision
        .checked_add(1)
        .ok_or(ApplicationError::Internal)?;
    let mut changes = BTreeMap::new();
    changes.insert(
        "deleted_at".into(),
        OrderAuditChange {
            before: None,
            after: Some(OrderAuditValue::Literal(deleted_at.to_rfc3339())),
        },
    );
    changes.insert(
        "revision".into(),
        OrderAuditChange {
            before: Some(OrderAuditValue::Literal(expected_revision.to_string())),
            after: Some(OrderAuditValue::Literal(resource_revision.to_string())),
        },
    );
    Ok(OrderAuditIntent {
        event_id: audit_event_id(correlation_id, "order.deleted", id, resource_revision),
        action: "order.deleted".into(),
        order_id: id,
        resource_revision,
        actor_subject: actor.subject.clone(),
        operation_id: "deleteOrder".into(),
        correlation_id,
        occurred_at: deleted_at,
        idempotency_key_digest: None,
        changes,
    })
}

fn validate_correlation_id(correlation_id: Uuid) -> Result<(), ApplicationError> {
    if correlation_id.is_nil() {
        return Err(ApplicationError::Validation(
            "audit correlation ID must be non-nil".into(),
        ));
    }
    Ok(())
}

fn sha256_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

fn audit_event_id(
    correlation_id: Uuid,
    action: &str,
    order_id: OrderId,
    resource_revision: u64,
) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(correlation_id.as_bytes());
    hasher.update([0]);
    hasher.update(action.as_bytes());
    hasher.update([0]);
    hasher.update(order_id.into_uuid().as_bytes());
    hasher.update(resource_revision.to_be_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn digest_json(value: &impl Serialize) -> Result<String, ApplicationError> {
    let encoded = serde_json::to_vec(value).map_err(|_| ApplicationError::Internal)?;
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    Ok(hex::encode(hasher.finalize()))
}

const fn order_status_name(status: OrderStatus) -> &'static str {
    match status {
        OrderStatus::Accepted => "accepted",
    }
}

fn parse_lines(lines: &[PlaceOrderLine]) -> Result<Vec<OrderLine>, DomainError> {
    lines
        .iter()
        .map(|line| {
            Ok(OrderLine {
                sku: Sku::parse(line.sku.clone())?,
                quantity: Quantity::new(line.quantity)?,
            })
        })
        .collect()
}

fn validate_list_query(query: &ListOrdersQuery) -> Result<(), ApplicationError> {
    if query.limit == 0 || query.limit > 100 || query.sort.is_empty() || query.sort.len() > 2 {
        return Err(ApplicationError::Validation(
            "order list query is outside the supported bounds".into(),
        ));
    }
    let fields = query
        .sort
        .iter()
        .map(|term| term.field)
        .collect::<BTreeSet<_>>();
    if fields.len() != query.sort.len() {
        return Err(ApplicationError::Validation(
            "order list sort fields must be unique".into(),
        ));
    }
    Ok(())
}

fn validate_idempotency_key(value: &str) -> Result<String, ApplicationError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control) {
        return Err(ApplicationError::Validation(
            "Idempotency-Key must contain 1 to 200 visible characters".into(),
        ));
    }
    Ok(value.to_owned())
}

fn request_fingerprint(
    actor: &Actor,
    command: &PlaceOrderCommand,
) -> Result<String, ApplicationError> {
    let canonical = serde_json::to_vec(&(actor.subject.as_str(), command))
        .map_err(|_| ApplicationError::Internal)?;
    let mut hasher = Sha256::new();
    hasher.update(canonical);
    Ok(hex::encode(hasher.finalize()))
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StoreError {
    #[error("the idempotency key was already used for a different request")]
    IdempotencyConflict,
    #[error("the persistence service is unavailable: {0}")]
    Unavailable(String),
    #[error("the persistence operation failed: {0}")]
    Internal(String),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ApplicationError {
    #[error("the caller is not permitted to perform this operation")]
    Forbidden,
    #[error("the requested resource was not found")]
    NotFound,
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("the idempotency key conflicts with an earlier request")]
    IdempotencyConflict,
    #[error("the resource changed after it was read")]
    PreconditionFailed,
    #[error("a required service is unavailable")]
    Unavailable,
    #[error("an internal error occurred")]
    Internal,
}

impl From<DomainError> for ApplicationError {
    fn from(error: DomainError) -> Self {
        Self::Validation(error.to_string())
    }
}

impl From<StoreError> for ApplicationError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::IdempotencyConflict => Self::IdempotencyConflict,
            StoreError::Unavailable(_) => Self::Unavailable,
            StoreError::Internal(_) => Self::Internal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug)]
    struct FixedClock(DateTime<Utc>);
    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    #[derive(Debug, Default)]
    struct FakeStore {
        calls: Mutex<usize>,
        last_audit: Mutex<Option<OrderAuditIntent>>,
    }
    #[async_trait]
    impl PlaceOrderPort for FakeStore {
        async fn place_order(
            &self,
            transaction: PlaceOrderTransaction,
        ) -> Result<PlaceOrderResult, StoreError> {
            *self.calls.lock().expect("test lock") += 1;
            *self.last_audit.lock().expect("test lock") = transaction.audit.clone();
            Ok(PlaceOrderResult {
                order: transaction.order,
                replayed: false,
            })
        }
    }

    fn command() -> PlaceOrderCommand {
        PlaceOrderCommand {
            customer_reference: "PO-42".into(),
            lines: vec![PlaceOrderLine {
                sku: "SKU-1".into(),
                quantity: 2,
            }],
        }
    }

    #[tokio::test]
    async fn forbidden_command_never_reaches_store() {
        let store = Arc::new(FakeStore::default());
        let clock = Arc::new(FixedClock(Utc::now()));
        let use_case = PlaceOrder::new(Arc::clone(&store), clock);
        let actor = Actor::service("user", Vec::<String>::new());
        assert_eq!(
            use_case.execute(&actor, command(), "key-1").await,
            Err(ApplicationError::Forbidden)
        );
        assert_eq!(*store.calls.lock().expect("test lock"), 0);
    }

    #[tokio::test]
    async fn valid_command_creates_order() {
        let store = Arc::new(FakeStore::default());
        let clock = Arc::new(FixedClock(Utc::now()));
        let use_case = PlaceOrder::new(Arc::clone(&store), clock);
        let actor = Actor::service("user", ["orders.create".to_owned()]);
        let correlation_id = Uuid::now_v7();
        let result = use_case
            .execute_correlated(&actor, command(), "key-1", correlation_id)
            .await
            .expect("place order");
        assert!(!result.replayed);
        assert_eq!(result.order.customer_reference.as_str(), "PO-42");
        let audit = store
            .last_audit
            .lock()
            .expect("test lock")
            .clone()
            .expect("audit intent");
        assert_eq!(audit.action, "order.created");
        assert_eq!(audit.actor_subject, "user");
        assert_eq!(audit.correlation_id, correlation_id);
        assert_eq!(audit.resource_revision, 1);
        assert!(matches!(
            audit.changes["customer_reference"].after,
            Some(OrderAuditValue::Digest(_))
        ));
    }

    #[tokio::test]
    async fn empty_updates_fail_before_the_update_port_is_called() {
        #[derive(Debug, Default)]
        struct FakeUpdatePort {
            calls: Mutex<usize>,
        }
        #[async_trait]
        impl UpdateOrderPort for FakeUpdatePort {
            async fn get_order_for_update(
                &self,
                _id: OrderId,
            ) -> Result<Option<Order>, StoreError> {
                *self.calls.lock().expect("test lock") += 1;
                Ok(None)
            }

            async fn save_order(
                &self,
                _order: Order,
                _expected_revision: u64,
            ) -> Result<ConditionalResult<Order>, StoreError> {
                *self.calls.lock().expect("test lock") += 1;
                Ok(ConditionalResult::NotFound)
            }
        }

        let store = Arc::new(FakeUpdatePort::default());
        let use_case = UpdateOrder::new(Arc::clone(&store), Arc::new(FixedClock(Utc::now())));
        let actor = Actor::service("user", ["orders.update".to_owned()]);
        let result = use_case
            .execute(
                &actor,
                OrderId::new(),
                1,
                UpdateOrderCommand {
                    customer_reference: None,
                    lines: None,
                },
            )
            .await;

        assert!(matches!(result, Err(ApplicationError::Validation(_))));
        assert_eq!(*store.calls.lock().expect("test lock"), 0);
    }

    #[tokio::test]
    async fn audit_history_requires_its_own_permission_before_reading() {
        #[derive(Debug, Default)]
        struct FakeHistory {
            calls: Mutex<usize>,
        }
        #[async_trait]
        impl ListOrderAuditHistoryPort for FakeHistory {
            async fn list_order_audit_history(
                &self,
                _query: ListOrderAuditHistoryQuery,
            ) -> Result<OrderAuditPage, StoreError> {
                *self.calls.lock().expect("test lock") += 1;
                Ok(OrderAuditPage {
                    events: Vec::new(),
                    next_cursor: None,
                })
            }
        }

        let store = Arc::new(FakeHistory::default());
        let use_case = ListOrderAuditHistory::new(Arc::clone(&store));
        let query = ListOrderAuditHistoryQuery {
            order_id: OrderId::new(),
            limit: 50,
            after: None,
            direction: OrderAuditSortDirection::NewestFirst,
        };
        let read_only = Actor::service("user", ["orders.read".to_owned()]);
        assert_eq!(
            use_case.execute(&read_only, query.clone()).await,
            Err(ApplicationError::Forbidden)
        );
        assert_eq!(*store.calls.lock().expect("test lock"), 0);

        let auditor = Actor::service("auditor", ["orders.audit.read".to_owned()]);
        assert_eq!(
            use_case.execute(&auditor, query).await.unwrap(),
            OrderAuditPage {
                events: Vec::new(),
                next_cursor: None,
            }
        );
        assert_eq!(*store.calls.lock().expect("test lock"), 1);
    }
}
