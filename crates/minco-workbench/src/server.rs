use crate::{WorkbenchError, render_mermaid};
use axum::http as axum_http;
use axum::{Router, body::Body, extract::State, routing::get};
use axum_http::{
    HeaderMap, Response, StatusCode, Uri,
    header::{
        CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST, ORIGIN, REFERRER_POLICY,
        X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
    },
};
use minco_project_view::ProjectView;
use std::{net::Ipv4Addr, sync::Arc};
use tokio::net::TcpListener;

const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; font-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

struct ServerState {
    authority: String,
    origin: String,
    project_view: Vec<u8>,
    mermaid: Vec<u8>,
}

pub async fn bind_loopback(port: u16) -> Result<TcpListener, WorkbenchError> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|source| WorkbenchError::Io {
            operation: "bind loopback workbench server",
            path: format!("127.0.0.1:{port}").into(),
            source,
        })
}

pub async fn serve_loopback(
    listener: TcpListener,
    view: ProjectView,
) -> Result<(), WorkbenchError> {
    let address = listener.local_addr().map_err(|source| WorkbenchError::Io {
        operation: "read workbench listener address",
        path: "loopback-listener".into(),
        source,
    })?;
    if !address.ip().is_loopback() {
        return Err(WorkbenchError::Io {
            operation: "validate workbench listener address",
            path: address.to_string().into(),
            source: std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "workbench listener is not loopback",
            ),
        });
    }
    let project_view = serde_json::to_vec(&view)?;
    let state = Arc::new(ServerState {
        authority: address.to_string(),
        origin: format!("http://{address}"),
        project_view,
        mermaid: render_mermaid(&view).into_bytes(),
    });
    let router = Router::new()
        .route("/", get(asset))
        .route("/index.html", get(asset))
        .route("/workbench.css", get(asset))
        .route("/workbench.js", get(asset))
        .route("/project-view.json", get(asset))
        .route("/project-view.mmd", get(asset))
        .with_state(state);

    axum::serve(listener, router)
        .await
        .map_err(|source| WorkbenchError::Io {
            operation: "serve loopback workbench",
            path: address.to_string().into(),
            source,
        })
}

async fn asset(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response<Body> {
    if headers.get(HOST).and_then(|value| value.to_str().ok()) != Some(&state.authority) {
        return response(
            StatusCode::MISDIRECTED_REQUEST,
            "text/plain; charset=utf-8",
            b"non-loopback Host rejected".to_vec(),
        );
    }
    if !headers
        .get(ORIGIN)
        .is_none_or(|value| value.to_str().is_ok_and(|origin| origin == state.origin))
    {
        return response(
            StatusCode::FORBIDDEN,
            "text/plain; charset=utf-8",
            b"cross-origin request rejected".to_vec(),
        );
    }

    let (content_type, body) = match uri.path() {
        "/" | "/index.html" => (
            "text/html; charset=utf-8",
            include_bytes!("../assets/index.html").to_vec(),
        ),
        "/workbench.css" => (
            "text/css; charset=utf-8",
            include_bytes!("../assets/workbench.css").to_vec(),
        ),
        "/workbench.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../assets/workbench.js").to_vec(),
        ),
        "/project-view.json" => ("application/json", state.project_view.clone()),
        "/project-view.mmd" => ("text/plain; charset=utf-8", state.mermaid.clone()),
        _ => {
            return response(
                StatusCode::NOT_FOUND,
                "text/plain; charset=utf-8",
                b"not found".to_vec(),
            );
        }
    };
    response(StatusCode::OK, content_type, body)
}

fn response(status: StatusCode, content_type: &'static str, body: Vec<u8>) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, content_type)
        .header(CACHE_CONTROL, "no-store")
        .header(CONTENT_SECURITY_POLICY, CSP)
        .header(X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(X_FRAME_OPTIONS, "DENY")
        .header(REFERRER_POLICY, "no-referrer")
        .body(Body::from(body))
        .expect("static workbench response headers are valid")
}
