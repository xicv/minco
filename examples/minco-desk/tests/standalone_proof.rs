//! The standalone private-beta proofs (ADR-0072): clean install,
//! migration idempotence, composition completeness, and live health —
//! all providerless, all in-process.

use std::collections::BTreeMap;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use minco_desk_example::{DeskConfig, build_desk, migrate};
use sqlx::Row as _;
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
        agent_token: "proof-agent-token-0123456789abcdef".into(),
        csrf_secret: "proof-csrf-secret-0123456789abcdef0123456789abcdef".into(),
        allowed_return_paths: BTreeMap::from([(
            "https://app.example.test".to_owned(),
            vec!["/orders".to_owned()],
        )]),
        environment: "local".into(),
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

    // The agent bootstrap requires identity — proving the full HTTP
    // middleware, router and service stack are wired.
    let bootstrap = desk
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent/bootstrap")
                .extension(minco_http::Principal {
                    subject: "agent-proof".into(),
                    permissions: ["ticketing.agent-console", "ticketing.agent.read"]
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                    claims: std::collections::BTreeMap::default(),
                })
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
    let principal = minco_http::Principal {
        subject: "agent-proof".into(),
        permissions: [
            "ticketing.create",
            "ticketing.manage",
            "ticketing.reply",
            "ticketing.agent-console",
            "ticketing.agent.read",
            "ticketing.agent.manage",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        claims: std::collections::BTreeMap::default(),
    };

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
