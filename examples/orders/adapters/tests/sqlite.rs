#![cfg(feature = "sqlite")]

use chrono::Utc;
use orders_adapters::SqliteOrderStore;
use orders_application::{OrderStore, PlaceOrderTransaction, StoreError};
use orders_domain::{CustomerReference, Order, OrderLine, Quantity, Sku};
use std::path::{Path, PathBuf};
use uuid::Uuid;

struct TestDatabase {
    path: PathBuf,
    store: SqliteOrderStore,
}

impl TestDatabase {
    async fn open() -> Self {
        let path = std::env::temp_dir().join(format!("minco-orders-{}.db", Uuid::new_v4()));
        let config = minco_sqlx_sqlite::SqlitePoolConfig::file(&path);
        let store = SqliteOrderStore::connect(&config)
            .await
            .expect("connect to SQLite test database");
        let migrations = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../migrations/sqlite");
        store
            .migrate(migrations)
            .await
            .expect("apply SQLite migrations");
        Self { path, store }
    }

    async fn close(self) {
        self.store.pool().close().await;
        remove_database_files(&self.path);
    }
}

fn remove_database_files(path: &Path) {
    for suffix in ["", "-shm", "-wal"] {
        let candidate = PathBuf::from(format!("{}{suffix}", path.display()));
        match std::fs::remove_file(&candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("remove {}: {error}", candidate.display()),
        }
    }
}

fn transaction(key: &str, fingerprint: &str) -> PlaceOrderTransaction {
    let order = Order::new(
        CustomerReference::parse("PO-SQLITE").expect("customer reference"),
        vec![OrderLine {
            sku: Sku::parse("SKU-SQLITE").expect("sku"),
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
    let database = TestDatabase::open().await;
    let key = format!("sqlite-replay-{}", Uuid::new_v4());

    let first = database
        .store
        .place_order(transaction(&key, "same-fingerprint"))
        .await
        .expect("first command");
    let replay = database
        .store
        .place_order(transaction(&key, "same-fingerprint"))
        .await
        .expect("replayed command");

    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(replay.order.id, first.order.id);
    database.close().await;
}

#[tokio::test]
async fn uses_orders_specific_migration_history() {
    let database = TestDatabase::open().await;

    let orders_history: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_minco_orders_migrations'",
    )
    .fetch_one(database.store.pool())
    .await
    .expect("inspect orders migration history");

    assert_eq!(orders_history, 1);
    database.close().await;
}

#[tokio::test]
async fn persists_orders_across_pool_restarts() {
    let path = std::env::temp_dir().join(format!("minco-orders-restart-{}.db", Uuid::new_v4()));
    let migrations = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../migrations/sqlite");
    let config = minco_sqlx_sqlite::SqlitePoolConfig::file(&path);
    let first_store = SqliteOrderStore::connect(&config)
        .await
        .expect("connect first SQLite pool");
    first_store
        .migrate(&migrations)
        .await
        .expect("apply SQLite migrations");
    let first = first_store
        .place_order(transaction(
            &format!("sqlite-restart-{}", Uuid::new_v4()),
            "restart-fingerprint",
        ))
        .await
        .expect("persist order");
    first_store.pool().close().await;

    let reopened_store = SqliteOrderStore::connect(&config)
        .await
        .expect("reopen SQLite pool");
    reopened_store
        .migrate(migrations)
        .await
        .expect("verify migrations after restart");
    let persisted = reopened_store
        .get_order(first.order.id)
        .await
        .expect("read persisted order")
        .expect("persisted order exists");

    assert_eq!(persisted, first.order);
    reopened_store.pool().close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn rejects_reused_keys_with_different_fingerprints() {
    let database = TestDatabase::open().await;
    let key = format!("sqlite-conflict-{}", Uuid::new_v4());

    database
        .store
        .place_order(transaction(&key, "first-fingerprint"))
        .await
        .expect("first command");
    let conflict = database
        .store
        .place_order(transaction(&key, "different-fingerprint"))
        .await;

    assert!(matches!(conflict, Err(StoreError::IdempotencyConflict)));
    database.close().await;
}

#[tokio::test]
async fn concurrent_retries_commit_one_order_and_replay_it() {
    let database = TestDatabase::open().await;
    let key = format!("sqlite-concurrent-{}", Uuid::new_v4());
    let first_store = database.store.clone();
    let second_store = database.store.clone();

    let (first, second) = tokio::join!(
        first_store.place_order(transaction(&key, "same-fingerprint")),
        second_store.place_order(transaction(&key, "same-fingerprint")),
    );
    let first = first.expect("first concurrent command");
    let second = second.expect("second concurrent command");

    assert_ne!(first.replayed, second.replayed);
    assert_eq!(first.order.id, second.order.id);
    database.close().await;
}
