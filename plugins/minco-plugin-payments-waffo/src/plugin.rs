use crate::{
    RawWaffoConfiguration, SecretResolver, WaffoClient, WaffoConfiguration, WaffoError,
    WaffoResponse, WaffoTransport, WaffoWebhookVerifier,
    configuration_schema::configuration_fields, validate_read_only_graphql,
};
use minco_config::EnvironmentClass;
use minco_core::{
    CapabilityProvision, CapabilityRequirement, DataClass, IdleCostClass, Plugin, PluginContext,
    PluginDescriptor, PluginError, PluginId, PluginStability, ResourceIntent, ResourceKind,
    WakeSource,
};
use minco_plugin_idempotency::IdempotencyService;
use semver::{Version, VersionReq};
use serde_json::Value;
use std::{fmt, sync::Arc};

pub const PLUGIN_ID: &str = "payments-waffo";

/// Static Waffo Pancake payment plugin.
#[derive(Debug, Default, Clone, Copy)]
pub struct WaffoPlugin;

impl Plugin for WaffoPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("payments-waffo").expect("static plugin ID is valid"),
            Version::new(1, 0, 0),
            "Waffo Pancake checkout, signed API, and verified webhook integration",
        );
        descriptor.core_compatibility = VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION")))
            .expect("static core compatibility is valid");
        descriptor.stability = PluginStability::Beta;
        descriptor.default_enabled = false;
        descriptor.documentation = Some("https://docs.rs/minco-plugin-payments-waffo".into());
        descriptor.data_classes = vec![
            DataClass::CustomerProvided,
            DataClass::Personal,
            DataClass::Confidential,
            DataClass::Secret,
        ];
        descriptor.plugin_dependencies =
            vec![PluginId::new("idempotency").expect("static dependency ID is valid")];
        descriptor.requires = vec![CapabilityRequirement {
            name: "idempotency.claim".into(),
            version: VersionReq::parse("^1.0.0").expect("static capability requirement is valid"),
        }];
        descriptor.provides = [
            "payments.checkout",
            "payments.query",
            "payments.webhook.verify",
            "payments.provider.waffo",
        ]
        .into_iter()
        .map(|name| CapabilityProvision {
            name: name.into(),
            version: Version::new(1, 0, 0),
        })
        .collect();
        descriptor.resources = vec![ResourceIntent {
            id: "waffo-pancake-api".into(),
            kind: ResourceKind::Custom("waffo_pancake_api".into()),
            idle_cost: IdleCostClass::ProviderManaged,
            wake_sources: vec![WakeSource::HttpRequest],
            dependencies: Vec::new(),
        }];
        descriptor.configuration = configuration_fields();
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let raw = context.configuration::<RawWaffoConfiguration>()?;
        let configuration = WaffoConfiguration::try_from(raw)
            .map_err(|error| PluginError::Installation(error.to_string()))?;
        let idempotency = context.services().get::<IdempotencyService>()?;
        context
            .services()
            .insert(Arc::new(WaffoService::new(configuration, idempotency)))?;
        Ok(())
    }
}

/// Unresolved Waffo service registered during deterministic plugin composition.
#[derive(Clone)]
pub struct WaffoService {
    configuration: Arc<WaffoConfiguration>,
    idempotency: Arc<IdempotencyService>,
}

impl fmt::Debug for WaffoService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WaffoService")
            .field("configuration", &self.configuration)
            .finish_non_exhaustive()
    }
}

impl WaffoService {
    pub fn new(configuration: WaffoConfiguration, idempotency: Arc<IdempotencyService>) -> Self {
        Self {
            configuration: Arc::new(configuration),
            idempotency,
        }
    }

    pub fn configuration(&self) -> &WaffoConfiguration {
        &self.configuration
    }

    /// Resolve the private key only when the application explicitly constructs a client.
    pub async fn client(
        &self,
        environment_class: EnvironmentClass,
        resolver: &dyn SecretResolver,
    ) -> Result<WaffoClient, WaffoError> {
        self.configuration
            .validate_environment_class(environment_class)?;
        let private_key = resolver
            .resolve(self.configuration.private_key_reference())
            .await?;
        WaffoClient::new(
            Arc::clone(&self.configuration),
            &private_key,
            Arc::clone(&self.idempotency),
        )
    }

    /// Construct a client with an application-owned transport after the same fail-closed guard.
    pub async fn client_with_transport(
        &self,
        environment_class: EnvironmentClass,
        resolver: &dyn SecretResolver,
        transport: Arc<dyn WaffoTransport>,
    ) -> Result<WaffoClient, WaffoError> {
        self.configuration
            .validate_environment_class(environment_class)?;
        let private_key = resolver
            .resolve(self.configuration.private_key_reference())
            .await?;
        WaffoClient::with_transport(
            Arc::clone(&self.configuration),
            &private_key,
            Arc::clone(&self.idempotency),
            transport,
        )
    }

    /// Execute a read-only GraphQL query after validating it before secret resolution.
    pub async fn graphql_query(
        &self,
        environment_class: EnvironmentClass,
        resolver: &dyn SecretResolver,
        query: &str,
        variables: Value,
    ) -> Result<WaffoResponse<Value>, WaffoError> {
        validate_graphql_request(query, &variables)?;
        let client = self.client(environment_class, resolver).await?;
        client.graphql_query(query, variables).await
    }

    /// Execute a validated read-only GraphQL query with an application-owned transport.
    pub async fn graphql_query_with_transport(
        &self,
        environment_class: EnvironmentClass,
        resolver: &dyn SecretResolver,
        transport: Arc<dyn WaffoTransport>,
        query: &str,
        variables: Value,
    ) -> Result<WaffoResponse<Value>, WaffoError> {
        validate_graphql_request(query, &variables)?;
        let client = self
            .client_with_transport(environment_class, resolver, transport)
            .await?;
        client.graphql_query(query, variables).await
    }

    /// Resolve and parse Waffo's public key only when webhook verification is requested.
    pub async fn webhook_verifier(
        &self,
        environment_class: EnvironmentClass,
        resolver: &dyn SecretResolver,
    ) -> Result<WaffoWebhookVerifier, WaffoError> {
        self.configuration
            .validate_environment_class(environment_class)?;
        let reference = self
            .configuration
            .webhook_public_key_reference()
            .ok_or(WaffoError::MissingWebhookVerificationConfiguration)?;
        let expected_store_id = self
            .configuration
            .store_id()
            .ok_or(WaffoError::MissingWebhookVerificationConfiguration)?;
        let public_key = resolver.resolve(reference).await?;
        WaffoWebhookVerifier::from_pem(
            self.configuration.environment(),
            expected_store_id,
            public_key.expose_for_verification(),
            self.configuration.webhook_past_tolerance(),
            self.configuration.webhook_future_tolerance(),
            self.configuration.webhook_max_bytes(),
        )
    }
}

fn validate_graphql_request(query: &str, variables: &Value) -> Result<(), WaffoError> {
    validate_read_only_graphql(query)?;
    if !variables.is_object() {
        return Err(WaffoError::InvalidConfiguration(
            "GraphQL variables must be a JSON object",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use minco_config::SecretReference;
    use minco_core::{PluginManager, PluginSelection};
    use minco_plugin_idempotency::IdempotencyPlugin;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Default)]
    struct CountingResolver(AtomicUsize);

    #[async_trait]
    impl SecretResolver for CountingResolver {
        async fn resolve(
            &self,
            _reference: &SecretReference,
        ) -> Result<crate::SecretValue, WaffoError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(WaffoError::SecretResolution)
        }
    }

    #[test]
    fn descriptor_is_opt_in_and_declares_security_contracts() {
        let descriptor = WaffoPlugin.descriptor();

        assert!(!descriptor.default_enabled);
        assert_eq!(descriptor.stability, PluginStability::Beta);
        assert_eq!(descriptor.plugin_dependencies[0].as_str(), "idempotency");
        assert!(
            descriptor
                .configuration
                .iter()
                .find(|field| field.key == "private_key")
                .is_some_and(|field| field.secret && field.required)
        );
        assert_eq!(
            descriptor.resources[0].idle_cost,
            IdleCostClass::ProviderManaged
        );
        assert!(
            descriptor
                .provides
                .iter()
                .any(|capability| capability.name == "payments.checkout")
        );
    }

    #[test]
    fn composition_registers_unresolved_service_without_network_access() {
        let mut manager = PluginManager::default();
        manager.register(IdempotencyPlugin::memory()).unwrap();
        manager.register(WaffoPlugin).unwrap();
        let plugin_id = PluginId::new(PLUGIN_ID).unwrap();
        let mut selection = PluginSelection::default();
        selection.enabled.insert(plugin_id.clone());
        selection.configuration.insert(
            plugin_id,
            json!({
                "merchant_id": "MER_0123456789ABCDEFGHIJKL",
                "private_key": "env:WAFFO_PRIVATE_KEY"
            }),
        );

        let application = manager.compose(&selection).unwrap();
        let service = application.services.get::<WaffoService>().unwrap();

        assert_eq!(
            service.configuration().merchant_id(),
            "MER_0123456789ABCDEFGHIJKL"
        );
    }

    #[test]
    fn environment_mismatch_precedes_every_secret_resolution() {
        let raw = serde_json::from_value::<RawWaffoConfiguration>(json!({
            "environment": "production",
            "merchant_id": "MER_0123456789ABCDEFGHIJKL",
            "private_key": "env:WAFFO_PRIVATE_KEY",
            "store_id": "STO_0123456789ABCDEFGHIJKL",
            "webhook_public_key": "env:WAFFO_WEBHOOK_PUBLIC_KEY"
        }))
        .unwrap();
        let service = WaffoService::new(
            WaffoConfiguration::try_from(raw).unwrap(),
            Arc::new(
                IdempotencyService::new(
                    Arc::new(minco_plugin_idempotency::MemoryIdempotencyStore::default()),
                    chrono::TimeDelta::minutes(5),
                )
                .unwrap(),
            ),
        );
        let resolver = CountingResolver::default();

        assert!(
            futures::executor::block_on(service.client(EnvironmentClass::Development, &resolver))
                .is_err()
        );
        assert_eq!(resolver.0.load(Ordering::SeqCst), 0);
        assert!(
            futures::executor::block_on(
                service.webhook_verifier(EnvironmentClass::Development, &resolver)
            )
            .is_err()
        );
        assert_eq!(resolver.0.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn invalid_graphql_precedes_secret_resolution() {
        let raw = serde_json::from_value::<RawWaffoConfiguration>(json!({
            "merchant_id": "MER_0123456789ABCDEFGHIJKL",
            "private_key": "env:WAFFO_PRIVATE_KEY"
        }))
        .unwrap();
        let service = WaffoService::new(
            WaffoConfiguration::try_from(raw).unwrap(),
            Arc::new(
                IdempotencyService::new(
                    Arc::new(minco_plugin_idempotency::MemoryIdempotencyStore::default()),
                    chrono::TimeDelta::minutes(5),
                )
                .unwrap(),
            ),
        );
        let resolver = CountingResolver::default();

        assert!(
            futures::executor::block_on(service.graphql_query(
                EnvironmentClass::Development,
                &resolver,
                "mutation Create { createStore { id } }",
                json!({}),
            ))
            .is_err()
        );
        assert_eq!(resolver.0.load(Ordering::SeqCst), 0);
    }
}
