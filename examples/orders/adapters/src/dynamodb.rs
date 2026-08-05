use async_trait::async_trait;
use aws_sdk_dynamodb::types::{AttributeValue, Put, TransactWriteItem};
use chrono::{DateTime, SecondsFormat, Utc};
use futures::{StreamExt as _, TryStreamExt as _, stream};
use minco_aws_dynamodb::DynamoDbProvider;
use orders_application::{
    ConditionalResult, DeleteOrderPort, GetOrderPort, ListOrdersPort, ListOrdersQuery, OrderCursor,
    OrderPage, OrderReadiness, OrderSortField, OrderSortTerm, PlaceOrderPort, PlaceOrderResult,
    PlaceOrderTransaction, SortDirection, StoreError, UpdateOrderPort,
};
use orders_domain::{CustomerReference, Order, OrderId, Sku};
use sha2::{Digest, Sha256};
use std::{cmp::Ordering, collections::HashMap, fmt::Write as _};

const PARTITION_KEY: &str = "pk";
const SORT_KEY: &str = "sk";
const ENTITY: &str = "entity";
const PAYLOAD: &str = "payload";
const REVISION: &str = "revision";
const STATUS: &str = "status";
const CREATED_AT: &str = "created_at";
const UPDATED_AT: &str = "updated_at";
const DELETED_AT: &str = "deleted_at";
const FINGERPRINT: &str = "request_fingerprint";
const GSI1_PK: &str = "gsi1pk";
const GSI1_SK: &str = "gsi1sk";
const GSI2_PK: &str = "gsi2pk";
const GSI2_SK: &str = "gsi2sk";
const GSI3_PK: &str = "gsi3pk";
const GSI3_SK: &str = "gsi3sk";
const GSI_CREATED_AT: &str = "orders-by-created-at";
const GSI_CREATED_AT_INVERTED_ID: &str = "orders-by-created-at-inverted-id";
const GSI_ID: &str = "orders-by-id";
const LIST_SHARDS: u8 = 16;
const MAX_QUERY_PAGES_PER_SHARD: usize = 128;
const MAX_CONCURRENT_SHARD_QUERIES: usize = 8;

#[derive(Clone)]
pub struct DynamoDbOrderStore {
    provider: DynamoDbProvider,
}

impl std::fmt::Debug for DynamoDbOrderStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DynamoDbOrderStore")
            .field("provider", &"[REDACTED]")
            .finish()
    }
}

impl DynamoDbOrderStore {
    #[must_use]
    pub const fn new(provider: DynamoDbProvider) -> Self {
        Self { provider }
    }

    const fn client(&self) -> &aws_sdk_dynamodb::Client {
        self.provider.client()
    }

    fn table_name(&self) -> &str {
        self.provider.table_name()
    }

    async fn replay(
        &self,
        idempotency_key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<PlaceOrderResult>, StoreError> {
        let output = self
            .client()
            .get_item()
            .table_name(self.table_name())
            .set_key(Some(idempotency_item_key(idempotency_key)))
            .consistent_read(true)
            .send()
            .await
            .map_err(|_| unavailable("DynamoDB GetItem failed"))?;
        let Some(item) = output.item() else {
            return Ok(None);
        };
        if string_attribute(item, ENTITY)? != "idempotency" {
            return Err(malformed_item());
        }
        if string_attribute(item, FINGERPRINT)? != request_fingerprint {
            return Err(StoreError::IdempotencyConflict);
        }
        let order: Order =
            serde_json::from_str(string_attribute(item, PAYLOAD)?).map_err(|_| malformed_item())?;
        validate_order(&order)?;
        Ok(Some(PlaceOrderResult {
            order,
            replayed: true,
        }))
    }

    async fn classify_condition<T>(&self, id: OrderId) -> Result<ConditionalResult<T>, StoreError> {
        Ok(if self.get_order(id).await?.is_some() {
            ConditionalResult::PreconditionFailed
        } else {
            ConditionalResult::NotFound
        })
    }

    async fn query_shard(
        &self,
        shard: u8,
        query: &ListOrdersQuery,
        plan: &DynamoSortPlan,
        target: usize,
    ) -> Result<Vec<Order>, StoreError> {
        let mut orders = Vec::new();
        let mut exclusive_start_key = None;
        for _ in 0..MAX_QUERY_PAGES_PER_SHARD {
            let mut request = self
                .client()
                .query()
                .table_name(self.table_name())
                .index_name(plan.index_name)
                .key_condition_expression(if plan.after_key.is_some() {
                    if plan.scan_forward {
                        "#gpk = :gpk AND #gsk > :after"
                    } else {
                        "#gpk = :gpk AND #gsk < :after"
                    }
                } else {
                    "#gpk = :gpk"
                })
                .expression_attribute_names("#gpk", plan.partition_attribute)
                .expression_attribute_names("#entity", ENTITY)
                .expression_attribute_names("#deleted", DELETED_AT)
                .expression_attribute_values(
                    ":gpk",
                    AttributeValue::S(format!("ORDERS#{shard:02}")),
                )
                .expression_attribute_values(":order", AttributeValue::S("order".into()))
                .filter_expression("#entity = :order AND attribute_not_exists(#deleted)")
                .scan_index_forward(plan.scan_forward)
                .limit(100)
                .set_exclusive_start_key(exclusive_start_key);
            if let Some(after_key) = &plan.after_key {
                request = request
                    .expression_attribute_names("#gsk", plan.sort_attribute)
                    .expression_attribute_values(":after", AttributeValue::S(after_key.clone()));
            }
            if let Some(status) = query.status {
                request = request
                    .expression_attribute_names("#status", STATUS)
                    .expression_attribute_values(
                        ":status",
                        AttributeValue::S(status_name(status).into()),
                    )
                    .filter_expression(
                        "#entity = :order AND attribute_not_exists(#deleted) AND #status = :status",
                    );
            }
            let output = request
                .send()
                .await
                .map_err(|_| unavailable("DynamoDB Query failed"))?;
            for item in output.items() {
                orders.push(decode_order_item(item)?);
                if orders.len() >= target {
                    return Ok(orders);
                }
            }
            exclusive_start_key = output.last_evaluated_key().cloned();
            if exclusive_start_key.is_none() {
                return Ok(orders);
            }
        }
        Err(unavailable("DynamoDB Query pagination bound reached"))
    }
}

#[async_trait]
impl PlaceOrderPort for DynamoDbOrderStore {
    async fn place_order(
        &self,
        transaction: PlaceOrderTransaction,
    ) -> Result<PlaceOrderResult, StoreError> {
        validate_order(&transaction.order)?;
        let order_item = order_item(&transaction.order)?;
        let idempotency_item = idempotency_item(&transaction)?;
        let order_put = Put::builder()
            .table_name(self.table_name())
            .set_item(Some(order_item))
            .condition_expression("attribute_not_exists(#pk)")
            .expression_attribute_names("#pk", PARTITION_KEY)
            .build()
            .map_err(|_| StoreError::Internal("invalid DynamoDB order transaction".into()))?;
        let idempotency_put = Put::builder()
            .table_name(self.table_name())
            .set_item(Some(idempotency_item))
            .condition_expression("attribute_not_exists(#pk)")
            .expression_attribute_names("#pk", PARTITION_KEY)
            .build()
            .map_err(|_| StoreError::Internal("invalid DynamoDB idempotency transaction".into()))?;
        let items = vec![
            TransactWriteItem::builder().put(order_put).build(),
            TransactWriteItem::builder().put(idempotency_put).build(),
        ];
        let token = transaction_token(
            &transaction.idempotency_key,
            &transaction.request_fingerprint,
        );
        match self
            .client()
            .transact_write_items()
            .set_transact_items(Some(items))
            .client_request_token(token)
            .send()
            .await
        {
            Ok(_) => Ok(PlaceOrderResult {
                order: transaction.order,
                replayed: false,
            }),
            Err(error)
                if error.as_service_error().is_some_and(|service| {
                    service.is_transaction_canceled_exception()
                        || service.is_idempotent_parameter_mismatch_exception()
                }) =>
            {
                for attempt in 0..4 {
                    if let Some(result) = self
                        .replay(
                            &transaction.idempotency_key,
                            &transaction.request_fingerprint,
                        )
                        .await?
                    {
                        return Ok(result);
                    }
                    if attempt < 3 {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                }
                Err(unavailable("DynamoDB transaction conflict did not settle"))
            }
            Err(_) => Err(unavailable("DynamoDB TransactWriteItems failed")),
        }
    }
}

#[async_trait]
impl GetOrderPort for DynamoDbOrderStore {
    async fn get_order(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        let output = self
            .client()
            .get_item()
            .table_name(self.table_name())
            .set_key(Some(order_key(id)))
            .consistent_read(true)
            .send()
            .await
            .map_err(|_| unavailable("DynamoDB GetItem failed"))?;
        output
            .item()
            .filter(|item| !item.contains_key(DELETED_AT))
            .map(decode_order_item)
            .transpose()
    }
}

#[async_trait]
impl ListOrdersPort for DynamoDbOrderStore {
    async fn list_orders(&self, query: ListOrdersQuery) -> Result<OrderPage, StoreError> {
        let plan = DynamoSortPlan::new(&query)?;
        let target = usize::from(query.limit) + 1;
        let shard_pages = stream::iter(0..LIST_SHARDS)
            .map(|shard| self.query_shard(shard, &query, &plan, target))
            .buffer_unordered(MAX_CONCURRENT_SHARD_QUERIES)
            .try_collect::<Vec<_>>()
            .await?;
        let mut orders = Vec::with_capacity(target.saturating_mul(usize::from(LIST_SHARDS)));
        orders.extend(shard_pages.into_iter().flatten());
        orders.sort_by(|left, right| compare_orders(left, right, &query.sort));
        let has_more = orders.len() > usize::from(query.limit);
        orders.truncate(usize::from(query.limit));
        let next_cursor = has_more && !orders.is_empty();
        Ok(OrderPage {
            next_cursor: next_cursor.then(|| {
                let order = orders.last().expect("checked non-empty");
                OrderCursor {
                    created_at: order.created_at,
                    id: order.id,
                }
            }),
            orders,
        })
    }
}

#[async_trait]
impl UpdateOrderPort for DynamoDbOrderStore {
    async fn get_order_for_update(&self, id: OrderId) -> Result<Option<Order>, StoreError> {
        self.get_order(id).await
    }

    async fn save_order(
        &self,
        order: Order,
        expected_revision: u64,
    ) -> Result<ConditionalResult<Order>, StoreError> {
        validate_order(&order)?;
        if order.revision != expected_revision.checked_add(1).unwrap_or(0) {
            return Err(StoreError::Internal(
                "DynamoDB update revision transition is invalid".into(),
            ));
        }
        let payload = serialize_order(&order)?;
        let indexes = index_attributes(&order);
        let result = self
            .client()
            .update_item()
            .table_name(self.table_name())
            .set_key(Some(order_key(order.id)))
            .update_expression(
                "SET #payload = :payload, #revision = :next, #updated = :updated, #status = :status, #g1pk = :g1pk, #g1sk = :g1sk, #g2pk = :g2pk, #g2sk = :g2sk, #g3pk = :g3pk, #g3sk = :g3sk",
            )
            .condition_expression(
                "attribute_exists(#pk) AND attribute_not_exists(#deleted) AND #revision = :expected",
            )
            .expression_attribute_names("#pk", PARTITION_KEY)
            .expression_attribute_names("#payload", PAYLOAD)
            .expression_attribute_names("#revision", REVISION)
            .expression_attribute_names("#updated", UPDATED_AT)
            .expression_attribute_names("#status", STATUS)
            .expression_attribute_names("#deleted", DELETED_AT)
            .expression_attribute_names("#g1pk", GSI1_PK)
            .expression_attribute_names("#g1sk", GSI1_SK)
            .expression_attribute_names("#g2pk", GSI2_PK)
            .expression_attribute_names("#g2sk", GSI2_SK)
            .expression_attribute_names("#g3pk", GSI3_PK)
            .expression_attribute_names("#g3sk", GSI3_SK)
            .expression_attribute_values(":payload", AttributeValue::S(payload))
            .expression_attribute_values(":expected", number(expected_revision))
            .expression_attribute_values(":next", number(order.revision))
            .expression_attribute_values(":updated", timestamp(order.updated_at))
            .expression_attribute_values(":status", AttributeValue::S(status_name(order.status).into()))
            .expression_attribute_values(":g1pk", indexes[GSI1_PK].clone())
            .expression_attribute_values(":g1sk", indexes[GSI1_SK].clone())
            .expression_attribute_values(":g2pk", indexes[GSI2_PK].clone())
            .expression_attribute_values(":g2sk", indexes[GSI2_SK].clone())
            .expression_attribute_values(":g3pk", indexes[GSI3_PK].clone())
            .expression_attribute_values(":g3sk", indexes[GSI3_SK].clone())
            .send()
            .await;
        match result {
            Ok(_) => Ok(ConditionalResult::Applied(order)),
            Err(error)
                if error
                    .as_service_error()
                    .is_some_and(
                        aws_sdk_dynamodb::operation::update_item::UpdateItemError::is_conditional_check_failed_exception,
                    ) =>
            {
                self.classify_condition(order.id).await
            }
            Err(_) => Err(unavailable("DynamoDB UpdateItem failed")),
        }
    }
}

#[async_trait]
impl DeleteOrderPort for DynamoDbOrderStore {
    async fn delete_order(
        &self,
        id: OrderId,
        expected_revision: u64,
        deleted_at: DateTime<Utc>,
    ) -> Result<ConditionalResult<()>, StoreError> {
        let Some(next_revision) = expected_revision.checked_add(1) else {
            return Err(StoreError::Internal(
                "DynamoDB delete revision transition is invalid".into(),
            ));
        };
        let result = self
            .client()
            .update_item()
            .table_name(self.table_name())
            .set_key(Some(order_key(id)))
            .update_expression(
                "SET #deleted = :deleted, #updated = :updated, #revision = :next REMOVE #g1pk, #g1sk, #g2pk, #g2sk, #g3pk, #g3sk",
            )
            .condition_expression(
                "attribute_exists(#pk) AND attribute_not_exists(#deleted) AND #revision = :expected",
            )
            .expression_attribute_names("#pk", PARTITION_KEY)
            .expression_attribute_names("#deleted", DELETED_AT)
            .expression_attribute_names("#updated", UPDATED_AT)
            .expression_attribute_names("#revision", REVISION)
            .expression_attribute_names("#g1pk", GSI1_PK)
            .expression_attribute_names("#g1sk", GSI1_SK)
            .expression_attribute_names("#g2pk", GSI2_PK)
            .expression_attribute_names("#g2sk", GSI2_SK)
            .expression_attribute_names("#g3pk", GSI3_PK)
            .expression_attribute_names("#g3sk", GSI3_SK)
            .expression_attribute_values(":deleted", timestamp(deleted_at))
            .expression_attribute_values(":updated", timestamp(deleted_at))
            .expression_attribute_values(":expected", number(expected_revision))
            .expression_attribute_values(":next", number(next_revision))
            .send()
            .await;
        match result {
            Ok(_) => Ok(ConditionalResult::Applied(())),
            Err(error)
                if error
                    .as_service_error()
                    .is_some_and(
                        aws_sdk_dynamodb::operation::update_item::UpdateItemError::is_conditional_check_failed_exception,
                    ) =>
            {
                self.classify_condition(id).await
            }
            Err(_) => Err(unavailable("DynamoDB UpdateItem failed")),
        }
    }
}

#[async_trait]
impl OrderReadiness for DynamoDbOrderStore {
    async fn ready(&self) -> bool {
        self.provider.ready().await.is_ok()
    }
}

struct DynamoSortPlan {
    index_name: &'static str,
    partition_attribute: &'static str,
    sort_attribute: &'static str,
    scan_forward: bool,
    after_key: Option<String>,
}

impl DynamoSortPlan {
    fn new(query: &ListOrdersQuery) -> Result<Self, StoreError> {
        let Some(first) = query.sort.first() else {
            return Err(StoreError::Internal(
                "DynamoDB list requires an explicit sort".into(),
            ));
        };
        let scan_forward = first.direction == SortDirection::Ascending;
        let (index_name, partition_attribute, sort_attribute, key) = match first.field {
            OrderSortField::Id => (
                GSI_ID,
                GSI3_PK,
                GSI3_SK,
                id_sort_key as fn(&OrderCursor) -> String,
            ),
            OrderSortField::CreatedAt => {
                let inverted = query.sort.get(1).is_some_and(|second| {
                    second.field == OrderSortField::Id && second.direction != first.direction
                });
                if inverted {
                    (
                        GSI_CREATED_AT_INVERTED_ID,
                        GSI2_PK,
                        GSI2_SK,
                        created_at_inverted_id_sort_key as fn(&OrderCursor) -> String,
                    )
                } else {
                    (
                        GSI_CREATED_AT,
                        GSI1_PK,
                        GSI1_SK,
                        created_at_sort_key as fn(&OrderCursor) -> String,
                    )
                }
            }
        };
        Ok(Self {
            index_name,
            partition_attribute,
            sort_attribute,
            scan_forward,
            after_key: query.after.as_ref().map(key),
        })
    }
}

fn order_key(id: OrderId) -> HashMap<String, AttributeValue> {
    HashMap::from([
        (
            PARTITION_KEY.into(),
            AttributeValue::S(format!("ORDER#{}", id.into_uuid())),
        ),
        (SORT_KEY.into(), AttributeValue::S("ORDER".into())),
    ])
}

fn idempotency_item_key(key: &str) -> HashMap<String, AttributeValue> {
    HashMap::from([
        (
            PARTITION_KEY.into(),
            AttributeValue::S(format!("IDEMPOTENCY#{}", digest_hex(key.as_bytes()))),
        ),
        (SORT_KEY.into(), AttributeValue::S("IDEMPOTENCY".into())),
    ])
}

fn order_item(order: &Order) -> Result<HashMap<String, AttributeValue>, StoreError> {
    let mut item = order_key(order.id);
    item.extend(index_attributes(order));
    item.extend([
        (ENTITY.into(), AttributeValue::S("order".into())),
        (PAYLOAD.into(), AttributeValue::S(serialize_order(order)?)),
        (REVISION.into(), number(order.revision)),
        (
            STATUS.into(),
            AttributeValue::S(status_name(order.status).into()),
        ),
        (CREATED_AT.into(), timestamp(order.created_at)),
        (UPDATED_AT.into(), timestamp(order.updated_at)),
    ]);
    Ok(item)
}

fn idempotency_item(
    transaction: &PlaceOrderTransaction,
) -> Result<HashMap<String, AttributeValue>, StoreError> {
    let mut item = idempotency_item_key(&transaction.idempotency_key);
    item.extend([
        (ENTITY.into(), AttributeValue::S("idempotency".into())),
        (
            FINGERPRINT.into(),
            AttributeValue::S(transaction.request_fingerprint.clone()),
        ),
        (
            PAYLOAD.into(),
            AttributeValue::S(serialize_order(&transaction.order)?),
        ),
    ]);
    Ok(item)
}

fn index_attributes(order: &Order) -> HashMap<String, AttributeValue> {
    let shard = order_shard(order.id);
    let partition = AttributeValue::S(format!("ORDERS#{shard:02}"));
    HashMap::from([
        (GSI1_PK.into(), partition.clone()),
        (
            GSI1_SK.into(),
            AttributeValue::S(created_at_order_sort_key(order)),
        ),
        (GSI2_PK.into(), partition.clone()),
        (
            GSI2_SK.into(),
            AttributeValue::S(created_at_inverted_id_order_sort_key(order)),
        ),
        (GSI3_PK.into(), partition),
        (GSI3_SK.into(), AttributeValue::S(id_order_sort_key(order))),
    ])
}

fn decode_order_item(item: &HashMap<String, AttributeValue>) -> Result<Order, StoreError> {
    if string_attribute(item, ENTITY)? != "order" || item.contains_key(DELETED_AT) {
        return Err(malformed_item());
    }
    let order: Order =
        serde_json::from_str(string_attribute(item, PAYLOAD)?).map_err(|_| malformed_item())?;
    validate_order(&order)?;
    if string_attribute(item, PARTITION_KEY)? != format!("ORDER#{}", order.id.into_uuid())
        || string_attribute(item, SORT_KEY)? != "ORDER"
        || number_attribute(item, REVISION)? != order.revision
        || string_attribute(item, STATUS)? != status_name(order.status)
    {
        return Err(malformed_item());
    }
    Ok(order)
}

fn validate_order(order: &Order) -> Result<(), StoreError> {
    if order.revision == 0
        || order.updated_at < order.created_at
        || CustomerReference::parse(order.customer_reference.as_str()).is_err()
        || order.lines.is_empty()
        || order.lines.len() > 100
    {
        return Err(malformed_item());
    }
    let mut skus = std::collections::BTreeSet::new();
    for line in &order.lines {
        if Sku::parse(line.sku.as_str()).is_err()
            || !(1..=1_000).contains(&line.quantity.get())
            || !skus.insert(line.sku.as_str())
        {
            return Err(malformed_item());
        }
    }
    Ok(())
}

fn serialize_order(order: &Order) -> Result<String, StoreError> {
    serde_json::to_string(order)
        .map_err(|_| StoreError::Internal("DynamoDB order serialization failed".into()))
}

fn string_attribute<'a>(
    item: &'a HashMap<String, AttributeValue>,
    name: &str,
) -> Result<&'a str, StoreError> {
    item.get(name)
        .and_then(|value| value.as_s().ok())
        .map(String::as_str)
        .ok_or_else(malformed_item)
}

fn number_attribute(item: &HashMap<String, AttributeValue>, name: &str) -> Result<u64, StoreError> {
    item.get(name)
        .and_then(|value| value.as_n().ok())
        .and_then(|value| value.parse().ok())
        .ok_or_else(malformed_item)
}

fn number(value: u64) -> AttributeValue {
    AttributeValue::N(value.to_string())
}

fn timestamp(value: DateTime<Utc>) -> AttributeValue {
    AttributeValue::S(value.to_rfc3339_opts(SecondsFormat::Nanos, true))
}

const fn status_name(_status: orders_domain::OrderStatus) -> &'static str {
    "accepted"
}

fn order_shard(id: OrderId) -> u8 {
    Sha256::digest(id.into_uuid().as_bytes())[0] % LIST_SHARDS
}

fn created_at_order_sort_key(order: &Order) -> String {
    format!(
        "{}#{:032x}",
        order.created_at.to_rfc3339_opts(SecondsFormat::Nanos, true),
        order.id.into_uuid().as_u128()
    )
}

fn created_at_inverted_id_order_sort_key(order: &Order) -> String {
    format!(
        "{}#{:032x}",
        order.created_at.to_rfc3339_opts(SecondsFormat::Nanos, true),
        !order.id.into_uuid().as_u128()
    )
}

fn id_order_sort_key(order: &Order) -> String {
    format!("{:032x}", order.id.into_uuid().as_u128())
}

fn created_at_sort_key(cursor: &OrderCursor) -> String {
    format!(
        "{}#{:032x}",
        cursor
            .created_at
            .to_rfc3339_opts(SecondsFormat::Nanos, true),
        cursor.id.into_uuid().as_u128()
    )
}

fn created_at_inverted_id_sort_key(cursor: &OrderCursor) -> String {
    format!(
        "{}#{:032x}",
        cursor
            .created_at
            .to_rfc3339_opts(SecondsFormat::Nanos, true),
        !cursor.id.into_uuid().as_u128()
    )
}

fn id_sort_key(cursor: &OrderCursor) -> String {
    format!("{:032x}", cursor.id.into_uuid().as_u128())
}

fn compare_orders(left: &Order, right: &Order, sort: &[OrderSortTerm]) -> Ordering {
    for term in sort {
        let ordering = match term.field {
            OrderSortField::CreatedAt => left.created_at.cmp(&right.created_at),
            OrderSortField::Id => left.id.into_uuid().cmp(&right.id.into_uuid()),
        };
        let ordering = match term.direction {
            SortDirection::Ascending => ordering,
            SortDirection::Descending => ordering.reverse(),
        };
        if !ordering.is_eq() {
            return ordering;
        }
    }
    let ordering = left.id.into_uuid().cmp(&right.id.into_uuid());
    match sort.first().map(|term| term.direction) {
        Some(SortDirection::Descending) => ordering.reverse(),
        _ => ordering,
    }
}

fn transaction_token(key: &str, fingerprint: &str) -> String {
    let digest = digest_hex(format!("{key}\0{fingerprint}").as_bytes());
    format!("minco-{}", &digest[..30])
}

fn digest_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn malformed_item() -> StoreError {
    StoreError::Internal("malformed DynamoDB order item".into())
}

fn unavailable(message: &'static str) -> StoreError {
    StoreError::Unavailable(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use orders_domain::{OrderLine, Quantity};

    fn order() -> Order {
        Order::new(
            CustomerReference::parse("PO-UNIT").unwrap(),
            vec![OrderLine {
                sku: Sku::parse("SKU-UNIT").unwrap(),
                quantity: Quantity::new(1).unwrap(),
            }],
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn idempotency_partition_key_hashes_the_raw_key() {
        let key = "private-idempotency-key";
        let item = idempotency_item_key(key);
        let persisted = item[PARTITION_KEY].as_s().unwrap();
        assert!(!persisted.contains(key));
        assert_eq!(persisted.len(), "IDEMPOTENCY#".len() + 64);
    }

    #[test]
    fn order_items_round_trip_and_fail_closed_when_revision_is_malformed() {
        let order = order();
        let mut item = order_item(&order).unwrap();
        assert_eq!(decode_order_item(&item), Ok(order));
        item.insert(REVISION.into(), AttributeValue::N("0".into()));
        assert!(matches!(
            decode_order_item(&item),
            Err(StoreError::Internal(_))
        ));
    }

    #[test]
    fn mixed_created_at_and_id_directions_select_the_inverted_index() {
        let query = ListOrdersQuery {
            limit: 20,
            after: None,
            sort: vec![
                OrderSortTerm {
                    field: OrderSortField::CreatedAt,
                    direction: SortDirection::Ascending,
                },
                OrderSortTerm {
                    field: OrderSortField::Id,
                    direction: SortDirection::Descending,
                },
            ],
            status: None,
        };
        let plan = DynamoSortPlan::new(&query).unwrap();
        assert_eq!(plan.index_name, GSI_CREATED_AT_INVERTED_ID);
        assert!(plan.scan_forward);
    }
}
