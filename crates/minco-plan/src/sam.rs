use crate::{
    AuthPlan, DatabaseDeployment, DeploymentPlan, FunctionPlan, FunctionRole, IngressPlan,
    PlanError, QueuePlan, RuntimePlan, TriggerPlan, sam_logical_id,
};
use minco_contract::HttpMethod;
use std::{collections::BTreeMap, fmt::Write as _};

pub fn render_sam(plan: &DeploymentPlan) -> Result<String, PlanError> {
    render_sam_with_code_uris(plan, &BTreeMap::new())
}

pub fn render_sam_with_code_uri(
    plan: &DeploymentPlan,
    code_uri: Option<&str>,
) -> Result<String, PlanError> {
    let mut code_uris = BTreeMap::new();
    if let Some(code_uri) = code_uri {
        let function = api_function(plan).ok_or(PlanError::MissingFunction)?;
        code_uris.insert(function.name.clone(), code_uri.to_owned());
    }
    render_sam_with_code_uris(plan, &code_uris)
}

pub fn render_sam_with_code_uris(
    plan: &DeploymentPlan,
    code_uris: &BTreeMap<String, String>,
) -> Result<String, PlanError> {
    if !matches!(&plan.runtime, RuntimePlan::LambdaZipArm64) {
        return Err(PlanError::UnsupportedDeployment(
            "SAM rendering requires lambda_zip_arm64".into(),
        ));
    }
    if !matches!(&plan.ingress, IngressPlan::ApiGatewayHttpApi) {
        return Err(PlanError::UnsupportedDeployment(
            "the initial SAM renderer requires api_gateway_http_api".into(),
        ));
    }
    if !matches!(
        &plan.database,
        DatabaseDeployment::NeonPostgres { .. }
            | DatabaseDeployment::SelfHostedPostgres { .. }
            | DatabaseDeployment::RdsPostgres { .. }
            | DatabaseDeployment::AuroraServerlessV2 { .. }
    ) {
        return Err(PlanError::UnsupportedDeployment(
            "this renderer requires an externally provisioned PostgreSQL-compatible database; DynamoDB needs a dedicated adapter/rendering plugin and mutable SQLite is rejected on Lambda".into(),
        ));
    }
    let function = api_function(plan).ok_or(PlanError::MissingFunction)?;
    let mut output = String::new();
    output.push_str("AWSTemplateFormatVersion: '2010-09-09'\n");
    output.push_str("Transform: AWS::Serverless-2016-10-31\n");
    writeln!(
        output,
        "Description: {}",
        yaml_quote(&format!(
            "Minco deployment for {} ({})",
            plan.application, plan.environment
        ))
    )
    .expect("writing to String cannot fail");
    let uses_database_parameter = plan
        .functions
        .iter()
        .any(|function| function.database_connections_per_instance > 0);
    if uses_database_parameter {
        render_database_parameters(&mut output);
    }
    output.push_str("Resources:\n");
    output.push_str("  HttpApi:\n");
    output.push_str("    Type: AWS::Serverless::HttpApi\n");
    output.push_str("    Properties:\n");
    output.push_str("      StageName: '$default'\n");
    output.push_str("      CorsConfiguration:\n");
    output.push_str("        AllowMethods: [GET, POST, PUT, PATCH, DELETE, OPTIONS]\n");
    output.push_str("        AllowHeaders:\n");
    for header in &plan.allowed_headers {
        writeln!(output, "          - {}", yaml_quote(header))
            .expect("writing to String cannot fail");
    }
    output.push_str("        AllowOrigins:\n");
    for origin in &plan.allowed_origins {
        writeln!(output, "          - {}", yaml_quote(origin))
            .expect("writing to String cannot fail");
    }
    if let AuthPlan::Jwt { issuer, audiences } = &plan.auth {
        output.push_str("      Auth:\n");
        output.push_str("        DefaultAuthorizer: JwtAuthorizer\n");
        output.push_str("        Authorizers:\n");
        output.push_str("          JwtAuthorizer:\n");
        output.push_str("            IdentitySource: '$request.header.Authorization'\n");
        output.push_str("            JwtConfiguration:\n");
        writeln!(output, "              issuer: {}", yaml_quote(issuer))
            .expect("writing to String cannot fail");
        output.push_str("              audience:\n");
        for audience in audiences {
            writeln!(output, "                - {}", yaml_quote(audience))
                .expect("writing to String cannot fail");
        }
    }
    for queue in &plan.queues {
        render_queue(&mut output, plan, queue);
    }
    output.push_str("  ApiFunction:\n");
    output.push_str("    Type: AWS::Serverless::Function\n");
    output.push_str("    Properties:\n");
    writeln!(
        output,
        "      FunctionName: {}",
        yaml_quote(&format!("{}-{}-api", plan.application, plan.environment))
    )
    .expect("writing to String cannot fail");
    writeln!(
        output,
        "      CodeUri: {}",
        yaml_quote(
            code_uris
                .get(&function.name)
                .map_or(&function.artifact_path, String::as_str)
        )
    )
    .expect("writing to String cannot fail");
    output.push_str("      Handler: bootstrap\n");
    output.push_str("      Runtime: provided.al2023\n");
    output.push_str("      Architectures: [arm64]\n");
    writeln!(output, "      MemorySize: {}", function.memory_mb)
        .expect("writing to String cannot fail");
    writeln!(output, "      Timeout: {}", function.timeout_seconds)
        .expect("writing to String cannot fail");
    writeln!(
        output,
        "      ReservedConcurrentExecutions: {}",
        function.reserved_concurrency
    )
    .expect("writing to String cannot fail");
    if function.database_connections_per_instance > 0 {
        render_vpc_config(&mut output);
    }
    output.push_str("      Environment:\n");
    output.push_str("        Variables:\n");
    writeln!(
        output,
        "          APP_ENV: {}",
        yaml_quote(&plan.environment)
    )
    .expect("writing to String cannot fail");
    if function.database_connections_per_instance > 0 {
        render_database_environment(&mut output, function);
    }
    writeln!(
        output,
        "          ALLOWED_ORIGINS: {}",
        yaml_quote(&plan.allowed_origins.join(","))
    )
    .expect("writing to String cannot fail");
    output.push_str("          ALLOW_DEVELOPMENT_HEADERS: 'false'\n");
    if function.database_connections_per_instance > 0 {
        output.push_str("      Policies:\n");
        output.push_str("        - Statement:\n");
        render_database_policy_statements(&mut output);
    }
    output.push_str("      Events:\n");
    for route in &plan.routes {
        writeln!(output, "        {}:", event_name(&route.operation_id))
            .expect("writing to String cannot fail");
        output.push_str("          Type: HttpApi\n");
        output.push_str("          Properties:\n");
        output.push_str("            ApiId: !Ref HttpApi\n");
        writeln!(output, "            Path: {}", yaml_quote(&route.path))
            .expect("writing to String cannot fail");
        writeln!(output, "            Method: {}", method(route.method))
            .expect("writing to String cannot fail");
        if !route.authenticated && matches!(&plan.auth, AuthPlan::Jwt { .. }) {
            output.push_str("            Auth:\n");
            output.push_str("              Authorizer: NONE\n");
        }
    }
    for worker in plan
        .functions
        .iter()
        .filter(|function| matches!(function.role, FunctionRole::Worker))
    {
        render_worker_function(&mut output, plan, worker, code_uris);
    }
    output.push_str("  ApiLogGroup:\n");
    output.push_str("    Type: AWS::Logs::LogGroup\n");
    output.push_str("    Properties:\n");
    output.push_str("      LogGroupName: !Sub '/aws/lambda/${ApiFunction}'\n");
    writeln!(output, "      RetentionInDays: {}", plan.log_retention_days)
        .expect("writing to String cannot fail");
    output.push_str("Outputs:\n");
    output.push_str("  ApiUrl:\n");
    output.push_str(
        "    Value: !Sub 'https://${HttpApi}.execute-api.${AWS::Region}.${AWS::URLSuffix}'\n",
    );
    output.push_str("  ApiFunctionName:\n");
    output.push_str("    Value: !Ref ApiFunction\n");
    Ok(output)
}

fn render_database_parameters(output: &mut String) {
    output.push_str("Parameters:\n");
    output.push_str("  DatabaseUrlParameterName:\n");
    output.push_str("    Type: String\n");
    output.push_str("    AllowedPattern: '^/[A-Za-z0-9_.\\-/]+$'\n");
    output.push_str("    Description: Existing SSM SecureString containing the pooled runtime PostgreSQL URL.\n");
    output.push_str("  DatabaseUrlKmsKeyArn:\n");
    output.push_str("    Type: String\n");
    output.push_str("    Default: ''\n");
    output.push_str(
        "    AllowedPattern: '^$|^arn:[a-z0-9-]+:kms:[a-z0-9-]+:[0-9]{12}:key/([A-Fa-f0-9-]+|mrk-[A-Fa-f0-9]+)$'\n",
    );
    output.push_str("    Description: Customer-managed KMS key ARN for the database parameter; leave empty only when the AWS-managed aws/ssm key is used.\n");
    output.push_str("  LambdaSubnetIds:\n");
    output.push_str("    Type: String\n");
    output.push_str("    Default: ''\n");
    output.push_str("    AllowedPattern: '^$|^subnet-[a-z0-9]+(,subnet-[a-z0-9]+)*$'\n");
    output.push_str("    Description: Optional comma-separated private subnet IDs for an externally provisioned VPC database profile.\n");
    output.push_str("  LambdaSecurityGroupIds:\n");
    output.push_str("    Type: String\n");
    output.push_str("    Default: ''\n");
    output.push_str("    AllowedPattern: '^$|^sg-[a-z0-9]+(,sg-[a-z0-9]+)*$'\n");
    output.push_str("    Description: Optional comma-separated security group IDs paired with LambdaSubnetIds.\n");
    output.push_str("Rules:\n");
    output.push_str("  VpcParametersArePaired:\n");
    output.push_str("    Assertions:\n");
    output.push_str("      - Assert: !Or\n");
    output.push_str("          - !And\n");
    output.push_str("            - !Equals [!Ref LambdaSubnetIds, '']\n");
    output.push_str("            - !Equals [!Ref LambdaSecurityGroupIds, '']\n");
    output.push_str("          - !And\n");
    output.push_str("            - !Not [!Equals [!Ref LambdaSubnetIds, '']]\n");
    output.push_str("            - !Not [!Equals [!Ref LambdaSecurityGroupIds, '']]\n");
    output.push_str(
        "        AssertDescription: LambdaSubnetIds and LambdaSecurityGroupIds must be set together.\n",
    );
    output.push_str("Conditions:\n");
    output.push_str(
        "  UsesCustomerManagedDatabaseKey: !Not [!Equals [!Ref DatabaseUrlKmsKeyArn, '']]\n",
    );
    output.push_str("  UsesVpc: !And\n");
    output.push_str("    - !Not [!Equals [!Ref LambdaSubnetIds, '']]\n");
    output.push_str("    - !Not [!Equals [!Ref LambdaSecurityGroupIds, '']]\n");
}

fn render_vpc_config(output: &mut String) {
    output.push_str("      VpcConfig: !If\n");
    output.push_str("        - UsesVpc\n");
    output.push_str("        - SubnetIds: !Split [',', !Ref LambdaSubnetIds]\n");
    output.push_str("          SecurityGroupIds: !Split [',', !Ref LambdaSecurityGroupIds]\n");
    output.push_str("        - !Ref AWS::NoValue\n");
}

fn render_database_environment(output: &mut String, function: &FunctionPlan) {
    output.push_str("          DATABASE_KIND: postgres\n");
    output.push_str("          DATABASE_URL_PARAMETER: !Ref DatabaseUrlParameterName\n");
    writeln!(
        output,
        "          DATABASE_MAX_CONNECTIONS: {}",
        yaml_quote(&function.database_connections_per_instance.to_string())
    )
    .expect("writing to String cannot fail");
}

fn render_database_policy_statements(output: &mut String) {
    output.push_str("            - Effect: Allow\n");
    output.push_str("              Action: [ssm:GetParameter]\n");
    output.push_str("              Resource: !Sub 'arn:${AWS::Partition}:ssm:${AWS::Region}:${AWS::AccountId}:parameter${DatabaseUrlParameterName}'\n");
    output.push_str("            - !If\n");
    output.push_str("              - UsesCustomerManagedDatabaseKey\n");
    output.push_str("              - Effect: Allow\n");
    output.push_str("                Action: [kms:Decrypt]\n");
    output.push_str("                Resource: !Ref DatabaseUrlKmsKeyArn\n");
    output.push_str("                Condition:\n");
    output.push_str("                  StringEquals:\n");
    output
        .push_str("                    kms:ViaService: !Sub 'ssm.${AWS::Region}.amazonaws.com'\n");
    output.push_str("                    kms:EncryptionContext:PARAMETER_ARN: !Sub 'arn:${AWS::Partition}:ssm:${AWS::Region}:${AWS::AccountId}:parameter${DatabaseUrlParameterName}'\n");
    output.push_str("              - !Ref AWS::NoValue\n");
    output.push_str("            - !If\n");
    output.push_str("              - UsesVpc\n");
    output.push_str("              - Effect: Allow\n");
    output.push_str("                Action:\n");
    output.push_str("                  - ec2:AssignPrivateIpAddresses\n");
    output.push_str("                  - ec2:CreateNetworkInterface\n");
    output.push_str("                  - ec2:DeleteNetworkInterface\n");
    output.push_str("                  - ec2:DescribeNetworkInterfaces\n");
    output.push_str("                  - ec2:DescribeSubnets\n");
    output.push_str("                  - ec2:UnassignPrivateIpAddresses\n");
    output.push_str("                Resource: '*'\n");
    output.push_str("              - !Ref AWS::NoValue\n");
}

fn render_queue(output: &mut String, plan: &DeploymentPlan, queue: &QueuePlan) {
    let resource = format!("{}Queue", sam_logical_id(&queue.id));
    writeln!(output, "  {resource}:").expect("writing to String cannot fail");
    output.push_str("    Type: AWS::SQS::Queue\n");
    output.push_str("    Properties:\n");
    let suffix = if queue.fifo { ".fifo" } else { "" };
    writeln!(
        output,
        "      QueueName: {}",
        yaml_quote(&format!(
            "{}-{}-{}{}",
            plan.application, plan.environment, queue.id, suffix
        ))
    )
    .expect("writing to String cannot fail");
    writeln!(output, "      FifoQueue: {}", queue.fifo).expect("writing to String cannot fail");
    output.push_str("      SqsManagedSseEnabled: true\n");
    writeln!(
        output,
        "      VisibilityTimeout: {}",
        queue.visibility_timeout_seconds
    )
    .expect("writing to String cannot fail");
    writeln!(
        output,
        "      MessageRetentionPeriod: {}",
        queue.retention_seconds
    )
    .expect("writing to String cannot fail");
    if let (Some(dead_letter_queue_id), Some(max_receive_count)) =
        (&queue.dead_letter_queue_id, queue.max_receive_count)
    {
        output.push_str("      RedrivePolicy:\n");
        writeln!(
            output,
            "        deadLetterTargetArn: !GetAtt {}Queue.Arn",
            sam_logical_id(dead_letter_queue_id)
        )
        .expect("writing to String cannot fail");
        writeln!(output, "        maxReceiveCount: {max_receive_count}")
            .expect("writing to String cannot fail");
    }
}

fn render_worker_function(
    output: &mut String,
    plan: &DeploymentPlan,
    function: &FunctionPlan,
    code_uris: &BTreeMap<String, String>,
) {
    let function_resource = format!("{}Function", sam_logical_id(&function.name));
    writeln!(output, "  {function_resource}:").expect("writing to String cannot fail");
    output.push_str("    Type: AWS::Serverless::Function\n");
    output.push_str("    Properties:\n");
    writeln!(
        output,
        "      FunctionName: {}",
        yaml_quote(&format!(
            "{}-{}-{}",
            plan.application, plan.environment, function.name
        ))
    )
    .expect("writing to String cannot fail");
    writeln!(
        output,
        "      CodeUri: {}",
        yaml_quote(
            code_uris
                .get(&function.name)
                .map_or(&function.artifact_path, String::as_str)
        )
    )
    .expect("writing to String cannot fail");
    output.push_str("      Handler: bootstrap\n");
    output.push_str("      Runtime: provided.al2023\n");
    output.push_str("      Architectures: [arm64]\n");
    writeln!(output, "      MemorySize: {}", function.memory_mb)
        .expect("writing to String cannot fail");
    writeln!(output, "      Timeout: {}", function.timeout_seconds)
        .expect("writing to String cannot fail");
    writeln!(
        output,
        "      ReservedConcurrentExecutions: {}",
        function.reserved_concurrency
    )
    .expect("writing to String cannot fail");
    if function.database_connections_per_instance > 0 {
        render_vpc_config(output);
    }
    output.push_str("      Environment:\n");
    output.push_str("        Variables:\n");
    writeln!(
        output,
        "          APP_ENV: {}",
        yaml_quote(&plan.environment)
    )
    .expect("writing to String cannot fail");
    if function.database_connections_per_instance > 0 {
        render_database_environment(output, function);
    }
    let has_sqs_trigger = plan.triggers.iter().any(|trigger| {
        matches!(
            trigger,
            TriggerPlan::Sqs { function_id, .. } if function_id == &function.name
        )
    });
    if function.database_connections_per_instance > 0 || has_sqs_trigger {
        output.push_str("      Policies:\n");
        output.push_str("        - Statement:\n");
        if function.database_connections_per_instance > 0 {
            render_database_policy_statements(output);
        }
    }
    for trigger in &plan.triggers {
        if let TriggerPlan::Sqs {
            function_id,
            queue_id,
            ..
        } = trigger
            && function_id == &function.name
        {
            output.push_str("            - Effect: Allow\n");
            output.push_str("              Action:\n");
            output.push_str("                - sqs:ChangeMessageVisibility\n");
            output.push_str("                - sqs:DeleteMessage\n");
            output.push_str("                - sqs:GetQueueAttributes\n");
            output.push_str("                - sqs:ReceiveMessage\n");
            writeln!(
                output,
                "              Resource: !GetAtt {}Queue.Arn",
                sam_logical_id(queue_id)
            )
            .expect("writing to String cannot fail");
        }
    }
    let triggers =
        plan.triggers
            .iter()
            .filter(|trigger| match trigger {
                TriggerPlan::Sqs { function_id, .. }
                | TriggerPlan::Schedule { function_id, .. } => function_id == &function.name,
                TriggerPlan::HttpApi { .. } => false,
            })
            .collect::<Vec<_>>();
    if !triggers.is_empty() {
        output.push_str("      Events:\n");
    }
    for trigger in triggers {
        match trigger {
            TriggerPlan::Sqs {
                id,
                queue_id,
                batch_size,
                batching_window_seconds,
                report_batch_item_failures,
                maximum_concurrency,
                ..
            } => {
                writeln!(output, "        {}Event:", sam_logical_id(id))
                    .expect("writing to String cannot fail");
                output.push_str("          Type: SQS\n");
                output.push_str("          Properties:\n");
                writeln!(
                    output,
                    "            Queue: !GetAtt {}Queue.Arn",
                    sam_logical_id(queue_id)
                )
                .expect("writing to String cannot fail");
                writeln!(output, "            BatchSize: {batch_size}")
                    .expect("writing to String cannot fail");
                writeln!(
                    output,
                    "            MaximumBatchingWindowInSeconds: {batching_window_seconds}"
                )
                .expect("writing to String cannot fail");
                output.push_str("            Enabled: true\n");
                if *report_batch_item_failures {
                    output.push_str("            FunctionResponseTypes:\n");
                    output.push_str("              - ReportBatchItemFailures\n");
                }
                output.push_str("            ScalingConfig:\n");
                writeln!(
                    output,
                    "              MaximumConcurrency: {maximum_concurrency}"
                )
                .expect("writing to String cannot fail");
            }
            TriggerPlan::Schedule {
                id,
                expression,
                enabled,
                purpose,
                ..
            } => {
                writeln!(output, "        {}Event:", sam_logical_id(id))
                    .expect("writing to String cannot fail");
                output.push_str("          Type: ScheduleV2\n");
                output.push_str("          Properties:\n");
                writeln!(
                    output,
                    "            ScheduleExpression: {}",
                    yaml_quote(expression)
                )
                .expect("writing to String cannot fail");
                writeln!(
                    output,
                    "            State: {}",
                    if *enabled { "ENABLED" } else { "DISABLED" }
                )
                .expect("writing to String cannot fail");
                writeln!(output, "            Description: {}", yaml_quote(purpose))
                    .expect("writing to String cannot fail");
                output.push_str("            FlexibleTimeWindow:\n");
                output.push_str("              Mode: 'OFF'\n");
            }
            TriggerPlan::HttpApi { .. } => {}
        }
    }
    let log_group = format!("{}LogGroup", sam_logical_id(&function.name));
    writeln!(output, "  {log_group}:").expect("writing to String cannot fail");
    output.push_str("    Type: AWS::Logs::LogGroup\n");
    output.push_str("    Properties:\n");
    writeln!(
        output,
        "      LogGroupName: !Sub '/aws/lambda/${{{function_resource}}}'"
    )
    .expect("writing to String cannot fail");
    writeln!(output, "      RetentionInDays: {}", plan.log_retention_days)
        .expect("writing to String cannot fail");
}

fn api_function(plan: &DeploymentPlan) -> Option<&FunctionPlan> {
    if plan.schema_version == 1 {
        plan.functions.first()
    } else {
        plan.functions
            .iter()
            .find(|function| matches!(function.role, FunctionRole::HttpApi))
    }
}

const fn method(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
        HttpMethod::Options => "OPTIONS",
        HttpMethod::Head => "HEAD",
    }
}

fn event_name(operation_id: &str) -> String {
    let mut output = String::new();
    let mut uppercase = true;
    for character in operation_id.chars() {
        if character.is_ascii_alphanumeric() {
            if uppercase {
                output.push(character.to_ascii_uppercase());
                uppercase = false;
            } else {
                output.push(character);
            }
        } else {
            uppercase = true;
        }
    }
    format!("{output}Event")
}

fn yaml_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AuthPlan, CostPolicy, DatabaseDeployment, FunctionPlan, FunctionRole, IngressPlan,
        NeonPlan, PerformancePolicy, RoutePlan, RuntimePlan,
    };

    #[test]
    fn renders_a_minimal_http_api_and_lambda() {
        let plan = DeploymentPlan {
            schema_version: 1,
            application: "demo".into(),
            environment: "dev".into(),
            region: "ap-southeast-2".into(),
            runtime: RuntimePlan::LambdaZipArm64,
            ingress: IngressPlan::ApiGatewayHttpApi,
            auth: AuthPlan::Jwt {
                issuer: "https://issuer.example.invalid".into(),
                audiences: vec!["orders".into()],
            },
            database: DatabaseDeployment::NeonPostgres {
                plan: NeonPlan::Free,
                compute_unit_hours: 1.0,
                storage_gb_month: 0.1,
                history_storage_gb_month: 0.0,
            },
            functions: vec![FunctionPlan {
                name: "api".into(),
                role: FunctionRole::HttpApi,
                artifact_path: "artifact.zip".into(),
                memory_mb: 512,
                timeout_seconds: 15,
                reserved_concurrency: 2,
                provisioned_concurrency: 0,
                database_connections_per_instance: 2,
            }],
            queues: Vec::new(),
            triggers: Vec::new(),
            iam_intents: Vec::new(),
            routes: vec![RoutePlan {
                operation_id: "getHealth".into(),
                method: HttpMethod::Get,
                path: "/health".into(),
                authenticated: false,
            }],
            application_graph: minco_core::ApplicationGraph::default(),
            local_aws_services: vec!["ssm".into(), "sts".into()],
            scheduled_wakeups: Vec::new(),
            uses_nat_gateway: false,
            allowed_origins: vec!["https://app.example.invalid".into()],
            allowed_headers: vec![
                "authorization".into(),
                "content-type".into(),
                "idempotency-key".into(),
                "x-request-id".into(),
            ],
            log_retention_days: 14,
            cost_policy: CostPolicy::default(),
            performance_policy: PerformancePolicy::default(),
        };
        let yaml = render_sam(&plan).expect("SAM");
        assert!(yaml.contains("AWS::Serverless::HttpApi"));
        assert!(yaml.contains("GetHealthEvent"));
        assert!(yaml.contains("Authorizer: NONE"));
        assert!(!yaml.contains("AllowOrigins: ['*']"));
        assert!(yaml.contains("UsesCustomerManagedDatabaseKey"));
        assert!(yaml.contains("Resource: !Ref DatabaseUrlKmsKeyArn"));
        assert!(yaml.contains("mrk-[A-Fa-f0-9]+"));
        assert!(yaml.contains("kms:EncryptionContext:PARAMETER_ARN"));
        assert!(!yaml.contains("Action: [kms:Decrypt]\n              Resource: '*'"));
        assert!(yaml.contains("UsesVpc: !And"));
        assert!(yaml.contains("VpcParametersArePaired"));
        assert!(yaml.contains(
            "AssertDescription: LambdaSubnetIds and LambdaSecurityGroupIds must be set together."
        ));
        assert!(yaml.contains("VpcConfig: !If"));
        assert!(yaml.contains("SubnetIds: !Split [',', !Ref LambdaSubnetIds]"));
        assert!(yaml.contains("ec2:CreateNetworkInterface"));
        assert!(!yaml.contains("AWS::EC2::NatGateway"));

        let relocated =
            render_sam_with_code_uri(&plan, Some("../../../artifact.zip")).expect("SAM");
        assert!(relocated.contains("CodeUri: '../../../artifact.zip'"));
    }
}
