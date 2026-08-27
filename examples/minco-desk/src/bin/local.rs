//! The standalone Minco Desk native binary (ADR-0072): one process,
//! one `SQLite` database, zero provider contact. The explicit jobs
//! worker runs on a bounded interval; nothing is scheduled implicitly
//! inside request handlers.
use anyhow::Result;
use minco_desk_example::{DeskConfig, build_desk};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    let config = DeskConfig::from_env()?;
    let address = (config.host.clone(), config.port);
    let worker_interval = std::time::Duration::from_millis(500);
    let desk = build_desk(&config).await?;
    let listener = TcpListener::bind(address).await?;
    // The loopback service token authorizes the agent/integration
    // surface; announce it once so the operator can drive the desk.
    eprintln!("Minco Desk agent token: {}", config.agent_token);
    tracing::info!(
        address = %listener.local_addr()?,
        project = %config.project_id,
        "Minco Desk standalone listening"
    );
    let desk_worker = desk.worker.clone();
    let worker = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(worker_interval);
        loop {
            ticker.tick().await;
            if let Err(error) = desk_worker.run_once().await {
                tracing::warn!(%error, "desk worker dispatch pass failed");
            }
        }
    });
    let worker_handle = worker;
    axum::serve(listener, desk.router)
        .with_graceful_shutdown(async move {
            worker_handle.abort();
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
