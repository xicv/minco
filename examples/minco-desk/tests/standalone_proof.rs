//! The standalone private-beta proofs (ADR-0072): clean install,
//! migration idempotence, composition completeness, and live health —
//! all providerless, all in-process.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use minco_desk_example::{DeskConfig, build_desk, migrate};
use sqlx::Row as _;
use std::collections::BTreeMap;
use tower::ServiceExt as _;

fn scratch_config(tag: &str) -> (tempfile::TempDir, DeskConfig) {
    let directory = tempfile::tempdir().expect("temp dir");
    let config = DeskConfig {
        host: "127.0.0.1".into(),
        port: 0,
        database_url: format!(
            "sqlite://{}?mode=rwc",
            directory
                .path()
                .join(format!("desk-{tag}.sqlite"))
                .display()
        ),
        project_id: "desk-proof".into(),
        portal_origin: "http://127.0.0.1:8090".into(),
        allowed_origins: vec!["http://127.0.0.1:8090".into()],
        mailbox_scope: "support@desk.example.test".into(),
        agent_token: "desk-proof-agent-token".into(),
        csrf_secret: "desk-proof-csrf-secret-desk-proof-csrf-secret".into(),
        allowed_return_paths: BTreeMap::from([(
            "https://app.example.test".to_owned(),
            vec!["/orders".to_owned()],
        )]),
        environment: "local".into(),
        inbound_auth_policy: minco_plugin_ticketing::InboundAuthPolicy::LocalTrusted,
        inbound_scan_verdicts: minco_plugin_ticketing::ScanVerdictPolicy::Local,
        inbound_authserv_id: "amazonses.com".into(),
        workspace_id: None,
        workspace_display_name: "Default workspace".into(),
    };
    (directory, config)
}

#[tokio::test]
async fn clean_install_creates_every_table_and_migrations_are_idempotent() {
    let (_directory, config) = scratch_config("clean");
    // First run: every migration applies to a fresh file.
    let pool = migrate(&config).await.expect("clean install migrates");
    let tables = sqlx::query("SELECT name FROM sqlite_master WHERE type='table'")
        .fetch_all(&pool)
        .await
        .expect("sqlite_master query");
    let names: std::collections::BTreeSet<String> = tables
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();
    for table in [
        "ticketing_tickets",
        "ticketing_messages",
        "ticketing_delivery_evidence",
        "ticketing_automation_proposals",
        "ticketing_clarifications",
        "minco_jobs",
    ] {
        assert!(
            names.contains(table),
            "table {table} must exist after clean install"
        );
    }
    // Second run on the same file: migrations are idempotent.
    migrate(&config)
        .await
        .expect("re-running migrations is safe");
}

#[tokio::test]
async fn composed_desk_serves_health_and_support_entry() {
    let (_directory, config) = scratch_config("compose");
    let desk = build_desk(&config).await.expect("compose the desk");
    // The composition graph records every selected service.
    let graph = desk.health_report.to_string();
    for service in [
        "health",
        "identity",
        "sessions",
        "idempotency",
        "notifications",
        "events",
    ] {
        assert!(graph.contains(service), "graph must record {service}");
    }

    // Liveness and readiness execute the registered checks (exact-head
    // reviews R10 and R33/P1-3): readiness now covers sessions,
    // idempotency receipts, the audit dispatch backlog and object
    // storage alongside the ticketing and jobs stores — all green on a
    // healthy desk, so both endpoints answer 200.
    for path in ["/live", "/ready"] {
        let probe = desk
            .router
            .clone()
            .oneshot(
                Request::get(path)
                    .extension(minco_http::Principal {
                        subject: "agent-proof".into(),
                        permissions: std::iter::once("ticketing.agent-console".into()).collect(),
                        claims: std::collections::BTreeMap::default(),
                    })
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("health probe dispatches");
        assert_eq!(
            probe.status(),
            http::StatusCode::OK,
            "{path} must report healthy with the expanded check set"
        );
    }

    // The agent bootstrap requires identity — proving the full HTTP
    // middleware, router and service stack are wired. The in-process
    // principal carries the desk's resolved scope (ADR-0076).
    let bootstrap = desk
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent/bootstrap")
                .extension(desk.agent_principal.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bootstrap.status(), StatusCode::OK);
    let body = bootstrap.into_body().collect().await.unwrap().to_bytes();
    let bootstrap: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(bootstrap["project_id"], "desk-proof");

    // The public support entry needs no identity at all.
    let entry = desk
        .router
        .oneshot(
            Request::get("/_minco/ticketing/support-entry.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(entry.status(), StatusCode::OK);
}

#[tokio::test]
async fn end_to_end_ticket_lifecycle_on_one_database() {
    let (_directory, config) = scratch_config("lifecycle");
    let desk = build_desk(&config).await.expect("compose the desk");
    // The in-process caller uses the desk's resolved scoped principal —
    // exactly what the bearer path injects (ADR-0076).
    let principal = desk.agent_principal.clone();

    // Create through the real HTTP surface.
    let created = desk
        .router
        .clone()
        .oneshot(
            Request::post("/_minco/ticketing/tickets")
                .extension(principal.clone())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "project_id": "desk-proof",
                        "subject": "Desk proof",
                        "description": "One ticket through the standalone stack.",
                        "requester": {"subject": "requester-1"},
                        "channel": "portal"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let body = created.into_body().collect().await.unwrap().to_bytes();
    let ticket: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let ticket_id = ticket["ticket"]["id"].as_str().unwrap().to_owned();

    // Search finds it (bounded search, ADR-0069).
    let search = desk
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent/search?q=desk%20proof")
                .extension(principal.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(search.status(), StatusCode::OK);
    let body = search.into_body().collect().await.unwrap().to_bytes();
    let results: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(results["data"].as_array().unwrap().len(), 1);

    // The agent detail records a view (collision indication, ADR-0067).
    let detail = desk
        .router
        .clone()
        .oneshot(
            Request::get(format!("/_minco/ticketing/agent/tickets/{ticket_id}"))
                .extension(principal)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let body = detail.into_body().collect().await.unwrap().to_bytes();
    let detail: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(detail["other_recent_viewers"].is_array());
    assert_eq!(detail["ticket"]["id"], ticket_id);
}

#[tokio::test]
async fn workspace_provisioning_binds_once_and_survives_rebuilds() {
    // ISO-1 on the real composition: fresh provisioning binds one identity,
    // repeated builds over the same database converge without rebinding,
    // and the agent principal carries the resolved workspace/project scope.
    let (_directory, config) = scratch_config("workspace");
    let first = build_desk(&config).await.expect("first build provisions");
    assert!(first.workspace_report.created);
    let second = build_desk(&config).await.expect("second build converges");
    assert!(!second.workspace_report.created);
    assert_eq!(
        first.workspace_report.workspace, second.workspace_report.workspace,
        "rebuilding over the same database preserves the bound identity"
    );
    assert!(second.workspace_report.orphaned_profiles.is_empty());
    assert_eq!(second.workspace_report.retained_grants, 0);

    // The desk-agent principal resolves from the provisioned profile and
    // carries exactly the canonical scope tokens (ADR-0076), stored in the
    // whitespace-tokenized principal scope claim.
    let tokens: std::collections::BTreeSet<String> = std::iter::once(format!(
        "workspace:{}",
        first.workspace_report.workspace.as_str()
    ))
    .chain(std::iter::once("project:desk-proof".to_owned()))
    .collect();
    let resolved =
        minco_plugin_workspace::ProjectScope::from_scope_tokens(&tokens).expect("tokens resolve");
    let claim = first
        .agent_principal
        .claims
        .get(minco_http::PRINCIPAL_SCOPES_CLAIM)
        .expect("the scope claim is present");
    let principal_tokens: std::collections::BTreeSet<String> =
        claim.split_ascii_whitespace().map(str::to_owned).collect();
    assert_eq!(
        minco_plugin_workspace::ProjectScope::from_scope_tokens(&principal_tokens)
            .expect("the agent principal carries a resolvable scope"),
        resolved,
        "the bearer principal's scope must equal the provisioned binding"
    );
    assert_eq!(first.agent_principal.subject, "desk-agent");
    assert_eq!(
        first.agent_principal.permissions.len(),
        9,
        "the profile ceiling bounds the injected capability set"
    );

    // The registry lives under its own exclusive ledger. The sqlx default
    // ledger belongs to the ticketing stream; the workspace stream must own
    // exactly one applied migration in its own table.
    let pool = migrate(&config).await.expect("migrate again");
    assert_eq!(
        minco_plugin_workspace::WORKSPACE_MIGRATION_LEDGER,
        "_minco_workspace_migrations"
    );
    let workspace_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _minco_workspace_migrations")
            .fetch_one(&pool)
            .await
            .expect("workspace ledger row count");
    assert_eq!(workspace_rows, 1, "the workspace ledger owns its migration");
    let binding: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspace_deployment_binding")
        .fetch_one(&pool)
        .await
        .expect("binding count");
    assert_eq!(binding, 1, "exactly one deployment binding row");
}

#[tokio::test]
async fn a_conflicting_workspace_pin_fails_closed_instead_of_rebinding() {
    // ISO-1: an existing database cannot be silently rebound to a
    // different pinned workspace identity.
    let (_directory, mut config) = scratch_config("rebind");
    config.workspace_id = Some("ws-first".into());
    let first = build_desk(&config).await.expect("pinned first build");
    assert_eq!(first.workspace_report.workspace.as_str(), "ws-first");
    let mut rebound = config.clone();
    rebound.workspace_id = Some("ws-second".into());
    let error = build_desk(&rebound)
        .await
        .expect_err("a different pin must fail closed");
    assert!(
        format!("{error:#}").contains("refusing to rebind"),
        "unexpected error: {error:#}"
    );
    // The same pin still converges.
    let repeat = build_desk(&config).await.expect("same pin converges");
    assert!(!repeat.workspace_report.created);
}

#[tokio::test]
async fn the_isolated_desk_denies_scopeless_and_foreign_principals() {
    // ISO-2/ISO-3 on the real composition: with isolation enabled, a
    // principal that carries no workspace/project scope — or a foreign
    // one — is denied on business routes even with full permissions,
    // while the desk-agent bearer path keeps working.
    let (_directory, config) = scratch_config("enforcement");
    let desk = build_desk(&config)
        .await
        .expect("compose the isolated desk");

    let scopeless = minco_http::Principal {
        subject: "agent-proof".into(),
        permissions: [
            "ticketing.create",
            "ticketing.agent.read",
            "ticketing.agent-console",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        claims: std::collections::BTreeMap::default(),
    };
    let denied = desk
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent/bootstrap")
                .extension(scopeless.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let foreign = scopeless
        .clone()
        .with_scopes(["workspace:ws-elsewhere", "project:desk-proof"]);
    let denied = desk
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent/bootstrap")
                .extension(foreign)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    // The bearer credential resolves the provisioned scope and passes the
    // same route (bootstrap also proves agent-console capability).
    let authorized = desk
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent/bootstrap")
                .header("authorization", format!("Bearer {}", config.agent_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
}
