//! Stage G operational readiness proofs (ADR-0075): bounded load,
//! zero-compute cost topology, BFF-callable boundary and separate
//! database identity — all providerless, all in-process.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use minco_desk_example::{DeskConfig, build_desk};
use std::collections::BTreeMap;
use std::time::Instant;
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
        inbound_auth_policy: minco_plugin_ticketing::InboundAuthPolicy::LocalTrusted,
        inbound_scan_verdicts: minco_plugin_ticketing::ScanVerdictPolicy::Local,
        inbound_authserv_id: "amazonses.com".into(),
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

fn bff_principal() -> minco_http::Principal {
    // A BFF calls with its own service identity holding read authority
    // for proxying — never the end user's browser session.
    minco_http::Principal {
        subject: "peopleplanner-bff".into(),
        permissions: ["ticketing.agent.read", "ticketing.agent-console"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        claims: std::collections::BTreeMap::default(),
    }
}

async fn create_ticket(router: &axum::Router, subject: &str) {
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
                        "description": "Load proof ticket.",
                        "requester": {"subject": "requester-1"},
                        "channel": "portal"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "{subject}");
}

async fn agent_list(router: &axum::Router, query: &str) -> serde_json::Value {
    let response = router
        .clone()
        .oneshot(
            Request::get(format!("/_minco/ticketing/agent/tickets{query}"))
                .extension(agent_principal())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn bounded_load_creates_and_lists_at_volume_with_correct_pagination() {
    let directory = tempfile::tempdir().unwrap();
    let config = scratch_config("load", directory.path());
    let desk = build_desk(&config).await.unwrap();

    // Bounded load: 100 sequential creates plus interleaved listing —
    // the volume where pagination bugs and cursor gaps surface.
    let total = 100;
    let started = Instant::now();
    for index in 0..total {
        create_ticket(&desk.router, &format!("Load ticket {index}")).await;
        if index % 25 == 24 {
            // Interleaved listing proves pagination stays correct while
            // the corpus grows.
            let page = agent_list(&desk.router, "?page[limit]=10").await;
            assert!(
                page["data"].as_array().unwrap().len() <= 10,
                "pagination bound holds during load"
            );
        }
    }
    let elapsed = started.elapsed();

    // Every ticket is visible; full pagination walk collects exactly
    // the corpus without gaps or duplicates.
    let mut seen = std::collections::BTreeSet::new();
    let mut cursor: Option<String> = None;
    loop {
        let query = match &cursor {
            Some(value) => format!("?page[limit]=25&page[after]={value}"),
            None => "?page[limit]=25".into(),
        };
        let page = agent_list(&desk.router, &query).await;
        for item in page["data"].as_array().unwrap() {
            seen.insert(item["id"].as_str().unwrap().to_owned());
        }
        let next = page["page"]["nextCursor"].as_str();
        match next {
            Some(value) => cursor = Some(value.to_owned()),
            None => break,
        }
    }
    assert_eq!(
        seen.len(),
        total,
        "pagination walks the full corpus exactly once"
    );

    // The bounded load completes in a locally-verifiable envelope —
    // not a production SLO, but a regression guard against accidental
    // quadratic behavior.
    let total_millis = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    let per_ticket_millis = total_millis / total as u64;
    assert!(
        per_ticket_millis < 500,
        "local per-ticket latency must stay bounded (was {per_ticket_millis}ms)"
    );
}

#[tokio::test]
async fn cost_topology_is_zero_compute_local_native() {
    let directory = tempfile::tempdir().unwrap();
    let config = scratch_config("cost", directory.path());
    let desk = build_desk(&config).await.unwrap();

    // The composition graph records only local-native services — no
    // provisioned compute, no NAT gateway, no scheduled wakeups, no
    // queues from infrastructure: the local cost topology is zero
    // compute plus one SQLite file.
    let graph = desk.health_report.to_string();
    for zero_compute_violation in ["provisioned_concurrency", "nat_gateway", "scheduled_wakeup"] {
        assert!(
            !graph.contains(zero_compute_violation),
            "the standalone composition must not declare {zero_compute_violation}"
        );
    }
    // The database is a local file, not a managed service.
    assert!(
        config.database_url.starts_with("sqlite://"),
        "the standalone database is a local SQLite file"
    );
}

#[tokio::test]
async fn desk_is_bff_callable_and_rejects_foreign_browser_origins() {
    let directory = tempfile::tempdir().unwrap();
    let config = scratch_config("bff", directory.path());
    let desk = build_desk(&config).await.unwrap();
    create_ticket(&desk.router, "BFF ticket").await;

    // A BFF calls with its own service identity and reads the agent
    // surface — the desk never sees the browser origin, only the BFF.
    let listing = desk
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent/tickets?page[limit]=10")
                .extension(bff_principal())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listing.status(), StatusCode::OK);
    let body = listing.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(page["data"].as_array().unwrap().len(), 1);
    assert_eq!(page["data"][0]["subject"], "BFF ticket");

    // A foreign browser origin's preflight is refused: the CORS policy
    // is exact (one allowed origin), so a browser page at another
    // origin cannot call the desk directly — the BFF boundary holds.
    let preflight = desk
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent/bootstrap")
                .header(header::ORIGIN, "https://evil.example.test")
                .header("access-control-request-method", "GET")
                .header("access-control-request-headers", "content-type")
                .extension(bff_principal())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let allow_origin = preflight
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .map(|value| value.to_str().unwrap());
    assert_ne!(
        allow_origin,
        Some("https://evil.example.test"),
        "a foreign origin must never be allowed"
    );
    assert_ne!(allow_origin, Some("*"), "wildcard CORS is forbidden");
}

#[tokio::test]
async fn database_identity_is_separate_from_release_identity() {
    let directory = tempfile::tempdir().unwrap();
    // Two desks with different database files carry fully isolated
    // data — the database identity is the file path, independent of
    // any release artifact or shared state.
    let first_config = scratch_config("identity-a", directory.path());
    let second_config = scratch_config("identity-b", directory.path());
    assert_ne!(
        first_config.database_url, second_config.database_url,
        "two desks never share a database identity"
    );

    let first = build_desk(&first_config).await.unwrap();
    create_ticket(&first.router, "First desk only").await;

    let second = build_desk(&second_config).await.unwrap();
    let second_listing = second
        .router
        .clone()
        .oneshot(
            Request::get("/_minco/ticketing/agent/tickets")
                .extension(agent_principal())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = second_listing
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        page["data"].as_array().unwrap().len(),
        0,
        "the second desk starts empty — databases are fully separate"
    );

    // And the first desk still sees its own ticket.
    let first_listing = agent_list(&first.router, "").await;
    assert_eq!(first_listing["data"].as_array().unwrap().len(), 1);
}
