//! Published-migration immutability and real-checksum upgrade proofs
//! (exact-head review R27/P0-1).
//!
//! `SQLx` records a checksum for every applied migration and refuses to
//! run when the embedded file no longer matches (`VersionMismatch`).
//! These proofs pin migration 0001 to the exact bytes released in
//! Minco 1.12 and upgrade a database whose migration history was
//! recorded by those exact released bytes.

use minco_plugin_audit::{AuditError, AuditEvent, AuditSink};
use minco_sqlx_sqlite::{SqlitePoolConfig, connect, plugin_adapters::SqliteAuditSink};
use sqlx::SqlitePool;
use std::fs;
use tempfile::TempDir;
use uuid::Uuid;

/// The plugin-storage migration exactly as released in Minco 1.12
/// (commit ee88437). This fixture is the upgrade oracle: the shipped
/// migration must stay byte-identical to it forever.
const RELEASED_0001: &str =
    include_str!("fixtures/minco_1_12_plugin_migrations/0001_plugin_storage.sql");
const SHIPPED_0001: &str = include_str!("../migrations/plugins/0001_plugin_storage.sql");
const SHIPPED_0002: &str = include_str!("../migrations/plugins/0002_audit_journal.sql");
const SHIPPED_0003: &str = include_str!("../migrations/plugins/0003_durable_jobs.sql");

#[test]
fn shipped_migration_0001_is_byte_identical_to_the_minco_1_12_release() {
    assert_eq!(
        SHIPPED_0001, RELEASED_0001,
        "migration 0001 is published and immutable; schema changes must be new forward migrations"
    );
}

async fn file_pool(path: &std::path::Path) -> SqlitePool {
    let mut config = SqlitePoolConfig::file(path);
    config.max_connections = 2;
    config.acquire_timeout_seconds = 5;
    connect(&config).await.expect("connect")
}

/// Build a database whose migration history carries the REAL sqlx
/// checksums of the Minco 1.12 released migration set (0001–0003), then
/// hand it to the current migrator.
async fn database_migrated_by_minco_1_12() -> (TempDir, SqlitePool) {
    let dir = TempDir::new().expect("temp dir");
    let migrations = dir.path().join("migrations");
    fs::create_dir(&migrations).expect("create migrations dir");
    fs::write(migrations.join("0001_plugin_storage.sql"), RELEASED_0001)
        .expect("write released 0001");
    fs::write(migrations.join("0002_audit_journal.sql"), SHIPPED_0002)
        .expect("write released 0002");
    fs::write(migrations.join("0003_durable_jobs.sql"), SHIPPED_0003).expect("write released 0003");

    let database = TempDir::new().expect("temp database dir");
    let pool = file_pool(&database.path().join("minco112.sqlite")).await;
    let mut released = sqlx::migrate::Migrator::new(migrations)
        .await
        .expect("released migrator");
    released.dangerous_set_table_name("_minco_plugin_storage_migrations");
    released
        .run(&pool)
        .await
        .expect("apply released migrations");
    (database, pool)
}

#[tokio::test]
async fn minco_1_12_database_upgrades_without_checksum_mismatch() {
    let (_database, pool) = database_migrated_by_minco_1_12().await;
    // The current migration set (restored 0001 + forward-only 0004)
    // must accept the recorded checksums and apply only the new
    // migration. Before R27 this returned VersionMismatch because 0001
    // had been edited in place.
    minco_sqlx_sqlite::plugin_adapters::migrate_plugin_storage(&pool)
        .await
        .expect("upgrade from Minco 1.12 must not hit a checksum mismatch");

    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _minco_plugin_storage_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(applied, 4);

    let fingerprint_column: Option<String> = sqlx::query_scalar(
        "SELECT type FROM pragma_table_info('minco_audit') WHERE name = 'fingerprint'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(fingerprint_column.as_deref(), Some("TEXT"));
}

#[tokio::test]
async fn upgraded_database_detects_audit_conflicts_and_adopts_legacy_rows() {
    let (_database, pool) = database_migrated_by_minco_1_12().await;
    minco_sqlx_sqlite::plugin_adapters::migrate_plugin_storage(&pool)
        .await
        .expect("upgrade");

    let sink = SqliteAuditSink::new(pool.clone());

    // Same id + same content → idempotent success, one row.
    let event = AuditEvent::new(
        "ticket.created",
        "ticketing.ticket",
        "ticket-1",
        Uuid::now_v7(),
    );
    sink.append(event.clone()).await.unwrap();
    sink.append(event.clone()).await.unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM minco_audit")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);

    // Same id + different content → integrity conflict, original kept.
    let mut conflicting = event.clone();
    conflicting.action = "ticket.updated".into();
    assert!(matches!(
        sink.append(conflicting).await,
        Err(AuditError::Conflict)
    ));
    let stored_action: String = sqlx::query_scalar("SELECT action FROM minco_audit WHERE id = ?")
        .bind(event.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_action, "ticket.created");

    // A row written before the 0004 migration (NULL fingerprint, the
    // exact byte shape a pre-0004 writer produced) is content-verified
    // and adopted on redelivery of identical content…
    let legacy = AuditEvent::new(
        "queue.created",
        "ticketing.queue",
        "queue-1",
        Uuid::now_v7(),
    );
    sqlx::query(
        "INSERT INTO minco_audit
         (id, action, resource_type, resource_id, actor_subject, correlation_id,
          occurred_at, metadata, fingerprint)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind(legacy.id)
    .bind(&legacy.action)
    .bind(&legacy.resource_type)
    .bind(&legacy.resource_id)
    .bind(&legacy.actor_subject)
    .bind(legacy.correlation_id)
    .bind(legacy.occurred_at)
    .bind(serde_json::to_string(&legacy.metadata).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    sink.append(legacy.clone()).await.unwrap();
    let adopted: Option<String> =
        sqlx::query_scalar("SELECT fingerprint FROM minco_audit WHERE id = ?")
            .bind(legacy.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        adopted.as_deref(),
        Some(minco_plugin_audit::event_fingerprint(&legacy).as_str())
    );

    // …while different content under a legacy id stays a conflict.
    let mut legacy_conflict = legacy.clone();
    legacy_conflict.resource_id = "queue-2".into();
    assert!(matches!(
        sink.append(legacy_conflict).await,
        Err(AuditError::Conflict)
    ));
}
