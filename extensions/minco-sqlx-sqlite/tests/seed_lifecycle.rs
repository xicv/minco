use minco_db::{SEED_SET_MANIFEST, SeedClass, SeedEnvironment, build_seed_plan, load_seed_catalog};
use minco_sqlx_sqlite::{SqlitePoolConfig, apply_seed_plan, connect, verify_seed_plan};
use std::{fs, path::PathBuf};
use tempfile::TempDir;

#[tokio::test]
async fn required_transaction_seed_plan_is_idempotent_and_verifiable() {
    let project = TempDir::new().expect("temporary project");
    let root = project.path().join("seeds");
    fs::create_dir(&root).expect("create seed root");
    fs::write(
        root.join("demo.sql"),
        concat!(
            "INSERT INTO example (id, value) VALUES (1, 'demo') ",
            "ON CONFLICT (id) DO UPDATE SET value = excluded.value;\n",
        ),
    )
    .expect("write seed SQL");
    fs::write(
        root.join("demo.verify.sql"),
        "SELECT EXISTS (SELECT 1 FROM example WHERE id = 1 AND value = 'demo');\n",
    )
    .expect("write seed verification");
    fs::write(
        root.join(SEED_SET_MANIFEST),
        concat!(
            "schema = 1\n",
            "id = \"test-sqlite-seeds\"\n",
            "owner = \"application:test\"\n",
            "backend = \"sqlite\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"test-sqlite-demo\"\n",
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
        Some("test-sqlite-seeds"),
    )
    .expect("build seed plan");
    let config = SqlitePoolConfig::file(project.path().join("test.sqlite"));
    let pool = connect(&config).await.expect("connect SQLite");
    sqlx::query("CREATE TABLE example (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
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
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM example")
        .fetch_one(&pool)
        .await
        .expect("count seeded rows");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn seed_verification_is_enforced_as_read_only_by_sqlite() {
    let project = TempDir::new().expect("temporary project");
    let root = project.path().join("seeds");
    fs::create_dir(&root).expect("create seed root");
    fs::write(
        root.join("demo.sql"),
        "INSERT INTO example (id, value) VALUES (1, 'demo');\n",
    )
    .expect("write seed SQL");
    fs::write(
        root.join("demo.verify.sql"),
        "DELETE FROM example WHERE id = 1 RETURNING true;\n",
    )
    .expect("write mutating seed verification");
    fs::write(
        root.join(SEED_SET_MANIFEST),
        concat!(
            "schema = 1\n",
            "id = \"test-sqlite-read-only\"\n",
            "owner = \"application:test\"\n",
            "backend = \"sqlite\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"test-sqlite-read-only-demo\"\n",
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
        Some("test-sqlite-read-only"),
    )
    .expect("build seed plan");
    let pool = connect(&SqlitePoolConfig::file(
        project.path().join("read-only.sqlite"),
    ))
    .await
    .expect("connect SQLite");
    sqlx::query("CREATE TABLE example (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
        .execute(&pool)
        .await
        .expect("create target table");
    apply_seed_plan(&pool, project.path(), &plan)
        .await
        .expect("apply seed plan");

    let error = verify_seed_plan(&pool, project.path(), &plan)
        .await
        .expect_err("mutating verification must be rejected");
    assert!(error.to_string().contains("readonly"));
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM example")
        .fetch_one(&pool)
        .await
        .expect("count seeded rows");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn changed_source_is_rejected_before_the_target_is_mutated() {
    let project = TempDir::new().expect("temporary project");
    let root = project.path().join("seeds");
    fs::create_dir(&root).expect("create seed root");
    fs::write(
        root.join("demo.sql"),
        "INSERT INTO example (id, value) VALUES (1, 'planned');\n",
    )
    .expect("write seed SQL");
    fs::write(root.join("demo.verify.sql"), "SELECT true;\n").expect("write verification SQL");
    fs::write(
        root.join(SEED_SET_MANIFEST),
        concat!(
            "schema = 1\n",
            "id = \"test-sqlite-drift\"\n",
            "owner = \"application:test\"\n",
            "backend = \"sqlite\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"test-sqlite-drift-demo\"\n",
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
        Some("test-sqlite-drift"),
    )
    .expect("build seed plan");
    let pool = connect(&SqlitePoolConfig::file(project.path().join("drift.sqlite")))
        .await
        .expect("connect SQLite");
    sqlx::query("CREATE TABLE example (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
        .execute(&pool)
        .await
        .expect("create target table");
    fs::write(
        root.join("demo.sql"),
        "INSERT INTO example (id, value) VALUES (1, 'changed');\n",
    )
    .expect("change source after planning");

    let error = apply_seed_plan(&pool, project.path(), &plan)
        .await
        .expect_err("changed source must fail");
    assert!(error.to_string().contains("changed after planning"));
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM example")
        .fetch_one(&pool)
        .await
        .expect("count target rows");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn required_transaction_rolls_back_the_whole_seed_plan() {
    let project = TempDir::new().expect("temporary project");
    let root = project.path().join("seeds");
    fs::create_dir(&root).expect("create seed root");
    fs::write(
        root.join("first.sql"),
        "INSERT INTO example (id, value) VALUES (1, 'must-roll-back');\n",
    )
    .expect("write first seed SQL");
    fs::write(root.join("first.verify.sql"), "SELECT true;\n")
        .expect("write first verification SQL");
    fs::write(
        root.join("second.sql"),
        "INSERT INTO table_that_does_not_exist (id) VALUES (1);\n",
    )
    .expect("write failing seed SQL");
    fs::write(root.join("second.verify.sql"), "SELECT true;\n")
        .expect("write second verification SQL");
    fs::write(
        root.join(SEED_SET_MANIFEST),
        concat!(
            "schema = 1\n",
            "id = \"test-sqlite-rollback\"\n",
            "owner = \"application:test\"\n",
            "backend = \"sqlite\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"test-sqlite-rollback-first\"\n",
            "version = 1\n",
            "class = \"demo\"\n",
            "source = \"first.sql\"\n",
            "verify = \"first.verify.sql\"\n",
            "depends_on = []\n",
            "environments = [\"local\"]\n",
            "idempotency = \"insert_once\"\n",
            "mutable_state = \"none\"\n",
            "risk = \"non_destructive\"\n",
            "transaction = \"required\"\n",
            "preservation = \"preserve_all_existing\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"test-sqlite-rollback-second\"\n",
            "version = 1\n",
            "class = \"demo\"\n",
            "source = \"second.sql\"\n",
            "verify = \"second.verify.sql\"\n",
            "depends_on = [\"test-sqlite-rollback-first\"]\n",
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
        Some("test-sqlite-rollback"),
    )
    .expect("build seed plan");
    let pool = connect(&SqlitePoolConfig::file(
        project.path().join("rollback.sqlite"),
    ))
    .await
    .expect("connect SQLite");
    sqlx::query("CREATE TABLE example (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
        .execute(&pool)
        .await
        .expect("create target table");

    apply_seed_plan(&pool, project.path(), &plan)
        .await
        .expect_err("second seed must fail");
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM example")
        .fetch_one(&pool)
        .await
        .expect("count target rows");
    assert_eq!(count, 0);
}
