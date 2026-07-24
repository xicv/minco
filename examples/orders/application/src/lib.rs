//! Use cases and use-case-shaped ports for the orders reference application.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use orders_domain::{CustomerReference, DomainError, Order, OrderId, OrderLine, Quantity, Sku};
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
    pub fn service(subject: impl Into<String>, permissions: impl IntoIterator<Item = String>) -> Self {
        Self { subject: subject.into(), permissions: permissions.into_iter().collect() }
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
pub trait OrderStore: Send + Sync {
    async fn place_order(&self, transaction: PlaceOrderTransaction) -> Result<PlaceOrderResult, StoreError>;
    async fn get_order(&self, id: OrderId) -> Result<Option<Order>, StoreError>;
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
    S: OrderStore + ?Sized,
    C: Clock + ?Sized,
{
    #[must_use]
    pub fn new(store: Arc<S>, clock: Arc<C>) -> Self {
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
        let lines = command
            .lines
            .iter()
            .map(|line| {
                Ok(OrderLine {
                    sku: Sku::parse(line.sku.clone())?,
                    quantity: Quantity::new(line.quantity)?,
                })
            })
            .collect::<Result<Vec<_>, DomainError>>()?;
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
    S: OrderStore + ?Sized,
{
    #[must_use]
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, actor: &Actor, id: OrderId) -> Result<Order, ApplicationError> {
        if !actor.has_permission("orders.read") {
            return Err(ApplicationError::Forbidden);
        }
        self.store.get_order(id).await?.ok_or(ApplicationError::NotFound)
    }
}

fn validate_idempotency_key(value: &str) -> Result<String, ApplicationError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control) {
        return Err(ApplicationError::Validation("Idempotency-Key must contain 1 to 200 visible characters".into()));
    }
    Ok(value.to_owned())
}

fn request_fingerprint(actor: &Actor, command: &PlaceOrderCommand) -> Result<String, ApplicationError> {
    let canonical = serde_json::to_vec(&(actor.subject.as_str(), command)).map_err(|_| ApplicationError::Internal)?;
    let mut hasher = Sha256::new();
    hasher.update(canonical);
    Ok(format!("{:x}", hasher.finalize()))
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
    impl Clock for FixedClock { fn now(&self) -> DateTime<Utc> { self.0 } }

    #[derive(Debug, Default)]
    struct FakeStore { calls: Mutex<usize> }
    #[async_trait]
    impl OrderStore for FakeStore {
        async fn place_order(&self, transaction: PlaceOrderTransaction) -> Result<PlaceOrderResult, StoreError> {
            *self.calls.lock().expect("test lock") += 1;
            Ok(PlaceOrderResult { order: transaction.order, replayed: false })
        }
        async fn get_order(&self, _id: OrderId) -> Result<Option<Order>, StoreError> { Ok(None) }
        async fn ready(&self) -> bool { true }
    }

    fn command() -> PlaceOrderCommand {
        PlaceOrderCommand { customer_reference: "PO-42".into(), lines: vec![PlaceOrderLine { sku: "SKU-1".into(), quantity: 2 }] }
    }

    #[tokio::test]
    async fn forbidden_command_never_reaches_store() {
        let store = Arc::new(FakeStore::default());
        let clock = Arc::new(FixedClock(Utc::now()));
        let use_case = PlaceOrder::new(Arc::clone(&store), clock);
        let actor = Actor::service("user", Vec::<String>::new());
        assert_eq!(use_case.execute(&actor, command(), "key-1").await, Err(ApplicationError::Forbidden));
        assert_eq!(*store.calls.lock().expect("test lock"), 0);
    }

    #[tokio::test]
    async fn valid_command_creates_order() {
        let store = Arc::new(FakeStore::default());
        let clock = Arc::new(FixedClock(Utc::now()));
        let use_case = PlaceOrder::new(store, clock);
        let actor = Actor::service("user", ["orders.create".to_owned()]);
        let result = use_case.execute(&actor, command(), "key-1").await.expect("place order");
        assert!(!result.replayed);
        assert_eq!(result.order.customer_reference.as_str(), "PO-42");
    }
}
