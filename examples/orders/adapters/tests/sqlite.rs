#![cfg(feature = "sqlite")]

use chrono::Utc;
use minco_aws_worker::MessageHandler as _;
use minco_plugin_audit::{
    AuditJournalStore, AuditLedgerWriter, AuditQuery, AuditReader, AuditRelay, AuditResourceRef,
};
use orders_adapters::SqliteOrderStore;
use orders_application::{
    Actor, ApplicationError, ConditionalResult, DeleteOrder, DeleteOrderPort, GetOrderPort,
    ListOrdersPort, ListOrdersQuery, OrderSortField, OrderSortTerm, PlaceOrder, PlaceOrderCommand,
    PlaceOrderLine, PlaceOrderPort, PlaceOrderTransaction, SortDirection, StoreError, SystemClock,
    UpdateOrder, UpdateOrderCommand, UpdateOrderPort,
};
use orders_domain::{CustomerReference, Order, OrderLine, Quantity, Sku};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
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
        let plugin_migrations = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../extensions/minco-sqlx-sqlite/migrations/plugins");
        minco_sqlx_sqlite::migrate_with_history_table(
            store.pool(),
            plugin_migrations,
            "_minco_plugin_migrations",
        )
        .await
        .expect("apply audit journal migrations");
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
        audit: None,
        confirmation_job: None,
    }
}

#[tokio::test]
async fn place_order_commits_the_confirmation_job_atomically() {
    let database = TestDatabase::open().await;
    let key = format!("sqlite-jobs-{}", Uuid::new_v4());
    let correlation = Uuid::now_v7();
    let mut command = transaction(&key, "fingerprint");
    command.confirmation_job = Some(orders_application::OrderConfirmationJob {
        order_id: command.order.id,
        correlation_id: correlation,
    });
    let placed = database
        .store
        .place_order(command)
        .await
        .expect("placed with a durable confirmation job");

    // The durable job and its publication intent committed with the order.
    let jobs: Vec<(String,)> = sqlx::query_as(
        "SELECT json_extract(envelope, '$.job_name') FROM minco_jobs \
         WHERE worker_profile = 'orders-notifications'",
    )
    .fetch_all(database.store.pool())
    .await
    .expect("query durable jobs");
    assert_eq!(jobs.len(), 1, "exactly one confirmation job per order");
    assert_eq!(
        jobs[0].0,
        orders_adapters::jobs::CONFIRMATION_JOB_NAME.to_owned()
    );
    let publications: Vec<(String,)> =
        sqlx::query_as("SELECT status FROM minco_job_publications WHERE status = 'pending'")
            .fetch_all(database.store.pool())
            .await
            .expect("query publication intents");
    assert_eq!(
        publications.len(),
        1,
        "the job carries one recoverable publication intent"
    );
    let _ = placed;
    database.close().await;
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

#[tokio::test]
async fn semantic_actions_commit_with_source_and_relay_once_to_a_separate_ledger() {
    let database = TestDatabase::open().await;
    let source_path = database.path;
    let store = Arc::new(database.store);
    let ledger_path =
        std::env::temp_dir().join(format!("minco-orders-audit-{}.db", Uuid::new_v4()));
    let ledger_pool =
        minco_sqlx_sqlite::connect(&minco_sqlx_sqlite::SqlitePoolConfig::file(&ledger_path))
            .await
            .expect("connect separate audit ledger");
    minco_sqlx_sqlite::audit_v2::validate_separate_audit_pools(store.pool(), &ledger_pool)
        .await
        .expect("separate files");
    minco_sqlx_sqlite::audit_v2::migrate_audit_ledger(&ledger_pool)
        .await
        .expect("migrate audit ledger");

    let actor = Actor::service(
        "orders-user",
        [
            "orders.create".to_owned(),
            "orders.update".to_owned(),
            "orders.delete".to_owned(),
        ],
    );
    let command = PlaceOrderCommand {
        customer_reference: "PO-AUDITED".into(),
        lines: vec![PlaceOrderLine {
            sku: "SKU-AUDITED".into(),
            quantity: 1,
        }],
    };
    let correlation_id = Uuid::now_v7();
    let placed = PlaceOrder::new(Arc::clone(&store), Arc::new(SystemClock))
        .execute_correlated(&actor, command.clone(), "audited-key", correlation_id)
        .await
        .expect("place audited order");
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM minco_audit_journal")
        .fetch_one(store.pool())
        .await
        .expect("pending journal count");
    assert_eq!(pending, 1);

    let ledger = Arc::new(minco_sqlx_sqlite::audit_v2::SqliteAuditLedger::new(
        ledger_pool.clone(),
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
            .expect("history before relay")
            .records
            .is_empty()
    );
    let journal: Arc<dyn AuditJournalStore> = Arc::new(
        minco_sqlx_sqlite::audit_v2::SqliteAuditJournal::new(store.pool().clone()),
    );
    let writer: Arc<dyn AuditLedgerWriter> = ledger.clone();
    let relay = AuditRelay::new(journal, writer);
    let report = relay
        .dispatch_once("sqlite-test-relay", 100, chrono::TimeDelta::minutes(1))
        .await
        .expect("dispatch create action");
    assert_eq!(report.inserted, 1);

    let replayed_order = PlaceOrder::new(Arc::clone(&store), Arc::new(SystemClock))
        .execute_correlated(&actor, command, "audited-key", correlation_id)
        .await
        .expect("idempotent replay");
    assert!(replayed_order.replayed);
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM minco_audit_journal")
        .fetch_one(store.pool())
        .await
        .expect("journal after replay");
    assert_eq!(pending, 0);

    let updated = UpdateOrder::new(Arc::clone(&store), Arc::new(SystemClock))
        .execute_correlated(
            &actor,
            placed.order.id,
            placed.order.revision,
            UpdateOrderCommand {
                customer_reference: Some("PO-AUDITED-UPDATED".into()),
                lines: None,
            },
            Uuid::now_v7(),
        )
        .await
        .expect("update audited order");
    assert_eq!(
        UpdateOrder::new(Arc::clone(&store), Arc::new(SystemClock))
            .execute_correlated(
                &actor,
                updated.id,
                placed.order.revision,
                UpdateOrderCommand {
                    customer_reference: Some("PO-RACING".into()),
                    lines: None,
                },
                Uuid::now_v7(),
            )
            .await,
        Err(ApplicationError::PreconditionFailed)
    );
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM minco_audit_journal")
        .fetch_one(store.pool())
        .await
        .expect("journal after race");
    assert_eq!(pending, 1);
    relay
        .dispatch_once("sqlite-test-relay", 100, chrono::TimeDelta::minutes(1))
        .await
        .expect("dispatch update action");

    DeleteOrder::new(Arc::clone(&store), Arc::new(SystemClock))
        .execute_correlated(&actor, updated.id, updated.revision, Uuid::now_v7())
        .await
        .expect("delete audited order");
    relay
        .dispatch_once("sqlite-test-relay", 100, chrono::TimeDelta::minutes(1))
        .await
        .expect("dispatch delete action");
    let history = ledger
        .list_resource_history(&query)
        .await
        .expect("history after deletion");
    assert_eq!(history.records.len(), 3);
    assert_eq!(history.records[0].action, "order.deleted");
    assert_eq!(history.records[1].action, "order.updated");
    assert_eq!(history.records[2].action, "order.created");
    assert!(
        history
            .records
            .iter()
            .all(|record| record.actor.subject.as_deref() == Some("orders-user"))
    );

    store.pool().close().await;
    ledger_pool.close().await;
    remove_database_files(&source_path);
    remove_database_files(&ledger_path);
}

/// A deterministic confirmation sink: records effects and can be scripted to
/// fail transiently or permanently.
#[derive(Debug, Default)]
struct RecordingSink {
    sent: std::sync::Mutex<Vec<String>>,
    transient_failures: std::sync::atomic::AtomicU32,
    permanent: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl orders_adapters::jobs::ConfirmationSink for RecordingSink {
    async fn send_confirmation(
        &self,
        confirmation: &orders_adapters::jobs::SendOrderConfirmation,
    ) -> Result<(), String> {
        if self.permanent.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("notify-rejected".into());
        }
        if self
            .transient_failures
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst)
            > 0
        {
            return Err("notification-unavailable".into());
        }
        self.sent
            .lock()
            .expect("sink lock")
            .push(confirmation.order_id.clone());
        Ok(())
    }
}

/// The golden durable-work slice: order commit, queue dispatch, typed
/// handler, one notification effect, duplicate suppression, durable retry
/// and inspectable permanent failure.
#[tokio::test]
async fn confirmation_job_reaches_the_handler_exactly_once() {
    let database = TestDatabase::open().await;
    let sink = std::sync::Arc::new(RecordingSink::default());
    let registry = std::sync::Arc::new(orders_adapters::jobs::confirmation_registry(sink.clone()));
    let (memory, _store, dispatcher) = minco_plugin_jobs::JobsServices::memory(registry.clone());
    let job_store = std::sync::Arc::new(minco_sqlx_sqlite::jobs::SqliteJobStore::new(
        sqlx::SqlitePool::connect(&format!("sqlite://{}", database.path.display()))
            .await
            .expect("job pool"),
    ));
    let services = minco_plugin_jobs::JobsServices::new(
        job_store.clone(),
        job_store.clone(),
        dispatcher.clone(),
        job_store.clone(),
        memory.clock.clone(),
        memory.executor.clone(),
    );

    // Place the order; the confirmation job committed with it.
    let key = format!("sqlite-golden-{}", Uuid::new_v4());
    let mut command = transaction(&key, "fingerprint");
    command.confirmation_job = Some(orders_application::OrderConfirmationJob {
        order_id: command.order.id,
        correlation_id: Uuid::now_v7(),
    });
    database
        .store
        .place_order(command)
        .await
        .expect("place order");

    // The real request-assisted publication driver: after commit, one
    // bounded pass claims the due publication and sends it through the
    // configured transport (the recording fake here).
    let report = services
        .dispatch_due_once(
            &format!("request-assisted-{key}"),
            10,
            chrono::TimeDelta::minutes(1),
        )
        .await
        .expect("request-assisted dispatch");
    assert_eq!(
        report.dispatched, 1,
        "the committed publication is delivered"
    );
    let sent = dispatcher.dispatched();
    assert_eq!(sent.len(), 1, "one transport delivery");
    let envelope = sent[0].envelope.clone();
    // The durable publication identity persisted at commit time is exactly
    // what the delivery carries: persistence-to-transport identity, read
    // from the database rather than from the delivery itself.
    let persisted: Vec<(String,)> =
        sqlx::query_as("SELECT publication_id FROM minco_job_publications")
            .fetch_all(database.store.pool())
            .await
            .expect("read persisted publication identities");
    assert_eq!(persisted.len(), 1);
    assert_eq!(
        sent[0].publication_id.to_string(),
        persisted[0].0,
        "the delivery carries its durable publication identity"
    );
    let jobs: Vec<(String,)> = sqlx::query_as("SELECT status FROM minco_job_publications")
        .fetch_all(database.store.pool())
        .await
        .expect("read publications");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].0, "published", "no orphan publication remains");
    let worker =
        minco_aws_worker::jobs::JobMessageHandler::durable("orders-jobs-worker", services.clone());
    let body = String::from_utf8(envelope.to_json_bytes().expect("serialize")).expect("utf-8");
    worker
        .handle(minco_aws_worker::WorkerMessage {
            message_id: "m-1".into(),
            body: body.clone(),
            attributes: std::collections::BTreeMap::new(),
            message_group_id: None,
        })
        .await
        .expect("first delivery acknowledged");
    // A duplicate delivery of the completed job is acknowledged without a
    // second business effect.
    worker
        .handle(minco_aws_worker::WorkerMessage {
            message_id: "m-2".into(),
            body,
            attributes: std::collections::BTreeMap::new(),
            message_group_id: None,
        })
        .await
        .expect("duplicate delivery acknowledged");
    assert_eq!(
        sink.sent.lock().expect("sink lock").len(),
        1,
        "exactly one confirmation effect"
    );
    let _ = dispatcher;
    database.close().await;
}

#[tokio::test]
async fn transient_confirmation_failure_retries_and_permanent_failure_is_inspectable() {
    let database = TestDatabase::open().await;
    let sink = std::sync::Arc::new(RecordingSink {
        sent: std::sync::Mutex::new(Vec::new()),
        transient_failures: std::sync::atomic::AtomicU32::new(1),
        permanent: std::sync::atomic::AtomicBool::new(false),
    });
    let registry = std::sync::Arc::new(orders_adapters::jobs::confirmation_registry(sink.clone()));
    let job_store = std::sync::Arc::new(minco_sqlx_sqlite::jobs::SqliteJobStore::new(
        sqlx::SqlitePool::connect(&format!("sqlite://{}", database.path.display()))
            .await
            .expect("job pool"),
    ));
    let services = minco_plugin_jobs::JobsServices::new(
        job_store.clone(),
        job_store.clone(),
        std::sync::Arc::new(minco_plugin_jobs::FakeJobDispatcher::new()),
        job_store.clone(),
        std::sync::Arc::new(minco_plugin_jobs::SystemJobClock),
        std::sync::Arc::new(minco_plugin_jobs::JobExecutor::new(registry.clone())),
    );

    let mut envelope =
        orders_adapters::jobs::confirmation_envelope(&orders_application::OrderConfirmationJob {
            order_id: orders_domain::OrderId::new(),
            correlation_id: Uuid::now_v7(),
        })
        .expect("envelope");
    // Fast backoff keeps the retry proof within the test budget.
    envelope.retry = Some(minco_plugin_jobs::RetryPolicy::fixed(5, 1));
    services
        .submit_durable(envelope.clone())
        .await
        .expect("submit");

    let worker = minco_aws_worker::jobs::JobMessageHandler::durable("w", services.clone());
    let body = String::from_utf8(envelope.to_json_bytes().unwrap()).unwrap();
    let message = |id: &str| minco_aws_worker::WorkerMessage {
        message_id: id.into(),
        body: body.clone(),
        attributes: std::collections::BTreeMap::new(),
        message_group_id: None,
    };
    worker.handle(message("m-1")).await.expect("retry acked");
    let record = services.store.get(envelope.job_id).await.unwrap().unwrap();
    assert_eq!(
        format!("{:?}", record.status),
        "Pending",
        "the retry is durable"
    );
    // After the retry delay the job succeeds.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    worker.handle(message("m-2")).await.expect("second ack");
    assert_eq!(sink.sent.lock().unwrap().len(), 1);

    // A permanently failing job becomes inspectable.
    let failing_sink = std::sync::Arc::new(RecordingSink {
        sent: std::sync::Mutex::new(Vec::new()),
        transient_failures: std::sync::atomic::AtomicU32::new(0),
        permanent: std::sync::atomic::AtomicBool::new(true),
    });
    let failing_registry =
        std::sync::Arc::new(orders_adapters::jobs::confirmation_registry(failing_sink));
    let failing_services = minco_plugin_jobs::JobsServices::new(
        job_store.clone(),
        job_store.clone(),
        std::sync::Arc::new(minco_plugin_jobs::FakeJobDispatcher::new()),
        job_store.clone(),
        std::sync::Arc::new(minco_plugin_jobs::SystemJobClock),
        std::sync::Arc::new(minco_plugin_jobs::JobExecutor::new(failing_registry)),
    );
    let mut exhausted =
        orders_adapters::jobs::confirmation_envelope(&orders_application::OrderConfirmationJob {
            order_id: orders_domain::OrderId::new(),
            correlation_id: Uuid::now_v7(),
        })
        .expect("envelope");
    exhausted.maximum_attempts = 1;
    exhausted.retry = Some(minco_plugin_jobs::RetryPolicy::fixed(1, 1));
    failing_services
        .submit_durable(exhausted.clone())
        .await
        .expect("submit failing");
    let failing_worker =
        minco_aws_worker::jobs::JobMessageHandler::durable("w", failing_services.clone());
    failing_worker
        .handle(minco_aws_worker::WorkerMessage {
            message_id: "m-3".into(),
            body: String::from_utf8(exhausted.to_json_bytes().unwrap()).unwrap(),
            attributes: std::collections::BTreeMap::new(),
            message_group_id: None,
        })
        .await
        .expect("permanent failure acked");
    let failed = failing_services.store.list_failed(10).await.unwrap();
    assert_eq!(failed.len(), 1, "the failure is inspectable");
    assert_eq!(failed[0].envelope.job_id, exhausted.job_id);
    database.close().await;
}
