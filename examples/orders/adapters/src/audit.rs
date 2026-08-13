use async_trait::async_trait;
use minco_core::DataClass;
use minco_plugin_audit::{
    AuditActor, AuditActorKind, AuditCursor, AuditDigest, AuditFieldChange, AuditLedgerError,
    AuditPage, AuditQuery, AuditReader, AuditRecordV2, AuditResourceRef, AuditSortDirection,
    AuditValue,
};
use orders_application::{
    ListOrderAuditHistoryPort, ListOrderAuditHistoryQuery, ORDERS_AUDIT_TENANT_SCOPE,
    OrderAuditActorKind, OrderAuditChange, OrderAuditCursor, OrderAuditEvent, OrderAuditIntent,
    OrderAuditPage, OrderAuditSortDirection, OrderAuditValue, StoreError,
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
pub struct OrderAuditReader {
    reader: Arc<dyn AuditReader>,
}

impl std::fmt::Debug for OrderAuditReader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OrderAuditReader")
            .finish_non_exhaustive()
    }
}

impl OrderAuditReader {
    #[must_use]
    pub fn new(reader: Arc<dyn AuditReader>) -> Self {
        Self { reader }
    }
}

#[async_trait]
impl ListOrderAuditHistoryPort for OrderAuditReader {
    async fn list_order_audit_history(
        &self,
        query: ListOrderAuditHistoryQuery,
    ) -> Result<OrderAuditPage, StoreError> {
        let mut ledger_query = AuditQuery::for_resource(
            ORDERS_AUDIT_TENANT_SCOPE,
            AuditResourceRef::new("order", query.order_id.into_uuid().to_string()),
        );
        ledger_query.limit = usize::from(query.limit);
        ledger_query.after = query.after.map(|cursor| AuditCursor {
            occurred_at: cursor.occurred_at,
            event_id: cursor.event_id,
        });
        ledger_query.direction = match query.direction {
            OrderAuditSortDirection::OldestFirst => AuditSortDirection::OldestFirst,
            OrderAuditSortDirection::NewestFirst => AuditSortDirection::NewestFirst,
        };
        self.reader
            .list_resource_history(&ledger_query)
            .await
            .map(order_page)
            .map_err(audit_error)
    }
}

pub(crate) fn audit_record(intent: &OrderAuditIntent) -> Result<AuditRecordV2, StoreError> {
    let mut record = AuditRecordV2::new(
        ORDERS_AUDIT_TENANT_SCOPE,
        intent.action.clone(),
        AuditResourceRef::new("order", intent.order_id.into_uuid().to_string()),
        AuditActor::human(intent.actor_subject.clone()),
        intent.operation_id.clone(),
        intent.correlation_id,
    );
    record.event_id = intent.event_id;
    record.resource_revision = Some(intent.resource_revision);
    record.occurred_at = intent.occurred_at;
    record.recorded_at = intent.occurred_at;
    record.data_class = DataClass::Personal;
    record.idempotency_key_digest = intent
        .idempotency_key_digest
        .as_ref()
        .map(|value| AuditDigest::sha256(value.clone()));
    record.changes = intent
        .changes
        .iter()
        .map(|(field, change)| {
            (
                field.clone(),
                AuditFieldChange {
                    before: change.before.as_ref().map(audit_value),
                    after: change.after.as_ref().map(audit_value),
                },
            )
        })
        .collect();
    record.validate().map_err(audit_error)?;
    Ok(record)
}

fn audit_value(value: &OrderAuditValue) -> AuditValue {
    match value {
        OrderAuditValue::Literal(value) => AuditValue::literal(value.clone()),
        OrderAuditValue::Digest(value) => AuditValue::Digest {
            digest: AuditDigest::sha256(value.clone()),
        },
        OrderAuditValue::Redacted => AuditValue::Redacted,
        OrderAuditValue::Omitted => AuditValue::Omitted,
    }
}

fn order_page(page: AuditPage) -> OrderAuditPage {
    OrderAuditPage {
        events: page.records.into_iter().map(order_event).collect(),
        next_cursor: page.next_cursor.map(|cursor| OrderAuditCursor {
            occurred_at: cursor.occurred_at,
            event_id: cursor.event_id,
        }),
    }
}

pub(crate) fn order_event(record: AuditRecordV2) -> OrderAuditEvent {
    OrderAuditEvent {
        event_id: record.event_id,
        action: record.action,
        resource_revision: record.resource_revision,
        actor_kind: match record.actor.kind {
            AuditActorKind::Human => OrderAuditActorKind::Human,
            AuditActorKind::Service => OrderAuditActorKind::Service,
            AuditActorKind::System => OrderAuditActorKind::System,
            AuditActorKind::Migration => OrderAuditActorKind::Migration,
            AuditActorKind::DatabasePrincipal => OrderAuditActorKind::DatabasePrincipal,
            AuditActorKind::Unknown => OrderAuditActorKind::Unknown,
        },
        actor_subject: record.actor.subject,
        operation_id: record.operation_id,
        correlation_id: record.correlation_id,
        occurred_at: record.occurred_at,
        recorded_at: record.recorded_at,
        changes: record
            .changes
            .into_iter()
            .map(|(field, change)| {
                (
                    field,
                    OrderAuditChange {
                        before: change.before.map(order_value),
                        after: change.after.map(order_value),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
    }
}

fn order_value(value: AuditValue) -> OrderAuditValue {
    match value {
        AuditValue::Literal { value } => OrderAuditValue::Literal(match value {
            serde_json::Value::String(value) => value,
            value => value.to_string(),
        }),
        AuditValue::Digest { digest } => OrderAuditValue::Digest(digest.value),
        AuditValue::Redacted => OrderAuditValue::Redacted,
        AuditValue::Omitted => OrderAuditValue::Omitted,
    }
}

pub(crate) fn audit_error(error: AuditLedgerError) -> StoreError {
    let permanent = error.is_permanent();
    drop(error);
    if permanent {
        StoreError::Internal("audit ledger rejected the semantic action".into())
    } else {
        StoreError::Unavailable("audit ledger is unavailable".into())
    }
}
