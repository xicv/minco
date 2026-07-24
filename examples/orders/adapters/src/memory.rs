use async_trait::async_trait;
use orders_application::{OrderStore, PlaceOrderResult, PlaceOrderTransaction, StoreError};
use orders_domain::{Order, OrderId};
use std::{collections::BTreeMap, sync::Mutex};

#[derive(Debug, Clone)]
struct IdempotencyRecord {
    fingerprint: String,
    order_id: OrderId,
}

#[derive(Debug, Default)]
struct MemoryState {
    orders: BTreeMap<uuid::Uuid, Order>,
    idempotency: BTreeMap<String, IdempotencyRecord>,
}

#[derive(Debug, Default)]
pub struct MemoryOrderStore {
    state: Mutex<MemoryState>,
}

impl MemoryOrderStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl OrderStore for MemoryOrderStore {
    async fn place_order(&self, transaction: PlaceOrderTransaction) -> Result<PlaceOrderResult, StoreError> {
        let mut state = self.state.lock().map_err(|_| StoreError::Internal("memory store lock was poisoned".into()))?;
        if let Some(existing) = state.idempotency.get(&transaction.idempotency_key) {
            if existing.fingerprint != transaction.request_fingerprint {
                return Err(StoreError::IdempotencyConflict);
            }
            let order = state
                .orders
                .get(&existing.order_id.into_uuid())
                .cloned()
                .ok_or_else(|| StoreError::Internal("idempotency record references a missing order".into()))?;
            return Ok(PlaceOrderResult { order, replayed: true });
        }
        let order_id = transaction.order.id;
        state.orders.insert(order_id.into_uuid(), transaction.order.clone());
        state.idempotency.insert(
            transaction.idempotency_key,
            IdempotencyRecord { fingerprint: transaction.request_fingerprint, order_id },
        );
        Ok(PlaceOrderResult { order: transaction.order, replayed: false })
    }

    async fn get_order(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        let state = self.state.lock().map_err(|_| StoreError::Internal("memory store lock was poisoned".into()))?;
        Ok(state.orders.get(&id.into_uuid()).cloned())
    }

    async fn ready(&self) -> bool {
        self.state.lock().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use orders_domain::{CustomerReference, OrderLine, Quantity, Sku};

    fn transaction(key: &str, fingerprint: &str) -> PlaceOrderTransaction {
        let order = Order::new(
            CustomerReference::parse("PO-1").expect("reference"),
            vec![OrderLine { sku: Sku::parse("SKU-1").expect("sku"), quantity: Quantity::new(1).expect("quantity") }],
            Utc::now(),
        )
        .expect("order");
        PlaceOrderTransaction { order, idempotency_key: key.into(), request_fingerprint: fingerprint.into() }
    }

    #[tokio::test]
    async fn replays_the_original_result() {
        let store = MemoryOrderStore::new();
        let first = store.place_order(transaction("key", "fingerprint")).await.expect("first");
        let second = store.place_order(transaction("key", "fingerprint")).await.expect("second");
        assert!(!first.replayed);
        assert!(second.replayed);
        assert_eq!(first.order.id, second.order.id);
    }

    #[tokio::test]
    async fn rejects_reused_keys_with_different_fingerprints() {
        let store = MemoryOrderStore::new();
        store.place_order(transaction("key", "one")).await.expect("first");
        let result = store.place_order(transaction("key", "two")).await;
        assert!(matches!(result, Err(StoreError::IdempotencyConflict)));
    }
}
