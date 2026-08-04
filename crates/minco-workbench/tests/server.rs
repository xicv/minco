use minco_project_view::load_project_view;
use minco_workbench::{bind_loopback, serve_loopback};
use reqwest::{Client, StatusCode, header};
use std::path::Path;

fn repository_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repository root")
}

#[tokio::test]
async fn server_is_loopback_only_and_enforces_host_origin_and_security_headers() {
    let view = load_project_view(&repository_root()).expect("repository ProjectView");
    let listener = bind_loopback(0).await.expect("bind loopback server");
    let address = listener.local_addr().expect("loopback address");
    assert!(address.ip().is_loopback());
    let origin = format!("http://{address}");
    let server = tokio::spawn(serve_loopback(listener, view));
    let client = Client::new();

    let response = client
        .get(format!("{origin}/project-view.json"))
        .header(header::HOST, address.to_string())
        .header(header::ORIGIN, &origin)
        .send()
        .await
        .expect("same-origin ProjectView request");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert!(response.headers().contains_key("content-security-policy"));
    assert!(
        !response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
    let body: serde_json::Value = response.json().await.expect("ProjectView JSON");
    assert_eq!(body["schema_version"], 1);

    let rejected_host = client
        .get(format!("{origin}/"))
        .header(header::HOST, "example.com")
        .send()
        .await
        .expect("non-loopback Host response");
    assert_eq!(rejected_host.status(), StatusCode::MISDIRECTED_REQUEST);

    let rejected_origin = client
        .get(format!("{origin}/"))
        .header(header::HOST, address.to_string())
        .header(header::ORIGIN, "https://example.com")
        .send()
        .await
        .expect("cross-origin response");
    assert_eq!(rejected_origin.status(), StatusCode::FORBIDDEN);
    assert!(
        !rejected_origin
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );

    let invalid_origin = client
        .get(format!("{origin}/"))
        .header(header::HOST, address.to_string())
        .header(
            header::ORIGIN,
            header::HeaderValue::from_bytes(b"\xff").expect("opaque invalid Origin value"),
        )
        .send()
        .await
        .expect("invalid Origin response");
    assert_eq!(invalid_origin.status(), StatusCode::FORBIDDEN);

    server.abort();
    let _ = server.await;
}
