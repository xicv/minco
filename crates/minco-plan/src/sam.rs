use crate::{AuthPlan, DatabaseDeployment, DeploymentPlan, IngressPlan, PlanError, RuntimePlan};
use minco_contract::HttpMethod;
use std::fmt::Write as _;

pub fn render_sam(plan: &DeploymentPlan) -> Result<String, PlanError> {
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
    let function = plan.functions.first().ok_or(PlanError::MissingFunction)?;
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
    output.push_str("Parameters:\n");
    output.push_str("  DatabaseUrlParameterName:\n");
    output.push_str("    Type: String\n");
    output.push_str("    Description: Existing SSM SecureString containing the pooled runtime PostgreSQL URL.\n");
    output.push_str("Resources:\n");
    output.push_str("  HttpApi:\n");
    output.push_str("    Type: AWS::Serverless::HttpApi\n");
    output.push_str("    Properties:\n");
    output.push_str("      StageName: '$default'\n");
    output.push_str("      CorsConfiguration:\n");
    output.push_str("        AllowMethods: [GET, POST, PUT, PATCH, DELETE, OPTIONS]\n");
    output.push_str(
        "        AllowHeaders: [Authorization, Content-Type, Idempotency-Key, X-Request-Id]\n",
    );
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
        yaml_quote(&function.artifact_path)
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
    output.push_str("      Environment:\n");
    output.push_str("        Variables:\n");
    writeln!(
        output,
        "          APP_ENV: {}",
        yaml_quote(&plan.environment)
    )
    .expect("writing to String cannot fail");
    output.push_str("          DATABASE_KIND: postgres\n");
    output.push_str("          DATABASE_URL_PARAMETER: !Ref DatabaseUrlParameterName\n");
    writeln!(
        output,
        "          DATABASE_MAX_CONNECTIONS: {}",
        yaml_quote(&function.database_connections_per_instance.to_string())
    )
    .expect("writing to String cannot fail");
    writeln!(
        output,
        "          ALLOWED_ORIGINS: {}",
        yaml_quote(&plan.allowed_origins.join(","))
    )
    .expect("writing to String cannot fail");
    output.push_str("          ALLOW_DEVELOPMENT_HEADERS: 'false'\n");
    output.push_str("      Policies:\n");
    output.push_str("        - Statement:\n");
    output.push_str("            - Effect: Allow\n");
    output.push_str("              Action: [ssm:GetParameter]\n");
    output.push_str("              Resource: !Sub 'arn:${AWS::Partition}:ssm:${AWS::Region}:${AWS::AccountId}:parameter${DatabaseUrlParameterName}'\n");
    output.push_str("            - Effect: Allow\n");
    output.push_str("              Action: [kms:Decrypt]\n");
    output.push_str("              Resource: '*'\n");
    output.push_str("              Condition:\n");
    output.push_str("                StringEquals:\n");
    output.push_str("                  kms:ViaService: !Sub 'ssm.${AWS::Region}.amazonaws.com'\n");
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
        AuthPlan, CostPolicy, DatabaseDeployment, FunctionPlan, IngressPlan, NeonPlan,
        PerformancePolicy, RoutePlan, RuntimePlan,
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
                artifact_path: "artifact.zip".into(),
                memory_mb: 512,
                timeout_seconds: 15,
                reserved_concurrency: 2,
                provisioned_concurrency: 0,
                database_connections_per_instance: 2,
            }],
            routes: vec![RoutePlan {
                operation_id: "getHealth".into(),
                method: HttpMethod::Get,
                path: "/health".into(),
                authenticated: false,
            }],
            scheduled_wakeups: Vec::new(),
            uses_nat_gateway: false,
            allowed_origins: vec!["https://app.example.invalid".into()],
            log_retention_days: 14,
            cost_policy: CostPolicy::default(),
            performance_policy: PerformancePolicy::default(),
        };
        let yaml = render_sam(&plan).expect("SAM");
        assert!(yaml.contains("AWS::Serverless::HttpApi"));
        assert!(yaml.contains("GetHealthEvent"));
        assert!(yaml.contains("Authorizer: NONE"));
        assert!(!yaml.contains("AllowOrigins: ['*']"));
    }
}
