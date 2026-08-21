use async_trait::async_trait;
use minco_plugin_audit::{
    AuditAppendReport, AuditCursor, AuditLedgerError, AuditLedgerWriter, AuditLifecyclePolicy,
    AuditPage, AuditQuery, AuditReader, AuditRecordV2, AuditSegmentState, AuditSegmentStatus,
    AuditStorageHealth, AuditStorageInspector, AuditStorageSnapshot, evaluate_storage_health,
};
use orders_application::{
    ConditionalResult, DeleteOrderPort, GetOrderPort, ListOrderAuditHistoryPort,
    ListOrderAuditHistoryQuery, ListOrdersPort, ListOrdersQuery, OrderAuditCursor,
    OrderAuditIntent, OrderAuditPage, OrderAuditSortDirection, OrderConfirmationJob, OrderCursor,
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
    audit: BTreeMap<uuid::Uuid, AuditRecordV2>,
    confirmation_jobs: Vec<OrderConfirmationJob>,
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

    /// Confirmation job intents recorded by `place_order`, in commit order.
    pub fn confirmation_jobs(&self) -> Vec<OrderConfirmationJob> {
        self.state
            .lock()
            .map(|state| state.confirmation_jobs.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl PlaceOrderPort for MemoryOrderStore {
    async fn place_order(
        &self,
        transaction: PlaceOrderTransaction,
    ) -> Result<PlaceOrderResult, StoreError> {
        let audit = transaction
            .audit
            .as_ref()
            .map(crate::audit::audit_record)
            .transpose()?;
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
        if let Some(audit) = &audit {
            ensure_audit_available(&state, audit)?;
        }
        let order_id = transaction.order.id;
        state
            .orders
            .insert(order_id.into_uuid(), transaction.order.clone());
        if let Some(confirmation) = &transaction.confirmation_job {
            state.confirmation_jobs.push(confirmation.clone());
        }
        state.idempotency.insert(
            transaction.idempotency_key,
            IdempotencyRecord {
                fingerprint: transaction.request_fingerprint,
                response: transaction.order.clone(),
            },
        );
        if let Some(audit) = audit {
            insert_audit(&mut state, audit)?;
        }
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

    async fn save_order_with_audit(
        &self,
        order: Order,
        expected_revision: u64,
        audit: OrderAuditIntent,
    ) -> Result<ConditionalResult<Order>, StoreError> {
        let audit = crate::audit::audit_record(&audit)?;
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
        insert_audit(&mut state, audit)?;
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

    async fn delete_order_with_audit(
        &self,
        id: OrderId,
        expected_revision: u64,
        _deleted_at: chrono::DateTime<chrono::Utc>,
        audit: OrderAuditIntent,
    ) -> Result<ConditionalResult<()>, StoreError> {
        let audit = crate::audit::audit_record(&audit)?;
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
        insert_audit(&mut state, audit)?;
        state.deleted.insert(id);
        drop(state);
        Ok(ConditionalResult::Applied(()))
    }
}

#[async_trait]
impl ListOrderAuditHistoryPort for MemoryOrderStore {
    async fn list_order_audit_history(
        &self,
        query: ListOrderAuditHistoryQuery,
    ) -> Result<OrderAuditPage, StoreError> {
        let order_id = query.order_id.into_uuid().to_string();
        let mut records = self
            .state
            .lock()
            .map_err(|_| StoreError::Internal("memory store lock was poisoned".into()))?
            .audit
            .values()
            .filter(|record| {
                record.resource.resource_type == "order" && record.resource.resource_id == order_id
            })
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| match query.direction {
            OrderAuditSortDirection::OldestFirst => {
                (left.occurred_at, left.event_id).cmp(&(right.occurred_at, right.event_id))
            }
            OrderAuditSortDirection::NewestFirst => {
                (right.occurred_at, right.event_id).cmp(&(left.occurred_at, left.event_id))
            }
        });
        if let Some(after) = query.after {
            records.retain(|record| match query.direction {
                OrderAuditSortDirection::OldestFirst => {
                    (record.occurred_at, record.event_id) > (after.occurred_at, after.event_id)
                }
                OrderAuditSortDirection::NewestFirst => {
                    (record.occurred_at, record.event_id) < (after.occurred_at, after.event_id)
                }
            });
        }
        let limit = usize::from(query.limit);
        let has_more = records.len() > limit;
        records.truncate(limit);
        let next_cursor =
            has_more
                .then(|| records.last())
                .flatten()
                .map(|record| OrderAuditCursor {
                    occurred_at: record.occurred_at,
                    event_id: record.event_id,
                });
        Ok(OrderAuditPage {
            events: records.into_iter().map(crate::audit::order_event).collect(),
            next_cursor,
        })
    }
}

#[async_trait]
impl AuditLedgerWriter for MemoryOrderStore {
    async fn append_batch(
        &self,
        records: &[AuditRecordV2],
    ) -> Result<AuditAppendReport, AuditLedgerError> {
        if records.is_empty() || records.len() > minco_plugin_audit::MAX_AUDIT_BATCH_RECORDS {
            return Err(AuditLedgerError::InvalidBatch(
                "invalid record count".into(),
            ));
        }
        for record in records {
            record.validate()?;
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| AuditLedgerError::Infrastructure)?;
        for record in records {
            if let Some(existing) = state.audit.get(&record.event_id)
                && existing != record
            {
                return Err(AuditLedgerError::EventConflict(record.event_id));
            }
        }
        let mut inserted = 0;
        for record in records {
            if state
                .audit
                .insert(record.event_id, record.clone())
                .is_none()
            {
                inserted += 1;
            }
        }
        Ok(AuditAppendReport {
            requested: records.len(),
            inserted,
            duplicates: records.len() - inserted,
        })
    }
}

#[async_trait]
impl AuditReader for MemoryOrderStore {
    async fn list_resource_history(
        &self,
        query: &AuditQuery,
    ) -> Result<AuditPage, AuditLedgerError> {
        query.validate()?;
        let mut records = self
            .state
            .lock()
            .map_err(|_| AuditLedgerError::Infrastructure)?
            .audit
            .values()
            .filter(|record| {
                record.tenant_scope == query.tenant_scope
                    && ((record.resource == query.resource)
                        || (query.include_related
                            && record.related_resources.iter().any(|related| {
                                related.resource == query.resource
                                    && query
                                        .relation
                                        .as_ref()
                                        .is_none_or(|relation| relation == &related.relation)
                            })))
            })
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            let ordering =
                (left.occurred_at, left.event_id).cmp(&(right.occurred_at, right.event_id));
            match query.direction {
                minco_plugin_audit::AuditSortDirection::OldestFirst => ordering,
                minco_plugin_audit::AuditSortDirection::NewestFirst => ordering.reverse(),
            }
        });
        if let Some(after) = query.after {
            records.retain(|record| {
                let position = (record.occurred_at, record.event_id);
                let cursor = (after.occurred_at, after.event_id);
                match query.direction {
                    minco_plugin_audit::AuditSortDirection::OldestFirst => position > cursor,
                    minco_plugin_audit::AuditSortDirection::NewestFirst => position < cursor,
                }
            });
        }
        let has_more = records.len() > query.limit;
        records.truncate(query.limit);
        let next_cursor = has_more
            .then(|| records.last())
            .flatten()
            .map(AuditCursor::from);
        Ok(AuditPage {
            records,
            next_cursor,
        })
    }
}

#[async_trait]
impl AuditStorageInspector for MemoryOrderStore {
    async fn storage_health(&self) -> Result<AuditStorageHealth, AuditLedgerError> {
        let state = self
            .state
            .lock()
            .map_err(|_| AuditLedgerError::Infrastructure)?;
        let mut records = state.audit.values().collect::<Vec<_>>();
        records.sort_by_key(|record| (record.occurred_at, record.event_id));
        let hot_bytes = records.iter().try_fold(0_u64, |total, record| {
            let bytes =
                u64::try_from(record.validate()?).map_err(|_| AuditLedgerError::Encoding)?;
            total.checked_add(bytes).ok_or(AuditLedgerError::Encoding)
        })?;
        let snapshot = AuditStorageSnapshot {
            provider: "memory".into(),
            hot_bytes,
            free_bytes: None,
            pending_records: 0,
            pending_bytes: 0,
            oldest_pending_seconds: None,
            quarantined_records: 0,
            archive_watermark: None,
            segments: vec![AuditSegmentStatus {
                segment_id: 1,
                state: AuditSegmentState::Active,
                record_count: u64::try_from(records.len())
                    .map_err(|_| AuditLedgerError::Encoding)?,
                encoded_bytes: hot_bytes,
                first: records.first().map(|record| AuditCursor::from(*record)),
                last: records.last().map(|record| AuditCursor::from(*record)),
                archive_receipt: None,
            }],
        };
        drop(state);
        evaluate_storage_health(AuditLifecyclePolicy::cloud_online(), snapshot)
    }
}

#[async_trait]
impl OrderReadiness for MemoryOrderStore {
    async fn ready(&self) -> bool {
        self.state.lock().is_ok()
    }
}

fn insert_audit(state: &mut MemoryState, record: AuditRecordV2) -> Result<(), StoreError> {
    match state.audit.get(&record.event_id) {
        Some(existing) if existing == &record => Ok(()),
        Some(_) => Err(StoreError::Internal("audit event ID conflict".into())),
        None => {
            state.audit.insert(record.event_id, record);
            Ok(())
        }
    }
}

fn ensure_audit_available(state: &MemoryState, record: &AuditRecordV2) -> Result<(), StoreError> {
    match state.audit.get(&record.event_id) {
        Some(existing) if existing == record => Ok(()),
        Some(_) => Err(StoreError::Internal("audit event ID conflict".into())),
        None => Ok(()),
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
            audit: None,
            confirmation_job: None,
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
