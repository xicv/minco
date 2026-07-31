//! Business invariants for the Minco orders reference application.
#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderId(Uuid);

impl OrderId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn into_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for OrderId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CustomerReference(String);

impl CustomerReference {
    pub fn parse(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into().trim().to_owned();
        if value.is_empty() || value.chars().count() > 64 || value.chars().any(char::is_control) {
            return Err(DomainError::InvalidCustomerReference);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sku(String);

impl Sku {
    pub fn parse(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into().trim().to_owned();
        if value.is_empty() || value.chars().count() > 80 || value.chars().any(char::is_control) {
            return Err(DomainError::InvalidSku);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Quantity(u32);

impl Quantity {
    pub fn new(value: i64) -> Result<Self, DomainError> {
        if !(1..=1_000).contains(&value) {
            return Err(DomainError::InvalidQuantity(value));
        }
        u32::try_from(value)
            .map(Self)
            .map_err(|_| DomainError::InvalidQuantity(value))
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderLine {
    pub sku: Sku,
    pub quantity: Quantity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Accepted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub customer_reference: CustomerReference,
    pub lines: Vec<OrderLine>,
    pub status: OrderStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
}

impl Order {
    pub fn new(
        customer_reference: CustomerReference,
        lines: Vec<OrderLine>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        validate_lines(&lines)?;
        Ok(Self {
            id: OrderId::new(),
            customer_reference,
            lines,
            status: OrderStatus::Accepted,
            created_at,
            updated_at: created_at,
            revision: 1,
        })
    }

    pub fn update(
        &mut self,
        customer_reference: Option<CustomerReference>,
        lines: Option<Vec<OrderLine>>,
        updated_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        if customer_reference.is_none() && lines.is_none() {
            return Err(DomainError::OrderUpdateHasNoChanges);
        }
        if updated_at < self.updated_at {
            return Err(DomainError::OrderUpdateMovesBackward);
        }
        if let Some(lines) = lines.as_ref() {
            validate_lines(lines)?;
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DomainError::OrderRevisionExhausted)?;
        if let Some(customer_reference) = customer_reference {
            self.customer_reference = customer_reference;
        }
        if let Some(lines) = lines {
            self.lines = lines;
        }
        self.updated_at = updated_at;
        self.revision = revision;
        Ok(())
    }
}

fn validate_lines(lines: &[OrderLine]) -> Result<(), DomainError> {
    if lines.is_empty() {
        return Err(DomainError::OrderHasNoLines);
    }
    if lines.len() > 100 {
        return Err(DomainError::TooManyOrderLines(lines.len()));
    }
    let mut skus = std::collections::BTreeSet::new();
    for line in lines {
        if !skus.insert(line.sku.as_str()) {
            return Err(DomainError::DuplicateSku(line.sku.as_str().to_owned()));
        }
    }
    Ok(())
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("customer reference must contain 1 to 64 visible characters")]
    InvalidCustomerReference,
    #[error("SKU must contain 1 to 80 visible characters")]
    InvalidSku,
    #[error("quantity {0} is outside the supported range 1..=1000")]
    InvalidQuantity(i64),
    #[error("an order must contain at least one line")]
    OrderHasNoLines,
    #[error("an order cannot contain more than 100 lines; received {0}")]
    TooManyOrderLines(usize),
    #[error("SKU {0} occurs more than once")]
    DuplicateSku(String),
    #[error("an order update must change at least one mutable field")]
    OrderUpdateHasNoChanges,
    #[error("an order update timestamp cannot move backward")]
    OrderUpdateMovesBackward,
    #[error("the order revision cannot be incremented")]
    OrderRevisionExhausted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_skus() {
        let reference = CustomerReference::parse("PO-42").expect("valid reference");
        let sku = Sku::parse("SKU-1").expect("valid SKU");
        let lines = vec![
            OrderLine {
                sku: sku.clone(),
                quantity: Quantity::new(1).expect("valid quantity"),
            },
            OrderLine {
                sku,
                quantity: Quantity::new(2).expect("valid quantity"),
            },
        ];
        let result = Order::new(reference, lines, Utc::now());
        assert!(matches!(result, Err(DomainError::DuplicateSku(_))));
    }

    #[test]
    fn rejects_control_characters() {
        assert_eq!(
            CustomerReference::parse("PO\n42"),
            Err(DomainError::InvalidCustomerReference)
        );
    }

    #[test]
    fn updates_increment_the_revision_and_require_a_real_change() {
        let created_at = Utc::now();
        let mut order = Order::new(
            CustomerReference::parse("PO-42").expect("reference"),
            vec![OrderLine {
                sku: Sku::parse("SKU-1").expect("sku"),
                quantity: Quantity::new(1).expect("quantity"),
            }],
            created_at,
        )
        .expect("order");

        assert_eq!(order.revision, 1);
        assert_eq!(
            order.update(None, None, created_at),
            Err(DomainError::OrderUpdateHasNoChanges)
        );
        order
            .update(
                Some(CustomerReference::parse("PO-43").expect("reference")),
                None,
                created_at + chrono::Duration::seconds(1),
            )
            .expect("update");
        assert_eq!(order.customer_reference.as_str(), "PO-43");
        assert_eq!(order.revision, 2);
        assert!(order.updated_at > order.created_at);
    }
}
