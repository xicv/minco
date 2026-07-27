use minco_config::{
    ConfigLayer, ConfigSourceKind, ConfigurationField, ConfigurationGraph, ConfigurationSchema,
    ConfigurationValueKind, Environment, EnvironmentClass, SecretReference,
};
use serde::Deserialize;
use serde_json::json;

fn schema() -> ConfigurationSchema {
    ConfigurationSchema::try_from_fields([
        ConfigurationField {
            key: "application.name".into(),
            kind: ConfigurationValueKind::String,
            required: true,
            secret: false,
            description: "Application service name".into(),
            default: Some(json!("orders-default")),
        },
        ConfigurationField {
            key: "database.url".into(),
            kind: ConfigurationValueKind::String,
            required: true,
            secret: true,
            description: "Opaque database credential reference".into(),
            default: None,
        },
        ConfigurationField {
            key: "plugins.idempotency.claim_timeout_seconds".into(),
            kind: ConfigurationValueKind::Integer,
            required: false,
            secret: false,
            description: "Abandoned claim timeout".into(),
            default: Some(json!(300)),
        },
    ])
    .expect("valid static schema")
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct ApplicationConfig {
    name: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct DatabaseConfig {
    url: SecretReference,
}

#[test]
fn graph_is_typed_deterministic_strict_and_secret_safe() {
    let defaults = ConfigLayer::from_toml(
        ConfigSourceKind::DefaultFile,
        "config/default.toml",
        r#"
schema = 1

[values.application]
name = "orders-file"

[values.database]
url = "env:ORDERS_DATABASE_URL"
"#,
    )
    .expect("default layer");
    let production = ConfigLayer::from_toml(
        ConfigSourceKind::EnvironmentFile,
        "config/production.toml",
        r#"
schema = 1
environment_class = "production"

[values.plugins.idempotency]
claim_timeout_seconds = 600
"#,
    )
    .expect("production layer");
    let cli = ConfigLayer::from_pairs(
        ConfigSourceKind::CliOverride,
        "command line",
        [("application.name", json!("orders-cli"))],
    )
    .expect("unique CLI paths");

    let graph = ConfigurationGraph::compile(
        &schema(),
        Environment::new("production", EnvironmentClass::Production),
        [defaults.clone(), production.clone(), cli.clone()],
    )
    .expect("valid production graph");
    let same_graph = ConfigurationGraph::compile(
        &schema(),
        Environment::new("production", EnvironmentClass::Production),
        [defaults, production, cli],
    )
    .expect("same graph");

    assert_eq!(graph.digest(), same_graph.digest());
    assert_eq!(
        graph
            .deserialize_namespace::<ApplicationConfig>("application")
            .expect("typed application config"),
        ApplicationConfig {
            name: "orders-cli".into()
        }
    );
    assert_eq!(
        graph
            .deserialize_namespace::<DatabaseConfig>("database")
            .expect("typed database config"),
        DatabaseConfig {
            url: SecretReference::environment_variable("ORDERS_DATABASE_URL")
                .expect("valid reference")
        }
    );

    let application_explanation = graph
        .explain("application.name")
        .expect("known application field");
    assert_eq!(application_explanation.value, Some(json!("orders-cli")));
    assert_eq!(
        application_explanation.provenance.source_kind,
        ConfigSourceKind::CliOverride
    );

    let secret_explanation = graph.explain("database.url").expect("known secret");
    assert_eq!(secret_explanation.value, None);
    assert!(secret_explanation.redacted);
    let secret_json = serde_json::to_string(&secret_explanation).expect("serializable explanation");
    assert!(!secret_json.contains("ORDERS_DATABASE_URL"));

    let development = ConfigurationGraph::compile(
        &schema(),
        Environment::new("dev", EnvironmentClass::Development),
        [
            ConfigLayer::from_toml(
                ConfigSourceKind::DefaultFile,
                "config/default.toml",
                r#"
schema = 1
[values.database]
url = "ssm:/orders/dev/database-url"
"#,
            )
            .expect("development defaults"),
            ConfigLayer::from_toml(
                ConfigSourceKind::EnvironmentFile,
                "config/dev.toml",
                "schema = 1\nenvironment_class = \"development\"",
            )
            .expect("development environment"),
        ],
    )
    .expect("development graph");
    let difference = development.diff(&graph);
    assert!(difference.changes.iter().any(|entry| {
        entry.path == "database.url"
            && entry.secret
            && entry.before.is_none()
            && entry.after.is_none()
    }));
    let difference_json = serde_json::to_string(&difference).expect("serializable diff");
    assert!(!difference_json.contains("ORDERS_DATABASE_URL"));
    assert!(!difference_json.contains("/orders/dev/database-url"));

    let unknown = ConfigLayer::from_toml(
        ConfigSourceKind::EnvironmentFile,
        "config/production.toml",
        r#"
schema = 1
environment_class = "production"
[values.application]
typo = "rejected"
"#,
    )
    .expect("syntactically valid layer");
    let error = ConfigurationGraph::compile(
        &schema(),
        Environment::new("production", EnvironmentClass::Production),
        [unknown],
    )
    .expect_err("unknown fields fail closed");
    assert_eq!(error.diagnostics()[0].code, "config.unknown_field");

    let local_override = ConfigLayer::from_pairs(
        ConfigSourceKind::LocalOverride,
        "config/local.toml",
        [("application.name", json!("unsafe-production-override"))],
    )
    .expect("unique local paths");
    let error = ConfigurationGraph::compile(
        &schema(),
        Environment::new("production", EnvironmentClass::Production),
        [local_override],
    )
    .expect_err("production rejects local overrides");
    assert_eq!(
        error.diagnostics()[0].code,
        "config.local_override_forbidden"
    );
}
