use crate::{DatabaseDeployment, DeploymentPlan, FunctionRole, NeonPlan, TriggerPlan};
use serde::{Deserialize, Serialize};

const NEON_PRICING_CAPTURED_AT: &str = "2026-07-31";
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostClass {
    ZeroCompute,
    RequestOnly,
    StorageOnly,
    ScheduledWakeup,
    FixedMonthly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PricingConfidence {
    Priced,
    Unpriced,
    RegionDependent,
    FreeTierDependent,
    EligibilityDependent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostEvidence {
    pub name: String,
    pub cost_class: CostClass,
    pub pricing_confidence: PricingConfidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseCostEstimate {
    pub provider: String,
    pub complete: bool,
    pub monthly_usd: Option<f64>,
    pub components: Vec<CostComponent>,
    pub missing_rates: Vec<String>,
    pub evidence: Vec<CostEvidence>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCostEstimate {
    pub complete: bool,
    pub schedules: Vec<ScheduleCostDimension>,
    pub workers: Vec<WorkerCostDimension>,
    pub queues: Vec<QueueCostDimension>,
    pub fixed_cost_resources: Vec<String>,
    pub request_based_resources: Vec<String>,
    pub missing_rates: Vec<String>,
    pub evidence: Vec<CostEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleCostDimension {
    pub trigger_id: String,
    pub function_id: String,
    pub expression: String,
    pub enabled: bool,
    pub estimated_monthly_invocations: Option<u64>,
    pub can_wake_scale_to_zero_database: bool,
    pub action_after_completion: Option<crate::ScheduleCompletionAction>,
    pub residual_resources: Vec<String>,
    pub manual_fallback: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerCostDimension {
    pub function_id: String,
    pub reserved_concurrency: u32,
    pub database_connections_per_instance: u32,
    pub maximum_database_connections: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueCostDimension {
    pub queue_id: String,
    pub fifo: bool,
    pub mappings: Vec<SqsMappingCostDimension>,
    pub regional_request_rate_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqsMappingCostDimension {
    pub trigger_id: String,
    pub function_id: String,
    pub batch_size: u32,
    pub maximum_concurrency: u32,
}

#[must_use]
pub fn estimate_runtime_cost(plan: &DeploymentPlan) -> RuntimeCostEstimate {
    let mut schedules: Vec<ScheduleCostDimension> = plan
        .triggers
        .iter()
        .filter_map(|trigger| {
            let TriggerPlan::Schedule {
                id,
                function_id,
                expression,
                enabled,
                cleanup,
                ..
            } = trigger
            else {
                return None;
            };
            Some(ScheduleCostDimension {
                trigger_id: id.clone(),
                function_id: function_id.clone(),
                expression: expression.clone(),
                enabled: *enabled,
                estimated_monthly_invocations: enabled
                    .then(|| monthly_schedule_invocations(expression))
                    .flatten(),
                can_wake_scale_to_zero_database: *enabled && plan.database.can_scale_to_zero(),
                action_after_completion: cleanup
                    .as_ref()
                    .map(|cleanup| cleanup.action_after_completion),
                residual_resources: cleanup
                    .as_ref()
                    .map_or_else(Vec::new, |cleanup| cleanup.residual_resources.clone()),
                manual_fallback: cleanup
                    .as_ref()
                    .map(|cleanup| cleanup.manual_fallback.clone()),
            })
        })
        .collect();
    if let Some(cleanup) = plan
        .preview
        .as_ref()
        .and_then(|preview| preview.cleanup_schedule.as_ref())
    {
        schedules.push(ScheduleCostDimension {
            trigger_id: "preview-cleanup".into(),
            function_id: "application-owned-preview-cleanup".into(),
            expression: cleanup.expression.clone(),
            enabled: true,
            estimated_monthly_invocations: monthly_schedule_invocations(&cleanup.expression),
            can_wake_scale_to_zero_database: false,
            action_after_completion: Some(cleanup.action_after_completion),
            residual_resources: cleanup.residual_resources.clone(),
            manual_fallback: Some(cleanup.manual_fallback.clone()),
        });
    }
    let workers = plan
        .functions
        .iter()
        .filter(|function| matches!(function.role, FunctionRole::Worker))
        .map(|function| WorkerCostDimension {
            function_id: function.name.clone(),
            reserved_concurrency: function.reserved_concurrency,
            database_connections_per_instance: function.database_connections_per_instance,
            maximum_database_connections: function
                .reserved_concurrency
                .saturating_mul(function.database_connections_per_instance),
        })
        .collect();
    let queues = plan
        .queues
        .iter()
        .map(|queue| {
            let mappings = plan
                .triggers
                .iter()
                .filter_map(|trigger| {
                    let TriggerPlan::Sqs {
                        id,
                        function_id,
                        queue_id,
                        batch_size,
                        maximum_concurrency,
                        ..
                    } = trigger
                    else {
                        return None;
                    };
                    (queue_id == &queue.id).then(|| SqsMappingCostDimension {
                        trigger_id: id.clone(),
                        function_id: function_id.clone(),
                        batch_size: *batch_size,
                        maximum_concurrency: *maximum_concurrency,
                    })
                })
                .collect();
            QueueCostDimension {
                queue_id: queue.id.clone(),
                fifo: queue.fifo,
                mappings,
                regional_request_rate_required: true,
            }
        })
        .collect::<Vec<_>>();
    let mut fixed_cost_resources = Vec::new();
    if plan.database.has_fixed_compute() {
        fixed_cost_resources.push(format!("database:{}", plan.database.kind_name()));
    }
    fixed_cost_resources.extend(
        plan.functions
            .iter()
            .filter(|function| function.provisioned_concurrency > 0)
            .map(|function| format!("provisioned_concurrency:{}", function.name)),
    );
    let mut request_based_resources = vec!["http_api".into()];
    request_based_resources.extend(
        plan.functions
            .iter()
            .map(|function| format!("lambda:{}", function.name)),
    );
    request_based_resources.extend(plan.queues.iter().map(|queue| format!("sqs:{}", queue.id)));
    request_based_resources.extend(
        schedules
            .iter()
            .map(|schedule| format!("schedule:{}", schedule.trigger_id)),
    );

    let mut missing_rates = vec![
        "regional_api_gateway_request_rate".into(),
        "regional_lambda_request_and_duration_rates".into(),
    ];
    if !queues.is_empty() {
        missing_rates.push("regional_sqs_request_rate".into());
    }
    if !schedules.is_empty() {
        missing_rates.push("regional_scheduler_invocation_rate".into());
    }
    let mut evidence = vec![
        cost_evidence(
            "http_api",
            CostClass::RequestOnly,
            PricingConfidence::RegionDependent,
        ),
        cost_evidence(
            "lambda_compute",
            CostClass::ZeroCompute,
            PricingConfidence::RegionDependent,
        ),
    ];
    evidence.extend(plan.queues.iter().map(|queue| {
        cost_evidence(
            &format!("sqs:{}", queue.id),
            CostClass::RequestOnly,
            PricingConfidence::RegionDependent,
        )
    }));
    evidence.extend(schedules.iter().map(|schedule| {
        cost_evidence(
            &format!("schedule:{}", schedule.trigger_id),
            CostClass::ScheduledWakeup,
            PricingConfidence::RegionDependent,
        )
    }));
    evidence.extend(
        plan.functions
            .iter()
            .filter(|function| function.provisioned_concurrency > 0)
            .map(|function| {
                cost_evidence(
                    &format!("provisioned_concurrency:{}", function.name),
                    CostClass::FixedMonthly,
                    PricingConfidence::RegionDependent,
                )
            }),
    );

    RuntimeCostEstimate {
        complete: false,
        schedules,
        workers,
        queues,
        fixed_cost_resources,
        request_based_resources,
        missing_rates,
        evidence,
    }
}

fn monthly_schedule_invocations(expression: &str) -> Option<u64> {
    if expression.starts_with("at(") && expression.ends_with(')') {
        return Some(1);
    }
    let body = expression.strip_prefix("rate(")?.strip_suffix(')')?.trim();
    let mut parts = body.split_whitespace();
    let value = parts.next()?.parse::<u64>().ok()?;
    let unit = parts.next()?;
    if value == 0 || parts.next().is_some() {
        return None;
    }
    let minutes = match unit {
        "minute" | "minutes" => value,
        "hour" | "hours" => value.checked_mul(60)?,
        "day" | "days" => value.checked_mul(24 * 60)?,
        _ => return None,
    };
    Some(43_830_u64.div_ceil(minutes))
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
            evidence: vec![cost_evidence(
                database.kind_name(),
                if database.has_fixed_compute() {
                    CostClass::FixedMonthly
                } else {
                    CostClass::ZeroCompute
                },
                PricingConfidence::Unpriced,
            )],
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
                cost_evidence("host", CostClass::FixedMonthly, PricingConfidence::Priced),
                cost_evidence("storage", CostClass::StorageOnly, PricingConfidence::Priced),
                cost_evidence("backup", CostClass::StorageOnly, PricingConfidence::Priced),
                cost_evidence(
                    "operations_allowance",
                    CostClass::FixedMonthly,
                    PricingConfidence::Priced,
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
                    CostClass::FixedMonthly,
                ),
                (
                    "storage",
                    *storage_rate_usd,
                    *storage_gb_month,
                    format!("{storage_gb_month} GB-month"),
                    CostClass::StorageOnly,
                ),
                (
                    "backup",
                    *backup_rate_usd,
                    *backup_gb_month,
                    format!("{backup_gb_month} GB-month"),
                    CostClass::StorageOnly,
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
                    if database.can_scale_to_zero() {
                        CostClass::ZeroCompute
                    } else {
                        CostClass::FixedMonthly
                    },
                ),
                (
                    "storage",
                    *storage_rate_usd,
                    *storage_gb_month,
                    format!("{storage_gb_month} GB-month"),
                    CostClass::StorageOnly,
                ),
                (
                    "io",
                    *io_million_rate_usd,
                    *io_million,
                    format!("{io_million} million I/O operations"),
                    CostClass::RequestOnly,
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
                    CostClass::RequestOnly,
                ),
                (
                    "writes",
                    *write_million_rate_usd,
                    *write_request_units_million,
                    format!("{write_request_units_million} million write request units"),
                    CostClass::RequestOnly,
                ),
                (
                    "storage",
                    *storage_rate_usd,
                    *storage_gb_month,
                    format!("{storage_gb_month} GB-month"),
                    CostClass::StorageOnly,
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
                cost_evidence("host", CostClass::FixedMonthly, PricingConfidence::Priced),
                cost_evidence("backup", CostClass::StorageOnly, PricingConfidence::Priced),
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
            evidence: vec![cost_evidence(
                "ephemeral_storage",
                CostClass::StorageOnly,
                PricingConfidence::Unpriced,
            )],
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
        NeonPlan::Free if compute <= 100.0 && storage <= 0.5 => DatabaseCostEstimate {
            provider: "neon_free".into(),
            complete: false,
            monthly_usd: None,
            components: Vec::new(),
            missing_rates: vec![
                "current Neon Free plan eligibility and per-project allowance".into(),
            ],
            evidence: vec![
                cost_evidence(
                    "compute_allowance",
                    CostClass::ZeroCompute,
                    PricingConfidence::FreeTierDependent,
                ),
                cost_evidence(
                    "storage_allowance",
                    CostClass::StorageOnly,
                    PricingConfidence::FreeTierDependent,
                ),
            ],
            notes: vec![
                "Usage fits the dated published allowance, but Minco does not treat provider \
                 eligibility or a future allowance as a complete zero-cost estimate."
                    .into(),
                neon_snapshot_note(),
            ],
        },
        NeonPlan::Free => DatabaseCostEstimate {
            provider: "neon_free".into(),
            complete: false,
            monthly_usd: None,
            components: Vec::new(),
            missing_rates: vec![
                "select a paid Neon plan or reduce usage to the free allowance".into(),
            ],
            evidence: vec![
                cost_evidence(
                    "compute_allowance",
                    CostClass::ZeroCompute,
                    PricingConfidence::FreeTierDependent,
                ),
                cost_evidence(
                    "storage_allowance",
                    CostClass::StorageOnly,
                    PricingConfidence::FreeTierDependent,
                ),
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
            vec![
                cost_evidence("compute", CostClass::ZeroCompute, PricingConfidence::Priced),
                cost_evidence("storage", CostClass::StorageOnly, PricingConfidence::Priced),
                cost_evidence(
                    "history_storage",
                    CostClass::StorageOnly,
                    PricingConfidence::Priced,
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
            vec![
                cost_evidence("compute", CostClass::ZeroCompute, PricingConfidence::Priced),
                cost_evidence("storage", CostClass::StorageOnly, PricingConfidence::Priced),
                cost_evidence(
                    "history_storage",
                    CostClass::StorageOnly,
                    PricingConfidence::Priced,
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
    inputs: Vec<(&str, Option<f64>, f64, String, CostClass)>,
    notes: Vec<String>,
) -> DatabaseCostEstimate {
    let mut components = Vec::new();
    let mut missing_rates = Vec::new();
    let mut evidence = Vec::new();
    for (name, rate, units, formula, cost_class) in inputs {
        evidence.push(cost_evidence(
            name,
            cost_class,
            if rate.is_some() {
                PricingConfidence::Priced
            } else {
                PricingConfidence::RegionDependent
            },
        ));
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
        evidence,
        notes,
    }
}

fn complete(
    provider: &str,
    components: Vec<CostComponent>,
    evidence: Vec<CostEvidence>,
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
        evidence,
        notes,
    }
}

fn cost_evidence(
    name: &str,
    cost_class: CostClass,
    pricing_confidence: PricingConfidence,
) -> CostEvidence {
    CostEvidence {
        name: name.into(),
        cost_class,
        pricing_confidence,
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
    fn neon_free_allowance_is_not_treated_as_a_complete_zero_cost() {
        let estimate = estimate_database_cost(&DatabaseDeployment::NeonPostgres {
            plan: NeonPlan::Free,
            compute_unit_hours: 20.0,
            storage_gb_month: 0.25,
            history_storage_gb_month: 0.0,
        });

        assert!(!estimate.complete);
        assert_eq!(estimate.monthly_usd, None);
        assert!(
            estimate
                .evidence
                .iter()
                .all(|item| item.pricing_confidence == PricingConfidence::FreeTierDependent)
        );
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
        assert_eq!(
            estimate.evidence,
            [
                CostEvidence {
                    name: "reads".into(),
                    cost_class: CostClass::RequestOnly,
                    pricing_confidence: PricingConfidence::RegionDependent,
                },
                CostEvidence {
                    name: "writes".into(),
                    cost_class: CostClass::RequestOnly,
                    pricing_confidence: PricingConfidence::RegionDependent,
                },
                CostEvidence {
                    name: "storage".into(),
                    cost_class: CostClass::StorageOnly,
                    pricing_confidence: PricingConfidence::RegionDependent,
                },
            ]
        );
    }

    #[test]
    fn aurora_zero_acu_and_fixed_rds_have_materially_different_cost_classes() {
        let aurora = estimate_database_cost(&DatabaseDeployment::AuroraServerlessV2 {
            minimum_acu: 0.0,
            auto_pause_seconds: Some(300),
            acu_hours: 1.0,
            acu_hour_rate_usd: None,
            storage_gb_month: 1.0,
            storage_rate_usd: None,
            io_million: 1.0,
            io_million_rate_usd: None,
        });
        let rds = estimate_database_cost(&DatabaseDeployment::RdsPostgres {
            instance_hours: 730.0,
            instance_hour_rate_usd: None,
            storage_gb_month: 20.0,
            storage_rate_usd: None,
            backup_gb_month: 20.0,
            backup_rate_usd: None,
            multi_az_multiplier: 1.0,
        });

        assert_eq!(aurora.evidence[0].cost_class, CostClass::ZeroCompute);
        assert_eq!(rds.evidence[0].cost_class, CostClass::FixedMonthly);
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
