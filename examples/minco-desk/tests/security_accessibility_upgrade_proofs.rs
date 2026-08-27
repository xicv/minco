//! Stage G security, accessibility and upgrade proofs (ADR-0074): the
//! desk's hardened surface, its accessible assets, and additive schema
//! upgrade without data loss — all providerless, all in-process.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use minco_desk_example::{DeskConfig, build_desk, migrate};
use std::collections::BTreeMap;
use tower::ServiceExt as _;

fn scratch_config(tag: &str, dir: &std::path::Path) -> DeskConfig {
    DeskConfig {
        host: "127.0.0.1".into(),
        port: 0,
        database_url: format!(
            "sqlite://{}?mode=rwc",
            dir.join(format!("desk-{tag}.sqlite")).display()
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
    }
}

fn agent_principal() -> minco_http::Principal {
    minco_http::Principal {
        subject: "agent-proof".into(),
        permissions: [
            "ticketing.create",
            "ticketing.manage",
            "ticketing.agent-console",
            "ticketing.agent.read",
            "ticketing.agent.manage",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        claims: std::collections::BTreeMap::default(),
    }
}

fn requester_principal(subject: &str) -> minco_http::Principal {
    minco_http::Principal {
        subject: subject.into(),
        permissions: std::iter::once("ticketing.read".to_owned()).collect(),
        claims: std::collections::BTreeMap::default(),
    }
}

#[tokio::test]
async fn every_public_surface_is_hardened() {
    let directory = tempfile::tempdir().unwrap();
    let config = scratch_config("security", directory.path());
    let desk = build_desk(&config).await.unwrap();

    // The agent console page carries a strict CSP, nosniff and
    // no-referrer — nothing injectable, nothing leakable.
    let page = desk
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let csp = page.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap();
    assert!(
        csp.contains("default-src 'none'"),
        "CSP must deny by default"
    );
    assert!(csp.contains("frame-ancestors 'none'"), "no framing allowed");
    assert_eq!(
        page.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff",
        "MIME sniffing must be disabled"
    );
    assert_eq!(
        page.headers()[header::REFERRER_POLICY],
        "no-referrer",
        "referrers must not leak"
    );

    // The public entry script and console stylesheet carry the same
    // nosniff and cache discipline.
    for (path, expected_type) in [
        (
            "/_minco/ticketing/support-entry.js",
            "application/javascript; charset=utf-8",
        ),
        (
            "/_minco/ticketing/agent/console.css",
            "text/css; charset=utf-8",
        ),
    ] {
        let asset = desk
            .router
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(asset.status(), StatusCode::OK, "{path} must serve");
        assert_eq!(
            asset.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff",
            "{path} must be nosniff"
        );
        assert_eq!(
            asset.headers()[header::CONTENT_TYPE],
            expected_type,
            "{path} content type must be exact"
        );
    }

    // The agent bootstrap never carries credentials, tokens or secret
    // material — only truthful permission-derived capabilities.
    let bootstrap = desk
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent/bootstrap")
                .extension(agent_principal())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bootstrap.status(), StatusCode::OK);
    let body = bootstrap.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&body);
    for secret_marker in ["token", "secret", "password", "credential", "api_key"] {
        assert!(
            !text.to_ascii_lowercase().contains(secret_marker),
            "bootstrap must never carry {secret_marker}"
        );
    }
}

#[tokio::test]
async fn requesters_are_isolated_and_unauthenticated_access_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let config = scratch_config("isolation", directory.path());
    let desk = build_desk(&config).await.unwrap();

    // An unauthenticated agent bootstrap is refused.
    let anonymous = desk
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent/bootstrap")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    // A requester cannot read another requester's ticket.
    let requester_a = requester_principal("requester-a");
    let created = desk
        .router
        .clone()
        .oneshot(
            Request::post("/_minco/ticketing/tickets")
                .extension(minco_http::Principal {
                    subject: "requester-a".into(),
                    permissions: ["ticketing.create", "ticketing.read"]
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                    claims: std::collections::BTreeMap::default(),
                })
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "project_id": "desk-proof",
                        "subject": "A's ticket",
                        "description": "Private to requester-a.",
                        "requester": {"subject": "requester-a"},
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

    let stranger = desk
        .router
        .clone()
        .oneshot(
            Request::get(format!("/_minco/ticketing/requester/tickets/{ticket_id}"))
                .extension(requester_principal("requester-b"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        stranger.status() == StatusCode::NOT_FOUND || stranger.status() == StatusCode::FORBIDDEN,
        "a stranger must never read another requester's ticket"
    );
    let _ = requester_a;
}

#[tokio::test]
async fn upgrade_from_an_earlier_schema_preserves_every_ticket() {
    let directory = tempfile::tempdir().unwrap();
    let config = scratch_config("upgrade", directory.path());

    // Phase 1: an "old" database at the first-generation schema — apply
    // only migration 0001 by hand, then create real data in it.
    let old_path = directory.path().join("desk-upgrade.sqlite");
    let old_url = format!("sqlite://{}?mode=rwc", old_path.display());
    let old_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect(&old_url)
        .await
        .unwrap();
    let first_migration = include_str!(
        "../../../plugins/minco-plugin-ticketing/migrations/sqlite/0001_ticketing.sql"
    );
    sqlx::raw_sql(first_migration)
        .execute(&old_pool)
        .await
        .expect("apply first-generation schema");
    // Record migration 0001 as already applied so the migrator advances
    // from 0002 instead of replaying 0001; the checksum is the SHA-384
    // of the file content sqlx itself uses.
    let checksum = {
        use sha2::Digest as _;
        let mut hasher = sha2::Sha384::new();
        hasher.update(first_migration.as_bytes());
        hasher.finalize().to_vec()
    };
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL,
            checksum BLOB NOT NULL,
            execution_time BIGINT
        )",
    )
    .execute(&old_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES (1, 'ticketing', 1, ?, 0)",
    )
    .bind(&checksum)
    .execute(&old_pool)
    .await
    .unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    let legacy_json = serde_json::json!({
        "id": "01890b0a-0000-7000-8000-000000000001",
        "project_id": "desk-proof",
        "display_reference": "TKT-OLD-1",
        "subject": "Old ticket",
        "description": "Created before the upgrade.",
        "requester": {"subject": "requester-old"},
        "status": "new",
        "revision": 0
    });
    sqlx::query(
        "INSERT INTO ticketing_tickets (project_id, id, display_reference, status, requester_subject, updated_at, revision, ticket_json)
         VALUES ('desk-proof', '01890b0a-0000-7000-8000-000000000001', 'TKT-OLD-1', 'new', 'requester-old', ?, 0, ?)",
    )
    .bind(&now)
    .bind(legacy_json.to_string())
    .execute(&old_pool)
    .await
    .unwrap();
    old_pool.close().await;

    // Phase 2: the upgrade — the current migrator advances the same
    // file through every later migration (0002..0011 + jobs storage).
    let upgraded_config = DeskConfig {
        database_url: old_url,
        ..config
    };
    let pool = migrate(&upgraded_config).await.expect("upgrade migrates");

    // The pre-upgrade ticket survived with its data intact.
    let survived: Option<(String, String)> = sqlx::query_as(
        "SELECT display_reference, subject FROM ticketing_tickets
          WHERE project_id = 'desk-proof' AND display_reference = 'TKT-OLD-1'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (reference, subject) = survived.expect("pre-upgrade ticket survives");
    assert_eq!(
        subject, "Old ticket",
        "the columnar migration backfilled from ticket_json"
    );
    let _ = reference;

    // And every newer table exists (the columns the later migrations
    // added are readable on the surviving row).
    let typed: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ticket_type, csat_json FROM ticketing_tickets WHERE display_reference = 'TKT-OLD-1'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(typed.is_some(), "columns from later migrations exist");
    assert_eq!(
        typed.as_ref().unwrap().0.as_deref(),
        Some("question"),
        "the typed-column default backfilled the pre-upgrade row"
    );

    // The upgraded database serves through the full desk stack.
    let desk = build_desk(&upgraded_config).await.unwrap();
    let listing = desk
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent/views/new-unassigned")
                .extension(agent_principal())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listing.status(), StatusCode::OK);
    let body = listing.into_body().collect().await.unwrap().to_bytes();
    let view: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        view["data"].as_array().is_some_and(|items| items
            .iter()
            .any(|item| item["display_reference"] == "TKT-OLD-1")),
        "the pre-upgrade ticket is visible through the upgraded surface"
    );
}
