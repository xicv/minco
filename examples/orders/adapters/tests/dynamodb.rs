#![cfg(feature = "dynamodb")]

use chrono::{Duration, TimeZone, Utc};
use minco_aws_dynamodb::DynamoDbConfig;
use minco_plugin_audit::{AuditQuery, AuditReader, AuditResourceRef};
use orders_adapters::DynamoDbOrderStore;
use orders_application::{
    Actor, ApplicationError, ConditionalResult, DeleteOrder, DeleteOrderPort, GetOrderPort,
    ListOrdersPort, ListOrdersQuery, OrderReadiness, OrderSortField, OrderSortTerm, PlaceOrder,
    PlaceOrderCommand, PlaceOrderLine, PlaceOrderPort, PlaceOrderTransaction, SortDirection,
    StoreError, SystemClock, UpdateOrder, UpdateOrderCommand, UpdateOrderPort,
};
use orders_domain::{CustomerReference, Order, OrderLine, OrderStatus, Quantity, Sku};
use std::sync::Arc;
use uuid::Uuid;

async fn store() -> DynamoDbOrderStore {
    let table = std::env::var("MINCO_ORDERS_TEST_DYNAMODB_TABLE")
        .expect("MINCO_ORDERS_TEST_DYNAMODB_TABLE must name a disposable Rustack table");
    let endpoint = std::env::var("MINCO_ORDERS_TEST_DYNAMODB_ENDPOINT")
        .expect("MINCO_ORDERS_TEST_DYNAMODB_ENDPOINT must name the loopback Rustack endpoint");
    let provider = DynamoDbConfig::new(table, "ap-southeast-2", Some(endpoint))
        .build()
        .await
        .expect("build DynamoDB provider");
    DynamoDbOrderStore::new(provider)
}

fn transaction(key: &str, fingerprint: &str, index: i64) -> PlaceOrderTransaction {
    let created_at = Utc
        .with_ymd_and_hms(2026, 8, 5, 1, 2, 3)
        .single()
        .expect("timestamp")
        + Duration::seconds(index / 2);
    let order = Order::new(
        CustomerReference::parse(format!("PO-DYNAMODB-{index}")).expect("customer reference"),
        vec![OrderLine {
            sku: Sku::parse(format!("SKU-DYNAMODB-{index}")).expect("sku"),
            quantity: Quantity::new(1).expect("quantity"),
        }],
        created_at,
    )
    .expect("order");
    PlaceOrderTransaction {
        order,
        idempotency_key: key.into(),
        request_fingerprint: fingerprint.into(),
        audit: None,
    }
}

fn sort_terms(
    first: (OrderSortField, SortDirection),
    second: Option<(OrderSortField, SortDirection)>,
) -> Vec<OrderSortTerm> {
    std::iter::once(OrderSortTerm {
        field: first.0,
        direction: first.1,
    })
    .chain(second.map(|(field, direction)| OrderSortTerm { field, direction }))
    .collect()
}

#[tokio::test]
#[ignore = "requires a disposable Rustack DynamoDB table"]
async fn all_orders_ports_preserve_idempotency_sort_cursor_revision_and_soft_delete() {
    let store = store().await;
    assert!(store.ready().await);

    let replay_key = format!("rustack-replay-{}", Uuid::new_v4());
    let first = store
        .place_order(transaction(&replay_key, "same-fingerprint", 0))
        .await
        .expect("first create");
    let replay = store
        .place_order(transaction(&replay_key, "same-fingerprint", 99))
        .await
        .expect("replay");
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(replay.order, first.order);
    assert!(matches!(
        store
            .place_order(transaction(&replay_key, "different-fingerprint", 100))
            .await,
        Err(StoreError::IdempotencyConflict)
    ));

    let concurrent_key = format!("rustack-concurrent-{}", Uuid::new_v4());
    let (left, right) = tokio::join!(
        store.place_order(transaction(&concurrent_key, "concurrent", 1)),
        store.place_order(transaction(&concurrent_key, "concurrent", 2)),
    );
    let left = left.expect("left concurrent create");
    let right = right.expect("right concurrent create");
    assert_ne!(left.replayed, right.replayed);
    assert_eq!(left.order.id, right.order.id);

    let mut orders = vec![first.order.clone(), left.order.clone()];
    for index in 3..8 {
        let key = format!("rustack-list-{index}-{}", Uuid::new_v4());
        orders.push(
            store
                .place_order(transaction(&key, &format!("fingerprint-{index}"), index))
                .await
                .expect("create list order")
                .order,
        );
    }

    let directions = [SortDirection::Ascending, SortDirection::Descending];
    for first_field in [OrderSortField::CreatedAt, OrderSortField::Id] {
        for first_direction in directions {
            for second in std::iter::once(None).chain(directions.into_iter().map(|direction| {
                Some((
                    if first_field == OrderSortField::CreatedAt {
                        OrderSortField::Id
                    } else {
                        OrderSortField::CreatedAt
                    },
                    direction,
                ))
            })) {
                let sort = sort_terms((first_field, first_direction), second);
                let mut listed = Vec::new();
                let mut after = None;
                loop {
                    let page = store
                        .list_orders(ListOrdersQuery {
                            limit: 2,
                            after,
                            sort: sort.clone(),
                            status: Some(OrderStatus::Accepted),
                        })
                        .await
                        .expect("list page");
                    listed.extend(page.orders);
                    match page.next_cursor {
                        Some(cursor) => after = Some(cursor),
                        None => break,
                    }
                }
                assert_eq!(listed.len(), orders.len());
                let mut expected = orders.clone();
                expected.sort_by(|left, right| compare(left, right, &sort));
                assert_eq!(listed, expected, "sort {sort:?}");
            }
        }
    }

    assert_eq!(
        store
            .delete_order(
                first.order.id,
                first.order.revision,
                first.order.updated_at + Duration::seconds(1),
            )
            .await,
        Ok(ConditionalResult::Applied(()))
    );
    assert_eq!(
        store.get_order(first.order.id).await.expect("get deleted"),
        None
    );
    let replay_after_delete = store
        .place_order(transaction(&replay_key, "same-fingerprint", 1000))
        .await
        .expect("replay after delete");
    assert!(replay_after_delete.replayed);
    assert_eq!(replay_after_delete.order, first.order);

    let mut changed = orders.pop().expect("order to update");
    changed
        .update(
            Some(CustomerReference::parse("PO-DYNAMODB-UPDATED").expect("reference")),
            None,
            changed.updated_at + Duration::seconds(1),
        )
        .expect("domain update");
    assert_eq!(
        store.save_order(changed.clone(), 1).await,
        Ok(ConditionalResult::Applied(changed.clone()))
    );
    assert_eq!(
        store.save_order(changed.clone(), 1).await,
        Ok(ConditionalResult::PreconditionFailed)
    );
    assert_eq!(
        store
            .delete_order(
                changed.id,
                changed.revision,
                changed.updated_at + Duration::seconds(1),
            )
            .await,
        Ok(ConditionalResult::Applied(()))
    );
    assert_eq!(
        store.get_order(changed.id).await.expect("get deleted"),
        None
    );
    assert_eq!(
        store
            .delete_order(changed.id, changed.revision, Utc::now())
            .await,
        Ok(ConditionalResult::NotFound)
    );
}

#[tokio::test]
#[ignore = "requires disposable Rustack Orders and audit DynamoDB tables"]
async fn audited_orders_actions_are_atomic_queryable_and_race_safe() {
    let table = std::env::var("MINCO_ORDERS_TEST_DYNAMODB_TABLE")
        .expect("MINCO_ORDERS_TEST_DYNAMODB_TABLE must name a disposable Rustack table");
    let audit_table = std::env::var("MINCO_ORDERS_TEST_DYNAMODB_AUDIT_TABLE")
        .expect("MINCO_ORDERS_TEST_DYNAMODB_AUDIT_TABLE must name a disposable audit table");
    let endpoint = std::env::var("MINCO_ORDERS_TEST_DYNAMODB_ENDPOINT")
        .expect("MINCO_ORDERS_TEST_DYNAMODB_ENDPOINT must name the loopback Rustack endpoint");
    let provider = DynamoDbConfig::new(table, "ap-southeast-2", Some(endpoint))
        .build()
        .await
        .expect("build DynamoDB provider");
    let store = Arc::new(
        DynamoDbOrderStore::with_audit(provider, audit_table).expect("configure audit table"),
    );
    let actor = Actor::service(
        "rustack-user",
        [
            "orders.create".to_owned(),
            "orders.update".to_owned(),
            "orders.delete".to_owned(),
        ],
    );
    let command = PlaceOrderCommand {
        customer_reference: format!("PO-AUDIT-{}", Uuid::new_v4()),
        lines: vec![PlaceOrderLine {
            sku: "SKU-AUDIT".into(),
            quantity: 1,
        }],
    };
    let correlation_id = Uuid::now_v7();
    let key = format!("rustack-audited-{}", Uuid::new_v4());
    let placed = PlaceOrder::new(Arc::clone(&store), Arc::new(SystemClock))
        .execute_correlated(&actor, command.clone(), &key, correlation_id)
        .await
        .expect("audited DynamoDB create");
    let ledger = store.audit_reader().expect("audit reader");
    let mut query = AuditQuery::for_resource(
        orders_application::ORDERS_AUDIT_TENANT_SCOPE,
        AuditResourceRef::new("order", placed.order.id.into_uuid().to_string()),
    );
    query.limit = 100;
    assert_eq!(
        ledger
            .list_resource_history(&query)
            .await
            .expect("create history")
            .records
            .len(),
        1
    );

    let replay = PlaceOrder::new(Arc::clone(&store), Arc::new(SystemClock))
        .execute_correlated(&actor, command, &key, correlation_id)
        .await
        .expect("audited replay");
    assert!(replay.replayed);
    assert_eq!(
        ledger
            .list_resource_history(&query)
            .await
            .expect("history after replay")
            .records
            .len(),
        1
    );

    let updated = UpdateOrder::new(Arc::clone(&store), Arc::new(SystemClock))
        .execute_correlated(
            &actor,
            placed.order.id,
            placed.order.revision,
            UpdateOrderCommand {
                customer_reference: Some(format!("PO-AUDIT-UPDATED-{}", Uuid::new_v4())),
                lines: None,
            },
            Uuid::now_v7(),
        )
        .await
        .expect("audited update");
    assert_eq!(
        UpdateOrder::new(Arc::clone(&store), Arc::new(SystemClock))
            .execute_correlated(
                &actor,
                updated.id,
                placed.order.revision,
                UpdateOrderCommand {
                    customer_reference: Some("PO-STALE".into()),
                    lines: None,
                },
                Uuid::now_v7(),
            )
            .await,
        Err(ApplicationError::PreconditionFailed)
    );
    assert_eq!(
        ledger
            .list_resource_history(&query)
            .await
            .expect("history after stale race")
            .records
            .len(),
        2
    );
    DeleteOrder::new(Arc::clone(&store), Arc::new(SystemClock))
        .execute_correlated(&actor, updated.id, updated.revision, Uuid::now_v7())
        .await
        .expect("audited delete");
    let history = ledger
        .list_resource_history(&query)
        .await
        .expect("history after delete");
    assert_eq!(history.records.len(), 3);
    assert_eq!(history.records[0].action, "order.deleted");
}

fn compare(left: &Order, right: &Order, sort: &[OrderSortTerm]) -> std::cmp::Ordering {
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
