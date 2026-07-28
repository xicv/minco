use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use uuid::Uuid;

fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("cargo-minco lives under the repository crates directory")
}

fn run_minco(arguments: &[&str], environment: Option<(&str, &str)>) -> Output {
    run_minco_at(repository_root(), arguments, environment)
}

fn run_minco_at(root: &Path, arguments: &[&str], environment: Option<(&str, &str)>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-minco"));
    command.args([
        "--root",
        root.to_str().expect("UTF-8 project path"),
        "--json",
    ]);
    command.args(arguments);
    if let Some((name, value)) = environment {
        command.env(name, value);
    }
    command.output().expect("run cargo-minco")
}

struct ProjectTarget {
    relative_directory: PathBuf,
}

impl ProjectTarget {
    fn new() -> Self {
        let relative_directory =
            PathBuf::from("target/minco").join(format!("seed-cli-{}", Uuid::now_v7()));
        fs::create_dir_all(repository_root().join(&relative_directory))
            .expect("create project-contained test directory");
        Self { relative_directory }
    }

    fn relative(&self, name: &str) -> PathBuf {
        self.relative_directory.join(name)
    }

    fn absolute(&self, name: &str) -> PathBuf {
        repository_root().join(self.relative(name))
    }
}

impl Drop for ProjectTarget {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(repository_root().join(&self.relative_directory));
    }
}

#[test]
fn demo_dry_run_emits_a_deterministic_source_plan_without_a_credential() {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args([
            "--root",
            repository_root().to_str().expect("UTF-8 repository path"),
            "--json",
            "db",
            "seed",
            "--profile",
            "demo",
            "--environment",
            "local",
            "--set",
            "orders-sqlite-seeds",
            "--dry-run",
        ])
        .env_remove("MINCO_SEED_DATABASE_URL")
        .output()
        .expect("run cargo-minco");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).expect("JSON seed plan");
    assert_eq!(plan["profile"], "demo");
    assert_eq!(plan["environment"], "local");
    assert_eq!(plan["selected_set"], "orders-sqlite-seeds");
    assert_eq!(plan["seeds"][0]["id"], "orders-sqlite-demo");
    assert_eq!(plan["digest"].as_str().expect("plan digest").len(), 64);
}

#[test]
fn source_verification_is_explicitly_target_free() {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args([
            "--root",
            repository_root().to_str().expect("UTF-8 repository path"),
            "--json",
            "db",
            "seed",
            "--verify",
        ])
        .env_remove("MINCO_SEED_DATABASE_URL")
        .output()
        .expect("run cargo-minco");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let verification: Value =
        serde_json::from_slice(&output.stdout).expect("JSON seed verification");
    assert_eq!(verification["source_verified"], true);
    assert_eq!(verification["target_inspected"], false);
    assert_eq!(verification["target_verified"], Value::Null);
    assert_eq!(
        verification["catalog_digest"]
            .as_str()
            .expect("catalog digest")
            .len(),
        64
    );

    let ignored_environment = run_minco(
        &["db", "seed", "--verify", "--environment", "production"],
        None,
    );
    assert!(!ignored_environment.status.success());
    assert!(
        String::from_utf8_lossy(&ignored_environment.stderr)
            .contains("source seed verification cannot include planning")
    );
}

#[test]
fn sqlite_execution_requires_the_reviewed_digest_and_emits_a_verified_receipt() {
    let target = ProjectTarget::new();
    let database_url = format!("sqlite://{}", target.absolute("orders.sqlite").display());
    let migration_plan = run_minco(&["db", "plan", "--set", "orders-sqlite"], None);
    assert!(migration_plan.status.success());
    let migration_plan: Value =
        serde_json::from_slice(&migration_plan.stdout).expect("migration plan JSON");
    let migration_digest = migration_plan["digest"]
        .as_str()
        .expect("migration plan digest");
    let migration_receipt = target.relative("migration-receipt.json");
    let migration = run_minco(
        &[
            "db",
            "migrate",
            "--set",
            "orders-sqlite",
            "--database-url-env",
            "MINCO_SEED_TEST_DATABASE_URL",
            "--expected-plan-digest",
            migration_digest,
            "--receipt",
            migration_receipt
                .to_str()
                .expect("UTF-8 migration receipt path"),
        ],
        Some(("MINCO_SEED_TEST_DATABASE_URL", &database_url)),
    );
    assert!(
        migration.status.success(),
        "migration stderr: {}",
        String::from_utf8_lossy(&migration.stderr)
    );

    let seed_plan = run_minco(
        &[
            "db",
            "seed",
            "--profile",
            "demo",
            "--environment",
            "local",
            "--set",
            "orders-sqlite-seeds",
            "--dry-run",
        ],
        None,
    );
    assert!(seed_plan.status.success());
    let seed_plan: Value = serde_json::from_slice(&seed_plan.stdout).expect("seed plan JSON");
    let seed_digest = seed_plan["digest"].as_str().expect("seed plan digest");
    let seed_receipt = target.relative("seed-receipt.json");

    let stale_receipt = target.relative("stale-seed-receipt.json");
    let stale = run_minco(
        &[
            "db",
            "seed",
            "--profile",
            "demo",
            "--environment",
            "local",
            "--set",
            "orders-sqlite-seeds",
            "--database-url-env",
            "MINCO_SEED_TEST_DATABASE_URL",
            "--expected-plan-digest",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--receipt",
            stale_receipt
                .to_str()
                .expect("UTF-8 stale seed receipt path"),
        ],
        Some(("MINCO_SEED_TEST_DATABASE_URL", &database_url)),
    );
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("seed plan digest changed"));
    assert!(!target.absolute("stale-seed-receipt.json").exists());

    let absent = run_minco(
        &[
            "db",
            "seed",
            "--verify",
            "--profile",
            "demo",
            "--environment",
            "local",
            "--set",
            "orders-sqlite-seeds",
            "--database-url-env",
            "MINCO_SEED_TEST_DATABASE_URL",
        ],
        Some(("MINCO_SEED_TEST_DATABASE_URL", &database_url)),
    );
    assert!(!absent.status.success());
    let absent: Value =
        serde_json::from_slice(&absent.stdout).expect("failed target verification JSON");
    assert_eq!(absent["target_inspected"], true);
    assert_eq!(absent["target_verified"], false);

    let seed = run_minco(
        &[
            "db",
            "seed",
            "--profile",
            "demo",
            "--environment",
            "local",
            "--set",
            "orders-sqlite-seeds",
            "--database-url-env",
            "MINCO_SEED_TEST_DATABASE_URL",
            "--expected-plan-digest",
            seed_digest,
            "--receipt",
            seed_receipt.to_str().expect("UTF-8 seed receipt path"),
        ],
        Some(("MINCO_SEED_TEST_DATABASE_URL", &database_url)),
    );
    assert!(
        seed.status.success(),
        "seed stderr: {}",
        String::from_utf8_lossy(&seed.stderr)
    );
    let receipt: Value = serde_json::from_slice(&seed.stdout).expect("seed receipt JSON");
    assert_eq!(receipt["outcome"], "succeeded");
    assert_eq!(receipt["plan_digest"], seed_digest);
    assert_eq!(receipt["target_verified"], true);
    assert!(receipt.get("database_url").is_none());

    let verification = run_minco(
        &[
            "db",
            "seed",
            "--verify",
            "--profile",
            "demo",
            "--environment",
            "local",
            "--set",
            "orders-sqlite-seeds",
            "--database-url-env",
            "MINCO_SEED_TEST_DATABASE_URL",
        ],
        Some(("MINCO_SEED_TEST_DATABASE_URL", &database_url)),
    );
    assert!(
        verification.status.success(),
        "verification stderr: {}",
        String::from_utf8_lossy(&verification.stderr)
    );
    let verification: Value =
        serde_json::from_slice(&verification.stdout).expect("seed target verification JSON");
    assert_eq!(verification["plan_digest"], seed_digest);
    assert_eq!(verification["selected_set"], "orders-sqlite-seeds");
    assert_eq!(verification["target_inspected"], true);
    assert_eq!(verification["target_verified"], true);
    assert!(
        verification["verification"]
            .as_array()
            .expect("seed verification entries")
            .iter()
            .all(|entry| entry["verified"] == true)
    );
}

#[test]
fn bootstrap_execution_requires_exact_environment_authorization_and_records_it() {
    let target = ProjectTarget::new();
    let project = target.absolute("bootstrap-project");
    let seeds = project.join("seeds");
    fs::create_dir_all(&seeds).expect("create bootstrap seed project");
    fs::write(
        project.join("minco.toml"),
        concat!(
            "schema = 1\n",
            "name = \"bootstrap-test\"\n",
            "contract = \"contract.yaml\"\n",
            "generated = \"generated.rs\"\n",
            "deployment_config = \"deployment.toml\"\n",
            "roadmap = \"roadmap.yaml\"\n",
            "tasks = \"tasks\"\n",
            "plugin_catalog = \"plugins.toml\"\n",
            "quality = \"quality.toml\"\n",
            "\n",
            "[seeds]\n",
            "roots = [\"seeds\"]\n",
        ),
    )
    .expect("write temporary minco manifest");
    fs::write(
        seeds.join(".minco-seeds.toml"),
        concat!(
            "schema = 1\n",
            "id = \"bootstrap-sqlite-seeds\"\n",
            "owner = \"application:bootstrap-test\"\n",
            "backend = \"sqlite\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"bootstrap-admin\"\n",
            "version = 1\n",
            "class = \"bootstrap\"\n",
            "source = \"bootstrap.sql\"\n",
            "verify = \"bootstrap.verify.sql\"\n",
            "depends_on = []\n",
            "environments = [\"staging\"]\n",
            "idempotency = \"upsert\"\n",
            "mutable_state = \"owned_rows\"\n",
            "risk = \"replaces_owned_rows\"\n",
            "transaction = \"required\"\n",
            "preservation = \"preserve_unowned_rows\"\n",
            "\n",
            "[[seed]]\n",
            "id = \"bootstrap-reference\"\n",
            "version = 1\n",
            "class = \"reference\"\n",
            "source = \"reference.sql\"\n",
            "verify = \"reference.verify.sql\"\n",
            "depends_on = [\"bootstrap-admin\"]\n",
            "environments = [\"staging\"]\n",
            "idempotency = \"insert_once\"\n",
            "mutable_state = \"none\"\n",
            "risk = \"non_destructive\"\n",
            "transaction = \"required\"\n",
            "preservation = \"preserve_all_existing\"\n",
        ),
    )
    .expect("write bootstrap seed manifest");
    fs::write(
        seeds.join("bootstrap.sql"),
        concat!(
            "CREATE TABLE IF NOT EXISTS bootstrap_identity (",
            "id INTEGER PRIMARY KEY, name TEXT NOT NULL);\n",
            "INSERT INTO bootstrap_identity (id, name) VALUES (1, 'bootstrap') ",
            "ON CONFLICT (id) DO UPDATE SET name = excluded.name;\n",
        ),
    )
    .expect("write bootstrap seed SQL");
    fs::write(
        seeds.join("bootstrap.verify.sql"),
        concat!(
            "SELECT EXISTS (SELECT 1 FROM bootstrap_identity ",
            "WHERE id = 1 AND name = 'bootstrap');\n",
        ),
    )
    .expect("write bootstrap verification SQL");
    fs::write(seeds.join("reference.sql"), "SELECT 1;\n").expect("write reference seed SQL");
    fs::write(seeds.join("reference.verify.sql"), "SELECT true;\n")
        .expect("write reference verification SQL");

    let plan = run_minco_at(
        &project,
        &[
            "db",
            "seed",
            "--profile",
            "bootstrap",
            "--environment",
            "staging",
            "--set",
            "bootstrap-sqlite-seeds",
            "--dry-run",
        ],
        None,
    );
    assert!(
        plan.status.success(),
        "plan stderr: {}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let plan: Value = serde_json::from_slice(&plan.stdout).expect("bootstrap plan JSON");
    let digest = plan["digest"].as_str().expect("bootstrap plan digest");
    let receipt = PathBuf::from("seed-receipt.json");
    let database_url = format!("sqlite://{}", project.join("bootstrap.sqlite").display());

    let unauthorized = run_minco_at(
        &project,
        &[
            "db",
            "seed",
            "--profile",
            "bootstrap",
            "--environment",
            "staging",
            "--set",
            "bootstrap-sqlite-seeds",
            "--database-url-env",
            "MINCO_BOOTSTRAP_TEST_DATABASE_URL",
            "--expected-plan-digest",
            digest,
            "--receipt",
            "seed-receipt.json",
        ],
        Some(("MINCO_BOOTSTRAP_TEST_DATABASE_URL", &database_url)),
    );
    assert!(!unauthorized.status.success());
    assert!(
        String::from_utf8_lossy(&unauthorized.stderr)
            .contains("requires --authorize-bootstrap staging")
    );
    assert!(!project.join(&receipt).exists());

    let reference_plan = run_minco_at(
        &project,
        &[
            "db",
            "seed",
            "--profile",
            "reference",
            "--environment",
            "staging",
            "--set",
            "bootstrap-sqlite-seeds",
            "--dry-run",
        ],
        None,
    );
    assert!(reference_plan.status.success());
    let reference_plan: Value =
        serde_json::from_slice(&reference_plan.stdout).expect("reference plan JSON");
    let reference_digest = reference_plan["digest"]
        .as_str()
        .expect("reference plan digest");
    let dependency_bypass = run_minco_at(
        &project,
        &[
            "db",
            "seed",
            "--profile",
            "reference",
            "--environment",
            "staging",
            "--set",
            "bootstrap-sqlite-seeds",
            "--database-url-env",
            "MINCO_BOOTSTRAP_TEST_DATABASE_URL",
            "--expected-plan-digest",
            reference_digest,
            "--receipt",
            "reference-receipt.json",
        ],
        Some(("MINCO_BOOTSTRAP_TEST_DATABASE_URL", &database_url)),
    );
    assert!(!dependency_bypass.status.success());
    assert!(
        String::from_utf8_lossy(&dependency_bypass.stderr)
            .contains("requires --authorize-bootstrap staging")
    );
    assert!(!project.join("reference-receipt.json").exists());

    let authorized = run_minco_at(
        &project,
        &[
            "db",
            "seed",
            "--profile",
            "bootstrap",
            "--environment",
            "staging",
            "--set",
            "bootstrap-sqlite-seeds",
            "--database-url-env",
            "MINCO_BOOTSTRAP_TEST_DATABASE_URL",
            "--expected-plan-digest",
            digest,
            "--receipt",
            "seed-receipt.json",
            "--authorize-bootstrap",
            "staging",
        ],
        Some(("MINCO_BOOTSTRAP_TEST_DATABASE_URL", &database_url)),
    );
    assert!(
        authorized.status.success(),
        "authorized stderr: {}",
        String::from_utf8_lossy(&authorized.stderr)
    );
    let authorized: Value =
        serde_json::from_slice(&authorized.stdout).expect("bootstrap receipt JSON");
    assert_eq!(authorized["outcome"], "succeeded");
    assert_eq!(authorized["bootstrap_authorized"], true);
    assert_eq!(authorized["target_verified"], true);
    assert!(project.join(receipt).is_file());
}
