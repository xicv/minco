//! Validated AWS `DynamoDB` provider primitives for application-owned access models.
#![forbid(unsafe_code)]

use minco_core::{
    CapabilityProvision, IdleCostClass, Plugin, PluginContext, PluginDescriptor, PluginError,
    PluginId, PluginStability, ResourceIntent, ResourceKind,
};
use serde::{Deserialize, Serialize};

pub mod audit_v2;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DynamoDbConfig {
    table_name: String,
    region: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    endpoint_url: Option<String>,
}

impl std::fmt::Debug for DynamoDbConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DynamoDbConfig")
            .field("table_name", &"[REDACTED]")
            .field("region", &self.region)
            .field(
                "endpoint_url",
                &self.endpoint_url.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

impl DynamoDbConfig {
    #[must_use]
    pub fn new(
        table_name: impl Into<String>,
        region: impl Into<String>,
        endpoint_url: Option<String>,
    ) -> Self {
        Self {
            table_name: table_name.into(),
            region: region.into(),
            endpoint_url,
        }
    }

    pub fn validate(&self) -> Result<(), DynamoDbError> {
        if !valid_table_name(&self.table_name) {
            return Err(DynamoDbError::InvalidConfiguration("table name is invalid"));
        }
        if !valid_region(&self.region) {
            return Err(DynamoDbError::InvalidConfiguration("region is invalid"));
        }
        if self
            .endpoint_url
            .as_deref()
            .is_some_and(|endpoint| !valid_endpoint(endpoint))
        {
            return Err(DynamoDbError::InvalidConfiguration(
                "endpoint override is invalid",
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    #[must_use]
    pub fn region(&self) -> &str {
        &self.region
    }

    #[must_use]
    pub fn endpoint_url(&self) -> Option<&str> {
        self.endpoint_url.as_deref()
    }

    pub async fn build(&self) -> Result<DynamoDbProvider, DynamoDbError> {
        self.validate()?;
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(self.region.clone()));
        if let Some(endpoint_url) = &self.endpoint_url {
            loader = loader.endpoint_url(endpoint_url);
        }
        let shared = loader.load().await;
        let client = aws_sdk_dynamodb::Client::new(&shared);
        DynamoDbProvider::new(client, self.clone())
    }
}

#[derive(Clone)]
pub struct DynamoDbProvider {
    client: aws_sdk_dynamodb::Client,
    config: DynamoDbConfig,
}

impl std::fmt::Debug for DynamoDbProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DynamoDbProvider")
            .field("config", &self.config)
            .field("client", &"[REDACTED PROVIDER]")
            .finish()
    }
}

impl DynamoDbProvider {
    pub fn new(
        client: aws_sdk_dynamodb::Client,
        config: DynamoDbConfig,
    ) -> Result<Self, DynamoDbError> {
        config.validate()?;
        Ok(Self { client, config })
    }

    #[must_use]
    pub const fn client(&self) -> &aws_sdk_dynamodb::Client {
        &self.client
    }

    #[must_use]
    pub fn table_name(&self) -> &str {
        self.config.table_name()
    }

    pub async fn ready(&self) -> Result<(), DynamoDbError> {
        self.client
            .describe_table()
            .table_name(self.table_name())
            .send()
            .await
            .map(|_| ())
            .map_err(|_| DynamoDbError::Provider("DescribeTable"))
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DynamoDbError {
    #[error("DynamoDB configuration is invalid: {0}")]
    InvalidConfiguration(&'static str),
    #[error("DynamoDB provider operation failed: {0}")]
    Provider(&'static str),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DynamoDbProviderPlugin;

impl Plugin for DynamoDbProviderPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("aws-dynamodb").expect("static plugin ID"),
            env!("CARGO_PKG_VERSION")
                .parse()
                .expect("package version is semver"),
            "Validated AWS DynamoDB provider primitives for explicit access models",
        );
        descriptor.documentation = Some("https://docs.rs/minco-aws-dynamodb".into());
        descriptor.core_compatibility = concat!("^", env!("CARGO_PKG_VERSION"))
            .parse()
            .expect("package version");
        descriptor.stability = PluginStability::Beta;
        descriptor.provides.push(CapabilityProvision {
            name: "aws.dynamodb.client".into(),
            version: env!("CARGO_PKG_VERSION")
                .parse()
                .expect("package version is semver"),
        });
        descriptor.resources.push(ResourceIntent {
            id: "aws-dynamodb-table".into(),
            kind: ResourceKind::DynamoDb,
            idle_cost: IdleCostClass::StorageOnly,
            wake_sources: Vec::new(),
            dependencies: Vec::new(),
        });
        descriptor
    }

    fn install(&self, _context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }
}

fn valid_table_name(value: &str) -> bool {
    (3..=255).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn valid_region(value: &str) -> bool {
    (3..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value.contains('-')
}

fn valid_endpoint(value: &str) -> bool {
    let Ok(uri) = value.parse::<http::Uri>() else {
        return false;
    };
    let Some(authority) = uri.authority() else {
        return false;
    };
    let Some(host) = uri.host() else {
        return false;
    };
    if authority.as_str().contains('@')
        || uri.query().is_some()
        || !matches!(uri.path(), "" | "/")
        || !matches!(uri.scheme_str(), Some("https" | "http"))
    {
        return false;
    }
    uri.scheme_str() == Some("https") || is_loopback_host(host)
}

fn is_loopback_host(host: &str) -> bool {
    let host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}
