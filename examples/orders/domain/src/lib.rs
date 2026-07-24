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
}

impl Order {
    pub fn new(
        customer_reference: CustomerReference,
        lines: Vec<OrderLine>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        if lines.is_empty() {
            return Err(DomainError::OrderHasNoLines);
        }
        if lines.len() > 100 {
            return Err(DomainError::TooManyOrderLines(lines.len()));
        }
        let mut skus = std::collections::BTreeSet::new();
        for line in &lines {
            if !skus.insert(line.sku.as_str()) {
                return Err(DomainError::DuplicateSku(line.sku.as_str().to_owned()));
            }
        }
        Ok(Self {
            id: OrderId::new(),
            customer_reference,
            lines,
            status: OrderStatus::Accepted,
            created_at,
        })
    }
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
}
