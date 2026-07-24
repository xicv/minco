use anyhow::Result;
use orders_service::{AppConfig, build_application};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env()?;
    let address = (config.host, config.port);
    let application = build_application(&config).await?;
    let listener = TcpListener::bind(address).await?;
    tracing::info!(address = %listener.local_addr()?, "Minco orders API listening");
    axum::serve(listener, application.router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
