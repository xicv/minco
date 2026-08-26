//! The standalone Minco Desk native binary (ADR-0072): one process,
//! one `SQLite` database, zero provider contact.
use anyhow::Result;
use minco_desk_example::{DeskConfig, build_desk};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    let config = DeskConfig::from_env()?;
    let address = (config.host.clone(), config.port);
    let desk = build_desk(&config).await?;
    let listener = TcpListener::bind(address).await?;
    tracing::info!(
        address = %listener.local_addr()?,
        project = %config.project_id,
        "Minco Desk standalone listening"
    );
    axum::serve(listener, desk.router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
