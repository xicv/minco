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
                workspace_isolation: workspace.is_some(),
                workspace_id: workspace.map(str::to_owned),
                ..minco_plugin_ticketing::TicketingConfig::default()
            },
        )
        .expect("service")
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
    // workspace binding.
    let grant = store
        .session_exchange_grant("key-a")
        .await
        .expect("grant read")
        .expect("grant exists");
    assert_eq!(grant.session_id, session_id);
    assert_eq!(grant.generation, 0);
    assert!(grant.revoked_at.is_none());
    assert_eq!(grant.project_id, "project-a");
    assert_eq!(grant.workspace_id.as_deref(), Some("ws-shared"));
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
            // The gate's own token list is the one documented exception.
            if file
                .file_name()
                .is_some_and(|name| name == "isolation_proofs.rs")
            {
                continue;
            }
            let contents = std::fs::read_to_string(&file).expect("read source");
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
