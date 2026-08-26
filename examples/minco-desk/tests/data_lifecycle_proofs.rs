//! Stage G data-lifecycle proofs (ADR-0073): backup, restore, retention
//! erasure and job recovery — all on the standalone desk's one `SQLite`
//! database, all providerless.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use minco_desk_example::{DeskConfig, build_desk, migrate};
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
        environment: "local".into(),
    }
}

fn agent_principal() -> minco_http::Principal {
    minco_http::Principal {
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
    }
}

async fn create_ticket_via_http(router: &axum::Router, subject: &str) -> String {
    let response = router
        .clone()
        .oneshot(
            Request::post("/_minco/ticketing/tickets")
                .extension(agent_principal())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "project_id": "desk-proof",
                        "subject": subject,
                        "description": "Lifecycle proof ticket.",
                        "requester": {"subject": "requester-1"},
                        "channel": "portal"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ticket: serde_json::Value = serde_json::from_slice(&body).unwrap();
    ticket["ticket"]["id"].as_str().unwrap().to_owned()
}

async fn resolve_ticket_via_http(router: &axum::Router, ticket_id: &str, revision: u64) {
    let response = router
        .clone()
        .oneshot(
            Request::patch(format!(
                "/_minco/ticketing/agent/tickets/{ticket_id}/management"
            ))
            .extension(agent_principal())
            .header("content-type", "application/json")
            .header(
                "if-match",
                format!("\"ticket:{ticket_id}:{}\"", revision + 1),
            )
            .body(Body::from(
                serde_json::json!({"status": "resolved", "resolution": "Done."}).to_string(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn backup_and_restore_preserve_every_ticket() {
    let directory = tempfile::tempdir().unwrap();
    let config = scratch_config("backup", directory.path());
    let desk = build_desk(&config).await.unwrap();
    let first = create_ticket_via_http(&desk.router, "Backup me").await;
    let second = create_ticket_via_http(&desk.router, "And me").await;

    // Backup: SQLite's online backup via VACUUM INTO (a consistent
    // snapshot without stopping the process). VACUUM INTO cannot take
    // parameters, so the statement is asserted safe: the path is a
    // tempfile we control, never request input.
    let backup_path = directory.path().join("desk-backup.sqlite");
    let pool = migrate(&config).await.unwrap();
    let _ = std::fs::remove_file(&backup_path);
    let statement = format!("VACUUM INTO '{}'", backup_path.display());
    sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
        .execute(&pool)
        .await
        .expect("VACUUM INTO backup");

    // Restore: open a fresh desk on the backup file and prove the data.
    let restored_config = DeskConfig {
        database_url: format!("sqlite://{}?mode=rwc", backup_path.display()),
        ..config.clone()
    };
    let restored = build_desk(&restored_config).await.unwrap();
    for ticket_id in [first.as_str(), second.as_str()] {
        let detail = restored
            .router
            .clone()
            .oneshot(
                Request::get(format!("/_minco/ticketing/agent/tickets/{ticket_id}"))
                    .extension(agent_principal())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            detail.status(),
            StatusCode::OK,
            "restored {ticket_id} must serve"
        );
    }
}

#[tokio::test]
async fn retention_erase_cascades_and_is_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let config = scratch_config("retention", directory.path());
    let desk = build_desk(&config).await.unwrap();
    let old = create_ticket_via_http(&desk.router, "Erase me").await;
    let keep = create_ticket_via_http(&desk.router, "Keep me").await;
    // Resolve only the first ticket; retention erases resolved tickets
    // closed before the cutoff.
    resolve_ticket_via_http(&desk.router, &old, 0).await;

    let erased = minco_desk_example::erase_resolved_before(
        &config,
        chrono::Utc::now() + chrono::TimeDelta::hours(1),
        100,
    )
    .await
    .expect("retention erase");
    assert_eq!(erased, 1, "exactly the resolved ticket is erased");

    // The erased ticket is gone; the open ticket survives; children
    // (views, clarifications…) cascaded with the ticket row.
    let pool = migrate(&config).await.unwrap();
    let remaining: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM ticketing_tickets ORDER BY created_at")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].0, keep);
    let orphan_views: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM ticketing_ticket_views WHERE ticket_id = ?")
            .bind(&old)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(orphan_views.0, 0, "child rows cascade with the ticket");
}

#[tokio::test]
async fn pending_jobs_survive_a_process_restart_and_recover() {
    let directory = tempfile::tempdir().unwrap();
    let config = scratch_config("recovery", directory.path());
    // First process: compose, enqueue durable jobs, then "die" (drop
    // everything — the pool included).
    {
        let desk = build_desk(&config).await.unwrap();
        let _ = desk; // composition applies migrations and wires jobs
        let pool = migrate(&config).await.unwrap();
        let jobs_store = Arc::new(minco_sqlx_sqlite::jobs::SqliteJobStore::new(pool.clone()));
        let registry = Arc::new(minco_plugin_jobs::JobHandlerRegistry::new());
        let jobs = minco_plugin_jobs::JobsServices::new(
            Arc::clone(&jobs_store) as Arc<_>,
            Arc::clone(&jobs_store) as Arc<_>,
            Arc::new(minco_plugin_jobs::FailClosedDispatcher),
            Arc::clone(&jobs_store) as Arc<_>,
            Arc::new(minco_plugin_jobs::SystemJobClock),
            Arc::new(minco_plugin_jobs::JobExecutor::new(registry)),
        );
        for index in 0..3 {
            let envelope = minco_plugin_ticketing::development_automation_envelope(
                &minco_plugin_ticketing::RunDevelopmentAutomation {
                    project_id: "desk-proof".into(),
                    ticket_id: minco_plugin_ticketing::TicketId::new(),
                    requested_by: format!("agent-{index}"),
                },
                uuid::Uuid::now_v7(),
                chrono::Utc::now(),
            )
            .unwrap();
            jobs.submit_durable(envelope).await.unwrap();
        }
        let pending: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM minco_jobs WHERE status = 'pending'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(pending.0, 3);
    }

    // Second process on the same file: the jobs are still there and a
    // claim pass recovers them for execution.
    let pool = migrate(&config).await.unwrap();
    let pending: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM minco_jobs WHERE status = 'pending'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending.0, 3, "durable jobs survive the restart");
    let jobs_store = Arc::new(minco_sqlx_sqlite::jobs::SqliteJobStore::new(pool.clone()));
    let registry = Arc::new(minco_plugin_jobs::JobHandlerRegistry::new());
    let jobs = minco_plugin_jobs::JobsServices::new(
        Arc::clone(&jobs_store) as Arc<_>,
        Arc::clone(&jobs_store) as Arc<_>,
        Arc::new(minco_plugin_jobs::FailClosedDispatcher),
        Arc::clone(&jobs_store) as Arc<_>,
        Arc::new(minco_plugin_jobs::SystemJobClock),
        Arc::new(minco_plugin_jobs::JobExecutor::new(registry)),
    );
    let report = jobs
        .dispatch_due_once("desk-recovery-proof", 10, chrono::TimeDelta::minutes(1))
        .await
        .expect("recovery dispatch pass");
    assert_eq!(
        report.claimed, 3,
        "every surviving job is claimable in the recovery pass"
    );
}

use std::sync::Arc;
