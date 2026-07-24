use crate::{DatabaseDeployment, NeonPlan};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostComponent {
    pub name: String,
    pub monthly_usd: f64,
    pub formula: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseCostEstimate {
    pub provider: String,
    pub complete: bool,
    pub monthly_usd: Option<f64>,
    pub components: Vec<CostComponent>,
    pub missing_rates: Vec<String>,
    pub notes: Vec<String>,
}

pub fn estimate_database_cost(database: &DatabaseDeployment) -> DatabaseCostEstimate {
    match database {
        DatabaseDeployment::NeonPostgres {
            plan,
            compute_unit_hours,
            storage_gb_month,
            history_storage_gb_month,
        } => estimate_neon(
            *plan,
            *compute_unit_hours,
            *storage_gb_month,
            *history_storage_gb_month,
        ),
        DatabaseDeployment::SelfHostedPostgres {
            host_monthly_usd,
            storage_gb_month,
            storage_rate_usd,
            backup_gb_month,
            backup_rate_usd,
            operations_monthly_usd,
        } => complete(
            "self_hosted_postgres",
            vec![
                component("host", *host_monthly_usd, "fixed host monthly price"),
                component(
                    "storage",
                    storage_gb_month * storage_rate_usd,
                    &format!("{storage_gb_month} GB-month × ${storage_rate_usd}"),
                ),
                component(
                    "backup",
                    backup_gb_month * backup_rate_usd,
                    &format!("{backup_gb_month} GB-month × ${backup_rate_usd}"),
                ),
                component(
                    "operations_allowance",
                    *operations_monthly_usd,
                    "explicit operational ownership allowance",
                ),
            ],
            vec![
                "Includes an explicit operations allowance because self-hosting transfers \
                 patching, backup, restore and incident responsibility to the owner."
                    .into(),
            ],
        ),
        DatabaseDeployment::RdsPostgres {
            instance_hours,
            instance_hour_rate_usd,
            storage_gb_month,
            storage_rate_usd,
            backup_gb_month,
            backup_rate_usd,
            multi_az_multiplier,
        } => with_optional_rates(
            "aws_rds_postgres",
            vec![
                (
                    "instance",
                    *instance_hour_rate_usd,
                    instance_hours * multi_az_multiplier,
                    format!("{instance_hours} hours × multi-AZ multiplier {multi_az_multiplier}"),
                ),
                (
                    "storage",
                    *storage_rate_usd,
                    *storage_gb_month,
                    format!("{storage_gb_month} GB-month"),
                ),
                (
                    "backup",
                    *backup_rate_usd,
                    *backup_gb_month,
                    format!("{backup_gb_month} GB-month"),
                ),
            ],
            vec![
                "AWS rates are intentionally supplied per region and instance class rather \
                 than embedded as globally stable constants."
                    .into(),
            ],
        ),
        DatabaseDeployment::AuroraServerlessV2 {
            acu_hours,
            acu_hour_rate_usd,
            storage_gb_month,
            storage_rate_usd,
            io_million,
            io_million_rate_usd,
            ..
        } => with_optional_rates(
            "aws_aurora_serverless_v2",
            vec![
                (
                    "compute",
                    *acu_hour_rate_usd,
                    *acu_hours,
                    format!("{acu_hours} ACU-hours"),
                ),
                (
                    "storage",
                    *storage_rate_usd,
                    *storage_gb_month,
                    format!("{storage_gb_month} GB-month"),
                ),
                (
                    "io",
                    *io_million_rate_usd,
                    *io_million,
                    format!("{io_million} million I/O operations"),
                ),
            ],
            vec![
                "Auto-pause eligibility and minimum ACU settings must be verified for the \
                 selected Aurora engine version and region."
                    .into(),
            ],
        ),
        DatabaseDeployment::DynamoDbOnDemand {
            read_request_units_million,
            read_million_rate_usd,
            write_request_units_million,
            write_million_rate_usd,
            storage_gb_month,
            storage_rate_usd,
        } => with_optional_rates(
            "aws_dynamodb_on_demand",
            vec![
                (
                    "reads",
                    *read_million_rate_usd,
                    *read_request_units_million,
                    format!("{read_request_units_million} million read request units"),
                ),
                (
                    "writes",
                    *write_million_rate_usd,
                    *write_request_units_million,
                    format!("{write_request_units_million} million write request units"),
                ),
                (
                    "storage",
                    *storage_rate_usd,
                    *storage_gb_month,
                    format!("{storage_gb_month} GB-month"),
                ),
            ],
            vec![
                "Model access patterns, item sizes, indexes, streams, backups and \
                 transactional multipliers before selecting DynamoDB."
                    .into(),
            ],
        ),
        DatabaseDeployment::SqlitePersistentHost {
            host_monthly_usd,
            backup_monthly_usd,
        } => complete(
            "sqlite_persistent_host",
            vec![
                component("host", *host_monthly_usd, "persistent single-process host"),
                component("backup", *backup_monthly_usd, "backup storage and transfer"),
            ],
            vec![
                "Suitable only where a single-writer deployment and persistent filesystem \
                 meet the application's concurrency and availability needs."
                    .into(),
            ],
        ),
        DatabaseDeployment::SqliteLambdaMutable { .. } => DatabaseCostEstimate {
            provider: "sqlite_lambda_mutable".into(),
            complete: false,
            monthly_usd: None,
            components: Vec::new(),
            missing_rates: Vec::new(),
            notes: vec![
                "Rejected: Lambda local storage is ephemeral and scoped to an execution \
                 environment; mutable SQLite cannot be the durable system of record."
                    .into(),
            ],
        },
    }
}

fn estimate_neon(plan: NeonPlan, compute: f64, storage: f64, history: f64) -> DatabaseCostEstimate {
    match plan {
        NeonPlan::Free if compute <= 100.0 && storage <= 0.5 => complete(
            "neon_free",
            Vec::new(),
            vec![
                "Within the published per-project Free plan allowance supplied by the \
                 configuration date."
                    .into(),
            ],
        ),
        NeonPlan::Free => DatabaseCostEstimate {
            provider: "neon_free".into(),
            complete: false,
            monthly_usd: None,
            components: Vec::new(),
            missing_rates: vec![
                "select a paid Neon plan or reduce usage to the free allowance".into(),
            ],
            notes: vec![
                "Free-plan overage is not extrapolated because plan transitions and \
                 allowances are product policy, not a linear usage rate."
                    .into(),
            ],
        },
        NeonPlan::Launch => complete(
            "neon_launch",
            vec![
                component(
                    "compute",
                    compute * 0.106,
                    &format!("{compute} CU-hours × $0.106"),
                ),
                component(
                    "storage",
                    storage * 0.35,
                    &format!("{storage} GB-month × $0.35"),
                ),
                component(
                    "history_storage",
                    history * 0.20,
                    &format!("{history} GB-month × $0.20"),
                ),
            ],
            vec![
                "Rates are the published Neon Launch rates captured in \
                 docs/research/sources.md; review and refresh the dated rate catalog before financial approval."
                    .into(),
            ],
        ),
        NeonPlan::Scale => complete(
            "neon_scale",
            vec![
                component(
                    "compute",
                    compute * 0.222,
                    &format!("{compute} CU-hours × $0.222"),
                ),
                component(
                    "storage",
                    storage * 0.35,
                    &format!("{storage} GB-month × $0.35"),
                ),
                component(
                    "history_storage",
                    history * 0.20,
                    &format!("{history} GB-month × $0.20"),
                ),
            ],
            vec![
                "Rates are the published Neon Scale rates captured in \
                 docs/research/sources.md; review and refresh the dated rate catalog before financial approval."
                    .into(),
            ],
        ),
    }
}

fn with_optional_rates(
    provider: &str,
    inputs: Vec<(&str, Option<f64>, f64, String)>,
    notes: Vec<String>,
) -> DatabaseCostEstimate {
    let mut components = Vec::new();
    let mut missing_rates = Vec::new();
    for (name, rate, units, formula) in inputs {
        if let Some(rate) = rate {
            components.push(component(
                name,
                rate * units,
                &format!("{formula} × ${rate}"),
            ));
        } else {
            missing_rates.push(format!("{name}_rate_usd"));
        }
    }
    let complete = missing_rates.is_empty();
    let monthly_usd = complete.then(|| {
        components
            .iter()
            .map(|component| component.monthly_usd)
            .sum()
    });
    DatabaseCostEstimate {
        provider: provider.into(),
        complete,
        monthly_usd,
        components,
        missing_rates,
        notes,
    }
}

fn complete(
    provider: &str,
    components: Vec<CostComponent>,
    notes: Vec<String>,
) -> DatabaseCostEstimate {
    let monthly_usd = Some(
        components
            .iter()
            .map(|component| component.monthly_usd)
            .sum(),
    );
    DatabaseCostEstimate {
        provider: provider.into(),
        complete: true,
        monthly_usd,
        components,
        missing_rates: Vec::new(),
        notes,
    }
}

fn component(name: &str, monthly_usd: f64, formula: &str) -> CostComponent {
    CostComponent {
        name: name.into(),
        monthly_usd,
        formula: formula.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neon_launch_is_usage_based() {
        let estimate = estimate_database_cost(&DatabaseDeployment::NeonPostgres {
            plan: NeonPlan::Launch,
            compute_unit_hours: 10.0,
            storage_gb_month: 1.0,
            history_storage_gb_month: 0.0,
        });
        assert!(estimate.complete);
        assert!((estimate.monthly_usd.unwrap() - 1.41).abs() < 0.001);
    }

    #[test]
    fn regional_aws_rate_omissions_are_visible() {
        let estimate = estimate_database_cost(&DatabaseDeployment::DynamoDbOnDemand {
            read_request_units_million: 1.0,
            read_million_rate_usd: None,
            write_request_units_million: 1.0,
            write_million_rate_usd: None,
            storage_gb_month: 1.0,
            storage_rate_usd: None,
        });
        assert!(!estimate.complete);
        assert_eq!(estimate.missing_rates.len(), 3);
    }
}
