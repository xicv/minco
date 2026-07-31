use async_trait::async_trait;
use orders_application::{
    ConditionalResult, DeleteOrderPort, GetOrderPort, ListOrdersPort, ListOrdersQuery, OrderCursor,
    OrderPage, OrderReadiness, OrderSortField, OrderSortTerm, PlaceOrderPort, PlaceOrderResult,
    PlaceOrderTransaction, SortDirection, StoreError, UpdateOrderPort,
};
use orders_domain::{Order, OrderId};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

#[derive(Debug, Clone)]
struct IdempotencyRecord {
    fingerprint: String,
    response: Order,
}

#[derive(Debug, Default)]
struct MemoryState {
    orders: BTreeMap<uuid::Uuid, Order>,
    idempotency: BTreeMap<String, IdempotencyRecord>,
    deleted: BTreeSet<uuid::Uuid>,
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
impl PlaceOrderPort for MemoryOrderStore {
    async fn place_order(
        &self,
        transaction: PlaceOrderTransaction,
    ) -> Result<PlaceOrderResult, StoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StoreError::Internal("memory store lock was poisoned".into()))?;
        if let Some(existing) = state.idempotency.get(&transaction.idempotency_key) {
            if existing.fingerprint != transaction.request_fingerprint {
                return Err(StoreError::IdempotencyConflict);
            }
            return Ok(PlaceOrderResult {
                order: existing.response.clone(),
                replayed: true,
            });
        }
        let order_id = transaction.order.id;
        state
            .orders
            .insert(order_id.into_uuid(), transaction.order.clone());
        state.idempotency.insert(
            transaction.idempotency_key,
            IdempotencyRecord {
                fingerprint: transaction.request_fingerprint,
                response: transaction.order.clone(),
            },
        );
        let result = PlaceOrderResult {
            order: transaction.order,
            replayed: false,
        };
        drop(state);
        Ok(result)
    }
}

#[async_trait]
impl GetOrderPort for MemoryOrderStore {
    async fn get_order(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| StoreError::Internal("memory store lock was poisoned".into()))?;
        if state.deleted.contains(&id.into_uuid()) {
            return Ok(None);
        }
        Ok(state.orders.get(&id.into_uuid()).cloned())
    }
}

#[async_trait]
impl ListOrdersPort for MemoryOrderStore {
    async fn list_orders(&self, query: ListOrdersQuery) -> Result<OrderPage, StoreError> {
        let sort = normalized_sort(&query.sort);
        let mut orders = {
            let state = self
                .state
                .lock()
                .map_err(|_| StoreError::Internal("memory store lock was poisoned".into()))?;
            state
                .orders
                .iter()
                .filter(|(id, _)| !state.deleted.contains(id))
                .map(|(_, order)| order.clone())
                .collect::<Vec<_>>()
        };
        orders.sort_by(|left, right| compare_orders(left, right, &sort));
        if let Some(after) = query.after {
            orders.retain(|order| compare_order_cursor(order, &after, &sort) == Ordering::Greater);
        }
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
impl UpdateOrderPort for MemoryOrderStore {
    async fn get_order_for_update(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        GetOrderPort::get_order(self, id).await
    }

    async fn save_order(
        &self,
        order: Order,
        expected_revision: u64,
    ) -> Result<ConditionalResult<Order>, StoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StoreError::Internal("memory store lock was poisoned".into()))?;
        let id = order.id.into_uuid();
        if state.deleted.contains(&id) {
            return Ok(ConditionalResult::NotFound);
        }
        let Some(current) = state.orders.get(&id) else {
            return Ok(ConditionalResult::NotFound);
        };
        if current.revision != expected_revision {
            return Ok(ConditionalResult::PreconditionFailed);
        }
        state.orders.insert(id, order.clone());
        drop(state);
        Ok(ConditionalResult::Applied(order))
    }
}

#[async_trait]
impl DeleteOrderPort for MemoryOrderStore {
    async fn delete_order(
        &self,
        id: OrderId,
        expected_revision: u64,
        _deleted_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<ConditionalResult<()>, StoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StoreError::Internal("memory store lock was poisoned".into()))?;
        let id = id.into_uuid();
        if state.deleted.contains(&id) {
            return Ok(ConditionalResult::NotFound);
        }
        let Some(order) = state.orders.get(&id) else {
            return Ok(ConditionalResult::NotFound);
        };
        if order.revision != expected_revision {
            return Ok(ConditionalResult::PreconditionFailed);
        }
        state.deleted.insert(id);
        drop(state);
        Ok(ConditionalResult::Applied(()))
    }
}

#[async_trait]
impl OrderReadiness for MemoryOrderStore {
    async fn ready(&self) -> bool {
        self.state.lock().is_ok()
    }
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

fn compare_orders(left: &Order, right: &Order, sort: &[OrderSortTerm]) -> Ordering {
    sort.iter()
        .map(|term| {
            let ordering = match term.field {
                OrderSortField::CreatedAt => left.created_at.cmp(&right.created_at),
                OrderSortField::Id => left.id.into_uuid().cmp(&right.id.into_uuid()),
            };
            match term.direction {
                SortDirection::Ascending => ordering,
                SortDirection::Descending => ordering.reverse(),
            }
        })
        .find(|ordering| *ordering != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

fn compare_order_cursor(order: &Order, cursor: &OrderCursor, sort: &[OrderSortTerm]) -> Ordering {
    sort.iter()
        .map(|term| {
            let ordering = match term.field {
                OrderSortField::CreatedAt => order.created_at.cmp(&cursor.created_at),
                OrderSortField::Id => order.id.into_uuid().cmp(&cursor.id.into_uuid()),
            };
            match term.direction {
                SortDirection::Ascending => ordering,
                SortDirection::Descending => ordering.reverse(),
            }
        })
        .find(|ordering| *ordering != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use orders_domain::{CustomerReference, OrderLine, Quantity, Sku};

    fn transaction(key: &str, fingerprint: &str) -> PlaceOrderTransaction {
        let order = Order::new(
            CustomerReference::parse("PO-1").expect("reference"),
            vec![OrderLine {
                sku: Sku::parse("SKU-1").expect("sku"),
                quantity: Quantity::new(1).expect("quantity"),
            }],
            Utc::now(),
        )
        .expect("order");
        PlaceOrderTransaction {
            order,
            idempotency_key: key.into(),
            request_fingerprint: fingerprint.into(),
        }
    }

    #[tokio::test]
    async fn replays_the_original_result() {
        let store = MemoryOrderStore::new();
        let first = store
            .place_order(transaction("key", "fingerprint"))
            .await
            .expect("first");
        assert_eq!(
            store
                .delete_order(first.order.id, first.order.revision, Utc::now())
                .await,
            Ok(ConditionalResult::Applied(()))
        );
        let second = store
            .place_order(transaction("key", "fingerprint"))
            .await
            .expect("second");
        assert!(!first.replayed);
        assert!(second.replayed);
        assert_eq!(first.order, second.order);
    }

    #[tokio::test]
    async fn rejects_reused_keys_with_different_fingerprints() {
        let store = MemoryOrderStore::new();
        store
            .place_order(transaction("key", "one"))
            .await
            .expect("first");
        let result = store.place_order(transaction("key", "two")).await;
        assert!(matches!(result, Err(StoreError::IdempotencyConflict)));
    }
}
