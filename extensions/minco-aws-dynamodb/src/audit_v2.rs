use async_trait::async_trait;
use aws_sdk_dynamodb::types::{AttributeValue, KeysAndAttributes, Put, TransactWriteItem};
use chrono::{Datelike as _, SecondsFormat};
use minco_plugin_audit::{
    AuditAppendReport, AuditCursor, AuditLedgerError, AuditLedgerWriter, AuditLifecyclePolicy,
    AuditPage, AuditQuery, AuditReader, AuditRecordV2, AuditResourceRef, AuditSegmentState,
    AuditSegmentStatus, AuditStorageHealth, AuditStorageInspector, AuditStorageSnapshot,
    evaluate_storage_health,
};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use uuid::Uuid;

const PARTITION_KEY: &str = "pk";
const SORT_KEY: &str = "sk";
const ENTITY: &str = "entity";
const RECORD: &str = "record";
const EVENT_ID: &str = "event_id";
const OCCURRED_AT: &str = "occurred_at";
const ENCODED_BYTES: &str = "encoded_bytes";
const CANONICAL_ENTITY: &str = "audit_event";
const PROJECTION_ENTITY: &str = "audit_projection";
const MAX_TRANSACTION_ITEMS: usize = 100;
const MAX_QUERY_PAGES: usize = 128;
const QUERY_PAGE_SIZE: i32 = 100;

#[derive(Clone)]
pub struct DynamoDbAuditLedger {
    client: aws_sdk_dynamodb::Client,
    table_name: String,
    lifecycle: AuditLifecyclePolicy,
}

impl std::fmt::Debug for DynamoDbAuditLedger {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DynamoDbAuditLedger")
            .field("client", &"[REDACTED PROVIDER]")
            .field("table_name", &"[REDACTED]")
            .field("lifecycle", &self.lifecycle)
            .finish()
    }
}

impl DynamoDbAuditLedger {
    pub fn new(
        client: aws_sdk_dynamodb::Client,
        table_name: impl Into<String>,
        lifecycle: AuditLifecyclePolicy,
    ) -> Result<Self, AuditLedgerError> {
        let table_name = table_name.into();
        if !super::valid_table_name(&table_name) {
            return Err(AuditLedgerError::InvalidLifecycle(
                "DynamoDB audit table name is invalid".into(),
            ));
        }
        lifecycle.validate()?;
        Ok(Self {
            client,
            table_name,
            lifecycle,
        })
    }

    pub fn from_provider(
        provider: &super::DynamoDbProvider,
        audit_table_name: impl Into<String>,
    ) -> Result<Self, AuditLedgerError> {
        let audit_table_name = audit_table_name.into();
        if audit_table_name == provider.table_name() {
            return Err(AuditLedgerError::InvalidLifecycle(
                "DynamoDB audit ledger requires a distinct table".into(),
            ));
        }
        Self::new(
            provider.client().clone(),
            audit_table_name,
            AuditLifecyclePolicy::cloud_online(),
        )
    }

    #[must_use]
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// Builds immutable audit puts to combine with application-owned source
    /// mutations in one `TransactWriteItems` request.
    pub fn transact_items(
        &self,
        record: &AuditRecordV2,
    ) -> Result<Vec<TransactWriteItem>, AuditLedgerError> {
        let prepared = prepare_record(&self.table_name, record)?;
        Ok(prepared.items)
    }

    async fn existing_records(
        &self,
        records: &BTreeMap<Uuid, AuditRecordV2>,
    ) -> Result<BTreeMap<Uuid, String>, AuditLedgerError> {
        if records.is_empty() {
            return Ok(BTreeMap::new());
        }
        let keys = records
            .keys()
            .copied()
            .map(canonical_key)
            .collect::<Vec<_>>();
        let request = KeysAndAttributes::builder()
            .set_keys(Some(keys))
            .consistent_read(true)
            .projection_expression("#event_id, #record")
            .expression_attribute_names("#event_id", EVENT_ID)
            .expression_attribute_names("#record", RECORD)
            .build()
            .map_err(|_| AuditLedgerError::Infrastructure)?;
        let mut request = Some(request);
        let mut existing = BTreeMap::new();
        for attempt in 0..4 {
            let output = self
                .client
                .batch_get_item()
                .request_items(
                    self.table_name.clone(),
                    request.take().expect("request exists for bounded retry"),
                )
                .send()
                .await
                .map_err(infrastructure)?;
            if let Some(items) = output
                .responses()
                .and_then(|responses| responses.get(&self.table_name))
            {
                for item in items {
                    let event_id = uuid_attribute(item, EVENT_ID)?;
                    let record = string_attribute(item, RECORD)?.to_owned();
                    existing.insert(event_id, record);
                }
            }
            request = output
                .unprocessed_keys()
                .and_then(|unprocessed| unprocessed.get(&self.table_name))
                .cloned();
            if request.as_ref().is_none_or(|value| value.keys().is_empty()) {
                return Ok(existing);
            }
            if attempt < 3 {
                tokio::time::sleep(std::time::Duration::from_millis(5 << attempt)).await;
            }
        }
        Err(AuditLedgerError::Infrastructure)
    }

    async fn append_new(
        &self,
        records: &[(AuditRecordV2, String)],
    ) -> Result<(), AuditLedgerError> {
        let items = prepare_transaction(&self.table_name, records)?;
        let token = transaction_token(records);
        let result = self
            .client
            .transact_write_items()
            .set_transact_items(Some(items))
            .client_request_token(token)
            .send()
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(error)
                if error.as_service_error().is_some_and(|service| {
                    service.is_transaction_canceled_exception()
                        || service.is_idempotent_parameter_mismatch_exception()
                }) =>
            {
                Err(AuditLedgerError::JournalClaimLost)
            }
            Err(_) => Err(AuditLedgerError::Infrastructure),
        }
    }
}

fn prepare_transaction(
    table_name: &str,
    records: &[(AuditRecordV2, String)],
) -> Result<Vec<TransactWriteItem>, AuditLedgerError> {
    let mut items = Vec::new();
    let mut item_bytes = 0usize;
    for (record, _) in records {
        let prepared = prepare_record(table_name, record)?;
        item_bytes = item_bytes
            .checked_add(prepared.item_bytes)
            .ok_or_else(|| AuditLedgerError::InvalidBatch("item bytes overflow".into()))?;
        items.extend(prepared.items);
    }
    if items.is_empty() || items.len() > MAX_TRANSACTION_ITEMS {
        return Err(AuditLedgerError::InvalidBatch(format!(
            "DynamoDB transaction requires 1 to {MAX_TRANSACTION_ITEMS} items"
        )));
    }
    if item_bytes > minco_plugin_audit::MAX_AUDIT_BATCH_BYTES {
        return Err(AuditLedgerError::BatchTooLarge {
            bytes: item_bytes,
            maximum: minco_plugin_audit::MAX_AUDIT_BATCH_BYTES,
        });
    }
    Ok(items)
}

#[async_trait]
impl AuditLedgerWriter for DynamoDbAuditLedger {
    async fn append_batch(
        &self,
        records: &[AuditRecordV2],
    ) -> Result<AuditAppendReport, AuditLedgerError> {
        let prepared = prepare_batch(records)?;
        let existing = self.existing_records(&prepared).await?;
        let mut duplicates = records.len().saturating_sub(prepared.len());
        let mut new = Vec::new();
        for (event_id, record) in &prepared {
            let encoded = encode_record(record)?;
            match existing.get(event_id) {
                Some(value) if value == &encoded => duplicates += 1,
                Some(_) => return Err(AuditLedgerError::EventConflict(*event_id)),
                None => new.push((record.clone(), encoded)),
            }
        }
        if new.is_empty() {
            return Ok(AuditAppendReport {
                requested: records.len(),
                inserted: 0,
                duplicates,
            });
        }
        match self.append_new(&new).await {
            Ok(()) => Ok(AuditAppendReport {
                requested: records.len(),
                inserted: new.len(),
                duplicates,
            }),
            Err(AuditLedgerError::JournalClaimLost) => {
                let raced = new
                    .iter()
                    .map(|(record, _)| (record.event_id, record.clone()))
                    .collect::<BTreeMap<_, _>>();
                let existing = self.existing_records(&raced).await?;
                for (record, encoded) in &new {
                    match existing.get(&record.event_id) {
                        Some(value) if value == encoded => {}
                        Some(_) => return Err(AuditLedgerError::EventConflict(record.event_id)),
                        None => return Err(AuditLedgerError::Infrastructure),
                    }
                }
                Ok(AuditAppendReport {
                    requested: records.len(),
                    inserted: 0,
                    duplicates: duplicates + new.len(),
                })
            }
            Err(error) => Err(error),
        }
    }
}

#[async_trait]
impl AuditReader for DynamoDbAuditLedger {
    async fn list_resource_history(
        &self,
        query: &AuditQuery,
    ) -> Result<AuditPage, AuditLedgerError> {
        query.validate()?;
        let partition = resource_partition(&query.tenant_scope, &query.resource);
        let mut exclusive_start_key = None;
        let mut records = Vec::with_capacity(query.limit + 1);
        let mut seen = BTreeSet::new();
        for page in 0..MAX_QUERY_PAGES {
            let mut request = self
                .client
                .query()
                .table_name(&self.table_name)
                .key_condition_expression(if query.after.is_some() {
                    match query.direction {
                        minco_plugin_audit::AuditSortDirection::OldestFirst => {
                            "#pk = :pk AND #sk > :after"
                        }
                        minco_plugin_audit::AuditSortDirection::NewestFirst => {
                            "#pk = :pk AND #sk < :after"
                        }
                    }
                } else {
                    "#pk = :pk"
                })
                .expression_attribute_names("#pk", PARTITION_KEY)
                .expression_attribute_values(":pk", AttributeValue::S(partition.clone()))
                .scan_index_forward(matches!(
                    query.direction,
                    minco_plugin_audit::AuditSortDirection::OldestFirst
                ))
                .consistent_read(true)
                .limit(QUERY_PAGE_SIZE)
                .set_exclusive_start_key(exclusive_start_key);
            if let Some(after) = query.after {
                request = request
                    .expression_attribute_names("#sk", SORT_KEY)
                    .expression_attribute_values(
                        ":after",
                        AttributeValue::S(cursor_sort_key(after)?),
                    );
            }
            let output = request.send().await.map_err(infrastructure)?;
            for item in output.items() {
                let record = decode_record(item)?;
                if matches_query(&record, query) && seen.insert(record.event_id) {
                    records.push(record);
                    if records.len() > query.limit {
                        break;
                    }
                }
            }
            exclusive_start_key = output.last_evaluated_key().cloned();
            if records.len() > query.limit || exclusive_start_key.is_none() {
                break;
            }
            if page + 1 == MAX_QUERY_PAGES {
                return Err(AuditLedgerError::Infrastructure);
            }
        }
        let has_more = records.len() > query.limit;
        records.truncate(query.limit);
        let next_cursor = has_more.then(|| {
            records
                .last()
                .map(AuditCursor::from)
                .expect("positive validated query limit")
        });
        Ok(AuditPage {
            records,
            next_cursor,
        })
    }
}

#[async_trait]
impl AuditStorageInspector for DynamoDbAuditLedger {
    async fn storage_health(&self) -> Result<AuditStorageHealth, AuditLedgerError> {
        let output = self
            .client
            .describe_table()
            .table_name(&self.table_name)
            .send()
            .await
            .map_err(infrastructure)?;
        let table = output.table().ok_or(AuditLedgerError::Infrastructure)?;
        let hot_bytes = u64::try_from(table.table_size_bytes().unwrap_or(0))
            .map_err(|_| AuditLedgerError::Infrastructure)?;
        let item_count = u64::try_from(table.item_count().unwrap_or(0))
            .map_err(|_| AuditLedgerError::Infrastructure)?;
        evaluate_storage_health(
            self.lifecycle,
            AuditStorageSnapshot {
                provider: "dynamodb".into(),
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
                    record_count: item_count,
                    encoded_bytes: hot_bytes,
                    first: None,
                    last: None,
                    archive_receipt: None,
                }],
            },
        )
    }
}

struct PreparedRecord {
    items: Vec<TransactWriteItem>,
    item_bytes: usize,
}

fn prepare_record(
    table_name: &str,
    record: &AuditRecordV2,
) -> Result<PreparedRecord, AuditLedgerError> {
    let encoded_bytes = record.validate()?;
    if !(0..=9999).contains(&record.occurred_at.year()) {
        return Err(AuditLedgerError::InvalidRecord(
            "DynamoDB audit timestamps require years 0000 through 9999".into(),
        ));
    }
    let encoded = encode_record(record)?;
    let mut raw_items = vec![canonical_item(record, &encoded, encoded_bytes)];
    let mut resources = BTreeSet::from([record.resource.clone()]);
    resources.extend(
        record
            .related_resources
            .iter()
            .map(|related| related.resource.clone()),
    );
    raw_items.extend(
        resources
            .iter()
            .map(|resource| projection_item(record, resource, &encoded, encoded_bytes))
            .collect::<Result<Vec<_>, _>>()?,
    );
    let item_bytes = raw_items.iter().try_fold(0usize, |total, item| {
        total
            .checked_add(dynamo_item_bytes(item))
            .ok_or_else(|| AuditLedgerError::InvalidBatch("item bytes overflow".into()))
    })?;
    let items = raw_items
        .into_iter()
        .map(|item| immutable_put(table_name, item))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreparedRecord { items, item_bytes })
}

fn prepare_batch(
    records: &[AuditRecordV2],
) -> Result<BTreeMap<Uuid, AuditRecordV2>, AuditLedgerError> {
    if records.is_empty() || records.len() > minco_plugin_audit::MAX_AUDIT_BATCH_RECORDS {
        return Err(AuditLedgerError::InvalidBatch(
            "invalid record count".into(),
        ));
    }
    let mut prepared = BTreeMap::new();
    for record in records {
        record.validate()?;
        if let Some(existing) = prepared.insert(record.event_id, record.clone())
            && existing != *record
        {
            return Err(AuditLedgerError::EventConflict(record.event_id));
        }
    }
    Ok(prepared)
}

fn canonical_item(
    record: &AuditRecordV2,
    encoded: &str,
    encoded_bytes: usize,
) -> HashMap<String, AttributeValue> {
    let mut item = canonical_key(record.event_id);
    item.insert(ENTITY.into(), AttributeValue::S(CANONICAL_ENTITY.into()));
    item.insert(
        EVENT_ID.into(),
        AttributeValue::S(record.event_id.to_string()),
    );
    item.insert(RECORD.into(), AttributeValue::S(encoded.into()));
    item.insert(
        OCCURRED_AT.into(),
        AttributeValue::S(timestamp(record.occurred_at)),
    );
    item.insert(
        ENCODED_BYTES.into(),
        AttributeValue::N(encoded_bytes.to_string()),
    );
    item
}

fn projection_item(
    record: &AuditRecordV2,
    resource: &AuditResourceRef,
    encoded: &str,
    encoded_bytes: usize,
) -> Result<HashMap<String, AttributeValue>, AuditLedgerError> {
    let mut item = HashMap::new();
    item.insert(
        PARTITION_KEY.into(),
        AttributeValue::S(resource_partition(&record.tenant_scope, resource)),
    );
    item.insert(
        SORT_KEY.into(),
        AttributeValue::S(cursor_sort_key(AuditCursor::from(record))?),
    );
    item.insert(ENTITY.into(), AttributeValue::S(PROJECTION_ENTITY.into()));
    item.insert(
        EVENT_ID.into(),
        AttributeValue::S(record.event_id.to_string()),
    );
    item.insert(RECORD.into(), AttributeValue::S(encoded.into()));
    item.insert(
        OCCURRED_AT.into(),
        AttributeValue::S(timestamp(record.occurred_at)),
    );
    item.insert(
        ENCODED_BYTES.into(),
        AttributeValue::N(encoded_bytes.to_string()),
    );
    Ok(item)
}

fn immutable_put(
    table_name: &str,
    item: HashMap<String, AttributeValue>,
) -> Result<TransactWriteItem, AuditLedgerError> {
    let put = Put::builder()
        .table_name(table_name)
        .set_item(Some(item))
        .condition_expression("attribute_not_exists(#pk)")
        .expression_attribute_names("#pk", PARTITION_KEY)
        .build()
        .map_err(|_| AuditLedgerError::Infrastructure)?;
    Ok(TransactWriteItem::builder().put(put).build())
}

fn canonical_key(event_id: Uuid) -> HashMap<String, AttributeValue> {
    HashMap::from([
        (
            PARTITION_KEY.into(),
            AttributeValue::S(format!("EVENT#{event_id}")),
        ),
        (SORT_KEY.into(), AttributeValue::S("EVENT".into())),
    ])
}

fn resource_partition(tenant_scope: &str, resource: &AuditResourceRef) -> String {
    let mut hasher = Sha256::new();
    hash_component(&mut hasher, tenant_scope.as_bytes());
    hash_component(&mut hasher, resource.resource_type.as_bytes());
    hash_component(&mut hasher, resource.resource_id.as_bytes());
    format!("RESOURCE#{:x}", hasher.finalize())
}

fn hash_component(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

fn cursor_sort_key(cursor: AuditCursor) -> Result<String, AuditLedgerError> {
    if !(0..=9999).contains(&cursor.occurred_at.year()) {
        return Err(AuditLedgerError::InvalidQuery(
            "DynamoDB audit cursor timestamp is out of range".into(),
        ));
    }
    Ok(format!(
        "{}#{}",
        timestamp(cursor.occurred_at),
        cursor.event_id
    ))
}

fn timestamp(value: chrono::DateTime<chrono::Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Nanos, true)
}

fn encode_record(record: &AuditRecordV2) -> Result<String, AuditLedgerError> {
    serde_json::to_string(record).map_err(|_| AuditLedgerError::Encoding)
}

fn decode_record(
    item: &HashMap<String, AttributeValue>,
) -> Result<AuditRecordV2, AuditLedgerError> {
    let record: AuditRecordV2 = serde_json::from_str(string_attribute(item, RECORD)?)
        .map_err(|_| AuditLedgerError::Encoding)?;
    record.validate()?;
    Ok(record)
}

fn matches_query(record: &AuditRecordV2, query: &AuditQuery) -> bool {
    record.tenant_scope == query.tenant_scope
        && (record.resource == query.resource
            || (query.include_related
                && record.related_resources.iter().any(|related| {
                    related.resource == query.resource
                        && query
                            .relation
                            .as_ref()
                            .is_none_or(|relation| relation == &related.relation)
                })))
}

fn string_attribute<'a>(
    item: &'a HashMap<String, AttributeValue>,
    name: &str,
) -> Result<&'a str, AuditLedgerError> {
    item.get(name)
        .and_then(|value| value.as_s().ok())
        .map(String::as_str)
        .ok_or(AuditLedgerError::Infrastructure)
}

fn uuid_attribute(
    item: &HashMap<String, AttributeValue>,
    name: &str,
) -> Result<Uuid, AuditLedgerError> {
    Uuid::parse_str(string_attribute(item, name)?).map_err(|_| AuditLedgerError::Infrastructure)
}

fn dynamo_item_bytes(item: &HashMap<String, AttributeValue>) -> usize {
    item.iter()
        .map(|(name, value)| {
            name.len()
                + match value {
                    AttributeValue::S(value) | AttributeValue::N(value) => value.len(),
                    AttributeValue::Bool(_) | AttributeValue::Null(_) => 1,
                    AttributeValue::B(value) => value.as_ref().len(),
                    AttributeValue::Ss(values) | AttributeValue::Ns(values) => {
                        values.iter().map(String::len).sum()
                    }
                    AttributeValue::Bs(values) => {
                        values.iter().map(|value| value.as_ref().len()).sum()
                    }
                    AttributeValue::L(values) => values.len(),
                    AttributeValue::M(values) => values.len(),
                    _ => 0,
                }
        })
        .sum()
}

fn transaction_token(records: &[(AuditRecordV2, String)]) -> String {
    let mut hasher = Sha256::new();
    for (record, encoded) in records {
        hasher.update(record.event_id.as_bytes());
        hash_component(&mut hasher, encoded.as_bytes());
    }
    format!("{:x}", hasher.finalize())[..36].to_owned()
}

fn infrastructure(_: impl std::fmt::Display) -> AuditLedgerError {
    AuditLedgerError::Infrastructure
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeDelta};
    use minco_plugin_audit::{AuditActor, AuditRelatedResource, AuditSortDirection};

    fn record(index: u32) -> AuditRecordV2 {
        let mut record = AuditRecordV2::new(
            "tenant-private",
            "order.status_changed",
            AuditResourceRef::new("order", format!("order-{index}")),
            AuditActor::human("subject"),
            "updateOrder",
            Uuid::from_u128(100 + u128::from(index)),
        );
        record.event_id = Uuid::from_u128(1_000 + u128::from(index));
        record.occurred_at = DateTime::from_timestamp(1_800_000_000 + i64::from(index), 0).unwrap();
        record.recorded_at = record.occurred_at + TimeDelta::seconds(1);
        record
    }

    #[test]
    fn transaction_items_are_bounded_immutable_and_hide_resource_keys() {
        let mut record = record(1);
        record.related_resources.extend([
            AuditRelatedResource {
                relation: "customer".into(),
                resource: AuditResourceRef::new("customer", "customer-secret"),
            },
            AuditRelatedResource {
                relation: "order".into(),
                resource: record.resource.clone(),
            },
        ]);
        let prepared = prepare_record("audit-table", &record).unwrap();
        assert_eq!(prepared.items.len(), 3);
        for item in &prepared.items {
            let put = item.put().unwrap();
            let partition = put.item()[PARTITION_KEY].as_s().unwrap();
            assert!(!partition.contains("customer-secret"));
            assert!(!partition.contains("tenant-private"));
            assert_eq!(
                put.condition_expression(),
                Some("attribute_not_exists(#pk)")
            );
        }
        assert!(prepared.item_bytes > record.validate().unwrap());
    }

    #[test]
    fn transaction_preflight_rejects_projection_fanout_over_provider_limit() {
        let mut records = Vec::new();
        for index in 0..11 {
            let mut item = record(index);
            for related in 0..8 {
                item.related_resources.push(AuditRelatedResource {
                    relation: format!("relation-{related}"),
                    resource: AuditResourceRef::new("related", format!("{index}-{related}")),
                });
            }
            records.push((item.clone(), encode_record(&item).unwrap()));
        }
        assert!(matches!(
            prepare_transaction("audit-table", &records),
            Err(AuditLedgerError::InvalidBatch(_))
        ));
    }

    #[test]
    fn resource_hash_and_cursor_order_are_stable() {
        let first = record(1);
        let second = record(2);
        let partition = resource_partition(&first.tenant_scope, &first.resource);
        assert_eq!(partition.len(), "RESOURCE#".len() + 64);
        assert!(!partition.contains(&first.resource.resource_id));
        assert!(
            cursor_sort_key(AuditCursor::from(&first)).unwrap()
                < cursor_sort_key(AuditCursor::from(&second)).unwrap()
        );
    }

    #[test]
    fn direct_and_relation_queries_are_explicit() {
        let mut item = record(1);
        let parent = AuditResourceRef::new("customer", "one");
        item.related_resources.push(AuditRelatedResource {
            relation: "customer".into(),
            resource: parent.clone(),
        });
        let mut query = AuditQuery::for_resource("tenant-private", parent);
        assert!(!matches_query(&item, &query));
        query.include_related = true;
        assert!(matches_query(&item, &query));
        query.relation = Some("other".into());
        assert!(!matches_query(&item, &query));
        query.direction = AuditSortDirection::OldestFirst;
        assert_eq!(query.direction, AuditSortDirection::OldestFirst);
    }
}
