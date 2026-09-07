//! Clean install and migration in one command (ADR-0072): a fresh
//! database file receives every table; an existing one advances.
use anyhow::Result;
use minco_desk_example::DeskConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let config = DeskConfig::from_env()?;
    let pool = minco_desk_example::migrate(&config).await?;
    let ticketing = sqlx::query("SELECT COUNT(*) FROM ticketing_tickets")
        .execute(&pool)
        .await?
        .rows_affected();
    let jobs = sqlx::query("SELECT COUNT(*) FROM minco_jobs")
        .execute(&pool)
        .await?
        .rows_affected();
    println!(
        "{{\"migrated\":true,\"ticketing_table_ready\":{ticketing},\"jobs_table_ready\":{jobs}}}"
    );
    Ok(())
}
