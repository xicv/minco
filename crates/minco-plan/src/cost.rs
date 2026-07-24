use crate::{DatabaseDeployment, NeonPlan};
use serde::{Deserialize, Serialize};

const NEON_PRICING_CAPTURED_AT: &str = "2026-07-24";
const NEON_PRICING_SOURCE: &str = "https://neon.com/pricing";
const NEON_LAUNCH_COMPUTE_UNIT_HOUR_USD: f64 = 0.106;
const NEON_SCALE_COMPUTE_UNIT_HOUR_USD: f64 = 0.222;
const NEON_STORAGE_GB_MONTH_USD: f64 = 0.35;
const NEON_HISTORY_STORAGE_GB_MONTH_USD: f64 = 0.20;

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
    let invalid_inputs = database.invalid_numeric_inputs();
    if !invalid_inputs.is_empty() {
        return DatabaseCostEstimate {
            provider: database.kind_name().into(),
            complete: false,
            monthly_usd: None,
            components: Vec::new(),
            missing_rates: Vec::new(),
            notes: vec![format!(
                "Invalid numeric inputs must be corrected before estimation: {}.",
                invalid_inputs.join(", ")
            )],
        };
    }
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
                 pricing snapshot."
                    .into(),
                neon_snapshot_note(),
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
                neon_snapshot_note(),
            ],
        },
        NeonPlan::Launch => complete(
            "neon_launch",
            vec![
                component(
                    "compute",
                    compute * NEON_LAUNCH_COMPUTE_UNIT_HOUR_USD,
                    &format!("{compute} CU-hours × ${NEON_LAUNCH_COMPUTE_UNIT_HOUR_USD}"),
                ),
                component(
                    "storage",
                    storage * NEON_STORAGE_GB_MONTH_USD,
                    &format!("{storage} GB-month × ${NEON_STORAGE_GB_MONTH_USD}"),
                ),
                component(
                    "history_storage",
                    history * NEON_HISTORY_STORAGE_GB_MONTH_USD,
                    &format!("{history} GB-month × ${NEON_HISTORY_STORAGE_GB_MONTH_USD}"),
                ),
            ],
            vec![neon_snapshot_note()],
        ),
        NeonPlan::Scale => complete(
            "neon_scale",
            vec![
                component(
                    "compute",
                    compute * NEON_SCALE_COMPUTE_UNIT_HOUR_USD,
                    &format!("{compute} CU-hours × ${NEON_SCALE_COMPUTE_UNIT_HOUR_USD}"),
                ),
                component(
                    "storage",
                    storage * NEON_STORAGE_GB_MONTH_USD,
                    &format!("{storage} GB-month × ${NEON_STORAGE_GB_MONTH_USD}"),
                ),
                component(
                    "history_storage",
                    history * NEON_HISTORY_STORAGE_GB_MONTH_USD,
                    &format!("{history} GB-month × ${NEON_HISTORY_STORAGE_GB_MONTH_USD}"),
                ),
            ],
            vec![neon_snapshot_note()],
        ),
    }
}

fn neon_snapshot_note() -> String {
    format!(
        "Neon rates captured on {NEON_PRICING_CAPTURED_AT} from {NEON_PRICING_SOURCE}; refresh the dated catalog before financial approval."
    )
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
            .fold(0.0, |total, component| total + component.monthly_usd),
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
    fn neon_free_zero_cost_is_canonical_and_dated() {
        let estimate = estimate_database_cost(&DatabaseDeployment::NeonPostgres {
            plan: NeonPlan::Free,
            compute_unit_hours: 20.0,
            storage_gb_month: 0.25,
            history_storage_gb_month: 0.0,
        });

        let monthly_usd = estimate.monthly_usd.expect("complete estimate");
        assert!(monthly_usd.abs() < f64::EPSILON);
        assert!(monthly_usd.is_sign_positive());
        assert!(
            estimate
                .notes
                .iter()
                .any(|note| note.contains(NEON_PRICING_CAPTURED_AT))
        );
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

    #[test]
    fn invalid_numeric_inputs_never_produce_a_complete_estimate() {
        let estimate = estimate_database_cost(&DatabaseDeployment::SelfHostedPostgres {
            host_monthly_usd: -1.0,
            storage_gb_month: 20.0,
            storage_rate_usd: 0.08,
            backup_gb_month: 20.0,
            backup_rate_usd: 0.05,
            operations_monthly_usd: 50.0,
        });

        assert!(!estimate.complete);
        assert_eq!(estimate.monthly_usd, None);
    }
}
