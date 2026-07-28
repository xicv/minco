use minco_db::{
    SEED_SET_MANIFEST, SeedClass, SeedEnvironment, SeedIdempotency, SeedMutableState,
    SeedPreservation, SeedRisk, SeedTransaction, build_seed_plan, load_seed_catalog,
    validate_seed_plan,
};
use std::{fs, path::PathBuf};
use tempfile::TempDir;

#[test]
fn seed_catalog_is_deterministic_and_carries_safety_metadata() {
    let project = TempDir::new().expect("temporary project");
    let root = project.path().join("seeds");
    fs::create_dir(&root).expect("create seed root");
    fs::write(
        root.join("reference.sql"),
        "INSERT INTO example (id, value) VALUES (1, 'reference') ON CONFLICT (id) DO NOTHING;\n",
    )
    .expect("write seed SQL");
    fs::write(
        root.join("reference.verify.sql"),
        "SELECT COUNT(*) = 1 FROM example WHERE id = 1;\n",
    )
    .expect("write seed verification");
    fs::write(
        root.join(SEED_SET_MANIFEST),
        concat!(
            "schema = 1\n",
            "id = \"orders-sqlite-seeds\"\n",
            "owner = \"application:orders\"\n",
            "backend = \"sqlite\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"orders-reference\"\n",
            "version = 1\n",
            "class = \"reference\"\n",
            "source = \"reference.sql\"\n",
            "verify = \"reference.verify.sql\"\n",
            "depends_on = []\n",
            "environments = [\"local\", \"development\", \"test\", \"staging\", \"production\"]\n",
            "idempotency = \"insert_once\"\n",
            "mutable_state = \"none\"\n",
            "risk = \"non_destructive\"\n",
            "transaction = \"required\"\n",
            "preservation = \"preserve_all_existing\"\n",
        ),
    )
    .expect("write seed manifest");

    let first =
        load_seed_catalog(project.path(), &[PathBuf::from("seeds")]).expect("load seed catalog");
    let second =
        load_seed_catalog(project.path(), &[PathBuf::from("seeds")]).expect("reload seed catalog");

    assert_eq!(first, second);
    assert_eq!(first.schema_version, 1);
    assert_eq!(first.digest.len(), 64);
    assert_eq!(first.sets.len(), 1);
    let seed = &first.sets[0].seeds[0];
    assert_eq!(seed.class, SeedClass::Reference);
    assert_eq!(seed.idempotency, SeedIdempotency::InsertOnce);
    assert_eq!(seed.mutable_state, SeedMutableState::None);
    assert_eq!(seed.risk, SeedRisk::NonDestructive);
    assert_eq!(seed.transaction, SeedTransaction::Required);
    assert_eq!(seed.preservation, SeedPreservation::PreserveAllExisting);
    assert_eq!(seed.source_sha256.len(), 64);
    assert_eq!(seed.verify_sha256.len(), 64);
}

#[test]
fn demo_plan_fails_in_production_and_orders_its_local_dependencies() {
    let project = TempDir::new().expect("temporary project");
    let root = project.path().join("seeds");
    fs::create_dir(&root).expect("create seed root");
    for name in ["reference", "demo"] {
        fs::write(
            root.join(format!("{name}.sql")),
            format!("SELECT '{name}';\n"),
        )
        .expect("write seed SQL");
        fs::write(root.join(format!("{name}.verify.sql")), "SELECT true;\n")
            .expect("write seed verification");
    }
    fs::write(
        root.join(SEED_SET_MANIFEST),
        concat!(
            "schema = 1\n",
            "id = \"orders-sqlite-seeds\"\n",
            "owner = \"application:orders\"\n",
            "backend = \"sqlite\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"orders-reference\"\n",
            "version = 1\n",
            "class = \"reference\"\n",
            "source = \"reference.sql\"\n",
            "verify = \"reference.verify.sql\"\n",
            "depends_on = []\n",
            "environments = [\"local\", \"development\", \"test\", \"staging\", \"production\"]\n",
            "idempotency = \"insert_once\"\n",
            "mutable_state = \"none\"\n",
            "risk = \"non_destructive\"\n",
            "transaction = \"required\"\n",
            "preservation = \"preserve_all_existing\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"orders-demo\"\n",
            "version = 1\n",
            "class = \"demo\"\n",
            "source = \"demo.sql\"\n",
            "verify = \"demo.verify.sql\"\n",
            "depends_on = [\"orders-reference\"]\n",
            "environments = [\"local\", \"development\"]\n",
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

    let error = build_seed_plan(
        &catalog,
        SeedClass::Demo,
        SeedEnvironment::Production,
        Some("orders-sqlite-seeds"),
    )
    .expect_err("demo seeding must fail closed in production");
    assert!(error.to_string().contains("demo"));

    let first = build_seed_plan(
        &catalog,
        SeedClass::Demo,
        SeedEnvironment::Local,
        Some("orders-sqlite-seeds"),
    )
    .expect("build local demo plan");
    let second = build_seed_plan(
        &catalog,
        SeedClass::Demo,
        SeedEnvironment::Local,
        Some("orders-sqlite-seeds"),
    )
    .expect("rebuild local demo plan");
    assert_eq!(first, second);
    assert_eq!(
        first
            .seeds
            .iter()
            .map(|seed| seed.id.as_str())
            .collect::<Vec<_>>(),
        ["orders-reference", "orders-demo"]
    );
    assert_eq!(first.digest.len(), 64);

    let mut tampered_production_plan = first;
    tampered_production_plan.environment = SeedEnvironment::Production;
    let error =
        validate_seed_plan(&tampered_production_plan).expect_err("adapter policy must fail closed");
    assert!(error.to_string().contains("forbidden in production"));

    let mut mixed_transactions = catalog;
    mixed_transactions.sets[0]
        .seeds
        .iter_mut()
        .find(|seed| seed.id == "orders-demo")
        .expect("demo seed")
        .transaction = SeedTransaction::Autocommit;
    let error = build_seed_plan(
        &mixed_transactions,
        SeedClass::Demo,
        SeedEnvironment::Local,
        Some("orders-sqlite-seeds"),
    )
    .expect_err("one executable plan cannot mix transaction behaviors");
    assert!(error.to_string().contains("transaction behaviors"));
}

#[test]
fn production_rejects_demo_or_test_dependencies_even_for_a_reference_profile() {
    let project = TempDir::new().expect("temporary project");
    let root = project.path().join("seeds");
    fs::create_dir(&root).expect("create seed root");
    for name in ["demo", "reference"] {
        fs::write(root.join(format!("{name}.sql")), "SELECT true;\n").expect("write seed SQL");
        fs::write(root.join(format!("{name}.verify.sql")), "SELECT true;\n")
            .expect("write seed verification");
    }
    fs::write(
        root.join(SEED_SET_MANIFEST),
        concat!(
            "schema = 1\n",
            "id = \"unsafe-reference-chain\"\n",
            "owner = \"application:orders\"\n",
            "backend = \"sqlite\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"hidden-demo-dependency\"\n",
            "version = 1\n",
            "class = \"demo\"\n",
            "source = \"demo.sql\"\n",
            "verify = \"demo.verify.sql\"\n",
            "depends_on = []\n",
            "environments = [\"production\"]\n",
            "idempotency = \"insert_once\"\n",
            "mutable_state = \"none\"\n",
            "risk = \"non_destructive\"\n",
            "transaction = \"required\"\n",
            "preservation = \"preserve_all_existing\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"apparently-safe-reference\"\n",
            "version = 1\n",
            "class = \"reference\"\n",
            "source = \"reference.sql\"\n",
            "verify = \"reference.verify.sql\"\n",
            "depends_on = [\"hidden-demo-dependency\"]\n",
            "environments = [\"production\"]\n",
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

    let error = build_seed_plan(
        &catalog,
        SeedClass::Reference,
        SeedEnvironment::Production,
        Some("unsafe-reference-chain"),
    )
    .expect_err("production must inspect the complete dependency closure");
    assert!(error.to_string().contains("hidden-demo-dependency"));
    assert!(error.to_string().contains("forbidden in production"));
}

#[test]
fn source_catalog_rejects_unknown_dependencies_and_duplicate_environments() {
    let project = TempDir::new().expect("temporary project");
    let root = project.path().join("seeds");
    fs::create_dir(&root).expect("create seed root");
    fs::write(root.join("demo.sql"), "SELECT true;\n").expect("write seed SQL");
    fs::write(root.join("demo.verify.sql"), "SELECT true;\n").expect("write seed verification");
    let manifest = |depends_on: &str, environments: &str| {
        format!(
            concat!(
                "schema = 1\n",
                "id = \"strict-source-seeds\"\n",
                "owner = \"application:orders\"\n",
                "backend = \"sqlite\"\n",
                "\n",
                "[[seed]]\n",
                "id = \"strict-source-demo\"\n",
                "version = 1\n",
                "class = \"demo\"\n",
                "source = \"demo.sql\"\n",
                "verify = \"demo.verify.sql\"\n",
                "depends_on = {depends_on}\n",
                "environments = {environments}\n",
                "idempotency = \"insert_once\"\n",
                "mutable_state = \"none\"\n",
                "risk = \"non_destructive\"\n",
                "transaction = \"required\"\n",
                "preservation = \"preserve_all_existing\"\n",
            ),
            depends_on = depends_on,
            environments = environments,
        )
    };
    fs::write(
        root.join(SEED_SET_MANIFEST),
        manifest("[\"missing-seed\"]", "[\"local\"]"),
    )
    .expect("write seed manifest");

    let error = load_seed_catalog(project.path(), &[PathBuf::from("seeds")])
        .expect_err("unknown dependencies must fail source verification");
    assert!(error.to_string().contains("unknown seed"));

    fs::write(
        root.join(SEED_SET_MANIFEST),
        manifest("[]", "[\"local\", \"local\"]"),
    )
    .expect("write duplicate environment manifest");
    let error = load_seed_catalog(project.path(), &[PathBuf::from("seeds")])
        .expect_err("duplicate environment entries must fail source verification");
    assert!(error.to_string().contains("repeats an environment"));
}
