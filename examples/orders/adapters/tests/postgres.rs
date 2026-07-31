#![cfg(feature = "postgres")]

use chrono::Utc;
use orders_adapters::PostgresOrderStore;
use orders_application::{
    ConditionalResult, DeleteOrderPort, GetOrderPort, PlaceOrderPort, PlaceOrderTransaction,
    StoreError, UpdateOrderPort,
};
use orders_domain::{CustomerReference, Order, OrderId, OrderLine, Quantity, Sku};
use std::path::PathBuf;
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
