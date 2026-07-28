use minco_db::{SEED_SET_MANIFEST, SeedClass, SeedEnvironment, build_seed_plan, load_seed_catalog};
use minco_sqlx_postgres::{PostgresPoolConfig, apply_seed_plan, connect, verify_seed_plan};
use std::{fs, path::PathBuf};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn required_transaction_seed_plan_is_idempotent_and_verifiable() {
    let Ok(url) = std::env::var("MINCO_TEST_POSTGRES_URL") else {
        eprintln!("MINCO_TEST_POSTGRES_URL not set; PostgreSQL seed proof skipped");
        return;
    };
    let suffix = Uuid::new_v4().simple().to_string();
    let table = format!("minco_seed_{suffix}");
    let project = TempDir::new().expect("temporary project");
    let root = project.path().join("seeds");
    fs::create_dir(&root).expect("create seed root");
    fs::write(
        root.join("demo.sql"),
        format!(
            concat!(
                "INSERT INTO {} (id, value) VALUES (1, 'demo') ",
                "ON CONFLICT (id) DO UPDATE SET value = EXCLUDED.value;\n",
            ),
            table,
        ),
    )
    .expect("write seed SQL");
    fs::write(
        root.join("demo.verify.sql"),
        format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE id = 1 AND value = 'demo');\n"),
    )
    .expect("write seed verification");
    fs::write(
        root.join(SEED_SET_MANIFEST),
        concat!(
            "schema = 1\n",
            "id = \"test-postgres-seeds\"\n",
            "owner = \"application:test\"\n",
            "backend = \"postgres\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"test-postgres-demo\"\n",
            "version = 1\n",
            "class = \"demo\"\n",
            "source = \"demo.sql\"\n",
            "verify = \"demo.verify.sql\"\n",
            "depends_on = []\n",
            "environments = [\"local\"]\n",
            "idempotency = \"upsert\"\n",
            "mutable_state = \"owned_rows\"\n",
            "risk = \"replaces_owned_rows\"\n",
            "transaction = \"required\"\n",
            "preservation = \"preserve_unowned_rows\"\n",
        ),
    )
    .expect("write seed manifest");
    let catalog =
        load_seed_catalog(project.path(), &[PathBuf::from("seeds")]).expect("load seed catalog");
    let plan = build_seed_plan(
        &catalog,
        SeedClass::Demo,
        SeedEnvironment::Local,
        Some("test-postgres-seeds"),
    )
    .expect("build seed plan");
    let pool = connect(&PostgresPoolConfig::serverless(url))
        .await
        .expect("connect PostgreSQL");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TABLE {table} (id BIGINT PRIMARY KEY, value TEXT NOT NULL)"
    )))
    .execute(&pool)
    .await
    .expect("create target table");

    apply_seed_plan(&pool, project.path(), &plan)
        .await
        .expect("apply first seed plan");
    apply_seed_plan(&pool, project.path(), &plan)
        .await
        .expect("reapply idempotent seed plan");

    let verification = verify_seed_plan(&pool, project.path(), &plan)
        .await
        .expect("verify seed plan");
    assert!(verification.iter().all(|entry| entry.verified));
    let count =
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
            .fetch_one(&pool)
            .await
            .expect("count seeded rows");
    assert_eq!(count, 1);

    sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE {table}")))
        .execute(&pool)
        .await
        .expect("clean up target table");
}

#[tokio::test]
async fn seed_verification_is_enforced_as_read_only_by_postgres() {
    let Ok(url) = std::env::var("MINCO_TEST_POSTGRES_URL") else {
        eprintln!("MINCO_TEST_POSTGRES_URL not set; PostgreSQL seed proof skipped");
        return;
    };
    let suffix = Uuid::new_v4().simple().to_string();
    let table = format!("minco_seed_read_only_{suffix}");
    let project = TempDir::new().expect("temporary project");
    let root = project.path().join("seeds");
    fs::create_dir(&root).expect("create seed root");
    fs::write(
        root.join("demo.sql"),
        format!("INSERT INTO {table} (id, value) VALUES (1, 'demo');\n"),
    )
    .expect("write seed SQL");
    fs::write(
        root.join("demo.verify.sql"),
        format!("DELETE FROM {table} WHERE id = 1 RETURNING true;\n"),
    )
    .expect("write mutating verification SQL");
    fs::write(
        root.join(SEED_SET_MANIFEST),
        concat!(
            "schema = 1\n",
            "id = \"test-postgres-read-only-seeds\"\n",
            "owner = \"application:test\"\n",
            "backend = \"postgres\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"test-postgres-read-only-demo\"\n",
            "version = 1\n",
            "class = \"demo\"\n",
            "source = \"demo.sql\"\n",
            "verify = \"demo.verify.sql\"\n",
            "depends_on = []\n",
            "environments = [\"local\"]\n",
            "idempotency = \"insert_once\"\n",
            "mutable_state = \"none\"\n",
            "risk = \"non_destructive\"\n",
            "transaction = \"required\"\n",
            "preservation = \"preserve_all_existing\"\n",
        ),
    )
    .expect("write seed manifest");
    let catalog =
        load_seed_catalog(project.path(), &[PathBuf::from("seeds")]).expect("load seed catalog");
    let plan = build_seed_plan(
        &catalog,
        SeedClass::Demo,
        SeedEnvironment::Local,
        Some("test-postgres-read-only-seeds"),
    )
    .expect("build seed plan");
    let pool = connect(&PostgresPoolConfig::serverless(url))
        .await
        .expect("connect PostgreSQL");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TABLE {table} (id BIGINT PRIMARY KEY, value TEXT NOT NULL)"
    )))
    .execute(&pool)
    .await
    .expect("create target table");
    apply_seed_plan(&pool, project.path(), &plan)
        .await
        .expect("apply seed plan");

    let error = verify_seed_plan(&pool, project.path(), &plan)
        .await
        .expect_err("mutating verification must be rejected");
    assert!(error.to_string().contains("read-only transaction"));
    let count =
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
            .fetch_one(&pool)
            .await
            .expect("count seeded rows");
    assert_eq!(count, 1);

    sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE {table}")))
        .execute(&pool)
        .await
        .expect("clean up target table");
}
