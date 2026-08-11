//! Use cases and use-case-shaped ports for the orders reference application.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use orders_domain::{
    CustomerReference, DomainError, Order, OrderId, OrderLine, OrderStatus, Quantity, Sku,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::Arc};
use thiserror::Error;

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
}

#[async_trait]
pub trait DeleteOrderPort: Send + Sync {
    async fn delete_order(
        &self,
        id: OrderId,
        expected_revision: u64,
        deleted_at: DateTime<Utc>,
    ) -> Result<ConditionalResult<()>, StoreError>;
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
        if !actor.has_permission("orders.create") {
            return Err(ApplicationError::Forbidden);
        }
        let idempotency_key = validate_idempotency_key(idempotency_key)?;
        let customer_reference = CustomerReference::parse(command.customer_reference.clone())?;
        let lines = parse_lines(&command.lines)?;
        let fingerprint = request_fingerprint(actor, &command)?;
        let order = Order::new(customer_reference, lines, self.clock.now())?;
        self.store
            .place_order(PlaceOrderTransaction {
                order,
                idempotency_key,
                request_fingerprint: fingerprint,
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
        if !actor.has_permission("orders.update") {
            return Err(ApplicationError::Forbidden);
        }
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
        order.update(customer_reference, lines, self.clock.now())?;
        match self.store.save_order(order, expected_revision).await? {
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
        if !actor.has_permission("orders.delete") {
            return Err(ApplicationError::Forbidden);
        }
        match self
            .store
            .delete_order(id, expected_revision, self.clock.now())
            .await?
        {
            ConditionalResult::Applied(()) => Ok(()),
            ConditionalResult::NotFound => Err(ApplicationError::NotFound),
            ConditionalResult::PreconditionFailed => Err(ApplicationError::PreconditionFailed),
        }
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
    }
    #[async_trait]
    impl PlaceOrderPort for FakeStore {
        async fn place_order(
            &self,
            transaction: PlaceOrderTransaction,
        ) -> Result<PlaceOrderResult, StoreError> {
            *self.calls.lock().expect("test lock") += 1;
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
        let use_case = PlaceOrder::new(store, clock);
        let actor = Actor::service("user", ["orders.create".to_owned()]);
        let result = use_case
            .execute(&actor, command(), "key-1")
            .await
            .expect("place order");
        assert!(!result.replayed);
        assert_eq!(result.order.customer_reference.as_str(), "PO-42");
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
}
