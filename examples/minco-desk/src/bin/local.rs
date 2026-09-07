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
    // surface. It is announced only in the local profile (exact-head
    // review R10): non-local startup already refused generated
    // credentials, so there is nothing ephemeral to print.
    if config.environment == "local" {
        eprintln!("Minco Desk agent token: {}", config.agent_token);
    }
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
    axum::serve(listener, desk.router)
        .with_graceful_shutdown(async move {
            // Wait for the shutdown signal FIRST; the graceful-shutdown
            // future is polled while the server runs, so cancelling the
            // worker before this await would stop it immediately
            // (exact-head review R1).
            let _ = tokio::signal::ctrl_c().await;
            worker.abort();
            // An aborted task resolves promptly; awaiting it here keeps
            // the exit clean instead of leaking the loop.
            let _ = worker.await;
        })
        .await?;
    Ok(())
}
