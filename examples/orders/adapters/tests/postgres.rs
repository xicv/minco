#![cfg(feature = "postgres")]

use chrono::Utc;
use minco_plugin_audit::{
    AuditJournalStore, AuditLedgerWriter, AuditQuery, AuditReader, AuditRelay, AuditResourceRef,
};
use orders_adapters::PostgresOrderStore;
use orders_application::{
    Actor, ApplicationError, ConditionalResult, DeleteOrder, DeleteOrderPort, GetOrderPort,
    PlaceOrder, PlaceOrderCommand, PlaceOrderLine, PlaceOrderPort, PlaceOrderTransaction,
    StoreError, SystemClock, UpdateOrder, UpdateOrderCommand, UpdateOrderPort,
};
use orders_domain::{CustomerReference, Order, OrderId, OrderLine, Quantity, Sku};
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

async fn store() -> PostgresOrderStore {
    let url = std::env::var("MINCO_ORDERS_TEST_POSTGRES_URL")
        .expect("MINCO_ORDERS_TEST_POSTGRES_URL must name a disposable PostgreSQL database");
    let config = minco_sqlx_postgres::PostgresPoolConfig {
        url,
        max_connections: 4,
        acquire_timeout_seconds: 5,
        idle_timeout_seconds: 30,
    };
    let store = PostgresOrderStore::connect(&config)
        .await
        .expect("connect to PostgreSQL test database");
    let migrations = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../migrations/postgres");
    store
        .migrate(migrations)
        .await
        .expect("apply PostgreSQL migrations");
    let plugin_migrations = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../extensions/minco-sqlx-postgres/migrations/plugins");
    minco_sqlx_postgres::migrate_with_history_table(
        store.pool(),
        plugin_migrations,
        "_minco_plugin_migrations",
    )
    .await
    .expect("apply audit journal migrations");
    store
}

async fn cleanup(store: &PostgresOrderStore, key: &str, order_id: OrderId) {
    sqlx::query("DELETE FROM order_idempotency WHERE idempotency_key = $1")
        .bind(key)
        .execute(store.pool())
        .await
        .expect("delete test idempotency record");
    sqlx::query("DELETE FROM orders WHERE id = $1")
        .bind(order_id.into_uuid())
        .execute(store.pool())
        .await
        .expect("delete test order");
}

fn transaction(key: &str, fingerprint: &str) -> PlaceOrderTransaction {
    let order = Order::new(
        CustomerReference::parse("PO-POSTGRES").expect("customer reference"),
        vec![OrderLine {
            sku: Sku::parse("SKU-POSTGRES").expect("sku"),
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
#[ignore = "requires MINCO_ORDERS_TEST_POSTGRES_URL"]
async fn replays_the_original_result() {
    let store = store().await;
    let key = format!("postgres-replay-{}", Uuid::new_v4());

    let first = store
        .place_order(transaction(&key, "same-fingerprint"))
        .await
        .expect("first command");
    assert_eq!(
        store
            .delete_order(first.order.id, first.order.revision, Utc::now())
            .await,
        Ok(ConditionalResult::Applied(()))
    );
    let replay = store
        .place_order(transaction(&key, "same-fingerprint"))
        .await
        .expect("replayed command");

    cleanup(&store, &key, first.order.id).await;
    store.pool().close().await;
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(replay.order, first.order);
}

#[tokio::test]
#[ignore = "requires MINCO_ORDERS_TEST_POSTGRES_URL"]
async fn rejects_reused_keys_with_different_fingerprints() {
    let store = store().await;
    let key = format!("postgres-conflict-{}", Uuid::new_v4());

    let first = store
        .place_order(transaction(&key, "first-fingerprint"))
        .await
        .expect("first command");
    let conflict = store
        .place_order(transaction(&key, "different-fingerprint"))
        .await;

    cleanup(&store, &key, first.order.id).await;
    store.pool().close().await;
    assert!(matches!(conflict, Err(StoreError::IdempotencyConflict)));
}

#[tokio::test]
#[ignore = "requires MINCO_ORDERS_TEST_POSTGRES_URL"]
async fn concurrent_retries_commit_one_order_and_replay_it() {
    let store = store().await;
    let key = format!("postgres-concurrent-{}", Uuid::new_v4());
    let first_store = store.clone();
    let second_store = store.clone();

    let (first, second) = tokio::join!(
        first_store.place_order(transaction(&key, "same-fingerprint")),
        second_store.place_order(transaction(&key, "same-fingerprint")),
    );
    let first = first.expect("first concurrent command");
    let second = second.expect("second concurrent command");

    cleanup(&store, &key, first.order.id).await;
    store.pool().close().await;
    assert_ne!(first.replayed, second.replayed);
    assert_eq!(first.order.id, second.order.id);
}

#[tokio::test]
#[ignore = "requires MINCO_ORDERS_TEST_POSTGRES_URL"]
async fn revision_checked_update_and_delete_are_atomic() {
    let store = store().await;
    let key = format!("postgres-resource-{}", Uuid::new_v4());
    let mut order = store
        .place_order(transaction(&key, "resource-fingerprint"))
        .await
        .expect("place order")
        .order;
    order
        .update(
            Some(CustomerReference::parse("PO-UPDATED").expect("reference")),
            None,
            Utc::now(),
        )
        .expect("domain update");
    assert!(matches!(
        store.save_order(order.clone(), 1).await,
        Ok(ConditionalResult::Applied(_))
    ));
    assert_eq!(
        store.save_order(order.clone(), 1).await,
        Ok(ConditionalResult::PreconditionFailed)
    );
    assert_eq!(
        store
            .delete_order(order.id, order.revision, Utc::now())
            .await,
        Ok(ConditionalResult::Applied(()))
    );
    assert_eq!(store.get_order(order.id).await.expect("get"), None);

    cleanup(&store, &key, order.id).await;
    store.pool().close().await;
}

#[tokio::test]
#[ignore = "requires distinct MINCO_ORDERS_TEST_POSTGRES_URL and MINCO_ORDERS_TEST_POSTGRES_AUDIT_URL databases"]
async fn semantic_actions_commit_with_source_and_relay_once_to_a_separate_ledger() {
    let store = Arc::new(store().await);
    let audit_url = std::env::var("MINCO_ORDERS_TEST_POSTGRES_AUDIT_URL").expect(
        "MINCO_ORDERS_TEST_POSTGRES_AUDIT_URL must name a distinct disposable PostgreSQL database",
    );
    let audit_pool = minco_sqlx_postgres::connect(
        &minco_sqlx_postgres::PostgresPoolConfig::serverless(audit_url),
    )
    .await
    .expect("connect separate PostgreSQL audit ledger");
    minco_sqlx_postgres::audit_v2::validate_separate_audit_pools(store.pool(), &audit_pool)
        .await
        .expect("separate PostgreSQL databases");
    minco_sqlx_postgres::audit_v2::migrate_audit_ledger(&audit_pool)
        .await
        .expect("migrate PostgreSQL audit ledger");

    let actor = Actor::service(
        "orders-postgres-user",
        [
            "orders.create".to_owned(),
            "orders.update".to_owned(),
            "orders.delete".to_owned(),
        ],
    );
    let command = PlaceOrderCommand {
        customer_reference: format!("PO-AUDITED-{}", Uuid::new_v4()),
        lines: vec![PlaceOrderLine {
            sku: "SKU-AUDITED".into(),
            quantity: 1,
        }],
    };
    let correlation_id = Uuid::now_v7();
    let key = format!("postgres-audited-{}", Uuid::new_v4());
    let placed = PlaceOrder::new(Arc::clone(&store), Arc::new(SystemClock))
        .execute_correlated(&actor, command.clone(), &key, correlation_id)
        .await
        .expect("place audited PostgreSQL order");
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM minco_audit_journal")
        .fetch_one(store.pool())
        .await
        .expect("pending PostgreSQL journal count");
    assert_eq!(pending, 1);

    let ledger = Arc::new(minco_sqlx_postgres::audit_v2::PostgresAuditLedger::new(
        audit_pool.clone(),
    ));
    let mut query = AuditQuery::for_resource(
        orders_application::ORDERS_AUDIT_TENANT_SCOPE,
        AuditResourceRef::new("order", placed.order.id.into_uuid().to_string()),
    );
    query.limit = 100;
    assert!(
        ledger
            .list_resource_history(&query)
            .await
            .expect("PostgreSQL history before relay")
            .records
            .is_empty()
    );
    let journal: Arc<dyn AuditJournalStore> = Arc::new(
        minco_sqlx_postgres::audit_v2::PostgresAuditJournal::new(store.pool().clone()),
    );
    let writer: Arc<dyn AuditLedgerWriter> = ledger.clone();
    let relay = AuditRelay::new(journal, writer);
    let worker_id = format!("postgres-test-relay-{}", Uuid::new_v4());
    let report = relay
        .dispatch_once(&worker_id, 100, chrono::TimeDelta::minutes(1))
        .await
        .expect("dispatch PostgreSQL create action");
    assert_eq!(report.inserted, 1);

    let replayed = PlaceOrder::new(Arc::clone(&store), Arc::new(SystemClock))
        .execute_correlated(&actor, command, &key, correlation_id)
        .await
        .expect("idempotent PostgreSQL replay");
    assert!(replayed.replayed);
    let updated = UpdateOrder::new(Arc::clone(&store), Arc::new(SystemClock))
        .execute_correlated(
            &actor,
            placed.order.id,
            placed.order.revision,
            UpdateOrderCommand {
                customer_reference: Some(format!("PO-AUDITED-UPDATED-{}", Uuid::new_v4())),
                lines: None,
            },
            Uuid::now_v7(),
        )
        .await
        .expect("update audited PostgreSQL order");
    assert_eq!(
        UpdateOrder::new(Arc::clone(&store), Arc::new(SystemClock))
            .execute_correlated(
                &actor,
                updated.id,
                placed.order.revision,
                UpdateOrderCommand {
                    customer_reference: Some("PO-POSTGRES-RACING".into()),
                    lines: None,
                },
                Uuid::now_v7(),
            )
            .await,
        Err(ApplicationError::PreconditionFailed)
    );
    relay
        .dispatch_once(&worker_id, 100, chrono::TimeDelta::minutes(1))
        .await
        .expect("dispatch PostgreSQL update action");
    DeleteOrder::new(Arc::clone(&store), Arc::new(SystemClock))
        .execute_correlated(&actor, updated.id, updated.revision, Uuid::now_v7())
        .await
        .expect("delete audited PostgreSQL order");
    relay
        .dispatch_once(&worker_id, 100, chrono::TimeDelta::minutes(1))
        .await
        .expect("dispatch PostgreSQL delete action");

    let history = ledger
        .list_resource_history(&query)
        .await
        .expect("PostgreSQL history after deletion");
    assert_eq!(history.records.len(), 3);
    assert_eq!(history.records[0].action, "order.deleted");
    assert_eq!(history.records[1].action, "order.updated");
    assert_eq!(history.records[2].action, "order.created");
    assert!(
        history
            .records
            .iter()
            .all(|record| record.actor.subject.as_deref() == Some("orders-postgres-user"))
    );

    cleanup(&store, &key, placed.order.id).await;
    store.pool().close().await;
    audit_pool.close().await;
}
