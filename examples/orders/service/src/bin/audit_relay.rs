use anyhow::{Context as _, Result};
use orders_service::{AppConfig, dispatch_audit_once};

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env()?;
    let worker_id = std::env::var("AUDIT_RELAY_WORKER_ID")
        .unwrap_or_else(|_| format!("orders-audit-relay-{}", std::process::id()));
    let limit = std::env::var("AUDIT_RELAY_BATCH_SIZE")
        .unwrap_or_else(|_| "100".into())
        .parse::<usize>()
        .context("AUDIT_RELAY_BATCH_SIZE must be an integer")?;
    let report = dispatch_audit_once(&config, &worker_id, limit).await?;
    println!(
        "{{\"claimed\":{},\"inserted\":{},\"duplicates\":{},\"retried\":{},\"quarantined\":{}}}",
        report.claimed, report.inserted, report.duplicates, report.retried, report.quarantined
    );
    Ok(())
}
