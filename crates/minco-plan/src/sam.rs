use crate::{
    AuthPlan, DatabaseDeployment, DeploymentPlan, DynamoDbDeletionPolicy, DynamoDbProjection,
    DynamoDbTablePlan, FunctionPlan, FunctionRole, IngressPlan, PlanError, QueuePlan,
    RealtimeDeployment, RuntimePlan, StaticSiteDeployment, TriggerPlan, oidc_client_id_pattern,
    realtime_oidc_auth_is_valid, sam_logical_id,
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
    let postgres_database = matches!(
        &plan.database,
        DatabaseDeployment::NeonPostgres { .. }
            | DatabaseDeployment::SelfHostedPostgres { .. }
            | DatabaseDeployment::RdsPostgres { .. }
            | DatabaseDeployment::AuroraServerlessV2 { .. }
    );
    if !postgres_database && plan.database.dynamodb_table().is_none() {
        return Err(PlanError::UnsupportedDeployment(
            "this renderer requires an externally provisioned PostgreSQL-compatible database; DynamoDB needs a dedicated adapter/rendering plugin and mutable SQLite is rejected on Lambda".into(),
        ));
    }
    if plan.triggers.iter().any(|trigger| {
        matches!(
            trigger,
            TriggerPlan::Schedule {
                cleanup: Some(_),
                ..
            }
        )
    }) {
        return Err(PlanError::UnsupportedDeployment(
            "EventBridge Scheduler ActionAfterCompletion is not exposed by the current AWS SAM ScheduleV2 or AWS::Scheduler::Schedule CloudFormation schemas; apply requires a future guarded Scheduler API operation and receipt".into(),
        ));
    }
    if let Some(realtime) = &plan.realtime
        && (!realtime.is_valid() || !realtime_oidc_auth_is_valid(&plan.auth))
    {
        return Err(PlanError::UnsupportedDeployment(
            "AppSync Events requires valid realtime policy and jwt subscriber authentication"
                .into(),
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
    let uses_audit_database_parameter = uses_database_parameter
        && plan
            .application_graph
            .capabilities
            .contains_key("audit.ledger");
    output.push_str("Parameters:\n");
    render_live_function_version_parameter(&mut output);
    if uses_database_parameter {
        render_database_parameters(&mut output, uses_audit_database_parameter);
    }
    if let Some(static_site) = &plan.static_site {
        render_static_site_parameters(&mut output, static_site);
    }
    output.push_str("Conditions:\n");
    output.push_str(
        "  LiveFunctionVersionIsCandidate: !Equals [!Ref LiveFunctionVersion, 'candidate']\n",
    );
    if uses_database_parameter {
        render_database_conditions(&mut output, uses_audit_database_parameter);
    }
    output.push_str("Resources:\n");
    if let Some(table) = plan.database.dynamodb_table() {
        render_dynamodb_table(&mut output, table, false);
    }
    if let Some(table) = plan.dynamodb_audit_table() {
        render_dynamodb_table(&mut output, &table, true);
    }
    if let Some(static_site) = &plan.static_site {
        render_static_site_resources(&mut output, static_site);
    }
    if let Some(realtime) = &plan.realtime {
        render_realtime_resources(&mut output, plan, realtime);
    }
    output.push_str("  HttpApi:\n");
    output.push_str("    Type: AWS::Serverless::HttpApi\n");
    output.push_str("    Properties:\n");
    output.push_str("      StageName: '$default'\n");
    output.push_str("      StageVariables:\n");
    output.push_str("        lambdaAlias: 'live'\n");
    output.push_str("      CorsConfiguration:\n");
    output.push_str("        AllowMethods: [GET, POST, PUT, PATCH, DELETE, OPTIONS]\n");
    output.push_str("        AllowHeaders:\n");
    for header in &plan.allowed_headers {
        writeln!(output, "          - {}", yaml_quote(header))
            .expect("writing to String cannot fail");
    }
    output.push_str("        ExposeHeaders:\n");
    for header in &plan.exposed_headers {
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
    render_http_api_definition(&mut output, plan);
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
    output.push_str("      AutoPublishAlias: candidate\n");
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
        render_database_environment(&mut output, function, uses_audit_database_parameter);
    }
    if let Some(table) = plan.database.dynamodb_table()
        && table.function_id == function.name
    {
        render_dynamodb_environment(&mut output, table);
    }
    if let Some(table) = plan.dynamodb_audit_table()
        && table.function_id == function.name
    {
        render_dynamodb_audit_environment(&mut output, &table);
    }
    writeln!(
        output,
        "          ALLOWED_ORIGINS: {}",
        yaml_quote(&plan.allowed_origins.join(","))
    )
    .expect("writing to String cannot fail");
    output.push_str("          ALLOW_DEVELOPMENT_HEADERS: 'false'\n");
    if let Some(realtime) = &plan.realtime {
        output.push_str("          MINCO_REALTIME_HTTP_ENDPOINT: !Sub 'https://${RealtimeApi.Dns.Http}/event'\n");
        writeln!(
            output,
            "          MINCO_REALTIME_NAMESPACE: {}",
            yaml_quote(&realtime.namespace)
        )
        .expect("writing to String cannot fail");
    }
    let uses_dynamodb = plan
        .database
        .dynamodb_table()
        .is_some_and(|table| table.function_id == function.name)
        || plan
            .dynamodb_audit_table()
            .is_some_and(|table| table.function_id == function.name);
    if function.database_connections_per_instance > 0 || uses_dynamodb || plan.realtime.is_some() {
        output.push_str("      Policies:\n");
        output.push_str("        - Statement:\n");
        if function.database_connections_per_instance > 0 {
            render_database_policy_statements(&mut output, uses_audit_database_parameter);
        }
        if let Some(table) = plan.database.dynamodb_table()
            && table.function_id == function.name
        {
            render_dynamodb_policy_statement(&mut output, table);
        }
        if let Some(table) = plan.dynamodb_audit_table()
            && table.function_id == function.name
        {
            render_dynamodb_audit_policy_statement(&mut output, &table);
        }
        if let Some(realtime) = &plan.realtime {
            render_realtime_policy_statement(&mut output, realtime);
        }
    }
    output.push_str("  CandidateApiInvokePermission:\n");
    output.push_str("    Type: AWS::Lambda::Permission\n");
    output.push_str("    Properties:\n");
    output.push_str("      Action: lambda:InvokeFunction\n");
    output.push_str("      FunctionName: !Ref ApiFunction.Alias\n");
    output.push_str("      Principal: apigateway.amazonaws.com\n");
    output.push_str("      SourceArn: !Sub 'arn:${AWS::Partition}:execute-api:${AWS::Region}:${AWS::AccountId}:${HttpApi}/*'\n");
    output.push_str("  LiveFunctionAlias:\n");
    output.push_str("    Type: AWS::Lambda::Alias\n");
    output.push_str("    Properties:\n");
    output.push_str(
        "      Description: !Sub 'Minco live API routing target ${LiveFunctionVersion}'\n",
    );
    output.push_str("      FunctionName: !Ref ApiFunction\n");
    output.push_str("      FunctionVersion: !If\n");
    output.push_str("        - LiveFunctionVersionIsCandidate\n");
    output.push_str("        - !GetAtt ApiFunction.Version.Version\n");
    output.push_str("        - !Ref LiveFunctionVersion\n");
    output.push_str("      Name: live\n");
    output.push_str("  LiveApiInvokePermission:\n");
    output.push_str("    Type: AWS::Lambda::Permission\n");
    output.push_str("    Properties:\n");
    output.push_str("      Action: lambda:InvokeFunction\n");
    output.push_str("      FunctionName: !Ref LiveFunctionAlias\n");
    output.push_str("      Principal: apigateway.amazonaws.com\n");
    output.push_str("      SourceArn: !Sub 'arn:${AWS::Partition}:execute-api:${AWS::Region}:${AWS::AccountId}:${HttpApi}/*'\n");
    output.push_str("  CandidateStage:\n");
    output.push_str("    Type: AWS::ApiGatewayV2::Stage\n");
    output.push_str("    Properties:\n");
    output.push_str("      ApiId: !Ref HttpApi\n");
    output.push_str("      AutoDeploy: true\n");
    output.push_str("      StageName: candidate\n");
    output.push_str("      StageVariables:\n");
    output.push_str("        lambdaAlias: 'candidate'\n");
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
    output.push_str("  CandidateApiUrl:\n");
    output.push_str(
        "    Value: !Sub 'https://${HttpApi}.execute-api.${AWS::Region}.${AWS::URLSuffix}/candidate'\n",
    );
    output.push_str("  ApiFunctionName:\n");
    output.push_str("    Value: !Ref ApiFunction\n");
    output.push_str("  CandidateAliasName:\n");
    output.push_str("    Value: candidate\n");
    if let Some(static_site) = &plan.static_site {
        render_static_site_outputs(&mut output, static_site);
    }
    if let Some(realtime) = &plan.realtime {
        render_realtime_outputs(&mut output, realtime);
    }
    Ok(output)
}

fn render_realtime_resources(
    output: &mut String,
    deployment: &DeploymentPlan,
    realtime: &RealtimeDeployment,
) {
    let AuthPlan::Jwt { issuer, audiences } = &deployment.auth else {
        return;
    };
    output.push_str("  RealtimeApi:\n");
    output.push_str("    Type: AWS::AppSync::Api\n");
    output.push_str("    Properties:\n");
    output.push_str("      Name: !Sub '${AWS::StackName}-realtime'\n");
    output.push_str("      EventConfig:\n");
    output.push_str("        AuthProviders:\n");
    output.push_str("          - AuthType: AWS_IAM\n");
    output.push_str("          - AuthType: OPENID_CONNECT\n");
    output.push_str("            OpenIDConnectConfig:\n");
    writeln!(output, "              Issuer: {}", yaml_quote(issuer))
        .expect("writing to String cannot fail");
    writeln!(
        output,
        "              ClientId: {}",
        yaml_quote(&oidc_client_id_pattern(audiences))
    )
    .expect("writing to String cannot fail");
    output.push_str("        ConnectionAuthModes:\n");
    output.push_str("          - AuthType: OPENID_CONNECT\n");
    output.push_str("        DefaultPublishAuthModes:\n");
    output.push_str("          - AuthType: AWS_IAM\n");
    output.push_str("        DefaultSubscribeAuthModes:\n");
    output.push_str("          - AuthType: OPENID_CONNECT\n");
    output.push_str("  RealtimeNamespace:\n");
    output.push_str("    Type: AWS::AppSync::ChannelNamespace\n");
    output.push_str("    Properties:\n");
    output.push_str("      ApiId: !GetAtt RealtimeApi.ApiId\n");
    writeln!(output, "      Name: {}", yaml_quote(&realtime.namespace))
        .expect("writing to String cannot fail");
    output.push_str("      CodeHandlers: |\n");
    output.push_str("        import { util } from '@aws-appsync/utils'\n");
    writeln!(
        output,
        "        const CLAIM = {}",
        yaml_quote(&realtime.subscriber_claim)
    )
    .expect("writing to String cannot fail");
    output.push_str("        export function onSubscribe(ctx) {\n");
    output.push_str("          const segments = ctx.info.channel.segments\n");
    output.push_str("          const claim = ctx.identity && ctx.identity.claims && ctx.identity.claims[CLAIM]\n");
    output.push_str("          if (typeof claim !== 'string' || segments.length < 2 || segments[1] !== claim) {\n");
    output.push_str("            util.unauthorized()\n");
    output.push_str("          }\n");
    output.push_str("        }\n");
    output.push_str("      PublishAuthModes:\n");
    output.push_str("        - AuthType: AWS_IAM\n");
    output.push_str("      SubscribeAuthModes:\n");
    output.push_str("        - AuthType: OPENID_CONNECT\n");
}

fn render_realtime_policy_statement(output: &mut String, realtime: &RealtimeDeployment) {
    output.push_str("            - Sid: MincoRealtimePublication\n");
    output.push_str("              Effect: Allow\n");
    output.push_str("              Action: appsync:EventPublish\n");
    writeln!(
        output,
        "              Resource: !Sub 'arn:${{AWS::Partition}}:appsync:${{AWS::Region}}:${{AWS::AccountId}}:apis/${{RealtimeApi.ApiId}}/channelNamespace/{}'",
        realtime.namespace
    )
    .expect("writing to String cannot fail");
}

fn render_realtime_outputs(output: &mut String, realtime: &RealtimeDeployment) {
    output.push_str("  RealtimeHttpEndpoint:\n");
    output.push_str("    Value: !Sub 'https://${RealtimeApi.Dns.Http}/event'\n");
    output.push_str("  RealtimeWebSocketEndpoint:\n");
    output.push_str("    Value: !Sub 'wss://${RealtimeApi.Dns.Realtime}/event/realtime'\n");
    output.push_str("  RealtimeNamespaceName:\n");
    writeln!(output, "    Value: {}", yaml_quote(&realtime.namespace))
        .expect("writing to String cannot fail");
}

fn render_static_site_parameters(output: &mut String, plan: &StaticSiteDeployment) {
    if plan.custom_domain.is_some() {
        output.push_str("  StaticSiteCertificateArn:\n");
        output.push_str("    Type: String\n");
        output.push_str(
            "    AllowedPattern: '^arn:[a-z0-9-]+:acm:us-east-1:[0-9]{12}:certificate/.+$'\n",
        );
        output.push_str("    Description: Existing ISSUED us-east-1 ACM certificate covering the exact static-site alias.\n");
    }
    if plan.manage_dns_alias {
        output.push_str("  StaticSiteHostedZoneId:\n");
        output.push_str("    Type: String\n");
        output.push_str("    AllowedPattern: '^Z[A-Z0-9]+$'\n");
        output.push_str("    Description: Existing Route 53 public hosted zone that owns the configured alias.\n");
    }
}

fn render_static_site_resources(output: &mut String, plan: &StaticSiteDeployment) {
    output.push_str("  StaticSiteBucket:\n");
    output.push_str("    Type: AWS::S3::Bucket\n");
    output.push_str("    DeletionPolicy: Retain\n");
    output.push_str("    UpdateReplacePolicy: Retain\n");
    output.push_str("    Properties:\n");
    output.push_str("      BucketEncryption:\n");
    output.push_str("        ServerSideEncryptionConfiguration:\n");
    output.push_str("          - ServerSideEncryptionByDefault:\n");
    output.push_str("              SSEAlgorithm: AES256\n");
    output.push_str("      OwnershipControls:\n");
    output.push_str("        Rules:\n");
    output.push_str("          - ObjectOwnership: BucketOwnerEnforced\n");
    output.push_str("      PublicAccessBlockConfiguration:\n");
    output.push_str("        BlockPublicAcls: true\n");
    output.push_str("        BlockPublicPolicy: true\n");
    output.push_str("        IgnorePublicAcls: true\n");
    output.push_str("        RestrictPublicBuckets: true\n");
    output.push_str("  StaticSiteOriginAccessControl:\n");
    output.push_str("    Type: AWS::CloudFront::OriginAccessControl\n");
    output.push_str("    Properties:\n");
    output.push_str("      OriginAccessControlConfig:\n");
    output.push_str("        Description: Minco private static-site S3 origin\n");
    output.push_str("        Name: !Sub 'minco-static-${AWS::StackName}'\n");
    output.push_str("        OriginAccessControlOriginType: s3\n");
    output.push_str("        SigningBehavior: always\n");
    output.push_str("        SigningProtocol: sigv4\n");
    output.push_str("  StaticSiteCachePolicy:\n");
    output.push_str("    Type: AWS::CloudFront::CachePolicy\n");
    output.push_str("    Properties:\n");
    output.push_str("      CachePolicyConfig:\n");
    output.push_str("        Comment: Minco static-site policy; origin Cache-Control metadata remains authoritative\n");
    output.push_str("        DefaultTTL: 0\n");
    writeln!(output, "        MaxTTL: {}", plan.immutable_cache_seconds)
        .expect("writing to String cannot fail");
    output.push_str("        MinTTL: 0\n");
    output.push_str("        Name: !Sub 'minco-static-${AWS::StackName}'\n");
    output.push_str("        ParametersInCacheKeyAndForwardedToOrigin:\n");
    output.push_str("          CookiesConfig:\n");
    output.push_str("            CookieBehavior: none\n");
    output.push_str("          EnableAcceptEncodingBrotli: true\n");
    output.push_str("          EnableAcceptEncodingGzip: true\n");
    output.push_str("          HeadersConfig:\n");
    output.push_str("            HeaderBehavior: none\n");
    output.push_str("          QueryStringsConfig:\n");
    output.push_str("            QueryStringBehavior: none\n");
    output.push_str("  StaticSiteDistribution:\n");
    output.push_str("    Type: AWS::CloudFront::Distribution\n");
    output.push_str("    Properties:\n");
    output.push_str("      DistributionConfig:\n");
    if let Some(domain) = &plan.custom_domain {
        output.push_str("        Aliases:\n");
        writeln!(output, "          - {}", yaml_quote(domain))
            .expect("writing to String cannot fail");
    } else {
        output.push_str("        Aliases: []\n");
    }
    output.push_str("        Comment: !Sub 'Minco static site ${AWS::StackName}'\n");
    if plan.spa_fallback {
        output.push_str("        CustomErrorResponses:\n");
        for error_code in [403, 404] {
            writeln!(output, "          - ErrorCode: {error_code}")
                .expect("writing to String cannot fail");
            output.push_str("            ResponseCode: 200\n");
            writeln!(
                output,
                "            ResponsePagePath: {}",
                yaml_quote(&format!("/{}", plan.index_document))
            )
            .expect("writing to String cannot fail");
            output.push_str("            ErrorCachingMinTTL: 0\n");
        }
    }
    output.push_str("        DefaultCacheBehavior:\n");
    output.push_str("          AllowedMethods: [GET, HEAD, OPTIONS]\n");
    output.push_str("          CachedMethods: [GET, HEAD]\n");
    output.push_str("          CachePolicyId: !Ref StaticSiteCachePolicy\n");
    output.push_str("          Compress: true\n");
    output.push_str("          TargetOriginId: StaticSiteS3Origin\n");
    output.push_str("          ViewerProtocolPolicy: redirect-to-https\n");
    writeln!(
        output,
        "        DefaultRootObject: {}",
        yaml_quote(&plan.index_document)
    )
    .expect("writing to String cannot fail");
    output.push_str("        Enabled: true\n");
    output.push_str("        HttpVersion: http2and3\n");
    writeln!(output, "        IPV6Enabled: {}", plan.ipv6_enabled)
        .expect("writing to String cannot fail");
    output.push_str("        Origins:\n");
    output.push_str("          - DomainName: !GetAtt StaticSiteBucket.RegionalDomainName\n");
    output.push_str("            Id: StaticSiteS3Origin\n");
    output.push_str("            OriginAccessControlId: !Ref StaticSiteOriginAccessControl\n");
    output.push_str("            S3OriginConfig:\n");
    output.push_str("              OriginAccessIdentity: ''\n");
    writeln!(output, "        PriceClass: {}", plan.price_class)
        .expect("writing to String cannot fail");
    output.push_str("        ViewerCertificate:\n");
    if plan.custom_domain.is_some() {
        output.push_str("          AcmCertificateArn: !Ref StaticSiteCertificateArn\n");
        output.push_str("          MinimumProtocolVersion: TLSv1.2_2021\n");
        output.push_str("          SslSupportMethod: sni-only\n");
    } else {
        output.push_str("          CloudFrontDefaultCertificate: true\n");
    }
    output.push_str("  StaticSiteBucketPolicy:\n");
    output.push_str("    Type: AWS::S3::BucketPolicy\n");
    output.push_str("    Properties:\n");
    output.push_str("      Bucket: !Ref StaticSiteBucket\n");
    output.push_str("      PolicyDocument:\n");
    output.push_str("        Version: '2012-10-17'\n");
    output.push_str("        Statement:\n");
    output.push_str("          - Sid: AllowCloudFrontReadOnly\n");
    output.push_str("            Effect: Allow\n");
    output.push_str("            Principal:\n");
    output.push_str("              Service: cloudfront.amazonaws.com\n");
    output.push_str("            Action: s3:GetObject\n");
    output.push_str("            Resource: !Sub '${StaticSiteBucket.Arn}/*'\n");
    output.push_str("            Condition:\n");
    output.push_str("              StringEquals:\n");
    output.push_str("                AWS:SourceArn: !Sub 'arn:${AWS::Partition}:cloudfront::${AWS::AccountId}:distribution/${StaticSiteDistribution}'\n");
    if plan.manage_dns_alias {
        render_static_site_dns_record(output, "StaticSiteDnsAlias", "A", plan);
        if plan.ipv6_enabled {
            render_static_site_dns_record(output, "StaticSiteDnsIpv6Alias", "AAAA", plan);
        }
    }
}

fn render_static_site_dns_record(
    output: &mut String,
    logical_id: &str,
    record_type: &str,
    plan: &StaticSiteDeployment,
) {
    writeln!(output, "  {logical_id}:").expect("writing to String cannot fail");
    output.push_str("    Type: AWS::Route53::RecordSet\n");
    output.push_str("    Properties:\n");
    output.push_str("      AliasTarget:\n");
    output.push_str("        DNSName: !GetAtt StaticSiteDistribution.DomainName\n");
    output.push_str("        EvaluateTargetHealth: false\n");
    output.push_str("        HostedZoneId: Z2FDTNDATAQYW2\n");
    output.push_str("      HostedZoneId: !Ref StaticSiteHostedZoneId\n");
    writeln!(
        output,
        "      Name: {}",
        yaml_quote(
            plan.custom_domain
                .as_deref()
                .expect("validated custom domain")
        )
    )
    .expect("writing to String cannot fail");
    writeln!(output, "      Type: {record_type}").expect("writing to String cannot fail");
}

fn render_static_site_outputs(output: &mut String, plan: &StaticSiteDeployment) {
    output.push_str("  StaticSiteBucketName:\n");
    output.push_str("    Value: !Ref StaticSiteBucket\n");
    output.push_str("  StaticSiteDistributionId:\n");
    output.push_str("    Value: !Ref StaticSiteDistribution\n");
    output.push_str("  StaticSiteDistributionDomainName:\n");
    output.push_str("    Value: !GetAtt StaticSiteDistribution.DomainName\n");
    output.push_str("  StaticSiteUrl:\n");
    if let Some(domain) = &plan.custom_domain {
        writeln!(
            output,
            "    Value: {}",
            yaml_quote(&format!("https://{domain}"))
        )
        .expect("writing to String cannot fail");
    } else {
        output.push_str("    Value: !Sub 'https://${StaticSiteDistribution.DomainName}'\n");
    }
}

fn render_live_function_version_parameter(output: &mut String) {
    output.push_str("  LiveFunctionVersion:\n");
    output.push_str("    Type: String\n");
    output.push_str("    Default: candidate\n");
    output.push_str("    AllowedPattern: '^(candidate|[1-9][0-9]*)$'\n");
    output.push_str(
        "    Description: Published API function version receiving live traffic; candidate is allowed only for initial deployment.\n",
    );
}

fn render_database_parameters(output: &mut String, uses_audit_database_parameter: bool) {
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
    if uses_audit_database_parameter {
        output.push_str("  AuditDatabaseUrlParameterName:\n");
        output.push_str("    Type: String\n");
        output.push_str("    AllowedPattern: '^/[A-Za-z0-9_.\\-/]+$'\n");
        output.push_str("    Description: Existing SSM SecureString containing the physically separate audit PostgreSQL URL.\n");
        output.push_str("  AuditDatabaseUrlKmsKeyArn:\n");
        output.push_str("    Type: String\n");
        output.push_str("    Default: ''\n");
        output.push_str(
            "    AllowedPattern: '^$|^arn:[a-z0-9-]+:kms:[a-z0-9-]+:[0-9]{12}:key/([A-Fa-f0-9-]+|mrk-[A-Fa-f0-9]+)$'\n",
        );
        output.push_str("    Description: Customer-managed KMS key ARN for the audit database parameter; leave empty only when the AWS-managed aws/ssm key is used.\n");
    }
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
}

fn render_database_conditions(output: &mut String, uses_audit_database_parameter: bool) {
    output.push_str(
        "  UsesCustomerManagedDatabaseKey: !Not [!Equals [!Ref DatabaseUrlKmsKeyArn, '']]\n",
    );
    if uses_audit_database_parameter {
        output.push_str("  UsesCustomerManagedAuditDatabaseKey: !Not [!Equals [!Ref AuditDatabaseUrlKmsKeyArn, '']]\n");
    }
    output.push_str("  UsesVpc: !And\n");
    output.push_str("    - !Not [!Equals [!Ref LambdaSubnetIds, '']]\n");
    output.push_str("    - !Not [!Equals [!Ref LambdaSecurityGroupIds, '']]\n");
}

fn render_http_api_definition(output: &mut String, plan: &DeploymentPlan) {
    output.push_str("      DefinitionBody:\n");
    output.push_str("        openapi: '3.0.1'\n");
    output.push_str("        info:\n");
    writeln!(
        output,
        "          title: {}",
        yaml_quote(&format!(
            "{}-{}-promotion-api",
            plan.application, plan.environment
        ))
    )
    .expect("writing to String cannot fail");
    output.push_str("          version: '1.0'\n");
    output.push_str("        paths:\n");
    let mut routes_by_path = BTreeMap::<&str, Vec<_>>::new();
    for route in &plan.routes {
        routes_by_path
            .entry(route.path.as_str())
            .or_default()
            .push(route);
    }
    for (path, routes) in routes_by_path {
        writeln!(output, "          {}:", yaml_quote(path)).expect("writing to String cannot fail");
        for route in routes {
            writeln!(
                output,
                "            {}:",
                method(route.method).to_ascii_lowercase()
            )
            .expect("writing to String cannot fail");
            writeln!(
                output,
                "              operationId: {}",
                yaml_quote(&route.operation_id)
            )
            .expect("writing to String cannot fail");
            output.push_str("              responses:\n");
            output.push_str("                default:\n");
            output.push_str("                  description: Lambda proxy response\n");
            output.push_str("              x-amazon-apigateway-integration:\n");
            output.push_str("                httpMethod: POST\n");
            output.push_str("                payloadFormatVersion: '2.0'\n");
            output.push_str("                type: aws_proxy\n");
            output.push_str("                uri: !Sub 'arn:${AWS::Partition}:apigateway:${AWS::Region}:lambda:path/2015-03-31/functions/arn:${AWS::Partition}:lambda:${AWS::Region}:${AWS::AccountId}:function:${ApiFunction}:${!stageVariables.lambdaAlias}/invocations'\n");
            if matches!(&plan.auth, AuthPlan::Jwt { .. }) {
                if route.authenticated {
                    output.push_str("              security:\n");
                    output.push_str("                - JwtAuthorizer: []\n");
                } else {
                    output.push_str("              security: []\n");
                }
            }
        }
    }
}

fn render_vpc_config(output: &mut String) {
    output.push_str("      VpcConfig: !If\n");
    output.push_str("        - UsesVpc\n");
    output.push_str("        - SubnetIds: !Split [',', !Ref LambdaSubnetIds]\n");
    output.push_str("          SecurityGroupIds: !Split [',', !Ref LambdaSecurityGroupIds]\n");
    output.push_str("        - !Ref AWS::NoValue\n");
}

fn render_dynamodb_table(
    output: &mut String,
    table: &DynamoDbTablePlan,
    deletion_protection: bool,
) {
    writeln!(output, "  {}:", table.logical_id).expect("writing to String cannot fail");
    output.push_str("    Type: AWS::DynamoDB::Table\n");
    let deletion_policy = match table.deletion_policy {
        DynamoDbDeletionPolicy::Delete => "Delete",
        DynamoDbDeletionPolicy::Retain => "Retain",
    };
    writeln!(output, "    DeletionPolicy: {deletion_policy}")
        .expect("writing to String cannot fail");
    writeln!(output, "    UpdateReplacePolicy: {deletion_policy}")
        .expect("writing to String cannot fail");
    output.push_str("    Properties:\n");
    output.push_str("      BillingMode: PAY_PER_REQUEST\n");
    writeln!(
        output,
        "      DeletionProtectionEnabled: {deletion_protection}"
    )
    .expect("writing to String cannot fail");
    output.push_str("      AttributeDefinitions:\n");
    let mut attributes = BTreeMap::new();
    attributes.insert(
        table.partition_key.name.as_str(),
        table.partition_key.scalar_type,
    );
    if let Some(sort_key) = &table.sort_key {
        attributes.insert(sort_key.name.as_str(), sort_key.scalar_type);
    }
    for index in &table.global_secondary_indexes {
        attributes.insert(
            index.partition_key.name.as_str(),
            index.partition_key.scalar_type,
        );
        if let Some(sort_key) = &index.sort_key {
            attributes.insert(sort_key.name.as_str(), sort_key.scalar_type);
        }
    }
    for (name, scalar_type) in attributes {
        writeln!(output, "        - AttributeName: {}", yaml_quote(name))
            .expect("writing to String cannot fail");
        writeln!(
            output,
            "          AttributeType: {}",
            scalar_type.cloudformation_code()
        )
        .expect("writing to String cannot fail");
    }
    output.push_str("      KeySchema:\n");
    render_dynamodb_key_schema(
        output,
        &table.partition_key.name,
        table.sort_key.as_ref().map(|key| key.name.as_str()),
        8,
    );
    if !table.global_secondary_indexes.is_empty() {
        output.push_str("      GlobalSecondaryIndexes:\n");
        for index in &table.global_secondary_indexes {
            writeln!(output, "        - IndexName: {}", yaml_quote(&index.name))
                .expect("writing to String cannot fail");
            output.push_str("          KeySchema:\n");
            render_dynamodb_key_schema(
                output,
                &index.partition_key.name,
                index.sort_key.as_ref().map(|key| key.name.as_str()),
                12,
            );
            output.push_str("          Projection:\n");
            let projection = match index.projection {
                DynamoDbProjection::All => "ALL",
                DynamoDbProjection::KeysOnly => "KEYS_ONLY",
            };
            writeln!(output, "            ProjectionType: {projection}")
                .expect("writing to String cannot fail");
        }
    }
    output.push_str("      PointInTimeRecoverySpecification:\n");
    writeln!(
        output,
        "        PointInTimeRecoveryEnabled: {}",
        table.point_in_time_recovery
    )
    .expect("writing to String cannot fail");
    output.push_str("      SSESpecification:\n");
    output.push_str("        SSEEnabled: true\n");
}

fn render_dynamodb_key_schema(
    output: &mut String,
    partition_key: &str,
    sort_key: Option<&str>,
    indent: usize,
) {
    let spaces = " ".repeat(indent);
    writeln!(
        output,
        "{spaces}- AttributeName: {}",
        yaml_quote(partition_key)
    )
    .expect("writing to String cannot fail");
    writeln!(output, "{spaces}  KeyType: HASH").expect("writing to String cannot fail");
    if let Some(sort_key) = sort_key {
        writeln!(output, "{spaces}- AttributeName: {}", yaml_quote(sort_key))
            .expect("writing to String cannot fail");
        writeln!(output, "{spaces}  KeyType: RANGE").expect("writing to String cannot fail");
    }
}

fn render_dynamodb_environment(output: &mut String, table: &DynamoDbTablePlan) {
    output.push_str("          DATABASE_KIND: dynamodb\n");
    writeln!(
        output,
        "          DYNAMODB_TABLE_NAME: !Ref {}",
        table.logical_id
    )
    .expect("writing to String cannot fail");
}

fn render_dynamodb_audit_environment(output: &mut String, table: &DynamoDbTablePlan) {
    writeln!(
        output,
        "          AUDIT_DYNAMODB_TABLE_NAME: !Ref {}",
        table.logical_id
    )
    .expect("writing to String cannot fail");
}

fn render_dynamodb_policy_statement(output: &mut String, table: &DynamoDbTablePlan) {
    output.push_str("            - Effect: Allow\n");
    output.push_str("              Action:\n");
    for action in [
        "dynamodb:DescribeTable",
        "dynamodb:GetItem",
        "dynamodb:TransactWriteItems",
        "dynamodb:UpdateItem",
    ] {
        writeln!(output, "                - {action}").expect("writing to String cannot fail");
    }
    writeln!(
        output,
        "              Resource: !GetAtt {}.Arn",
        table.logical_id
    )
    .expect("writing to String cannot fail");
    output.push_str("            - Effect: Allow\n");
    output.push_str("              Action: dynamodb:Query\n");
    if table.global_secondary_indexes.is_empty() {
        writeln!(
            output,
            "              Resource: !GetAtt {}.Arn",
            table.logical_id
        )
        .expect("writing to String cannot fail");
    } else {
        output.push_str("              Resource:\n");
        for index in &table.global_secondary_indexes {
            writeln!(
                output,
                "                - !Sub '${{{}.Arn}}/index/{}'",
                table.logical_id, index.name
            )
            .expect("writing to String cannot fail");
        }
    }
}

fn render_dynamodb_audit_policy_statement(output: &mut String, table: &DynamoDbTablePlan) {
    output.push_str("            - Effect: Allow\n");
    output.push_str("              Action:\n");
    for action in [
        "dynamodb:BatchGetItem",
        "dynamodb:DescribeTable",
        "dynamodb:Query",
        "dynamodb:TransactWriteItems",
    ] {
        writeln!(output, "                - {action}").expect("writing to String cannot fail");
    }
    writeln!(
        output,
        "              Resource: !GetAtt {}.Arn",
        table.logical_id
    )
    .expect("writing to String cannot fail");
}

fn render_database_environment(
    output: &mut String,
    function: &FunctionPlan,
    uses_audit_database_parameter: bool,
) {
    output.push_str("          DATABASE_KIND: postgres\n");
    output.push_str("          DATABASE_URL_PARAMETER: !Ref DatabaseUrlParameterName\n");
    if uses_audit_database_parameter {
        output.push_str(
            "          AUDIT_DATABASE_URL_PARAMETER: !Ref AuditDatabaseUrlParameterName\n",
        );
    }
    writeln!(
        output,
        "          DATABASE_MAX_CONNECTIONS: {}",
        yaml_quote(&function.database_connections_per_instance.to_string())
    )
    .expect("writing to String cannot fail");
}

fn render_database_policy_statements(output: &mut String, uses_audit_database_parameter: bool) {
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
    if uses_audit_database_parameter {
        output.push_str("            - Effect: Allow\n");
        output.push_str("              Action: [ssm:GetParameter]\n");
        output.push_str("              Resource: !Sub 'arn:${AWS::Partition}:ssm:${AWS::Region}:${AWS::AccountId}:parameter${AuditDatabaseUrlParameterName}'\n");
        output.push_str("            - !If\n");
        output.push_str("              - UsesCustomerManagedAuditDatabaseKey\n");
        output.push_str("              - Effect: Allow\n");
        output.push_str("                Action: [kms:Decrypt]\n");
        output.push_str("                Resource: !Ref AuditDatabaseUrlKmsKeyArn\n");
        output.push_str("                Condition:\n");
        output.push_str("                  StringEquals:\n");
        output.push_str(
            "                    kms:ViaService: !Sub 'ssm.${AWS::Region}.amazonaws.com'\n",
        );
        output.push_str("                    kms:EncryptionContext:PARAMETER_ARN: !Sub 'arn:${AWS::Partition}:ssm:${AWS::Region}:${AWS::AccountId}:parameter${AuditDatabaseUrlParameterName}'\n");
        output.push_str("              - !Ref AWS::NoValue\n");
    }
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
        render_database_environment(
            output,
            function,
            plan.application_graph
                .capabilities
                .contains_key("audit.ledger"),
        );
    }
    if let Some(table) = plan.database.dynamodb_table()
        && table.function_id == function.name
    {
        render_dynamodb_environment(output, table);
    }
    if let Some(table) = plan.dynamodb_audit_table()
        && table.function_id == function.name
    {
        render_dynamodb_audit_environment(output, &table);
    }
    let has_sqs_trigger = plan.triggers.iter().any(|trigger| {
        matches!(
            trigger,
            TriggerPlan::Sqs { function_id, .. } if function_id == &function.name
        )
    });
    let uses_dynamodb = plan
        .database
        .dynamodb_table()
        .is_some_and(|table| table.function_id == function.name)
        || plan
            .dynamodb_audit_table()
            .is_some_and(|table| table.function_id == function.name);
    if function.database_connections_per_instance > 0 || uses_dynamodb || has_sqs_trigger {
        output.push_str("      Policies:\n");
        output.push_str("        - Statement:\n");
        if function.database_connections_per_instance > 0 {
            render_database_policy_statements(
                output,
                plan.application_graph
                    .capabilities
                    .contains_key("audit.ledger"),
            );
        }
        if let Some(table) = plan.database.dynamodb_table()
            && table.function_id == function.name
        {
            render_dynamodb_policy_statement(output, table);
        }
        if let Some(table) = plan.dynamodb_audit_table()
            && table.function_id == function.name
        {
            render_dynamodb_audit_policy_statement(output, &table);
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

fn yaml_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AuthPlan, CostPolicy, DatabaseDeployment, FunctionPlan, FunctionRole, IngressPlan,
        NeonPlan, PerformancePolicy, RealtimeDeployment, RoutePlan, RuntimePlan,
    };

    fn minimal_http_plan() -> DeploymentPlan {
        DeploymentPlan {
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
            routes: vec![
                RoutePlan {
                    operation_id: "getHealth".into(),
                    method: HttpMethod::Get,
                    path: "/health".into(),
                    authenticated: false,
                },
                RoutePlan {
                    operation_id: "getOrder".into(),
                    method: HttpMethod::Get,
                    path: "/orders/{orderId}".into(),
                    authenticated: true,
                },
            ],
            application_graph: minco_core::ApplicationGraph::default(),
            static_site: None,
            realtime: None,
            preview: None,
            local_aws_services: vec!["ssm".into(), "sts".into()],
            scheduled_wakeups: Vec::new(),
            uses_nat_gateway: false,
            allowed_origins: vec!["https://app.example.invalid".into()],
            allowed_headers: vec![
                "authorization".into(),
                "content-type".into(),
                "idempotency-key".into(),
                "if-match".into(),
                "if-none-match".into(),
                "x-request-id".into(),
            ],
            exposed_headers: vec![
                "deprecation".into(),
                "etag".into(),
                "link".into(),
                "location".into(),
                "retry-after".into(),
                "sunset".into(),
                "www-authenticate".into(),
                "x-request-id".into(),
            ],
            log_retention_days: 14,
            cost_policy: CostPolicy::default(),
            performance_policy: PerformancePolicy::default(),
        }
    }

    #[test]
    fn renders_a_minimal_http_api_and_lambda() {
        let plan = minimal_http_plan();
        let yaml = render_sam(&plan).expect("SAM");
        assert!(yaml.contains("AWS::Serverless::HttpApi"));
        assert!(yaml.contains(
            "        ExposeHeaders:\n          - 'deprecation'\n          - 'etag'\n          - 'link'\n          - 'location'\n          - 'retry-after'\n          - 'sunset'\n          - 'www-authenticate'\n          - 'x-request-id'\n"
        ));
        assert!(yaml.contains("operationId: 'getHealth'"));
        assert!(yaml.contains("security: []"));
        assert!(yaml.contains("operationId: 'getOrder'"));
        assert!(yaml.contains("security:\n                - JwtAuthorizer: []"));
        assert!(yaml.contains("Authorizers:\n          JwtAuthorizer:"));
        assert!(!yaml.contains("DefaultAuthorizer:"));
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

    #[test]
    fn audit_ledger_capability_injects_a_distinct_postgres_secret_boundary() {
        let mut plan = minimal_http_plan();
        plan.application_graph
            .capabilities
            .insert("audit.ledger".into(), "2.0.0".parse().expect("version"));

        let yaml = render_sam(&plan).expect("SAM");

        assert!(yaml.contains("  AuditDatabaseUrlParameterName:\n"));
        assert!(yaml.contains(
            "          AUDIT_DATABASE_URL_PARAMETER: !Ref AuditDatabaseUrlParameterName\n"
        ));
        assert!(yaml.contains("  UsesCustomerManagedAuditDatabaseKey:"));
        assert!(yaml.contains("Resource: !Ref AuditDatabaseUrlKmsKeyArn"));
        assert!(yaml.contains("parameter${AuditDatabaseUrlParameterName}"));
        assert!(!yaml.contains("AUDIT_DATABASE_URL:"));
    }

    #[test]
    fn renders_subscriber_only_appsync_events_with_exact_publish_iam() {
        let mut plan = minimal_http_plan();
        plan.realtime = Some(RealtimeDeployment {
            namespace: "orders".into(),
            max_event_bytes: 5 * 1024,
            subscriber_claim: "tenant_id".into(),
        });

        let yaml = render_sam(&plan).expect("SAM");

        assert!(yaml.contains("Type: AWS::AppSync::Api"));
        assert!(yaml.contains("DefaultPublishAuthModes:\n          - AuthType: AWS_IAM"));
        assert!(yaml.contains("DefaultSubscribeAuthModes:\n          - AuthType: OPENID_CONNECT"));
        assert!(yaml.contains("ConnectionAuthModes:\n          - AuthType: OPENID_CONNECT"));
        assert!(yaml.contains("Type: AWS::AppSync::ChannelNamespace"));
        assert!(yaml.contains("Name: 'orders'"));
        assert!(yaml.contains("CodeHandlers: |"));
        assert!(yaml.contains("ctx.identity.claims[CLAIM]"));
        assert!(yaml.contains("const CLAIM = 'tenant_id'"));
        assert!(yaml.contains("segments[1] !== claim"));
        assert!(yaml.contains("MINCO_REALTIME_HTTP_ENDPOINT:"));
        assert!(yaml.contains("Action: appsync:EventPublish"));
        assert!(yaml.contains("apis/${RealtimeApi.ApiId}/channelNamespace/orders"));
        assert!(!yaml.contains("apis/${RealtimeApi.ApiId}/*"));
        assert!(!yaml.contains("appsync:EventConnect"));
        assert!(!yaml.contains("appsync:EventSubscribe"));
        assert!(!yaml.contains("API_KEY"));
    }

    #[test]
    fn rejects_invalid_realtime_policy_before_rendering_javascript() {
        let mut plan = minimal_http_plan();
        plan.realtime = Some(RealtimeDeployment {
            namespace: "orders".into(),
            max_event_bytes: 5 * 1024,
            subscriber_claim: "tenant_id\nthrow new Error('injected')".into(),
        });

        let error = render_sam(&plan).unwrap_err();

        assert!(matches!(error, PlanError::UnsupportedDeployment(_)));
    }

    #[test]
    fn groups_methods_under_one_openapi_path_item() {
        let mut plan = minimal_http_plan();
        plan.routes.push(RoutePlan {
            operation_id: "updateOrder".into(),
            method: HttpMethod::Patch,
            path: "/orders/{orderId}".into(),
            authenticated: true,
        });

        let yaml = render_sam(&plan).expect("SAM");

        assert_eq!(
            yaml.matches("          '/orders/{orderId}':\n").count(),
            1,
            "an OpenAPI path item must contain every method for that path"
        );
        assert!(yaml.contains(
            "          '/orders/{orderId}':\n            get:\n              operationId: 'getOrder'"
        ));
        assert!(yaml.contains(
            "              security:\n                - JwtAuthorizer: []\n            patch:\n              operationId: 'updateOrder'"
        ));
    }

    #[test]
    fn isolates_candidate_verification_from_the_live_stage() {
        let yaml = render_sam(&minimal_http_plan()).expect("SAM");

        assert!(yaml.contains("  LiveFunctionVersion:"));
        assert!(yaml.contains("StageName: '$default'"));
        assert!(yaml.contains("  CandidateStage:\n    Type: AWS::ApiGatewayV2::Stage"));
        assert!(yaml.contains("AutoPublishAlias: candidate"));
        assert!(yaml.contains("lambdaAlias: 'candidate'"));
        assert!(yaml.contains("lambdaAlias: 'live'"));
        assert!(yaml.contains("${!stageVariables.lambdaAlias}/invocations"));
        assert!(yaml.contains("  CandidateApiInvokePermission:"));
        assert!(yaml.contains("      FunctionName: !Ref ApiFunction.Alias"));
        assert!(yaml.contains("  LiveFunctionAlias:"));
        assert!(yaml.contains("      FunctionVersion: !If"));
        assert!(yaml.contains("        - !GetAtt ApiFunction.Version.Version"));
        assert!(yaml.contains("  LiveApiInvokePermission:"));
        assert!(yaml.contains("      FunctionName: !Ref LiveFunctionAlias"));
        assert_eq!(
            yaml.matches("      FunctionName: !Ref ApiFunction\n")
                .count(),
            1,
            "only the live alias may name the unqualified function"
        );
    }
}
