//! Durability and trust-boundary proofs for the standalone desk
//! (M14-T74 review findings 2 and 3). Every request crosses a real TCP
//! socket; no test injects a principal by hand.
//!
//! * The trust boundary is the durable session cookie plus the loopback
//!   service bearer token — forged development headers authorize nothing.
//! * Requester sessions, idempotent session exchange and job records
//!   survive a full process restart on the same database.
//! * A public agent reply commits its notification job in the same
//!   transaction; the explicit worker completes it after a restart.

use minco_desk_example::{DeskConfig, build_desk, migrate};
use sqlx::Row as _;
use std::collections::BTreeMap;

fn scratch_config(tag: &str, directory: &std::path::Path) -> DeskConfig {
    DeskConfig {
        host: "127.0.0.1".into(),
        port: 0,
        database_url: format!(
            "sqlite://{}?mode=rwc",
            directory
                .join(format!("desk-durability-{tag}.sqlite"))
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
    }
}

/// Serves the desk router on a real ephemeral socket; returns the origin.
async fn serve(router: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind the desk listener");
    let address = listener.local_addr().expect("desk address");
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve the desk");
    });
    format!("http://{address}")
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

async fn issue_handoff(origin: &str, agent_token: &str) -> String {
    let response = reqwest::Client::new()
        .post(format!("{origin}/_minco/ticketing/integrations/handoffs"))
        .header("authorization", bearer(agent_token))
        .json(&serde_json::json!({
            "project_id": "desk-proof",
            "requester_subject": "user-1",
            "requester_permissions": ["ticketing.requester.read", "ticketing.requester.write"],
            "surface": "portal",
            "context": {"page_url": "https://app.example.test/orders/1"},
            "return_location": "https://app.example.test/orders/1"
        }))
        .send()
        .await
        .expect("issue the handoff over real HTTP");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let grant: serde_json::Value = response.json().await.expect("handoff grant body");
    let launch_url = grant["launch_url"].as_str().expect("launch URL");
    launch_url
        .split('#')
        .nth(1)
        .and_then(|fragment| fragment.strip_prefix("handoff="))
        .expect("the opaque handoff token in the launch fragment")
        .to_owned()
}

async fn exchange_session(
    origin: &str,
    handoff_token: &str,
) -> (reqwest::StatusCode, Option<String>, serde_json::Value) {
    let response = reqwest::Client::new()
        .post(format!("{origin}/_minco/ticketing/requester/sessions"))
        .header("x-minco-ticketing-handoff", handoff_token)
        .json(&serde_json::json!({"portal_origin": "http://127.0.0.1:8090"}))
        .send()
        .await
        .expect("exchange the handoff over real HTTP");
    let status = response.status();
    let cookie = response
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::to_owned);
    let body: serde_json::Value = response.json().await.expect("session grant body");
    (status, cookie, body)
}

#[tokio::test]
async fn trust_boundary_is_the_bearer_and_session_not_development_headers() {
    let directory = tempfile::tempdir().expect("temp dir");
    let config = scratch_config("boundary", directory.path());
    let desk = build_desk(&config).await.expect("compose the desk");
    let origin = serve(desk.router.clone()).await;
    let client = reqwest::Client::new();

    // No token, no session: the agent surface refuses.
    let anonymous = client
        .get(format!("{origin}/_minco/ticketing/agent/bootstrap"))
        .send()
        .await
        .expect("anonymous bootstrap");
    assert_eq!(anonymous.status(), reqwest::StatusCode::UNAUTHORIZED);

    // Forged development identity headers authorize nothing.
    let forged = client
        .get(format!("{origin}/_minco/ticketing/agent/bootstrap"))
        .header("x-minco-subject", "attacker")
        .header("x-minco-permissions", "ticketing.manage")
        .send()
        .await
        .expect("forged headers");
    assert_eq!(forged.status(), reqwest::StatusCode::UNAUTHORIZED);

    // A wrong bearer token is refused.
    let wrong = client
        .get(format!("{origin}/_minco/ticketing/agent/bootstrap"))
        .header("authorization", bearer("not-the-token"))
        .send()
        .await
        .expect("wrong bearer");
    assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);

    // The loopback service bearer token opens the agent surface.
    let authorized = client
        .get(format!("{origin}/_minco/ticketing/agent/bootstrap"))
        .header("authorization", bearer(&config.agent_token))
        .send()
        .await
        .expect("bearer bootstrap");
    assert_eq!(authorized.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn sessions_idempotency_and_jobs_survive_a_full_restart() {
    let directory = tempfile::tempdir().expect("temp dir");
    let config = scratch_config("restart", directory.path());
    let desk = build_desk(&config).await.expect("compose the first desk");
    let origin = serve(desk.router.clone()).await;
    let client = reqwest::Client::new();
    let agent = bearer(&config.agent_token);

    // A ticket for user-1; notifications land on the in-app channel —
    // the providerless desk deliberately has no mail transport.
    let created = client
        .post(format!("{origin}/_minco/ticketing/tickets"))
        .header("authorization", &agent)
        .json(&serde_json::json!({
            "project_id": "desk-proof",
            "subject": "The widget stopped",
            "description": "It stopped after the last update yesterday.",
            "requester": {"subject": "user-1"},
            "channel": "portal"
        }))
        .send()
        .await
        .expect("create over real HTTP");
    assert_eq!(created.status(), reqwest::StatusCode::CREATED);
    let ticket: serde_json::Value = created.json().await.expect("ticket body");
    let ticket_id = ticket["ticket"]["id"]
        .as_str()
        .expect("ticket id")
        .to_owned();
    let revision = ticket["ticket"]["revision"].as_u64().expect("revision");

    // A public agent reply commits the notification job in the same
    // transaction (review finding 3): the reply is accepted and the job
    // row is durable before any worker runs.
    let replied = client
        .post(format!(
            "{origin}/_minco/ticketing/tickets/{ticket_id}/agent-replies"
        ))
        .header("authorization", &agent)
        .header(
            "if-match",
            format!("\"ticket:{ticket_id}:{}\"", revision + 1),
        )
        .json(&serde_json::json!({"body": "We shipped a fix; please reopen if it persists."}))
        .send()
        .await
        .expect("agent reply over real HTTP");
    assert_eq!(replied.status(), reqwest::StatusCode::OK);
    {
        let pool = migrate(&config).await.expect("reopen the desk database");
        let row = sqlx::query(
            "SELECT COUNT(*) AS n FROM minco_jobs WHERE worker_profile = 'ticketing-mail' AND status = 'pending'",
        )
        .fetch_optional(&pool)
        .await
        .expect("count durable pending jobs");
        let count: i64 = row.map_or(-1, |r| r.get::<i64, _>("n"));
        assert!(
            count >= 1,
            "the notification job must be durable and pending before the worker runs"
        );
    }

    // Requester portal: handoff, session exchange, cookie replay.
    let handoff = issue_handoff(&origin, &config.agent_token).await;
    let (status, cookie, grant) = exchange_session(&origin, &handoff).await;
    assert_eq!(status, reqwest::StatusCode::CREATED);
    let cookie = cookie.expect("the session cookie");
    assert!(grant.get("session_token").is_none());
    let (replay_status, replay_cookie, replay_grant) = exchange_session(&origin, &handoff).await;
    assert_eq!(replay_status, reqwest::StatusCode::CREATED);
    assert_eq!(replay_cookie.as_deref(), Some(cookie.as_str()));
    assert_eq!(replay_grant, grant);

    let listed = client
        .get(format!("{origin}/_minco/ticketing/requester/tickets"))
        .header("cookie", &cookie)
        .send()
        .await
        .expect("requester list over real HTTP");
    assert_eq!(listed.status(), reqwest::StatusCode::OK);

    // ---- Full process restart on the same database ----
    drop(desk);
    let desk = build_desk(&config)
        .await
        .expect("compose the restarted desk");
    let origin = serve(desk.router.clone()).await;

    // The durable session still authorizes the requester surface.
    let after_restart = client
        .get(format!("{origin}/_minco/ticketing/requester/tickets"))
        .header("cookie", &cookie)
        .send()
        .await
        .expect("requester list after restart");
    assert_eq!(
        after_restart.status(),
        reqwest::StatusCode::OK,
        "durable sessions survive the restart"
    );
    let page: serde_json::Value = after_restart.json().await.expect("list body");
    assert_eq!(page["data"].as_array().map(Vec::len), Some(1));

    // The explicit worker completes the pending notification job in the
    // restarted process.
    let report = desk.worker.run_once().await.expect("worker pass");
    assert!(
        report.dispatched >= 1,
        "the durable notification job must dispatch in the restarted process"
    );
    {
        let pool = migrate(&config).await.expect("reopen the desk database");
        let rows =
            sqlx::query("SELECT status FROM minco_jobs WHERE worker_profile = 'ticketing-mail'")
                .fetch_all(&pool)
                .await
                .expect("read job statuses");
        let statuses: Vec<String> = rows
            .iter()
            .map(|row| row.get::<String, _>("status"))
            .collect();
        assert!(
            statuses.iter().all(|status| status == "succeeded"),
            "every notification job must reach terminal success: {statuses:?}"
        );
    }
    drop(desk);
}

#[tokio::test]
async fn logout_expires_the_browser_cookie_and_revokes_the_session() {
    let directory = tempfile::tempdir().expect("temp dir");
    let config = scratch_config("logout", directory.path());
    let desk = build_desk(&config).await.expect("compose the desk");
    let origin = serve(desk.router.clone()).await;
    let client = reqwest::Client::new();

    let handoff = issue_handoff(&origin, &config.agent_token).await;
    let (_, cookie, _) = exchange_session(&origin, &handoff).await;
    let cookie = cookie.expect("session cookie");
    let csrf = exchange_session(&origin, &handoff).await.2["csrf_token"]
        .as_str()
        .expect("csrf token")
        .to_owned();

    let logout = client
        .post(format!("{origin}/_minco/ticketing/requester/logout"))
        .header("cookie", &cookie)
        .header("x-minco-csrf", &csrf)
        .send()
        .await
        .expect("logout over real HTTP");
    assert_eq!(logout.status(), reqwest::StatusCode::NO_CONTENT);
    let expiry = logout
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("logout expires the cookie");
    assert!(expiry.contains("Max-Age=0"));

    // The revoked session no longer authorizes anything.
    let refused = client
        .get(format!("{origin}/_minco/ticketing/requester/tickets"))
        .header("cookie", &cookie)
        .send()
        .await
        .expect("list after logout");
    assert_eq!(refused.status(), reqwest::StatusCode::UNAUTHORIZED);
    drop(desk);
}
