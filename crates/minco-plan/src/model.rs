use crate::sam_logical_id;
use minco_contract::{ContractDocument, HttpMethod};
use minco_core::{ApplicationGraph, ResourceKind};
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
    pub queues: Vec<QueuePlan>,
    #[serde(default)]
    pub triggers: Vec<TriggerPlan>,
    #[serde(default)]
    pub scheduled_wakeups: Vec<String>,
    #[serde(default)]
    pub uses_nat_gateway: bool,
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_allowed_headers")]
    pub allowed_headers: Vec<String>,
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
        self.into_plan_with_graph(contract, ApplicationGraph::default())
    }

    #[must_use]
    pub fn into_plan_with_graph(
        self,
        contract: &ContractDocument,
        application_graph: ApplicationGraph,
    ) -> DeploymentPlan {
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
        let local_aws_services = local_aws_services(
            &self.runtime,
            &self.database,
            &application_graph,
            &self.queues,
        );
        let iam_intents = derive_iam_intents(
            self.schema_version,
            &self.runtime,
            &self.database,
            &self.functions,
            &self.triggers,
        );
        let mut allowed_headers = self
            .allowed_headers
            .into_iter()
            .map(|configured| match configured.parse::<http::HeaderName>() {
                Ok(header) => header.as_str().to_owned(),
                Err(_) => configured,
            })
            .collect::<Vec<_>>();
        allowed_headers.sort();
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
            queues: self.queues,
            triggers: self.triggers,
            iam_intents,
            routes,
            application_graph,
            local_aws_services,
            scheduled_wakeups: self.scheduled_wakeups,
            uses_nat_gateway: self.uses_nat_gateway,
            allowed_origins: self.allowed_origins,
            allowed_headers,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queues: Vec<QueuePlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<TriggerPlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub iam_intents: Vec<IamIntent>,
    pub routes: Vec<RoutePlan>,
    #[serde(default)]
    pub application_graph: ApplicationGraph,
    #[serde(default)]
    pub local_aws_services: Vec<String>,
    pub scheduled_wakeups: Vec<String>,
    pub uses_nat_gateway: bool,
    pub allowed_origins: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub log_retention_days: u32,
    pub cost_policy: CostPolicy,
    pub performance_policy: PerformancePolicy,
}

fn local_aws_services(
    runtime: &RuntimePlan,
    database: &DatabaseDeployment,
    graph: &ApplicationGraph,
    queues: &[QueuePlan],
) -> Vec<String> {
    let mut services = std::collections::BTreeSet::new();
    if matches!(runtime, RuntimePlan::LambdaZipArm64) {
        services.extend(["ssm".to_owned(), "sts".to_owned()]);
    }
    if matches!(database, DatabaseDeployment::DynamoDbOnDemand { .. }) {
        services.insert("dynamodb".into());
    }
    if !queues.is_empty() {
        services.insert("sqs".into());
    }
    for resource in graph.resources.values() {
        match resource.kind {
            ResourceKind::S3Bucket => {
                services.insert("s3".into());
            }
            ResourceKind::SqsQueue => {
                services.insert("sqs".into());
            }
            ResourceKind::SsmParameter => {
                services.insert("ssm".into());
            }
            ResourceKind::DynamoDb => {
                services.insert("dynamodb".into());
            }
            _ => {}
        }
    }
    services.into_iter().collect()
}

impl DeploymentPlan {
    #[must_use]
    pub fn http_api_function_id(&self) -> Option<&str> {
        if self.schema_version == 1 {
            return self
                .functions
                .first()
                .map(|function| function.name.as_str());
        }
        self.triggers.iter().find_map(|trigger| {
            let TriggerPlan::HttpApi { function_id, .. } = trigger else {
                return None;
            };
            Some(function_id.as_str())
        })
    }

    #[must_use]
    pub fn http_api_trigger_id(&self) -> Option<&str> {
        (self.schema_version == 2)
            .then(|| {
                self.triggers.iter().find_map(|trigger| {
                    let TriggerPlan::HttpApi { id, .. } = trigger else {
                        return None;
                    };
                    Some(id.as_str())
                })
            })
            .flatten()
    }

    #[must_use]
    pub fn operation_function_id(&self, operation_id: &str) -> Option<&str> {
        self.routes
            .iter()
            .any(|route| route.operation_id == operation_id)
            .then(|| self.http_api_function_id())
            .flatten()
    }

    /// Converts the legacy API-only schema into the explicit trigger schema.
    ///
    /// Legacy scheduled strings cannot be migrated safely because they do not
    /// identify a function or carry enablement and purpose, so they fail with
    /// a stable rejection instead of being guessed.
    pub fn migrate_to_latest(mut self) -> Result<Self, PlanError> {
        match self.schema_version {
            2 => Ok(self),
            1 => {
                if !self.queues.is_empty()
                    || !self.triggers.is_empty()
                    || !self.scheduled_wakeups.is_empty()
                {
                    return Err(PlanError::SchemaMigration(
                        "MINCO-PLAN-MIGRATE-001: schema 1 can migrate only an API-only plan \
                         without queue, trigger, or scheduled-wakeup fields"
                            .into(),
                    ));
                }
                let [function] = self.functions.as_mut_slice() else {
                    return Err(PlanError::SchemaMigration(
                        "MINCO-PLAN-MIGRATE-002: schema 1 must contain exactly one API function"
                            .into(),
                    ));
                };
                if !matches!(function.role, FunctionRole::HttpApi) {
                    return Err(PlanError::SchemaMigration(
                        "MINCO-PLAN-MIGRATE-002: schema 1 must contain exactly one API function"
                            .into(),
                    ));
                }
                self.triggers.push(TriggerPlan::HttpApi {
                    id: "http-api".into(),
                    function_id: function.name.clone(),
                });
                self.schema_version = 2;
                self.iam_intents = derive_iam_intents(
                    self.schema_version,
                    &self.runtime,
                    &self.database,
                    &self.functions,
                    &self.triggers,
                );
                Ok(self)
            }
            version => Err(PlanError::SchemaMigration(format!(
                "MINCO-PLAN-MIGRATE-003: unsupported source schema version {version}"
            ))),
        }
    }

    #[must_use]
    pub fn validate(&self) -> Vec<PlanDiagnostic> {
        let mut diagnostics = Vec::new();
        if !matches!(self.schema_version, 1 | 2) {
            diagnostics.push(error(
                "MINCO-PLAN-001",
                "unsupported deployment plan schema version",
            ));
        }
        let expected_local_services = local_aws_services(
            &self.runtime,
            &self.database,
            &self.application_graph,
            &self.queues,
        );
        if self.local_aws_services != expected_local_services {
            diagnostics.push(error(
                "MINCO-PLAN-003",
                "local_aws_services does not match the configured application graph",
            ));
        }
        if self.routes.iter().any(|route| route.authenticated)
            && matches!(&self.auth, AuthPlan::None)
        {
            diagnostics.push(error("MINCO-AUTH-001", "the contract contains authenticated operations but no deployment authorizer is configured"));
        }
        if self.schema_version == 1 {
            if self.functions.len() != 1 {
                diagnostics.push(error(
                    "MINCO-PLAN-002",
                    "the initial Minco AWS profile requires exactly one API function",
                ));
            }
            if !self.queues.is_empty() || !self.triggers.is_empty() || !self.iam_intents.is_empty()
            {
                diagnostics.push(error(
                    "MINCO-PLAN-004",
                    "queues, typed IAM intent, and structured triggers require schema version 2",
                ));
            }
            if self
                .functions
                .iter()
                .any(|function| !matches!(function.role, FunctionRole::HttpApi))
            {
                diagnostics.push(error(
                    "MINCO-PLAN-005",
                    "schema version 1 supports only its single HTTP API function",
                ));
            }
        }
        if self.schema_version == 2 {
            if !self.scheduled_wakeups.is_empty() {
                diagnostics.push(error(
                    "MINCO-SCHEDULE-003",
                    "schema version 2 rejects unstructured scheduled_wakeups; use a schedule trigger",
                ));
            }
            validate_multi_runtime_topology(self, &mut diagnostics);
            if self.iam_intents
                != derive_iam_intents(
                    self.schema_version,
                    &self.runtime,
                    &self.database,
                    &self.functions,
                    &self.triggers,
                )
            {
                diagnostics.push(error(
                    "MINCO-IAM-001",
                    "iam_intents does not match the selected functions and triggers",
                ));
            }
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
        if self.allowed_headers.is_empty() {
            diagnostics.push(error(
                "MINCO-HTTP-003",
                "at least one exact CORS request header is required",
            ));
        }
        let mut seen_headers = std::collections::BTreeSet::new();
        for configured in &self.allowed_headers {
            let Ok(header) = configured.parse::<http::HeaderName>() else {
                diagnostics.push(error(
                    "MINCO-HTTP-004",
                    &format!("invalid CORS request header: {configured}"),
                ));
                continue;
            };
            if header.as_str() == "*" {
                diagnostics.push(error(
                    "MINCO-HTTP-005",
                    "wildcard CORS request headers are forbidden",
                ));
            } else if !seen_headers.insert(header.as_str().to_owned()) {
                diagnostics.push(error(
                    "MINCO-HTTP-006",
                    &format!("duplicate CORS request header: {}", header.as_str()),
                ));
            }
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
        let enabled_schedules = self
            .triggers
            .iter()
            .filter(|trigger| matches!(trigger, TriggerPlan::Schedule { enabled: true, .. }))
            .map(TriggerPlan::id)
            .collect::<Vec<_>>();
        if self.cost_policy.deny_scheduled_wakeups
            && (!self.scheduled_wakeups.is_empty() || !enabled_schedules.is_empty())
        {
            diagnostics.push(error(
                "MINCO-COST-002",
                &if enabled_schedules.is_empty() {
                    "minimal-idle profile forbids scheduled wakeups".to_owned()
                } else {
                    format!(
                        "minimal-idle profile forbids enabled schedules: {}",
                        enabled_schedules.join(", ")
                    )
                },
            ));
        } else if !self.cost_policy.deny_scheduled_wakeups {
            for trigger in &self.triggers {
                if let TriggerPlan::Schedule {
                    id,
                    function_id,
                    expression,
                    enabled: true,
                    ..
                } = trigger
                {
                    diagnostics.push(information(
                        "MINCO-COST-009",
                        &format!(
                            "enabled schedule {id} invokes {function_id} with {expression}; it \
                             is a request-based wake source and can wake a scale-to-zero database: {}",
                            self.database.can_scale_to_zero()
                        ),
                    ));
                }
            }
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
            if self.schema_version == 2
                && (function.memory_mb == 0
                    || function.timeout_seconds == 0
                    || function.reserved_concurrency == 0)
            {
                diagnostics.push(error(
                    "MINCO-PERF-004",
                    &format!(
                        "function {} requires non-zero memory, timeout, and reserved concurrency",
                        function.name
                    ),
                ));
            }
            if function.provisioned_concurrency > function.reserved_concurrency {
                diagnostics.push(error(
                    "MINCO-COST-010",
                    &format!(
                        "function {} provisioned concurrency exceeds reserved concurrency",
                        function.name
                    ),
                ));
            }
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
            .fold(0, u32::saturating_add);
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

fn validate_multi_runtime_topology(plan: &DeploymentPlan, diagnostics: &mut Vec<PlanDiagnostic>) {
    let mut functions = std::collections::BTreeMap::new();
    for function in &plan.functions {
        if !is_stable_id(&function.name) {
            diagnostics.push(error(
                "MINCO-PLAN-010",
                &format!(
                    "function {} does not use a stable identifier",
                    function.name
                ),
            ));
        }
        if functions.insert(function.name.as_str(), function).is_some() {
            diagnostics.push(error(
                "MINCO-PLAN-011",
                &format!("duplicate function identifier: {}", function.name),
            ));
        }
    }
    let api_functions = plan
        .functions
        .iter()
        .filter(|function| matches!(function.role, FunctionRole::HttpApi))
        .count();
    if api_functions != 1 {
        diagnostics.push(error(
            "MINCO-PLAN-012",
            "schema version 2 requires exactly one HTTP API function",
        ));
    }

    let mut queues = std::collections::BTreeMap::new();
    for queue in &plan.queues {
        if !is_stable_id(&queue.id) {
            diagnostics.push(error(
                "MINCO-PLAN-010",
                &format!("queue {} does not use a stable identifier", queue.id),
            ));
        }
        if queues.insert(queue.id.as_str(), queue).is_some() {
            diagnostics.push(error(
                "MINCO-PLAN-013",
                &format!("duplicate queue identifier: {}", queue.id),
            ));
        }
        if queue.visibility_timeout_seconds > 43_200 {
            diagnostics.push(error(
                "MINCO-SQS-010",
                &format!(
                    "queue {} visibility timeout exceeds 43200 seconds",
                    queue.id
                ),
            ));
        }
        if !(60..=1_209_600).contains(&queue.retention_seconds) {
            diagnostics.push(error(
                "MINCO-SQS-011",
                &format!(
                    "queue {} retention must be between 60 and 1209600 seconds",
                    queue.id
                ),
            ));
        }
    }
    for queue in &plan.queues {
        match (&queue.dead_letter_queue_id, queue.max_receive_count) {
            (Some(dead_letter_queue_id), Some(max_receive_count)) => {
                if dead_letter_queue_id == &queue.id {
                    diagnostics.push(error(
                        "MINCO-SQS-004",
                        &format!("queue {} cannot be its own dead-letter queue", queue.id),
                    ));
                }
                match queues.get(dead_letter_queue_id.as_str()) {
                    Some(dead_letter_queue) if dead_letter_queue.fifo != queue.fifo => {
                        diagnostics.push(error(
                            "MINCO-SQS-003",
                            &format!(
                                "queue {} and dead-letter queue {dead_letter_queue_id} must \
                                 use the same FIFO setting",
                                queue.id
                            ),
                        ));
                    }
                    Some(_) => {}
                    None => diagnostics.push(error(
                        "MINCO-PLAN-015",
                        &format!(
                            "queue {} references missing dead-letter queue \
                             {dead_letter_queue_id}",
                            queue.id
                        ),
                    )),
                }
                if !(1..=1_000).contains(&max_receive_count) {
                    diagnostics.push(error(
                        "MINCO-SQS-005",
                        &format!(
                            "queue {} max_receive_count must be between 1 and 1000",
                            queue.id
                        ),
                    ));
                }
            }
            (None, None) => {}
            _ => diagnostics.push(error(
                "MINCO-SQS-004",
                &format!(
                    "queue {} must configure dead_letter_queue_id and max_receive_count together",
                    queue.id
                ),
            )),
        }
    }
    if let Some(queue_id) = redrive_cycle_start(&queues) {
        diagnostics.push(error(
            "MINCO-SQS-006",
            &format!("dead-letter queue references contain a cycle from {queue_id}"),
        ));
    }

    let mut trigger_ids = std::collections::BTreeSet::new();
    let mut http_triggers = 0;
    for trigger in &plan.triggers {
        if !is_stable_id(trigger.id()) {
            diagnostics.push(error(
                "MINCO-PLAN-010",
                &format!("trigger {} does not use a stable identifier", trigger.id()),
            ));
        }
        if !trigger_ids.insert(trigger.id()) {
            diagnostics.push(error(
                "MINCO-PLAN-014",
                &format!("duplicate trigger identifier: {}", trigger.id()),
            ));
        }
        match trigger {
            TriggerPlan::HttpApi { function_id, .. } => {
                http_triggers += 1;
                match functions.get(function_id.as_str()) {
                    Some(function) if matches!(function.role, FunctionRole::HttpApi) => {}
                    Some(_) => diagnostics.push(error(
                        "MINCO-PLAN-016",
                        &format!(
                            "HTTP API trigger {} must target an HTTP API function",
                            trigger.id()
                        ),
                    )),
                    None => diagnostics.push(error(
                        "MINCO-PLAN-015",
                        &format!(
                            "trigger {} references missing function {function_id}",
                            trigger.id()
                        ),
                    )),
                }
            }
            TriggerPlan::Sqs {
                function_id,
                queue_id,
                batch_size,
                batching_window_seconds,
                report_batch_item_failures,
                maximum_concurrency,
                ..
            } => {
                let function = functions.get(function_id.as_str()).copied();
                match function {
                    Some(function) if matches!(function.role, FunctionRole::Worker) => {}
                    Some(_) => diagnostics.push(error(
                        "MINCO-PLAN-016",
                        &format!("SQS trigger {} must target a worker function", trigger.id()),
                    )),
                    None => diagnostics.push(error(
                        "MINCO-PLAN-015",
                        &format!(
                            "trigger {} references missing function {function_id}",
                            trigger.id()
                        ),
                    )),
                }
                let queue = queues.get(queue_id.as_str()).copied();
                if queue.is_none() {
                    diagnostics.push(error(
                        "MINCO-PLAN-015",
                        &format!(
                            "trigger {} references missing queue {queue_id}",
                            trigger.id()
                        ),
                    ));
                }
                if let (Some(function), Some(queue)) = (function, queue) {
                    let required_visibility = function
                        .timeout_seconds
                        .saturating_mul(6)
                        .saturating_add(*batching_window_seconds);
                    if queue.visibility_timeout_seconds < required_visibility {
                        diagnostics.push(error(
                            "MINCO-SQS-002",
                            &format!(
                                "queue {queue_id} visibility timeout must be at least \
                                 {required_visibility}s for trigger {}",
                                trigger.id()
                            ),
                        ));
                    }
                    if !(2..=1_000).contains(maximum_concurrency)
                        || *maximum_concurrency > function.reserved_concurrency
                        || *maximum_concurrency > plan.cost_policy.max_reserved_concurrency
                    {
                        diagnostics.push(error(
                            "MINCO-SQS-007",
                            &format!(
                                "SQS trigger {} maximum concurrency must be 2 to 1000 and \
                                 cannot exceed worker or cost-policy concurrency",
                                trigger.id()
                            ),
                        ));
                    }
                    let maximum_batch_size = if queue.fifo { 10 } else { 10_000 };
                    if !(1..=maximum_batch_size).contains(batch_size) {
                        diagnostics.push(error(
                            "MINCO-SQS-008",
                            &format!(
                                "SQS trigger {} batch size must be 1 to {maximum_batch_size}",
                                trigger.id()
                            ),
                        ));
                    }
                    if *batching_window_seconds > 300
                        || (queue.fifo && *batching_window_seconds != 0)
                        || (!queue.fifo && *batch_size > 10 && *batching_window_seconds == 0)
                    {
                        diagnostics.push(error(
                            "MINCO-SQS-009",
                            &format!(
                                "SQS trigger {} uses an invalid batching window",
                                trigger.id()
                            ),
                        ));
                    }
                }
                if !report_batch_item_failures {
                    diagnostics.push(error(
                        "MINCO-SQS-001",
                        &format!(
                            "SQS trigger {} must enable ReportBatchItemFailures",
                            trigger.id()
                        ),
                    ));
                }
            }
            TriggerPlan::Schedule {
                function_id,
                expression,
                purpose,
                ..
            } => {
                if !functions.contains_key(function_id.as_str()) {
                    diagnostics.push(error(
                        "MINCO-PLAN-015",
                        &format!(
                            "trigger {} references missing function {function_id}",
                            trigger.id()
                        ),
                    ));
                }
                if purpose.trim().is_empty() {
                    diagnostics.push(error(
                        "MINCO-SCHEDULE-001",
                        &format!("schedule {} requires a reviewable purpose", trigger.id()),
                    ));
                }
                if !is_schedule_expression(expression) {
                    diagnostics.push(error(
                        "MINCO-SCHEDULE-002",
                        &format!(
                            "schedule {} must use an EventBridge at(...), rate(...), or \
                             cron(...) expression",
                            trigger.id()
                        ),
                    ));
                }
            }
        }
    }
    for function in plan
        .functions
        .iter()
        .filter(|function| matches!(function.role, FunctionRole::Worker))
    {
        let mapping_concurrency = plan
            .triggers
            .iter()
            .filter_map(|trigger| {
                let TriggerPlan::Sqs {
                    function_id,
                    maximum_concurrency,
                    ..
                } = trigger
                else {
                    return None;
                };
                (function_id == &function.name).then_some(*maximum_concurrency)
            })
            .fold(0, u32::saturating_add);
        if mapping_concurrency > function.reserved_concurrency {
            diagnostics.push(error(
                "MINCO-SQS-012",
                &format!(
                    "worker {} has aggregate SQS maximum concurrency {mapping_concurrency}, exceeding reserved concurrency {}",
                    function.name, function.reserved_concurrency
                ),
            ));
        }
    }
    if http_triggers != 1 {
        diagnostics.push(error(
            "MINCO-PLAN-017",
            "schema version 2 requires exactly one explicit HTTP API trigger",
        ));
    }
    validate_sam_resource_identifiers(plan, diagnostics);
}

fn validate_sam_resource_identifiers(plan: &DeploymentPlan, diagnostics: &mut Vec<PlanDiagnostic>) {
    let mut resources = std::collections::BTreeMap::from([
        ("ApiFunction".to_owned(), "HTTP API function".to_owned()),
        ("ApiLogGroup".to_owned(), "HTTP API log group".to_owned()),
    ]);
    let mut insert_resource = |logical_id: String, owner: String| {
        if let Some(existing) = resources.insert(logical_id.clone(), owner.clone()) {
            diagnostics.push(error(
                "MINCO-PLAN-018",
                &format!(
                    "{owner} and {existing} collapse to the same SAM logical identifier {logical_id}"
                ),
            ));
        }
    };
    for function in plan
        .functions
        .iter()
        .filter(|function| matches!(function.role, FunctionRole::Worker))
    {
        let prefix = sam_logical_id(&function.name);
        insert_resource(
            format!("{prefix}Function"),
            format!("worker function {}", function.name),
        );
        insert_resource(
            format!("{prefix}LogGroup"),
            format!("worker log group {}", function.name),
        );
    }
    for queue in &plan.queues {
        insert_resource(
            format!("{}Queue", sam_logical_id(&queue.id)),
            format!("queue {}", queue.id),
        );
    }

    let mut events = std::collections::BTreeMap::new();
    for route in &plan.routes {
        let event_id = format!("{}Event", sam_logical_id(&route.operation_id));
        let key = ("__http_api", event_id.clone());
        if let Some(existing) = events.insert(key, route.operation_id.as_str()) {
            diagnostics.push(error(
                "MINCO-PLAN-018",
                &format!(
                    "operations {} and {} collapse to the same SAM event identifier {event_id}",
                    route.operation_id, existing
                ),
            ));
        }
    }
    for trigger in &plan.triggers {
        let function_id = match trigger {
            TriggerPlan::Sqs { function_id, .. } | TriggerPlan::Schedule { function_id, .. } => {
                function_id
            }
            TriggerPlan::HttpApi { .. } => continue,
        };
        let event_id = format!("{}Event", sam_logical_id(trigger.id()));
        let key = (function_id.as_str(), event_id.clone());
        if let Some(existing) = events.insert(key, trigger.id()) {
            diagnostics.push(error(
                "MINCO-PLAN-018",
                &format!(
                    "triggers {} and {} on function {function_id} collapse to the same SAM event identifier {event_id}",
                    trigger.id(),
                    existing
                ),
            ));
        }
    }

    if matches!(plan.runtime, RuntimePlan::LambdaZipArm64) {
        for function in &plan.functions {
            let name = if matches!(function.role, FunctionRole::HttpApi) {
                format!("{}-{}-api", plan.application, plan.environment)
            } else {
                format!(
                    "{}-{}-{}",
                    plan.application, plan.environment, function.name
                )
            };
            if !valid_lambda_function_name(&name) {
                diagnostics.push(error(
                    "MINCO-AWS-001",
                    &format!(
                        "function {} derives invalid Lambda FunctionName {name}",
                        function.name
                    ),
                ));
            }
        }
        for queue in &plan.queues {
            let suffix = if queue.fifo { ".fifo" } else { "" };
            let name = format!(
                "{}-{}-{}{}",
                plan.application, plan.environment, queue.id, suffix
            );
            if !valid_sqs_queue_name(&name, queue.fifo) {
                diagnostics.push(error(
                    "MINCO-AWS-002",
                    &format!("queue {} derives invalid SQS QueueName {name}", queue.id),
                ));
            }
        }
    }
}

fn valid_lambda_function_name(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_sqs_queue_name(value: &str, fifo: bool) -> bool {
    if !(1..=80).contains(&value.len()) {
        return false;
    }
    let body = if fifo {
        let Some(body) = value.strip_suffix(".fifo") else {
            return false;
        };
        body
    } else {
        value
    };
    !body.is_empty()
        && body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn redrive_cycle_start<'a>(
    queues: &std::collections::BTreeMap<&'a str, &'a QueuePlan>,
) -> Option<&'a str> {
    for start in queues.keys().copied() {
        let mut visited = std::collections::BTreeSet::new();
        let mut current = start;
        loop {
            if !visited.insert(current) {
                return Some(start);
            }
            let next = queues
                .get(current)
                .and_then(|queue| queue.dead_letter_queue_id.as_deref());
            match next {
                Some(next) if queues.contains_key(next) => current = next,
                _ => break,
            }
        }
    }
    None
}

fn is_stable_id(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(first) if first.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn is_schedule_expression(value: &str) -> bool {
    (1..=256).contains(&value.len())
        && !value.contains(['\r', '\n'])
        && value.ends_with(')')
        && ["at(", "rate(", "cron("]
            .iter()
            .any(|prefix| value.starts_with(prefix) && value.len() > prefix.len() + 1)
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

    #[must_use]
    pub fn can_scale_to_zero(&self) -> bool {
        match self {
            Self::NeonPostgres { .. } | Self::DynamoDbOnDemand { .. } => true,
            Self::AuroraServerlessV2 {
                minimum_acu,
                auto_pause_seconds,
                ..
            } => *minimum_acu == 0.0 && auto_pause_seconds.is_some(),
            Self::SelfHostedPostgres { .. }
            | Self::RdsPostgres { .. }
            | Self::SqlitePersistentHost { .. }
            | Self::SqliteLambdaMutable { .. } => false,
        }
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
    #[serde(default, skip_serializing_if = "FunctionRole::is_http_api")]
    pub role: FunctionRole,
    pub artifact_path: String,
    pub memory_mb: u32,
    pub timeout_seconds: u32,
    pub reserved_concurrency: u32,
    pub provisioned_concurrency: u32,
    pub database_connections_per_instance: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionRole {
    #[default]
    HttpApi,
    Worker,
}

impl FunctionRole {
    // Serde's `skip_serializing_if` contract passes this field by reference.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    const fn is_http_api(&self) -> bool {
        matches!(self, Self::HttpApi)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuePlan {
    pub id: String,
    pub fifo: bool,
    pub visibility_timeout_seconds: u32,
    pub retention_seconds: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_letter_queue_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_receive_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriggerPlan {
    HttpApi {
        id: String,
        function_id: String,
    },
    Sqs {
        id: String,
        function_id: String,
        queue_id: String,
        batch_size: u32,
        batching_window_seconds: u32,
        report_batch_item_failures: bool,
        maximum_concurrency: u32,
    },
    Schedule {
        id: String,
        function_id: String,
        expression: String,
        enabled: bool,
        purpose: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IamIntent {
    pub function_id: String,
    pub actions: Vec<String>,
    pub resource: IamResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IamResource {
    DatabaseUrlParameter,
    DatabaseUrlKmsKey,
    Queue { queue_id: String },
    Function { function_id: String },
}

fn derive_iam_intents(
    schema_version: u32,
    runtime: &RuntimePlan,
    database: &DatabaseDeployment,
    functions: &[FunctionPlan],
    triggers: &[TriggerPlan],
) -> Vec<IamIntent> {
    if schema_version != 2 {
        return Vec::new();
    }
    let mut intents = Vec::new();
    if matches!(runtime, RuntimePlan::LambdaZipArm64)
        && matches!(
            database,
            DatabaseDeployment::NeonPostgres { .. }
                | DatabaseDeployment::SelfHostedPostgres { .. }
                | DatabaseDeployment::RdsPostgres { .. }
                | DatabaseDeployment::AuroraServerlessV2 { .. }
        )
    {
        for function in functions
            .iter()
            .filter(|function| function.database_connections_per_instance > 0)
        {
            intents.push(IamIntent {
                function_id: function.name.clone(),
                actions: vec!["ssm:GetParameter".into()],
                resource: IamResource::DatabaseUrlParameter,
            });
            intents.push(IamIntent {
                function_id: function.name.clone(),
                actions: vec!["kms:Decrypt".into()],
                resource: IamResource::DatabaseUrlKmsKey,
            });
        }
    }
    for trigger in triggers {
        match trigger {
            TriggerPlan::Sqs {
                function_id,
                queue_id,
                ..
            } => {
                intents.push(IamIntent {
                    function_id: function_id.clone(),
                    actions: [
                        "sqs:ChangeMessageVisibility",
                        "sqs:DeleteMessage",
                        "sqs:GetQueueAttributes",
                        "sqs:ReceiveMessage",
                    ]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                    resource: IamResource::Queue {
                        queue_id: queue_id.clone(),
                    },
                });
            }
            TriggerPlan::Schedule { function_id, .. } => {
                intents.push(IamIntent {
                    function_id: function_id.clone(),
                    actions: vec!["lambda:InvokeFunction".into()],
                    resource: IamResource::Function {
                        function_id: function_id.clone(),
                    },
                });
            }
            TriggerPlan::HttpApi { .. } => {}
        }
    }
    intents.sort_by(|left, right| {
        left.function_id
            .cmp(&right.function_id)
            .then_with(|| iam_resource_key(&left.resource).cmp(&iam_resource_key(&right.resource)))
    });
    intents
}

fn iam_resource_key(resource: &IamResource) -> (&str, &str) {
    match resource {
        IamResource::DatabaseUrlParameter => ("database_url_parameter", ""),
        IamResource::DatabaseUrlKmsKey => ("database_url_kms_key", ""),
        IamResource::Queue { queue_id } => ("queue", queue_id),
        IamResource::Function { function_id } => ("function", function_id),
    }
}

impl TriggerPlan {
    fn id(&self) -> &str {
        match self {
            Self::HttpApi { id, .. } | Self::Sqs { id, .. } | Self::Schedule { id, .. } => id,
        }
    }
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

fn default_allowed_headers() -> Vec<String> {
    [
        "authorization",
        "content-type",
        "idempotency-key",
        "x-request-id",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
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
    #[error("deployment plan schema migration failed: {0}")]
    SchemaMigration(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_contract::{ContractDocument, OwnedOperation};
    use minco_core::{
        GraphBuilder, IdleCostClass, PluginDescriptor, PluginId, ResourceIntent, ResourceKind,
    };

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
                role: FunctionRole::HttpApi,
                artifact_path: "target/lambda/orders-lambda/bootstrap.zip".into(),
                memory_mb: 512,
                timeout_seconds: 15,
                reserved_concurrency: 2,
                provisioned_concurrency: 0,
                database_connections_per_instance: 2,
            }],
            queues: Vec::new(),
            triggers: Vec::new(),
            scheduled_wakeups: Vec::new(),
            uses_nat_gateway: false,
            allowed_origins: vec!["https://app.example.invalid".into()],
            allowed_headers: default_allowed_headers(),
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
    fn request_headers_are_normalized_and_sorted_in_the_plan() {
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
        let mut deployment = config(DatabaseDeployment::NeonPostgres {
            plan: NeonPlan::Free,
            compute_unit_hours: 1.0,
            storage_gb_month: 0.1,
            history_storage_gb_month: 0.0,
        });
        deployment.allowed_headers = vec!["X-Request-ID".into(), "Authorization".into()];
        let plan = deployment.into_plan(&contract);
        assert_eq!(plan.allowed_headers, ["authorization", "x-request-id"]);
    }

    #[test]
    fn plan_serializes_the_selected_graph_and_derives_local_aws_services() {
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
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("aws-provider").unwrap(),
            "1.0.0".parse().unwrap(),
            "AWS provider",
        );
        descriptor.resources.extend([
            ResourceIntent {
                id: "attachments".into(),
                kind: ResourceKind::S3Bucket,
                idle_cost: IdleCostClass::StorageOnly,
                wake_sources: Vec::new(),
                dependencies: Vec::new(),
            },
            ResourceIntent {
                id: "events".into(),
                kind: ResourceKind::SqsQueue,
                idle_cost: IdleCostClass::ZeroCompute,
                wake_sources: Vec::new(),
                dependencies: Vec::new(),
            },
        ]);
        let mut builder = GraphBuilder::default();
        builder.add_plugin(descriptor);
        let graph = builder.build().unwrap();

        let plan = config(DatabaseDeployment::NeonPostgres {
            plan: NeonPlan::Free,
            compute_unit_hours: 1.0,
            storage_gb_month: 0.1,
            history_storage_gb_month: 0.0,
        })
        .into_plan_with_graph(&contract, graph);

        assert_eq!(
            plan.application_graph.plugins[0].id.as_str(),
            "aws-provider"
        );
        assert_eq!(plan.local_aws_services, ["s3", "sqs", "ssm", "sts"]);
    }

    #[test]
    fn plan_rejects_a_tampered_local_service_projection() {
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
        let mut plan = config(DatabaseDeployment::NeonPostgres {
            plan: NeonPlan::Free,
            compute_unit_hours: 1.0,
            storage_gb_month: 0.1,
            history_storage_gb_month: 0.0,
        })
        .into_plan(&contract);
        plan.local_aws_services.push("s3".into());

        assert!(
            plan.validate()
                .iter()
                .any(|diagnostic| diagnostic.code == "MINCO-PLAN-003")
        );
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
