#![cfg(feature = "sqlite")]

use chrono::Utc;
use orders_adapters::SqliteOrderStore;
use orders_application::{
    ConditionalResult, DeleteOrderPort, GetOrderPort, ListOrdersPort, ListOrdersQuery,
    OrderSortField, OrderSortTerm, PlaceOrderPort, PlaceOrderTransaction, SortDirection,
    StoreError, UpdateOrderPort,
};
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
    assert_eq!(
        database
            .store
            .delete_order(first.order.id, first.order.revision, Utc::now())
            .await,
        Ok(ConditionalResult::Applied(()))
    );
    let replay = database
        .store
        .place_order(transaction(&key, "same-fingerprint"))
        .await
        .expect("replayed command");

    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(replay.order, first.order);
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
async fn migration_backfills_replay_snapshots_for_existing_idempotency_rows() {
    let path = std::env::temp_dir().join(format!("minco-orders-upgrade-{}.db", Uuid::new_v4()));
    let config = minco_sqlx_sqlite::SqlitePoolConfig::file(&path);
    let store = SqliteOrderStore::connect(&config)
        .await
        .expect("connect to SQLite upgrade database");
    sqlx::raw_sql(sqlx::AssertSqlSafe(include_str!(
        "../../migrations/sqlite/0001_orders.sql"
    )))
    .execute(store.pool())
    .await
    .expect("apply legacy migration");
    let order_id = Uuid::now_v7();
    let created_at = "2026-07-31T00:00:00Z";
    sqlx::query(
        "INSERT INTO orders (id, customer_reference, lines, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(order_id.to_string())
    .bind("PO-UPGRADE")
    .bind(r#"[{"sku":"SKU-UPGRADE","quantity":1}]"#)
    .bind("accepted")
    .bind(created_at)
    .execute(store.pool())
    .await
    .expect("insert legacy order");
    sqlx::query(
        "INSERT INTO order_idempotency (idempotency_key, request_fingerprint, order_id) VALUES (?1, ?2, ?3)",
    )
    .bind("upgrade-key")
    .bind("upgrade-fingerprint")
    .bind(order_id.to_string())
    .execute(store.pool())
    .await
    .expect("insert legacy idempotency row");

    let migrations = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../migrations/sqlite");
    store.migrate(migrations).await.expect("upgrade migrations");
    let replay = store
        .place_order(transaction("upgrade-key", "upgrade-fingerprint"))
        .await
        .expect("replay upgraded result");

    assert!(replay.replayed);
    assert_eq!(replay.order.id.into_uuid(), order_id);
    assert_eq!(replay.order.customer_reference.as_str(), "PO-UPGRADE");
    assert_eq!(
        replay.order.created_at.to_rfc3339(),
        "2026-07-31T00:00:00+00:00"
    );
    store.pool().close().await;
    remove_database_files(&path);
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

#[tokio::test]
async fn cursor_update_and_delete_are_revision_safe() {
    let database = TestDatabase::open().await;
    let mut created = Vec::new();
    for index in 0..3 {
        created.push(
            database
                .store
                .place_order(transaction(
                    &format!("sqlite-resource-{index}-{}", Uuid::new_v4()),
                    &format!("resource-{index}"),
                ))
                .await
                .expect("place order")
                .order,
        );
    }
    let sort = vec![
        OrderSortTerm {
            field: OrderSortField::CreatedAt,
            direction: SortDirection::Descending,
        },
        OrderSortTerm {
            field: OrderSortField::Id,
            direction: SortDirection::Descending,
        },
    ];
    let first_page = database
        .store
        .list_orders(ListOrdersQuery {
            limit: 2,
            after: None,
            sort: sort.clone(),
            status: None,
        })
        .await
        .expect("first page");
    assert_eq!(first_page.orders.len(), 2);
    let cursor = first_page.next_cursor.expect("next cursor");
    let second_page = database
        .store
        .list_orders(ListOrdersQuery {
            limit: 2,
            after: Some(cursor),
            sort,
            status: None,
        })
        .await
        .expect("second page");
    assert_eq!(second_page.orders.len(), 1);
    assert!(
        first_page
            .orders
            .iter()
            .all(|order| order.id != second_page.orders[0].id)
    );

    let mut changed = created.pop().expect("created order");
    changed
        .update(
            Some(CustomerReference::parse("PO-UPDATED").expect("reference")),
            None,
            Utc::now(),
        )
        .expect("domain update");
    assert!(matches!(
        database.store.save_order(changed.clone(), 1).await,
        Ok(ConditionalResult::Applied(_))
    ));
    assert_eq!(
        database.store.save_order(changed.clone(), 1).await,
        Ok(ConditionalResult::PreconditionFailed)
    );
    assert_eq!(
        database
            .store
            .delete_order(changed.id, changed.revision, Utc::now())
            .await,
        Ok(ConditionalResult::Applied(()))
    );
    assert_eq!(
        database.store.get_order(changed.id).await.expect("get"),
        None
    );
    database.close().await;
}
