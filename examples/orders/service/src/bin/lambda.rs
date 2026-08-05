use anyhow::{Context, Result};
use axum::middleware;
use orders_service::{AppConfig, build_application};

#[tokio::main]
async fn main() -> Result<()> {
    let config = if std::env::var("DATABASE_KIND").as_deref() == Ok("dynamodb") {
        AppConfig::from_env()?
    } else {
        let parameter = std::env::var("DATABASE_URL_PARAMETER")
            .context("DATABASE_URL_PARAMETER is required")?;
        let database_url = minco_aws_lambda::load_secure_parameter(&parameter).await?;
        AppConfig::from_env_with_database_url(Some(database_url))?
    };
    let application = build_application(&config).await?;
    let router = application.router.layer(middleware::from_fn(
        minco_aws_lambda::inject_api_gateway_principal,
    ));
    minco_aws_lambda::run_router(router).await
}
