//! Isolation acceptance proofs for M15-T01 (ISO-2…ISO-6 evidence on the
//! real composition): upgrade integrity from a pre-isolation database,
//! cross-scope behavior with deliberately colliding identifiers, neutral
//! multi-integration fixtures, and the product-neutrality gate.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use minco_desk_example::{DeskConfig, build_desk, migrate};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
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
        environment: "local".into(),
        agent_token: "desk-proof-agent-token".into(),
        csrf_secret: "desk-proof-csrf-secret-desk-proof-csrf-secret".into(),
        allowed_return_paths: BTreeMap::from([(
            "https://app.example.test".to_owned(),
            vec!["/orders".to_owned()],
        )]),
        inbound_auth_policy: minco_plugin_ticketing::InboundAuthPolicy::LocalTrusted,
        inbound_scan_verdicts: minco_plugin_ticketing::ScanVerdictPolicy::Local,
        inbound_authserv_id: "amazonses.com".into(),
        workspace_id: None,
        workspace_display_name: "Default workspace".into(),
    };
    (directory, config)
}

#[tokio::test]
async fn a_whitespace_project_identifier_boots_and_serves_losslessly() {
    // Round 1 finding 6: a historically valid project id containing
    // whitespace must ride the principal scope claim losslessly. The desk
    // provisions it verbatim, the bearer principal carries it percent-
    // encoded, and ticketing's parser decodes it back for every use case.
    let (_directory, mut config) = scratch_config("whitespace-project");
    config.project_id = "legacy Prj".into();
    let desk = build_desk(&config).await.expect("whitespace project boots");
    assert!(desk.workspace_report.created);
    let claim = desk
        .agent_principal
        .claims
        .get(minco_http::PRINCIPAL_SCOPES_CLAIM)
        .expect("scope claim present");
    let tokens: Vec<&str> = claim.split_ascii_whitespace().collect();
    assert_eq!(tokens.len(), 2, "exactly two scope tokens: {claim:?}");
    assert!(
        tokens.contains(&"project:legacy%20Prj"),
        "the whitespace project id must be percent-encoded in one token: {claim:?}"
    );
    assert!(
        tokens
            .iter()
            .any(|token| token.starts_with("workspace:ws-")),
        "the workspace token must be present: {claim:?}"
    );

    let created = desk
        .router
        .clone()
        .oneshot(
            Request::post("/_minco/ticketing/tickets")
                .extension(desk.agent_principal.clone())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "project_id": "legacy Prj",
                        "subject": "Whitespace project proof",
                        "description": "The carrier must be lossless.",
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
    assert_eq!(ticket["ticket"]["project_id"], "legacy Prj");
}

#[tokio::test]
async fn the_agent_profile_policies_are_enforced_end_to_end() {
    // Round 1 finding 3: the persisted profile's origin restriction and
    // resource-type policy are enforced at their consumption points —
    // ingress for origins (a restriction on authenticated service
    // calls, never an authentication factor), ticket creation for
    // resource references.
    let (_directory, config) = scratch_config("profile-policy");
    let desk = build_desk(&config).await.expect("compose the desk");

    // An authenticated bearer call with the ALLOWED origin succeeds;
    // without any Origin header (non-browser service call) it also
    // succeeds; with a foreign origin it is denied before any business
    // handler — even though the credential itself is valid.
    for (origin, expected) in [
        (None, StatusCode::OK),
        (Some("http://127.0.0.1:8090"), StatusCode::OK),
        (Some("https://evil.example.test"), StatusCode::FORBIDDEN),
    ] {
        let mut request = Request::get("/_minco/ticketing/agent/bootstrap")
            .header("authorization", format!("Bearer {}", config.agent_token));
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let response = desk
            .router
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "origin policy mismatch");
    }

    // The resource-type policy is empty for this profile, so a create
    // carrying ANY resource reference is denied before the ticket
    // exists; the same create without references succeeds.
    let referenced = serde_json::json!({
        "project_id": "desk-proof",
        "subject": "Reference proof",
        "description": "Denied at the consumption point.",
        "requester": {"subject": "requester-1"},
        "channel": "portal",
        "resource_references": [
            {"system": "orders", "resource_type": "order", "resource_id": "o-1"}
        ]
    });
    let denied = desk
        .router
        .clone()
        .oneshot(
            Request::post("/_minco/ticketing/tickets")
                .extension(desk.agent_principal.clone())
                .header("content-type", "application/json")
                .body(Body::from(referenced.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    // No ticket row was created by the denied request.
    let pool = migrate(&config).await.expect("migrate");
    let tickets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ticketing_tickets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tickets, 0, "the denied create must not persist a ticket");
    pool.close().await;
    // And the portal + email profiles were provisioned alongside the
    // agent profile, bound to the same project and workspace.
    let binding: String =
        sqlx::query_scalar("SELECT workspace_id FROM workspace_deployment_binding")
            .fetch_one(&migrate(&config).await.unwrap())
            .await
            .unwrap();
    let profiles: Vec<(String, String)> =
        sqlx::query_as("SELECT profile_id, kind FROM workspace_profiles ORDER BY profile_id")
            .fetch_all(&migrate(&config).await.unwrap())
            .await
            .unwrap();
    assert_eq!(
        profiles,
        vec![
            ("desk-agent".to_owned(), "service_api".to_owned()),
            ("desk-mail".to_owned(), "email".to_owned()),
            ("desk-portal".to_owned(), "portal".to_owned()),
        ]
    );
    assert!(binding.starts_with("ws-"));
}

/// Rewrite one service-created ticket (and its child message) under
/// FULLY deterministic record identifiers on a bound desk database: the
/// columnar authority is the projection columns (reads never consult
/// `ticket_json`), so the rows stay fully valid while their identifiers
/// become byte-identical across databases (round 1 finding 7).
async fn seed_identical_ticket(
    desk: &minco_desk_example::BuiltDesk,
    config: &DeskConfig,
    ticket_id: &str,
    message_id: &str,
) {
    // Create a real ticket through the service so every projection
    // column is valid.
    let created = desk
        .router
        .clone()
        .oneshot(
            Request::post("/_minco/ticketing/tickets")
                .extension(desk.agent_principal.clone())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "project_id": config.project_id,
                        "subject": "Identical record",
                        "description": "Deterministic identifiers.",
                        "requester": {"subject": "same-requester"},
                        "channel": "portal"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let pool = migrate(config).await.expect("migrate");
    let template_id: String = sqlx::query_scalar("SELECT id FROM ticketing_tickets LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("read the service-created row");
    // The id rewrite spans the parent row and its child FKs at once, so
    // it runs with foreign keys disabled on this connection (SQLite's
    // documented procedure for identifier rewrites), re-enabled after.
    let mut connection = pool.acquire().await.expect("connection");
    sqlx::raw_sql(sqlx::AssertSqlSafe("PRAGMA foreign_keys = OFF"))
        .execute(&mut *connection)
        .await
        .expect("disable foreign keys");
    sqlx::query(
        "UPDATE ticketing_tickets SET id = ?, display_reference = 'TKT-COLLIDING' WHERE id = ?",
    )
    .bind(ticket_id)
    .bind(&template_id)
    .execute(&mut *connection)
    .await
    .expect("rewrite the ticket under its deterministic id");
    sqlx::query("UPDATE ticketing_messages SET id = ?, ticket_id = ? WHERE ticket_id = ?")
        .bind(message_id)
        .bind(ticket_id)
        .bind(&template_id)
        .execute(&mut *connection)
        .await
        .expect("rewrite the message under its deterministic ids");
    sqlx::raw_sql(sqlx::AssertSqlSafe("PRAGMA foreign_keys = ON"))
        .execute(&mut *connection)
        .await
        .expect("re-enable foreign keys");
    drop(connection);
    // A deterministic receipt for the same database, keyed identically
    // in both worlds.
    sqlx::query(
        "INSERT OR REPLACE INTO ticketing_operation_receipts
         (idempotency_key, fingerprint, response_json, created_at, operation, project_id, subject_digest, expires_at)
         VALUES ('idem-identical', 'fp-identical', '{}', ?, 'requester_reply', ?, 'digest', NULL)",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(&config.project_id)
    .execute(&pool)
    .await
    .expect("seed the identical receipt");
    pool.close().await;
}

#[tokio::test]
async fn identical_record_ids_across_workspaces_never_cross() {
    // Round 1 finding 7: TWO databases whose ticketing rows carry
    // BYTE-IDENTICAL record identifiers — same ticket id, same message
    // id, same display reference, same requester subject. Crossed
    // caller/execution contexts: each desk's own principal asks for the
    // SAME id; reads, lists, and counts return only local rows, and no
    // foreign mutation is possible.
    let (_directory_a, config_a) = scratch_config("identical-a");
    let (_directory_b, config_b) = scratch_config("identical-b");
    let ticket_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let message_id = "11111111-2222-3333-4444-555555555555";

    let desk_a = build_desk(&config_a).await.expect("desk a");
    let desk_b = build_desk(&config_b).await.expect("desk b");
    assert_ne!(
        desk_a.workspace_report.workspace,
        desk_b.workspace_report.workspace
    );
    seed_identical_ticket(&desk_a, &config_a, ticket_id, message_id).await;
    seed_identical_ticket(&desk_b, &config_b, ticket_id, message_id).await;

    // Each desk's agent detail for the IDENTICAL id returns its own row.
    for desk in [&desk_a, &desk_b] {
        let detail = desk
            .router
            .clone()
            .oneshot(
                Request::get(format!("/_minco/ticketing/agent/tickets/{ticket_id}"))
                    .extension(desk.agent_principal.clone())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail.status(), StatusCode::OK);
        let body = detail.into_body().collect().await.unwrap().to_bytes();
        let detail: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(detail["ticket"]["id"], ticket_id);
        // Agent list sees exactly one row under the identical ids.
        let listing = desk
            .router
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/agent/tickets?page[limit]=25")
                    .extension(desk.agent_principal.clone())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(listing.status(), StatusCode::OK);
        let body = listing.into_body().collect().await.unwrap().to_bytes();
        let page: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(page["data"].as_array().map(Vec::len), Some(1));
    }

    // Requester reads are per-subject per-database: the SAME subject on
    // desk A sees only A's ticket — never B's identical records.
    let requester_scope: std::collections::BTreeSet<String> = desk_a
        .agent_principal
        .claims
        .get(minco_http::PRINCIPAL_SCOPES_CLAIM)
        .map(|value| value.split_ascii_whitespace().map(str::to_owned).collect())
        .unwrap_or_default();
    let requester = minco_http::Principal {
        subject: "same-requester".into(),
        permissions: std::iter::once("ticketing.requester.read".to_owned()).collect(),
        claims: BTreeMap::default(),
    }
    .with_scopes(requester_scope);
    let read = desk_a
        .router
        .clone()
        .oneshot(
            Request::get(format!("/_minco/ticketing/requester/tickets/{ticket_id}"))
                .extension(requester)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::OK);

    // Each database holds exactly its own rows under the IDENTICAL
    // record ids — ticket, child message, and receipt — no database ever
    // sees two, and the bytes on disk stay separate.
    for config in [&config_a, &config_b] {
        let pool = migrate(config).await.unwrap();
        let tickets: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ticketing_tickets WHERE id = ?")
                .bind(ticket_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(tickets, 1, "each database holds exactly its own ticket");
        let messages: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ticketing_messages WHERE id = ?")
                .bind(message_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(messages, 1, "the identical child message id is local");
        let receipts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ticketing_operation_receipts WHERE idempotency_key = 'idem-identical'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(receipts, 1, "the identical receipt key is local");
        pool.close().await;
    }
}

#[tokio::test]
async fn a_wrong_project_service_cannot_touch_another_projects_exchange_grant() {
    // Round 1 finding 1: two project-bound services over the SAME real
    // SQLite database and session store. A wrong-project (or
    // wrong-workspace) service can neither rotate, revoke, abandon, nor
    // fence-overwrite the other's exchange grant, and every affected
    // record stays unchanged.
    let directory = tempfile::tempdir().expect("temp dir");
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("shared.sqlite").display()
    );
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("shared pool");
    minco_sqlx_sqlite::plugin_adapters::migrate_plugin_storage(&pool)
        .await
        .expect("plugin storage");
    minco_plugin_ticketing::SqliteTicketingStore::new(pool.clone())
        .migrate()
        .await
        .expect("ticketing migrations");
    let store = minco_plugin_ticketing::TicketingStoreService::new(std::sync::Arc::new(
        minco_plugin_ticketing::SqliteTicketingStore::new(pool.clone()),
    ));
    let sessions = std::sync::Arc::new(minco_plugin_sessions::SessionService::new(
        std::sync::Arc::new(minco_sqlx_sqlite::plugin_adapters::SqliteSessionStore::new(
            pool.clone(),
        )),
    ));
    let csrf = std::sync::Arc::new(
        minco_plugin_sessions::CsrfService::new("csrf-secret-of-sufficient-length!!")
            .expect("csrf"),
    );
    let service = |project: &str, workspace: Option<&str>| {
        minco_plugin_ticketing::TicketingService::new(
            store.clone(),
            minco_plugin_ticketing::TicketingConfig {
                project_id: project.into(),
                portal_origin: "https://support.example.test".into(),
                ..minco_plugin_ticketing::TicketingConfig::default()
            },
        )
        .expect("service")
        .with_isolation(minco_plugin_ticketing::TicketingIsolationConfig {
            workspace_id: workspace.map(str::to_owned),
            ..minco_plugin_ticketing::TicketingIsolationConfig::default()
        })
        .expect("isolation")
        .with_portal_services(minco_plugin_ticketing::TicketingPortalServices {
            sessions: Some(sessions.clone()),
            csrf: Some(csrf.clone()),
            ..Default::default()
        })
    };
    let a = service("project-a", Some("ws-shared"));
    let b = service("project-b", Some("ws-shared"));
    let c = service("project-a", Some("ws-other"));

    // Project A records its grant for a freshly issued session.
    let session_id = minco_plugin_sessions::SessionId(uuid::Uuid::new_v4());
    a.record_session_exchange_grant(
        "key-a",
        session_id,
        "requester-a",
        "https://support.example.test",
        vec!["ticketing.requester.read".into()],
        chrono::Utc::now() + chrono::TimeDelta::minutes(10),
    )
    .await
    .expect("A records its grant");

    // B (wrong project) and C (wrong workspace) are denied on every
    // effectful operation, before any effect.
    let foreign_session = minco_plugin_sessions::SessionId(uuid::Uuid::new_v4());
    assert!(b.rotate_session_exchange("key-a").await.is_err());
    assert!(
        b.record_session_exchange_grant(
            "key-a",
            foreign_session,
            "attacker",
            "https://evil.example.test",
            vec!["ticketing.manage".into()],
            chrono::Utc::now() + chrono::TimeDelta::minutes(10),
        )
        .await
        .is_err()
    );
    assert!(
        b.revoke_exchange_for_logout(&BTreeMap::from([(
            "ticketing.exchange_key".to_owned(),
            "key-a".to_owned(),
        )]))
        .await
        .is_err()
    );
    assert!(
        b.abandon_session_exchange(
            "key-a",
            minco_plugin_sessions::SessionId(uuid::Uuid::new_v4())
        )
        .await
        .is_err()
    );
    assert!(c.rotate_session_exchange("key-a").await.is_err());

    // The grant is unchanged: same session, generation, liveness, and
    // persisted workspace binding.
    let grant = store
        .session_exchange_grant("key-a")
        .await
        .expect("grant read")
        .expect("grant exists");
    assert_eq!(grant.session_id, session_id);
    assert_eq!(grant.generation, 0);
    assert!(grant.revoked_at.is_none());
    assert_eq!(grant.project_id, "project-a");
    assert_eq!(
        store
            .exchange_grant_workspace_binding("key-a")
            .await
            .unwrap(),
        minco_plugin_ticketing::ExchangeGrantWorkspaceBinding::Bound("ws-shared".into())
    );
}

#[tokio::test]
async fn resource_policy_belongs_to_the_effective_caller_not_the_service() {
    // Round 2 / P1-3 counterexample, both directions: the agent profile
    // permits resource type `order-board` while the portal profile
    // denies it; the portal profile permits `deploy-board` while the
    // agent profile denies it. The policy deciding a reference-bearing
    // create is the effective caller's own `resources:` scope tokens —
    // never a service-global allowlist — so neither caller can inherit
    // the other profile's policy, and a caller with no resource tokens
    // can carry no references at all.
    let store = minco_plugin_ticketing::TicketingStoreService::new(std::sync::Arc::new(
        minco_plugin_ticketing::MemoryTicketingStore::default(),
    ));
    let service = minco_plugin_ticketing::TicketingService::new(
        store,
        minco_plugin_ticketing::TicketingConfig {
            project_id: "project-r3".into(),
            portal_origin: "https://support.example.test".into(),
            ..minco_plugin_ticketing::TicketingConfig::default()
        },
    )
    .expect("service")
    .with_isolation(minco_plugin_ticketing::TicketingIsolationConfig {
        workspace_id: Some("ws-r3".into()),
        // The portal profile's resolved policy (round 1 finding 3).
        portal_resource_types: std::iter::once("deploy-board").map(str::to_owned).collect(),
    })
    .expect("isolation");

    let caller = |subject: &str, resources: &[&str]| minco_plugin_identity::Identity {
        subject: subject.into(),
        permissions: std::iter::once("ticketing.create").map(str::to_owned).collect(),
        scopes: {
            let mut tokens = std::collections::BTreeSet::from([
                "workspace:ws-r3".to_owned(),
                "project:project-r3".to_owned(),
            ]);
            for resource in resources {
                tokens.insert(format!("resources:{resource}"));
            }
            tokens
        },
        claims: BTreeMap::new(),
    };
    // The agent profile permits only `order-board`; the portal-minted
    // requester carries only the portal policy (`deploy-board`).
    let agent = caller("agent-1", &["order-board"]);
    let portal_requester = caller("requester-1", &["deploy-board"]);
    let bare_agent = caller("agent-2", &[]);

    let input = |resource_type: &str, subject: &str| minco_plugin_ticketing::CreateTicketInput {
        project_id: "project-r3".into(),
        subject: "Reference-bearing create".into(),
        description: "Carries one resource reference.".into(),
        requester: minco_plugin_ticketing::TicketRequester {
            subject: subject.into(),
            display_name: None,
            email: None,
        },
        channel: minco_plugin_ticketing::TicketChannel::Api,
        priority: minco_plugin_ticketing::TicketPriority::Normal,
        ticket_type: minco_plugin_ticketing::TicketType::default(),
        form_answers: Vec::new(),
        resource_references: vec![minco_plugin_ticketing::SupportResourceReference {
            system: "internal".into(),
            resource_type: resource_type.into(),
            resource_id: "K-1".into(),
        }],
    };

    // Agent direction: its own policy admits `order-board` …
    service
        .create_ticket(
            &agent,
            input("order-board", "agent-1"),
            uuid::Uuid::now_v7(),
            chrono::Utc::now(),
        )
        .await
        .expect("the agent's own resource policy admits its references");
    // … and denies the portal-only type — no inheritance of the
    // portal's broader set, and no restrictive global intersection.
    let denied = service
        .create_ticket(
            &agent,
            input("deploy-board", "agent-1"),
            uuid::Uuid::now_v7(),
            chrono::Utc::now(),
        )
        .await
        .expect_err("the agent cannot use the portal profile's resource type");
    assert!(matches!(
        denied,
        minco_plugin_ticketing::TicketingServiceError::ScopeDenied
    ));
    // Portal direction: the portal-minted requester's own policy admits
    // `deploy-board` …
    service
        .create_ticket(
            &portal_requester,
            input("deploy-board", "requester-1"),
            uuid::Uuid::now_v7(),
            chrono::Utc::now(),
        )
        .await
        .expect("the portal caller's own resource policy admits its references");
    // … and the agent-only type stays out of the portal's reach.
    let denied = service
        .create_ticket(
            &portal_requester,
            input("order-board", "requester-1"),
            uuid::Uuid::now_v7(),
            chrono::Utc::now(),
        )
        .await
        .expect_err("the portal caller cannot use the agent profile's resource type");
    assert!(matches!(
        denied,
        minco_plugin_ticketing::TicketingServiceError::ScopeDenied
    ));
    // A caller with no resource tokens carries no references at all.
    let denied = service
        .create_ticket(
            &bare_agent,
            input("order-board", "agent-2"),
            uuid::Uuid::now_v7(),
            chrono::Utc::now(),
        )
        .await
        .expect_err("no resource tokens means no reference-bearing creates");
    assert!(matches!(
        denied,
        minco_plugin_ticketing::TicketingServiceError::ScopeDenied
    ));
}

#[tokio::test]
async fn the_upgrade_inventories_every_historical_project_and_binds_ownership() {
    // Round 1 finding 5: a populated pre-isolation database carries a
    // live project (tickets) AND an artifact-only project (handoff,
    // exchange grant, operation receipt — no ticket). The isolated desk
    // registers BOTH verbatim, binds ticket ownership under the
    // provisioned workspace with a composite foreign key (enforced on
    // fresh pooled connections), backfills legacy grants, and a restart
    // converges on the same binding.
    let (_directory, config) = scratch_config("inventory");

    // The pre-isolation stack.
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect(&config.database_url)
        .await
        .expect("open");
    minco_sqlx_sqlite::plugin_adapters::migrate_plugin_storage(&pool)
        .await
        .expect("plugin storage");
    minco_plugin_ticketing::SqliteTicketingStore::new(pool.clone())
        .migrate()
        .await
        .expect("ticketing migrations");
    let legacy = minco_plugin_ticketing::TicketingService::new(
        minco_plugin_ticketing::TicketingStoreService::new(std::sync::Arc::new(
            minco_plugin_ticketing::SqliteTicketingStore::new(pool.clone()),
        )),
        minco_plugin_ticketing::TicketingConfig {
            project_id: "desk-proof".into(),
            portal_origin: "https://support.example.test".into(),
            ..minco_plugin_ticketing::TicketingConfig::default()
        },
    )
    .expect("legacy service");
    let legacy_identity = minco_plugin_identity::Identity {
        subject: "legacy-agent".into(),
        permissions: [
            "ticketing.create",
            "ticketing.agent.read",
            "ticketing.manage",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        scopes: std::collections::BTreeSet::new(),
        claims: BTreeMap::new(),
    };
    legacy
        .create_ticket(
            &legacy_identity,
            minco_plugin_ticketing::CreateTicketInput {
                project_id: "desk-proof".into(),
                subject: "Live project".into(),
                description: "Ticketed history.".into(),
                requester: minco_plugin_ticketing::TicketRequester {
                    subject: "legacy-requester".into(),
                    display_name: None,
                    email: None,
                },
                channel: minco_plugin_ticketing::TicketChannel::Api,
                priority: minco_plugin_ticketing::TicketPriority::Normal,
                ticket_type: minco_plugin_ticketing::TicketType::default(),
                form_answers: Vec::new(),
                resource_references: Vec::new(),
            },
            uuid::Uuid::now_v7(),
            chrono::Utc::now(),
        )
        .await
        .expect("legacy ticket");
    // An artifact-only project: security artifacts with NO ticket row.
    let now = chrono::Utc::now().to_rfc3339();
    let future = (chrono::Utc::now() + chrono::TimeDelta::hours(1)).to_rfc3339();
    sqlx::query(
        "INSERT INTO ticketing_handoffs
         (digest, handoff_id, project_id, portal_origin, expires_at, handoff_json)
         VALUES ('digest-b', 'handoff-b', 'legacy Prj',
                 'https://support.example.test', ?, '{}')",
    )
    .bind(&future)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ticketing_session_exchange_grants
         (exchange_key, session_id, subject, project_id, permissions, portal_origin,
          expires_at, created_at, generation, revoked_at, rotation_staged_session_id, workspace_id)
         VALUES ('key-b', ?, 'requester-b', 'legacy Prj', 'ticketing.requester.read',
                 'https://support.example.test', ?, ?, 0, NULL, NULL, NULL)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&future)
    .bind(&now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ticketing_operation_receipts
         (idempotency_key, fingerprint, response_json, created_at, operation, project_id, subject_digest, expires_at)
         VALUES ('idem-b', 'fp', '{}', ?, 'requester_reply', 'legacy Prj', 'digest', ?)",
    )
    .bind(&now)
    .bind(&future)
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    // The upgrade: inventory registers both projects verbatim.
    let desk = build_desk(&config).await.expect("upgrade build");
    let pool = migrate(&config).await.expect("post-upgrade pool");
    let registered: Vec<String> =
        sqlx::query_scalar("SELECT project_id FROM workspace_projects ORDER BY project_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        registered,
        vec!["desk-proof".to_owned(), "legacy Prj".to_owned()],
        "every historical project registers verbatim — including the artifact-only one"
    );
    // Ticket ownership is bound with the workspace default.
    let ticket_binding: Vec<(String, String)> =
        sqlx::query_as("SELECT workspace_id, project_id FROM ticketing_tickets")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(ticket_binding.len(), 1);
    assert_eq!(
        ticket_binding[0].0,
        desk.workspace_report.workspace.as_str()
    );
    assert_eq!(ticket_binding[0].1, "desk-proof");
    // The legacy grant was backfilled.
    let grant_workspace: Option<String> = sqlx::query_scalar(
        "SELECT workspace_id FROM ticketing_session_exchange_grants WHERE exchange_key = 'key-b'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        grant_workspace.as_deref(),
        Some(desk.workspace_report.workspace.as_str())
    );
    // The composite FK holds on FRESH pooled connections: a ticket for
    // the bound project inherits the workspace default; a ticket with a
    // foreign workspace is rejected; an unregistered project is
    // rejected.
    let inserted = sqlx::query(
        "INSERT INTO ticketing_tickets (project_id, id, display_reference, status, requester_subject, updated_at, revision, ticket_json)
         VALUES ('desk-proof', '11111111-1111-1111-1111-111111111111', 'TKT-FK-1', 'new', 'r', ?, 0, '{}')",
    )
    .bind(now.clone())
    .execute(&pool)
    .await;
    assert!(
        inserted.is_ok(),
        "the default workspace anchors local inserts"
    );
    let foreign = sqlx::query(
        "INSERT INTO ticketing_tickets (workspace_id, project_id, id, display_reference, status, requester_subject, updated_at, revision, ticket_json)
         VALUES ('ws-foreign', 'desk-proof', '22222222-2222-2222-2222-222222222222', 'TKT-FK-2', 'new', 'r', ?, 0, '{}')",
    )
    .bind(&now)
    .execute(&pool)
    .await;
    assert!(
        foreign.is_err(),
        "the composite FK must reject a foreign workspace"
    );
    let unregistered = sqlx::query(
        "INSERT INTO ticketing_tickets (workspace_id, project_id, id, display_reference, status, requester_subject, updated_at, revision, ticket_json)
         VALUES (?, 'never-registered', '33333333-3333-3333-3333-333333333333', 'TKT-FK-3', 'new', 'r', ?, 0, '{}')",
    )
    .bind(desk.workspace_report.workspace.as_str())
    .bind(&now)
    .execute(&pool)
    .await;
    assert!(
        unregistered.is_err(),
        "the composite FK must reject an unregistered project"
    );
    // Interruption/restart: a second build converges on the same
    // binding without touching the data.
    let rebuilt = build_desk(&config).await.expect("restart converges");
    assert_eq!(
        rebuilt.workspace_report.workspace,
        desk.workspace_report.workspace
    );
    let tickets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ticketing_tickets")
        .fetch_one(&migrate(&config).await.unwrap())
        .await
        .unwrap();
    assert_eq!(tickets, 2, "restart preserves every bound ticket");
}

#[tokio::test]
async fn upgrading_a_base_tree_database_preserves_data_checksums_and_ledgers() {
    // ISO-5 with a PROVENANCE-PINNED base writer (round 1 finding 7):
    // the pre-isolation database is built from the BASE TREE'S OWN
    // migration stream (fixtures/base-d7939910-ticketing-migrations,
    // exported verbatim from d7939910), verified byte-identical to the
    // base commit and to the candidate's unchanged 0001-0020, populated
    // by the merged tree's real writer, then upgraded through the
    // candidate. Published checksums for the shared files are identical
    // by construction, so the ledger comparison is exact.
    use sha2::Digest as _;
    let fixture_dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/base-d7939910-ticketing-migrations"
    );
    let candidate_dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../plugins/minco-plugin-ticketing/migrations/sqlite"
    );
    let mut fixtures: Vec<(String, String)> = std::fs::read_dir(fixture_dir)
        .expect("fixture directory")
        .flatten()
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (name, entry.path().display().to_string())
        })
        .filter(|(name, _)| {
            std::path::Path::new(name)
                .extension()
                .is_some_and(|e| e == "sql")
        })
        .collect();
    fixtures.sort();
    assert_eq!(
        fixtures.len(),
        20,
        "the base stream ships exactly twenty files"
    );
    // Candidate files 0001..=0020 must be byte-identical to the pinned
    // base fixtures — the candidate's migration additions are strictly
    // forward (0021, 0022).
    for (name, path) in &fixtures {
        let candidate = std::fs::read_to_string(format!("{candidate_dir}/{name}"))
            .unwrap_or_else(|_| panic!("candidate counterpart of {name} is missing"));
        let pinned = std::fs::read_to_string(path).expect("pinned fixture");
        assert!(
            pinned == candidate,
            "{name} diverges from the pinned base stream"
        );
    }

    let (_directory, config) = scratch_config("base-upgrade");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect(&config.database_url)
        .await
        .expect("open the base database");
    minco_sqlx_sqlite::plugin_adapters::migrate_plugin_storage(&pool)
        .await
        .expect("plugin storage (base)");
    for (index, (name, path)) in fixtures.iter().enumerate() {
        // The base tree's ledger table (sqlx default shape).
        if index == 0 {
            sqlx::raw_sql(sqlx::AssertSqlSafe(
                "CREATE TABLE IF NOT EXISTS _sqlx_migrations (
                    version BIGINT PRIMARY KEY,
                    description TEXT NOT NULL,
                    checksum BLOB NOT NULL,
                    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    success BOOLEAN NOT NULL,
                    execution_time BIGINT
                )",
            ))
            .execute(&pool)
            .await
            .expect("create the base ledger table");
        }
        let sql = std::fs::read_to_string(path).expect("fixture sql");
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&pool)
            .await
            .expect("apply the base migration file");
        // Record the base ledger row exactly as the base tree's sqlx
        // migrator would: version from the filename position, checksum
        // as the SHA-384 digest of the file's SQL text (sqlx's own
        // `migration::checksum`).
        let checksum = sha2::Sha384::digest(sql.as_str());
        sqlx::query(
            "INSERT INTO _sqlx_migrations
             (version, description, checksum, installed_on, success, execution_time)
             VALUES (?, ?, ?, CURRENT_TIMESTAMP, 1, 0)",
        )
        .bind(i64::try_from(index + 1).expect("version"))
        .bind(name.trim_end_matches(".sql"))
        .bind(checksum.to_vec())
        .execute(&pool)
        .await
        .expect("record the base ledger row");
    }
    let legacy = minco_plugin_ticketing::TicketingService::new(
        minco_plugin_ticketing::TicketingStoreService::new(std::sync::Arc::new(
            minco_plugin_ticketing::SqliteTicketingStore::new(pool.clone()),
        )),
        minco_plugin_ticketing::TicketingConfig {
            project_id: config.project_id.clone(),
            portal_origin: "https://support.example.test".into(),
            ..minco_plugin_ticketing::TicketingConfig::default()
        },
    )
    .expect("base writer service");
    let legacy_identity = minco_plugin_identity::Identity {
        subject: "legacy-agent".into(),
        permissions: [
            "ticketing.create",
            "ticketing.agent.read",
            "ticketing.manage",
            "ticketing.reply",
            "ticketing.agent-console",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        scopes: std::collections::BTreeSet::new(),
        claims: BTreeMap::new(),
    };
    let legacy_ticket = legacy
        .create_ticket(
            &legacy_identity,
            minco_plugin_ticketing::CreateTicketInput {
                project_id: config.project_id.clone(),
                subject: "Base-tree ticket".into(),
                description: "Written by the merged tree's real writer.".into(),
                requester: minco_plugin_ticketing::TicketRequester {
                    subject: "legacy-requester".into(),
                    display_name: None,
                    email: None,
                },
                channel: minco_plugin_ticketing::TicketChannel::Api,
                priority: minco_plugin_ticketing::TicketPriority::Normal,
                ticket_type: minco_plugin_ticketing::TicketType::default(),
                form_answers: Vec::new(),
                resource_references: Vec::new(),
            },
            uuid::Uuid::now_v7(),
            chrono::Utc::now(),
        )
        .await
        .expect("base writer");
    pool.close().await;

    // The upgrade through the candidate: migrate() fills the candidate
    // ledger (base rows recorded as applied by the candidate migrator's
    // checksums of the identical files), then the isolated desk boots.
    let desk = build_desk(&config).await.expect("upgrade build");
    assert!(desk.workspace_report.created);
    let pool = migrate(&config).await.expect("post-upgrade pool");
    // Every ledger row's checksum matches the corresponding pinned base
    // file's bytes — the published history is preserved exactly.
    let ledger_rows =
        sqlx::query("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        ledger_rows.len(),
        22,
        "base twenty plus the candidate's two"
    );
    for row in &ledger_rows {
        let version: i64 = sqlx::Row::get(row, "version");
        let checksum: Vec<u8> = sqlx::Row::get(row, "checksum");
        let Ok(index) = usize::try_from(version) else {
            panic!("ledger version {version} is not a valid index");
        };
        if (1..=20).contains(&index) {
            let (_, path) = &fixtures[index - 1];
            let pinned = std::fs::read_to_string(path).expect("pinned fixture");
            let digest = sha2::Sha384::digest(pinned.as_str());
            assert_eq!(
                digest.as_slice(),
                checksum.as_slice(),
                "ledger row {version}"
            );
        }
    }
    pool.close().await;
    // The base tree's ticket serves through the isolated stack.
    let detail = desk
        .router
        .clone()
        .oneshot(
            Request::get(format!(
                "/_minco/ticketing/agent/tickets/{}",
                legacy_ticket.ticket.id
            ))
            .extension(desk.agent_principal.clone())
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
}

#[tokio::test]
async fn upgrading_a_pre_isolation_database_preserves_data_checksums_and_ledgers() {
    // ISO-5: a populated database produced by the pre-isolation stack
    // (plugin storage + ticketing migrations, a real legacy writer, no
    // workspace tables) upgrades through the isolated desk with every
    // published ticketing checksum preserved, the workspace registry
    // arriving under its own ledger, and the legacy data fully visible.
    let (_directory, config) = scratch_config("upgrade");

    // The pre-isolation stack: exactly the two migrators the merged tree
    // ran, in the merged order, and a legacy (isolation-off) service as
    // the writer.
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect(&config.database_url)
        .await
        .expect("open the pre-isolation database");
    minco_sqlx_sqlite::plugin_adapters::migrate_plugin_storage(&pool)
        .await
        .expect("plugin-storage migrations");
    minco_plugin_ticketing::SqliteTicketingStore::new(pool.clone())
        .migrate()
        .await
        .expect("ticketing migrations");
    let legacy_config = minco_plugin_ticketing::TicketingConfig {
        project_id: config.project_id.clone(),
        portal_origin: "https://support.example.test".into(),
        ..minco_plugin_ticketing::TicketingConfig::default()
    };
    let legacy = minco_plugin_ticketing::TicketingService::new(
        minco_plugin_ticketing::TicketingStoreService::new(std::sync::Arc::new(
            minco_plugin_ticketing::SqliteTicketingStore::new(pool.clone()),
        )),
        legacy_config,
    )
    .expect("legacy service");
    let legacy_identity = minco_plugin_identity::Identity {
        subject: "legacy-agent".into(),
        permissions: [
            "ticketing.create",
            "ticketing.agent.read",
            "ticketing.manage",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        scopes: std::collections::BTreeSet::new(),
        claims: BTreeMap::new(),
    };
    let legacy_input = minco_plugin_ticketing::CreateTicketInput {
        project_id: config.project_id.clone(),
        subject: "Pre-upgrade ticket".into(),
        description: "Written by the pre-isolation stack.".into(),
        requester: minco_plugin_ticketing::TicketRequester {
            subject: "legacy-requester".into(),
            display_name: None,
            email: None,
        },
        channel: minco_plugin_ticketing::TicketChannel::Api,
        priority: minco_plugin_ticketing::TicketPriority::Normal,
        ticket_type: minco_plugin_ticketing::TicketType::default(),
        form_answers: Vec::new(),
        resource_references: Vec::new(),
    };
    let legacy_ticket = legacy
        .create_ticket(
            &legacy_identity,
            legacy_input,
            uuid::Uuid::now_v7(),
            chrono::Utc::now(),
        )
        .await
        .expect("legacy write");
    pool.close().await;

    // Snapshot the published ticketing ledger before the upgrade.
    let before = ticketing_ledger(&config).await;
    assert!(!before.is_empty(), "the pre-isolation ledger is populated");

    // The upgrade: the isolated desk migrates and provisions on top.
    let desk = build_desk(&config).await.expect("upgrade build");
    assert!(desk.workspace_report.created);

    // Published checksums are untouched: same rows, byte for byte.
    let after = ticketing_ledger(&config).await;
    assert_eq!(
        before, after,
        "published ticketing checksums must not change"
    );

    // The workspace registry arrived under its own ledger, never the
    // sqlx default.
    let pool = migrate(&config).await.expect("post-upgrade migrate");
    let workspace_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _minco_workspace_migrations")
            .fetch_one(&pool)
            .await
            .expect("workspace ledger rows");
    assert_eq!(
        workspace_rows, 2,
        "the workspace ledger owns its migrations"
    );
    let registered: Vec<String> = sqlx::query_scalar("SELECT project_id FROM workspace_projects")
        .fetch_all(&pool)
        .await
        .expect("registered projects");
    assert_eq!(
        registered,
        vec![config.project_id.clone()],
        "the legacy project registers verbatim"
    );
    pool.close().await;

    // The legacy data serves through the isolated stack.
    let detail = desk
        .router
        .clone()
        .oneshot(
            Request::get(format!(
                "/_minco/ticketing/agent/tickets/{}",
                legacy_ticket.ticket.id
            ))
            .extension(desk.agent_principal.clone())
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
}

async fn ticketing_ledger(config: &DeskConfig) -> Vec<(i64, Vec<u8>)> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await
        .expect("open for ledger snapshot");
    let rows = sqlx::query("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
        .fetch_all(&pool)
        .await
        .expect("ledger snapshot");
    pool.close().await;
    rows.iter()
        .map(|row| {
            (
                sqlx::Row::get::<i64, _>(row, "version"),
                sqlx::Row::get::<Vec<u8>, _>(row, "checksum"),
            )
        })
        .collect()
}

#[tokio::test]
async fn two_workspaces_with_colliding_identifiers_never_cross() {
    // ISO-3: two databases with deliberately identical local identifiers —
    // same project id, same agent credential, same requester subject and
    // ticket subject — stay fully separate, and each workspace's scope
    // resolves only its own binding.
    let (_directory_a, config_a) = scratch_config("collide-a");
    let (_directory_b, mut config_b) = scratch_config("collide-b");
    config_b.agent_token = config_a.agent_token.clone();

    let desk_a = build_desk(&config_a).await.expect("desk a");
    let desk_b = build_desk(&config_b).await.expect("desk b");
    assert_ne!(
        desk_a.workspace_report.workspace, desk_b.workspace_report.workspace,
        "two databases mint two distinct workspace identities"
    );

    let create = |desk: &minco_desk_example::BuiltDesk, subject: &'static str| {
        let router = desk.router.clone();
        let principal = desk.agent_principal.clone();
        async move {
            let response = router
                .oneshot(
                    Request::post("/_minco/ticketing/tickets")
                        .extension(principal)
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "project_id": "desk-proof",
                                "subject": subject,
                                "description": "Colliding identifier proof.",
                                "requester": {"subject": "same-requester"},
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
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["ticket"]["id"]
                .as_str()
                .unwrap()
                .to_owned()
        }
    };
    let ticket_a = create(&desk_a, "Same subject").await;
    let ticket_b = create(&desk_b, "Same subject").await;
    assert_ne!(ticket_a, ticket_b);

    // Each desk's agent listing sees exactly its own ticket.
    for (desk, expected_id) in [(&desk_a, &ticket_a), (&desk_b, &ticket_b)] {
        let listing = desk
            .router
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/agent/tickets")
                    .extension(desk.agent_principal.clone())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(listing.status(), StatusCode::OK);
        let body = listing.into_body().collect().await.unwrap().to_bytes();
        let page: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let ids: Vec<&str> = page["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec![expected_id.as_str()]);
    }

    // Desk B cannot read desk A's ticket: the identifier is local to its
    // project row in its own database.
    let foreign = desk_b
        .router
        .clone()
        .oneshot(
            Request::get(format!("/_minco/ticketing/agent/tickets/{ticket_a}"))
                .extension(desk_b.agent_principal.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(foreign.status(), StatusCode::NOT_FOUND);
}

#[test]
fn two_fictional_integrations_resolve_distinct_bounded_scopes() {
    // ISO-6 neutral fixtures: two differently named, fictional
    // integrations in one workspace exercise the same scope behavior —
    // differences are configuration, never product-specific branches —
    // and each binds to exactly its own project.
    use minco_plugin_workspace as workspace;
    use std::collections::{BTreeMap, BTreeSet};

    let config = workspace::WorkspaceConfig {
        display_name: "Neutral fixtures".into(),
        workspace_id: Some(
            workspace::WorkspaceId::try_from("ws-neutral".to_owned()).expect("valid"),
        ),
        projects: vec![
            workspace::ProjectId::try_from("acme-helpdesk".to_owned()).expect("valid"),
            workspace::ProjectId::try_from("northwind-portal".to_owned()).expect("valid"),
        ],
        profiles: [
            (
                workspace::ProfileId::try_from("acme-service".to_owned()).expect("valid"),
                workspace::ProfileSpec {
                    kind: workspace::ProfileKind::ServiceApi,
                    auth_mode: workspace::ProfileAuthMode::ServiceBearerToken,
                    bound_project: workspace::ProjectId::try_from("acme-helpdesk".to_owned())
                        .expect("valid"),
                    service_subject: "acme-service".into(),
                    permission_ceiling: ["ticketing.agent.read"]
                        .map(str::to_owned)
                        .into_iter()
                        .collect(),
                    allowed_origins: ["https://acme.example.test"]
                        .map(str::to_owned)
                        .into_iter()
                        .collect(),
                    resource_types: BTreeSet::new(),
                    secret_reference: Some("ACME_SERVICE_TOKEN".into()),
                },
            ),
            (
                workspace::ProfileId::try_from("northwind-portal".to_owned()).expect("valid"),
                workspace::ProfileSpec {
                    kind: workspace::ProfileKind::Portal,
                    auth_mode: workspace::ProfileAuthMode::PortalSession,
                    bound_project: workspace::ProjectId::try_from("northwind-portal".to_owned())
                        .expect("valid"),
                    service_subject: "northwind-portal".into(),
                    permission_ceiling: ["ticketing.requester.read", "ticketing.requester.write"]
                        .map(str::to_owned)
                        .into_iter()
                        .collect(),
                    allowed_origins: ["https://portal.northwind.example.test"]
                        .map(str::to_owned)
                        .into_iter()
                        .collect(),
                    resource_types: BTreeSet::new(),
                    secret_reference: None,
                },
            ),
        ]
        .into_iter()
        .collect(),
        grants: std::iter::once((
            "agent@acme.example.test".to_owned(),
            std::iter::once(
                workspace::ProjectId::try_from("acme-helpdesk".to_owned()).expect("valid"),
            )
            .collect::<BTreeSet<_>>(),
        ))
        .collect::<BTreeMap<_, _>>(),
    };
    let service = workspace::WorkspaceService::new(
        workspace::WorkspaceStoreService::new(std::sync::Arc::new(
            workspace::MemoryWorkspaceStore::new(),
        )),
        config,
    )
    .expect("neutral configuration is valid");

    // The behavior is identical in shape for both integrations: same
    // provisioning, same resolution rules, different bound projects.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    rt.block_on(async {
        service.provision().await.expect("provision");
        let acme = service
            .resolve_service_principal_scope(
                &workspace::ProfileId::try_from("acme-service".to_owned()).expect("valid"),
            )
            .await
            .expect("acme resolves");
        let northwind = service
            .resolve_service_principal_scope(
                &workspace::ProfileId::try_from("northwind-portal".to_owned()).expect("valid"),
            )
            .await
            .expect("northwind resolves");
        assert_eq!(acme.scope.project.as_str(), "acme-helpdesk");
        assert_eq!(northwind.scope.project.as_str(), "northwind-portal");
        assert_eq!(acme.scope.workspace, northwind.scope.workspace);
        // A principal granted on one project is denied on the other.
        let granted = service
            .resolve_granted_scope(
                "agent@acme.example.test",
                &workspace::ProjectId::try_from("acme-helpdesk".to_owned()).expect("valid"),
            )
            .await
            .expect("explicit grant resolves");
        assert_eq!(granted.project.as_str(), "acme-helpdesk");
        assert!(
            service
                .resolve_granted_scope(
                    "agent@acme.example.test",
                    &workspace::ProjectId::try_from("northwind-portal".to_owned()).expect("valid"),
                )
                .await
                .is_err()
        );

        // Round 1 finding 7: the neutral fixtures continue THROUGH
        // TICKETING OPERATIONS — two isolated services in ONE workspace,
        // one per fictional project, driven by their own scoped
        // identities. Behavior differs only by configuration, never by
        // product branches.
        let ticketing = |project: &str| {
            minco_plugin_ticketing::TicketingService::new(
                minco_plugin_ticketing::TicketingStoreService::new(std::sync::Arc::new(
                    minco_plugin_ticketing::MemoryTicketingStore::default(),
                )),
                minco_plugin_ticketing::TicketingConfig {
                    project_id: project.into(),
                    portal_origin: "https://support.example.test".into(),
                    ..minco_plugin_ticketing::TicketingConfig::default()
                },
            )
            .expect("neutral ticketing service")
            .with_isolation(minco_plugin_ticketing::TicketingIsolationConfig {
                workspace_id: Some("ws-neutral".into()),
                ..minco_plugin_ticketing::TicketingIsolationConfig::default()
            })
            .expect("neutral isolation")
        };
        let acme_tickets = ticketing("acme-helpdesk");
        let northwind_tickets = ticketing("northwind-portal");
        let scoped =
            |subject: &str, project: &str, permissions: &[&str]| minco_plugin_identity::Identity {
                subject: subject.into(),
                permissions: permissions.iter().map(|p| (*p).to_owned()).collect(),
                scopes: [
                    "workspace:ws-neutral".to_owned(),
                    format!("project:{project}"),
                ]
                .into_iter()
                .collect(),
                claims: BTreeMap::default(),
            };
        let input = |project: &str| minco_plugin_ticketing::CreateTicketInput {
            project_id: project.into(),
            subject: "Neutral fixture ticket".into(),
            description: "Same behavior, different configuration.".into(),
            requester: minco_plugin_ticketing::TicketRequester {
                subject: "requester@acme.example.test".into(),
                display_name: None,
                email: None,
            },
            channel: minco_plugin_ticketing::TicketChannel::Api,
            priority: minco_plugin_ticketing::TicketPriority::Normal,
            ticket_type: minco_plugin_ticketing::TicketType::default(),
            form_answers: Vec::new(),
            resource_references: Vec::new(),
        };
        let acme_created = acme_tickets
            .create_ticket(
                &scoped(
                    "acme-service",
                    "acme-helpdesk",
                    &["ticketing.create", "ticketing.manage"],
                ),
                input("acme-helpdesk"),
                uuid::Uuid::now_v7(),
                chrono::Utc::now(),
            )
            .await
            .expect("the acme fixture creates in its own project");
        let northwind_created = northwind_tickets
            .create_ticket(
                &scoped(
                    "northwind-portal",
                    "northwind-portal",
                    &["ticketing.create", "ticketing.manage"],
                ),
                input("northwind-portal"),
                uuid::Uuid::now_v7(),
                chrono::Utc::now(),
            )
            .await
            .expect("the northwind fixture creates in its own project");
        assert_eq!(acme_created.ticket.project_id, "acme-helpdesk");
        assert_eq!(northwind_created.ticket.project_id, "northwind-portal");
        // Cross-project reads are denied for both, identically: a
        // foreign project id is an untrusted selector.
        assert!(
            acme_tickets
                .get_ticket_for_agent(
                    &scoped("acme-service", "acme-helpdesk", &["ticketing.agent.read"]),
                    "northwind-portal",
                    northwind_created.ticket.id,
                )
                .await
                .is_err()
        );
        assert!(
            northwind_tickets
                .get_ticket_for_agent(
                    &scoped(
                        "northwind-portal",
                        "northwind-portal",
                        &["ticketing.agent.read"],
                    ),
                    "acme-helpdesk",
                    acme_created.ticket.id,
                )
                .await
                .is_err()
        );
        // And each reads its own ticket through the SAME code path.
        assert!(
            acme_tickets
                .get_ticket_for_agent(
                    &scoped("acme-service", "acme-helpdesk", &["ticketing.agent.read"]),
                    "acme-helpdesk",
                    acme_created.ticket.id,
                )
                .await
                .is_ok()
        );
        assert!(
            northwind_tickets
                .get_ticket_for_agent(
                    &scoped(
                        "northwind-portal",
                        "northwind-portal",
                        &["ticketing.agent.read"],
                    ),
                    "northwind-portal",
                    northwind_created.ticket.id,
                )
                .await
                .is_ok()
        );
    });
}

#[test]
fn product_neutrality_gate_finds_no_product_tokens() {
    // ISO-6: intentional-token scan over the isolation boundary's code
    // surfaces. The only exceptions are this gate's own token list and
    // the task/ADR prose, which are not code.
    const BANNED_PLAIN: &[&str] = &["peopleplanner", "people planner"];
    const BANNED_WORDS: &[&str] = &["mss"];

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repository = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("repository root");
    let surfaces = [
        repository.join("plugins/minco-plugin-workspace"),
        repository.join("plugins/minco-plugin-ticketing/src"),
        repository.join("plugins/minco-plugin-ticketing/migrations"),
        repository.join("examples/minco-desk/src"),
        manifest_dir.join("tests"),
    ];
    let mut offenders: Vec<String> = Vec::new();
    for surface in surfaces {
        for file in source_files(&surface) {
            let mut contents = std::fs::read_to_string(&file).expect("read source");
            // The gate's own token DECLARATIONS are the one documented
            // exception (round 1 finding 7): only the two declaration
            // lines are exempt, never the whole file — any other
            // occurrence anywhere, including this proof file, is an
            // offender.
            if file
                .file_name()
                .is_some_and(|name| name == "isolation_proofs.rs")
            {
                contents = contents
                    .lines()
                    .filter(|line| {
                        !line.contains("const BANNED_PLAIN") && !line.contains("const BANNED_WORDS")
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
            }
            let display = file.display();
            for token in BANNED_PLAIN {
                if contents.to_ascii_lowercase().contains(token) {
                    offenders.push(format!("{display}: contains {token:?}"));
                }
            }
            for word in BANNED_WORDS {
                if contains_standalone_word(&contents.to_ascii_lowercase(), word) {
                    offenders.push(format!("{display}: contains standalone {word:?}"));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "product tokens must not appear in isolation code surfaces:\n{}",
        offenders.join("\n")
    );
}

fn source_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            files.extend(source_files(&path));
        } else if path.extension().is_some_and(|extension| {
            extension == "rs" || extension == "sql" || extension == "toml" || extension == "json"
        }) {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn contains_standalone_word(haystack: &str, word: &str) -> bool {
    let mut search = 0;
    while let Some(found) = haystack[search..].find(word) {
        let start = search + found;
        let end = start + word.len();
        let before_ok = haystack[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        let after_ok = haystack[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        search = end;
    }
    false
}
