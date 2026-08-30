//! Published-migration immutability, real-checksum upgrade and audit
//! conflict parity on `PostgreSQL` (exact-head reviews `R27` and `R31`).
//! Gated on `MINCO_TEST_POSTGRES_URL` exactly like the other `PostgreSQL`
//! proofs; each run creates and drops its own database so the shared
//! test server is never polluted.

use minco_plugin_audit::{AuditEvent, AuditSink};
use minco_sqlx_postgres::plugin_adapters::{PostgresAuditSink, migrate_plugin_storage};
use sqlx::PgPool;
use std::fs;
use tempfile::TempDir;
use uuid::Uuid;

const RELEASED_0001: &str =
    include_str!("fixtures/minco_1_12_plugin_migrations/0001_plugin_storage.sql");
const SHIPPED_0001: &str = include_str!("../migrations/plugins/0001_plugin_storage.sql");
const SHIPPED_0002: &str = include_str!("../migrations/plugins/0002_audit_journal.sql");
const SHIPPED_0003: &str = include_str!("../migrations/plugins/0003_durable_jobs.sql");

#[test]
fn shipped_postgres_migration_0001_is_byte_identical_to_the_minco_1_12_release() {
    assert_eq!(
        SHIPPED_0001, RELEASED_0001,
        "published PostgreSQL migration 0001 is immutable; changes must be new forward migrations"
    );
}

/// One isolated database per test run: created from the server named by
/// `MINCO_TEST_POSTGRES_URL`, dropped on drop.
struct TestDatabase {
    pool: PgPool,
    name: String,
    server_url: String,
}

impl TestDatabase {
    async fn create() -> Option<Self> {
        let url = std::env::var("MINCO_TEST_POSTGRES_URL").ok()?;
        let options = url
            .parse::<sqlx::postgres::PgConnectOptions>()
            .expect("parse url");
        let maintenance = sqlx::postgres::PgPoolOptions::new()
            .connect_with(options.clone())
            .await
            .expect("connect server");
        let name = format!("minco_audit_proof_{}", Uuid::now_v7().simple());
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&maintenance)
            .await
            .expect("create isolated database");
        maintenance.close().await;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_with(options.database(&name))
            .await
            .expect("connect isolated database");
        Some(Self {
            pool,
            name,
            server_url: url,
        })
    }

    /// Apply the Minco 1.12 released migration set so the recorded
    /// checksums are the real released checksums.
    async fn migrate_from_release(&self) {
        let dir = TempDir::new().expect("temp dir");
        let migrations = dir.path().join("migrations");
        fs::create_dir(&migrations).expect("create migrations dir");
        fs::write(migrations.join("0001_plugin_storage.sql"), RELEASED_0001)
            .expect("write released 0001");
        fs::write(migrations.join("0002_audit_journal.sql"), SHIPPED_0002)
            .expect("write released 0002");
        fs::write(migrations.join("0003_durable_jobs.sql"), SHIPPED_0003)
            .expect("write released 0003");
        let mut released = sqlx::migrate::Migrator::new(migrations)
            .await
            .expect("released migrator");
        released.dangerous_set_table_name("_minco_plugin_storage_migrations");
        released.run(&self.pool).await.expect("apply released set");
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let name = self.name.clone();
        let url = self.server_url.clone();
        // Best-effort synchronous teardown; a leaked test database is
        // preferable to blocking the test runtime on pool handles.
        std::process::Command::new("psql")
            .arg("-d")
            .arg(&url)
            .arg("-c")
            .arg(format!("DROP DATABASE IF EXISTS {name}"))
            .output()
            .ok();
    }
}

#[tokio::test]
async fn minco_1_12_postgres_database_upgrades_and_detects_audit_conflicts() {
    let Some(database) = TestDatabase::create().await else {
        eprintln!("MINCO_TEST_POSTGRES_URL not set; PostgreSQL upgrade proof skipped");
        return;
    };
    database.migrate_from_release().await;

    // The current set (unmodified 0001 + additive 0004) must accept the
    // released checksums and apply only the new migration.
    migrate_plugin_storage(&database.pool)
        .await
        .expect("upgrade from Minco 1.12 must not hit a checksum mismatch");
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _minco_plugin_storage_migrations")
        .fetch_one(&database.pool)
        .await
        .unwrap();
    assert_eq!(applied, 4);
    let has_fingerprint: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'minco_audit' AND column_name = 'fingerprint')",
    )
    .fetch_one(&database.pool)
    .await
    .unwrap();
    assert!(has_fingerprint);

    let sink = PostgresAuditSink::new(database.pool.clone());

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
        .fetch_one(&database.pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);

    // Same id + different content → integrity conflict (R31: PostgreSQL
    // must no longer swallow the conflict with ON CONFLICT DO NOTHING).
    let mut conflicting = event.clone();
    conflicting.action = "ticket.updated".into();
    assert!(
        matches!(
            sink.append(conflicting).await,
            Err(error) if minco_plugin_audit::is_audit_conflict(&error)
        ),
        "the conflict rides the stable Append code"
    );

    // A pre-0004 row (NULL fingerprint) is content-verified and adopted
    // on redelivery of identical content; different content conflicts.
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
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL)",
    )
    .bind(legacy.id)
    .bind(&legacy.action)
    .bind(&legacy.resource_type)
    .bind(&legacy.resource_id)
    .bind(&legacy.actor_subject)
    .bind(legacy.correlation_id)
    .bind(legacy.occurred_at)
    .bind(serde_json::to_value(&legacy.metadata).unwrap())
    .execute(&database.pool)
    .await
    .unwrap();
    sink.append(legacy.clone()).await.unwrap();
    let adopted: Option<String> =
        sqlx::query_scalar("SELECT fingerprint FROM minco_audit WHERE id = $1")
            .bind(legacy.id)
            .fetch_one(&database.pool)
            .await
            .unwrap();
    assert_eq!(
        adopted.as_deref(),
        Some(minco_plugin_audit::event_fingerprint(&legacy).as_str())
    );
    let mut legacy_conflict = legacy.clone();
    legacy_conflict.resource_id = "queue-2".into();
    assert!(
        matches!(
            sink.append(legacy_conflict).await,
            Err(error) if minco_plugin_audit::is_audit_conflict(&error)
        ),
        "the unadoption conflict rides the stable Append code"
    );
}
