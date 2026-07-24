use minco_contract::{ContractDocument, HttpMethod};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Information,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanDiagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
}

/// Environment-owned deployment inputs. HTTP routes are deliberately absent: they are
/// derived from the canonical `OpenAPI` contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentConfig {
    pub schema_version: u32,
    pub application: String,
    pub environment: String,
    pub region: String,
    pub runtime: RuntimePlan,
    pub ingress: IngressPlan,
    pub auth: AuthPlan,
    pub database: DatabaseDeployment,
    pub functions: Vec<FunctionPlan>,
    #[serde(default)]
    pub scheduled_wakeups: Vec<String>,
    #[serde(default)]
    pub uses_nat_gateway: bool,
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_log_retention_days")]
    pub log_retention_days: u32,
    #[serde(default)]
    pub cost_policy: CostPolicy,
    #[serde(default)]
    pub performance_policy: PerformancePolicy,
}

impl DeploymentConfig {
    #[must_use]
    pub fn into_plan(self, contract: &ContractDocument) -> DeploymentPlan {
        let routes = contract
            .operations
            .iter()
            .map(|operation| RoutePlan {
                operation_id: operation.operation_id.clone(),
                method: operation.method,
                path: operation.path.clone(),
                authenticated: operation.authenticated,
            })
            .collect();
        DeploymentPlan {
            schema_version: self.schema_version,
            application: self.application,
            environment: self.environment,
            region: self.region,
            runtime: self.runtime,
            ingress: self.ingress,
            auth: self.auth,
            database: self.database,
            functions: self.functions,
            routes,
            scheduled_wakeups: self.scheduled_wakeups,
            uses_nat_gateway: self.uses_nat_gateway,
            allowed_origins: self.allowed_origins,
            log_retention_days: self.log_retention_days,
            cost_policy: self.cost_policy,
            performance_policy: self.performance_policy,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentPlan {
    pub schema_version: u32,
    pub application: String,
    pub environment: String,
    pub region: String,
    pub runtime: RuntimePlan,
    pub ingress: IngressPlan,
    pub auth: AuthPlan,
    pub database: DatabaseDeployment,
    pub functions: Vec<FunctionPlan>,
    pub routes: Vec<RoutePlan>,
    pub scheduled_wakeups: Vec<String>,
    pub uses_nat_gateway: bool,
    pub allowed_origins: Vec<String>,
    pub log_retention_days: u32,
    pub cost_policy: CostPolicy,
    pub performance_policy: PerformancePolicy,
}

impl DeploymentPlan {
    #[must_use]
    pub fn validate(&self) -> Vec<PlanDiagnostic> {
        let mut diagnostics = Vec::new();
        if self.schema_version != 1 {
            diagnostics.push(error(
                "MINCO-PLAN-001",
                "unsupported deployment plan schema version",
            ));
        }
        if self.routes.iter().any(|route| route.authenticated)
            && matches!(&self.auth, AuthPlan::None)
        {
            diagnostics.push(error("MINCO-AUTH-001", "the contract contains authenticated operations but no deployment authorizer is configured"));
        }
        if self.functions.len() != 1 {
            diagnostics.push(error(
                "MINCO-PLAN-002",
                "the initial Minco AWS profile requires exactly one API function",
            ));
        }
        if self.allowed_origins.is_empty() {
            diagnostics.push(error(
                "MINCO-HTTP-001",
                "at least one exact CORS origin is required",
            ));
        }
        if self.allowed_origins.iter().any(|origin| origin == "*") {
            diagnostics.push(error("MINCO-HTTP-002", "wildcard CORS is forbidden"));
        }
        if self.log_retention_days == 0 {
            diagnostics.push(error(
                "MINCO-COST-006",
                "log retention must be explicit and greater than zero",
            ));
        }
        if self.cost_policy.deny_nat_gateway && self.uses_nat_gateway {
            diagnostics.push(error(
                "MINCO-COST-001",
                "minimal-idle profile forbids a NAT Gateway",
            ));
        }
        if self.cost_policy.deny_scheduled_wakeups && !self.scheduled_wakeups.is_empty() {
            diagnostics.push(error(
                "MINCO-COST-002",
                "minimal-idle profile forbids scheduled wakeups",
            ));
        }
        if self.cost_policy.deny_fixed_compute && self.database.has_fixed_compute() {
            diagnostics.push(error(
                "MINCO-COST-007",
                &format!(
                    "database profile {} has fixed provisioned compute",
                    self.database.kind_name()
                ),
            ));
        }
        for function in &self.functions {
            if self.cost_policy.deny_provisioned_concurrency && function.provisioned_concurrency > 0
            {
                diagnostics.push(error(
                    "MINCO-COST-003",
                    &format!("function {} enables provisioned concurrency", function.name),
                ));
            }
            if function.reserved_concurrency > self.cost_policy.max_reserved_concurrency {
                diagnostics.push(error(
                    "MINCO-COST-004",
                    &format!(
                        "function {} reserved concurrency {} exceeds {}",
                        function.name,
                        function.reserved_concurrency,
                        self.cost_policy.max_reserved_concurrency
                    ),
                ));
            }
            if function.timeout_seconds > self.performance_policy.max_lambda_timeout_seconds {
                diagnostics.push(error(
                    "MINCO-PERF-001",
                    &format!(
                        "function {} timeout {}s exceeds {}s",
                        function.name,
                        function.timeout_seconds,
                        self.performance_policy.max_lambda_timeout_seconds
                    ),
                ));
            }
            if function.memory_mb > self.performance_policy.max_lambda_memory_mb {
                diagnostics.push(error(
                    "MINCO-PERF-002",
                    &format!(
                        "function {} memory {}MB exceeds {}MB",
                        function.name,
                        function.memory_mb,
                        self.performance_policy.max_lambda_memory_mb
                    ),
                ));
            }
        }
        let possible_connections: u32 = self
            .functions
            .iter()
            .map(|function| {
                function
                    .reserved_concurrency
                    .saturating_mul(function.database_connections_per_instance)
            })
            .sum();
        if self.database.is_relational()
            && possible_connections > self.cost_policy.max_database_connections
        {
            diagnostics.push(error(
                "MINCO-COST-005",
                &format!(
                    "Lambda concurrency can create {possible_connections} database connections, exceeding {}",
                    self.cost_policy.max_database_connections
                ),
            ));
        }
        if matches!(
            &self.database,
            DatabaseDeployment::SqliteLambdaMutable { .. }
        ) {
            diagnostics.push(error(
                "MINCO-DB-001",
                "mutable SQLite is not supported on Lambda ephemeral storage",
            ));
        }
        if matches!(&self.database, DatabaseDeployment::DynamoDbOnDemand { .. }) {
            diagnostics.push(information(
                "MINCO-DB-002",
                "DynamoDB is an alternate persistence model, not a transparent replacement for relational PostgreSQL adapters",
            ));
        }
        for field in self.database.invalid_numeric_inputs() {
            diagnostics.push(error(
                "MINCO-COST-008",
                &format!("{field} has an invalid numeric value for its cost profile"),
            ));
        }
        if let DatabaseDeployment::AuroraServerlessV2 {
            minimum_acu,
            auto_pause_seconds,
            ..
        } = &self.database
        {
            if *minimum_acu == 0.0 && auto_pause_seconds.is_none() {
                diagnostics.push(warning(
                    "MINCO-DB-003",
                    "Aurora minimum ACU is zero but no auto-pause interval is recorded",
                ));
            }
            if auto_pause_seconds
                .is_some_and(|seconds| !(300..=86_400).contains(&seconds) || *minimum_acu != 0.0)
            {
                diagnostics.push(error(
                    "MINCO-DB-004",
                    "Aurora auto-pause must be 300 to 86400 seconds and requires minimum_acu = 0",
                ));
            }
            if minimum_acu.is_finite()
                && (*minimum_acu > 256.0 || (*minimum_acu * 2.0).fract() != 0.0)
            {
                diagnostics.push(error(
                    "MINCO-DB-005",
                    "Aurora minimum_acu must use 0.5 ACU increments and not exceed 256",
                ));
            }
        }
        diagnostics
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthPlan {
    None,
    DevelopmentHeaders,
    Jwt {
        issuer: String,
        audiences: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePlan {
    LambdaZipArm64,
    LocalNative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngressPlan {
    ApiGatewayHttpApi,
    LambdaFunctionUrl,
    LocalTcp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DatabaseDeployment {
    NeonPostgres {
        plan: NeonPlan,
        compute_unit_hours: f64,
        storage_gb_month: f64,
        history_storage_gb_month: f64,
    },
    SelfHostedPostgres {
        host_monthly_usd: f64,
        storage_gb_month: f64,
        storage_rate_usd: f64,
        backup_gb_month: f64,
        backup_rate_usd: f64,
        operations_monthly_usd: f64,
    },
    RdsPostgres {
        instance_hours: f64,
        instance_hour_rate_usd: Option<f64>,
        storage_gb_month: f64,
        storage_rate_usd: Option<f64>,
        backup_gb_month: f64,
        backup_rate_usd: Option<f64>,
        multi_az_multiplier: f64,
    },
    AuroraServerlessV2 {
        minimum_acu: f64,
        auto_pause_seconds: Option<u32>,
        acu_hours: f64,
        acu_hour_rate_usd: Option<f64>,
        storage_gb_month: f64,
        storage_rate_usd: Option<f64>,
        io_million: f64,
        io_million_rate_usd: Option<f64>,
    },
    DynamoDbOnDemand {
        read_request_units_million: f64,
        read_million_rate_usd: Option<f64>,
        write_request_units_million: f64,
        write_million_rate_usd: Option<f64>,
        storage_gb_month: f64,
        storage_rate_usd: Option<f64>,
    },
    SqlitePersistentHost {
        host_monthly_usd: f64,
        backup_monthly_usd: f64,
    },
    SqliteLambdaMutable {
        expected_storage_gb: f64,
    },
}

impl DatabaseDeployment {
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::NeonPostgres { .. } => "neon_postgres",
            Self::SelfHostedPostgres { .. } => "self_hosted_postgres",
            Self::RdsPostgres { .. } => "rds_postgres",
            Self::AuroraServerlessV2 { .. } => "aurora_serverless_v2",
            Self::DynamoDbOnDemand { .. } => "dynamodb_on_demand",
            Self::SqlitePersistentHost { .. } => "sqlite_persistent_host",
            Self::SqliteLambdaMutable { .. } => "sqlite_lambda_mutable",
        }
    }

    #[must_use]
    pub fn has_fixed_compute(&self) -> bool {
        match self {
            Self::SelfHostedPostgres { .. }
            | Self::RdsPostgres { .. }
            | Self::SqlitePersistentHost { .. } => true,
            Self::AuroraServerlessV2 { minimum_acu, .. } => *minimum_acu > 0.0,
            Self::NeonPostgres { .. }
            | Self::DynamoDbOnDemand { .. }
            | Self::SqliteLambdaMutable { .. } => false,
        }
    }

    #[must_use]
    pub const fn is_relational(&self) -> bool {
        !matches!(self, Self::DynamoDbOnDemand { .. })
    }

    pub(crate) fn invalid_numeric_inputs(&self) -> Vec<&'static str> {
        let mut invalid = Vec::new();
        match self {
            Self::NeonPostgres {
                compute_unit_hours,
                storage_gb_month,
                history_storage_gb_month,
                ..
            } => {
                check_non_negative(&mut invalid, "compute_unit_hours", *compute_unit_hours);
                check_non_negative(&mut invalid, "storage_gb_month", *storage_gb_month);
                check_non_negative(
                    &mut invalid,
                    "history_storage_gb_month",
                    *history_storage_gb_month,
                );
            }
            Self::SelfHostedPostgres {
                host_monthly_usd,
                storage_gb_month,
                storage_rate_usd,
                backup_gb_month,
                backup_rate_usd,
                operations_monthly_usd,
            } => {
                check_non_negative(&mut invalid, "host_monthly_usd", *host_monthly_usd);
                check_non_negative(&mut invalid, "storage_gb_month", *storage_gb_month);
                check_non_negative(&mut invalid, "storage_rate_usd", *storage_rate_usd);
                check_non_negative(&mut invalid, "backup_gb_month", *backup_gb_month);
                check_non_negative(&mut invalid, "backup_rate_usd", *backup_rate_usd);
                check_non_negative(
                    &mut invalid,
                    "operations_monthly_usd",
                    *operations_monthly_usd,
                );
            }
            Self::RdsPostgres {
                instance_hours,
                instance_hour_rate_usd,
                storage_gb_month,
                storage_rate_usd,
                backup_gb_month,
                backup_rate_usd,
                multi_az_multiplier,
            } => {
                check_non_negative(&mut invalid, "instance_hours", *instance_hours);
                check_optional_non_negative(
                    &mut invalid,
                    "instance_hour_rate_usd",
                    *instance_hour_rate_usd,
                );
                check_non_negative(&mut invalid, "storage_gb_month", *storage_gb_month);
                check_optional_non_negative(&mut invalid, "storage_rate_usd", *storage_rate_usd);
                check_non_negative(&mut invalid, "backup_gb_month", *backup_gb_month);
                check_optional_non_negative(&mut invalid, "backup_rate_usd", *backup_rate_usd);
                if !multi_az_multiplier.is_finite() || *multi_az_multiplier < 1.0 {
                    invalid.push("multi_az_multiplier");
                }
            }
            Self::AuroraServerlessV2 {
                minimum_acu,
                acu_hours,
                acu_hour_rate_usd,
                storage_gb_month,
                storage_rate_usd,
                io_million,
                io_million_rate_usd,
                ..
            } => {
                check_non_negative(&mut invalid, "minimum_acu", *minimum_acu);
                check_non_negative(&mut invalid, "acu_hours", *acu_hours);
                check_optional_non_negative(&mut invalid, "acu_hour_rate_usd", *acu_hour_rate_usd);
                check_non_negative(&mut invalid, "storage_gb_month", *storage_gb_month);
                check_optional_non_negative(&mut invalid, "storage_rate_usd", *storage_rate_usd);
                check_non_negative(&mut invalid, "io_million", *io_million);
                check_optional_non_negative(
                    &mut invalid,
                    "io_million_rate_usd",
                    *io_million_rate_usd,
                );
            }
            Self::DynamoDbOnDemand {
                read_request_units_million,
                read_million_rate_usd,
                write_request_units_million,
                write_million_rate_usd,
                storage_gb_month,
                storage_rate_usd,
            } => {
                check_non_negative(
                    &mut invalid,
                    "read_request_units_million",
                    *read_request_units_million,
                );
                check_optional_non_negative(
                    &mut invalid,
                    "read_million_rate_usd",
                    *read_million_rate_usd,
                );
                check_non_negative(
                    &mut invalid,
                    "write_request_units_million",
                    *write_request_units_million,
                );
                check_optional_non_negative(
                    &mut invalid,
                    "write_million_rate_usd",
                    *write_million_rate_usd,
                );
                check_non_negative(&mut invalid, "storage_gb_month", *storage_gb_month);
                check_optional_non_negative(&mut invalid, "storage_rate_usd", *storage_rate_usd);
            }
            Self::SqlitePersistentHost {
                host_monthly_usd,
                backup_monthly_usd,
            } => {
                check_non_negative(&mut invalid, "host_monthly_usd", *host_monthly_usd);
                check_non_negative(&mut invalid, "backup_monthly_usd", *backup_monthly_usd);
            }
            Self::SqliteLambdaMutable {
                expected_storage_gb,
            } => {
                check_non_negative(&mut invalid, "expected_storage_gb", *expected_storage_gb);
            }
        }
        invalid
    }
}

fn check_non_negative(invalid: &mut Vec<&'static str>, field: &'static str, value: f64) {
    if !value.is_finite() || value < 0.0 {
        invalid.push(field);
    }
}

fn check_optional_non_negative(
    invalid: &mut Vec<&'static str>,
    field: &'static str,
    value: Option<f64>,
) {
    if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
        invalid.push(field);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NeonPlan {
    Free,
    Launch,
    Scale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionPlan {
    pub name: String,
    pub artifact_path: String,
    pub memory_mb: u32,
    pub timeout_seconds: u32,
    pub reserved_concurrency: u32,
    pub provisioned_concurrency: u32,
    pub database_connections_per_instance: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutePlan {
    pub operation_id: String,
    pub method: HttpMethod,
    pub path: String,
    pub authenticated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// Each independent switch is part of the serialized deployment-policy contract.
#[allow(clippy::struct_excessive_bools)]
pub struct CostPolicy {
    pub deny_fixed_compute: bool,
    pub deny_nat_gateway: bool,
    pub deny_provisioned_concurrency: bool,
    pub deny_scheduled_wakeups: bool,
    pub max_reserved_concurrency: u32,
    pub max_database_connections: u32,
}

impl Default for CostPolicy {
    fn default() -> Self {
        Self {
            deny_fixed_compute: true,
            deny_nat_gateway: true,
            deny_provisioned_concurrency: true,
            deny_scheduled_wakeups: true,
            max_reserved_concurrency: 5,
            max_database_connections: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformancePolicy {
    pub max_lambda_timeout_seconds: u32,
    pub max_lambda_memory_mb: u32,
    pub max_request_body_bytes: u64,
    pub target_artifact_bytes: u64,
}

impl Default for PerformancePolicy {
    fn default() -> Self {
        Self {
            max_lambda_timeout_seconds: 30,
            max_lambda_memory_mb: 1024,
            max_request_body_bytes: 1_048_576,
            target_artifact_bytes: 25 * 1024 * 1024,
        }
    }
}

const fn default_log_retention_days() -> u32 {
    14
}

fn error(code: &str, message: &str) -> PlanDiagnostic {
    PlanDiagnostic {
        code: code.into(),
        severity: Severity::Error,
        message: message.into(),
    }
}

fn warning(code: &str, message: &str) -> PlanDiagnostic {
    PlanDiagnostic {
        code: code.into(),
        severity: Severity::Warning,
        message: message.into(),
    }
}

fn information(code: &str, message: &str) -> PlanDiagnostic {
    PlanDiagnostic {
        code: code.into(),
        severity: Severity::Information,
        message: message.into(),
    }
}

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("failed to serialize plan: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported SAM route method: {0}")]
    UnsupportedMethod(String),
    #[error("deployment plan has no API function")]
    MissingFunction,
    #[error("unsupported deployment combination: {0}")]
    UnsupportedDeployment(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_contract::{ContractDocument, OwnedOperation};

    fn config(database: DatabaseDeployment) -> DeploymentConfig {
        DeploymentConfig {
            schema_version: 1,
            application: "example".into(),
            environment: "dev".into(),
            region: "ap-southeast-2".into(),
            runtime: RuntimePlan::LambdaZipArm64,
            ingress: IngressPlan::ApiGatewayHttpApi,
            auth: AuthPlan::Jwt {
                issuer: "https://issuer.example.invalid".into(),
                audiences: vec!["orders".into()],
            },
            database,
            functions: vec![FunctionPlan {
                name: "api".into(),
                artifact_path: "target/lambda/orders-lambda/bootstrap.zip".into(),
                memory_mb: 512,
                timeout_seconds: 15,
                reserved_concurrency: 2,
                provisioned_concurrency: 0,
                database_connections_per_instance: 2,
            }],
            scheduled_wakeups: Vec::new(),
            uses_nat_gateway: false,
            allowed_origins: vec!["https://app.example.invalid".into()],
            log_retention_days: 14,
            cost_policy: CostPolicy::default(),
            performance_policy: PerformancePolicy::default(),
        }
    }

    #[test]
    fn routes_are_derived_from_the_contract() {
        let contract = ContractDocument {
            source: "inline".into(),
            openapi_version: "3.1.0".into(),
            title: "test".into(),
            version: "1".into(),
            sha256: "hash".into(),
            operations: vec![OwnedOperation {
                operation_id: "getHealth".into(),
                method: HttpMethod::Get,
                path: "/health".into(),
                authenticated: false,
                idempotent: false,
            }],
            schema_names: Vec::new(),
            raw: serde_json::json!({}),
        };
        let plan = config(DatabaseDeployment::NeonPostgres {
            plan: NeonPlan::Free,
            compute_unit_hours: 1.0,
            storage_gb_month: 0.1,
            history_storage_gb_month: 0.0,
        })
        .into_plan(&contract);
        assert_eq!(plan.routes[0].operation_id, "getHealth");
    }

    #[test]
    fn fixed_compute_is_rejected_by_minimal_idle_policy() {
        let contract = ContractDocument {
            source: "inline".into(),
            openapi_version: "3.1.0".into(),
            title: "test".into(),
            version: "1".into(),
            sha256: "hash".into(),
            operations: Vec::new(),
            schema_names: Vec::new(),
            raw: serde_json::json!({}),
        };
        let plan = config(DatabaseDeployment::RdsPostgres {
            instance_hours: 730.0,
            instance_hour_rate_usd: Some(0.1),
            storage_gb_month: 20.0,
            storage_rate_usd: Some(0.1),
            backup_gb_month: 0.0,
            backup_rate_usd: Some(0.0),
            multi_az_multiplier: 1.0,
        })
        .into_plan(&contract);
        assert!(
            plan.validate()
                .iter()
                .any(|diagnostic| diagnostic.code == "MINCO-COST-007")
        );
    }

    #[test]
    fn negative_cost_inputs_are_rejected() {
        let contract = ContractDocument {
            source: "inline".into(),
            openapi_version: "3.1.0".into(),
            title: "test".into(),
            version: "1".into(),
            sha256: "hash".into(),
            operations: Vec::new(),
            schema_names: Vec::new(),
            raw: serde_json::json!({}),
        };
        let plan = config(DatabaseDeployment::SelfHostedPostgres {
            host_monthly_usd: -1.0,
            storage_gb_month: 20.0,
            storage_rate_usd: 0.08,
            backup_gb_month: 20.0,
            backup_rate_usd: 0.05,
            operations_monthly_usd: 50.0,
        })
        .into_plan(&contract);

        assert!(
            plan.validate()
                .iter()
                .any(|diagnostic| diagnostic.code == "MINCO-COST-008")
        );
    }

    #[test]
    fn aurora_auto_pause_interval_must_match_zero_capacity() {
        let contract = ContractDocument {
            source: "inline".into(),
            openapi_version: "3.1.0".into(),
            title: "test".into(),
            version: "1".into(),
            sha256: "hash".into(),
            operations: Vec::new(),
            schema_names: Vec::new(),
            raw: serde_json::json!({}),
        };
        let invalid_timeout = config(DatabaseDeployment::AuroraServerlessV2 {
            minimum_acu: 0.0,
            auto_pause_seconds: Some(299),
            acu_hours: 1.0,
            acu_hour_rate_usd: Some(0.1),
            storage_gb_month: 1.0,
            storage_rate_usd: Some(0.1),
            io_million: 1.0,
            io_million_rate_usd: Some(0.1),
        })
        .into_plan(&contract);
        let nonzero_minimum = config(DatabaseDeployment::AuroraServerlessV2 {
            minimum_acu: 0.5,
            auto_pause_seconds: Some(300),
            acu_hours: 1.0,
            acu_hour_rate_usd: Some(0.1),
            storage_gb_month: 1.0,
            storage_rate_usd: Some(0.1),
            io_million: 1.0,
            io_million_rate_usd: Some(0.1),
        })
        .into_plan(&contract);

        assert!(
            invalid_timeout
                .validate()
                .iter()
                .any(|diagnostic| diagnostic.code == "MINCO-DB-004")
        );
        assert!(
            nonzero_minimum
                .validate()
                .iter()
                .any(|diagnostic| diagnostic.code == "MINCO-DB-004")
        );
    }
}
