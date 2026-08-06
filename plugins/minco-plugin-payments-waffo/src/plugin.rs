use crate::{
    RawWaffoConfiguration, SecretResolver, WaffoClient, WaffoConfiguration, WaffoError,
    WaffoWebhookVerifier, configuration_schema::configuration_fields,
};
use minco_core::{
    CapabilityProvision, CapabilityRequirement, DataClass, IdleCostClass, Plugin, PluginContext,
    PluginDescriptor, PluginError, PluginId, PluginStability, ResourceIntent, ResourceKind,
    WakeSource,
};
use minco_plugin_idempotency::IdempotencyService;
use semver::{Version, VersionReq};
use std::{fmt, sync::Arc};

pub const PLUGIN_ID: &str = "payments-waffo";

/// Static Waffo Pancake payment plugin.
#[derive(Debug, Default, Clone, Copy)]
pub struct WaffoPlugin;

impl Plugin for WaffoPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new(PLUGIN_ID).expect("static plugin ID is valid"),
            Version::new(1, 0, 0),
            "Waffo Pancake checkout, signed API, and verified webhook integration",
        );
        descriptor.core_compatibility =
            VersionReq::parse("^1.1.0").expect("static core compatibility is valid");
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
            .finish()
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
    pub async fn client(&self, resolver: &dyn SecretResolver) -> Result<WaffoClient, WaffoError> {
        let private_key = resolver
            .resolve(self.configuration.private_key_reference())
            .await?;
        WaffoClient::new(
            Arc::clone(&self.configuration),
            private_key,
            Arc::clone(&self.idempotency),
        )
    }

    /// Resolve and parse Waffo's public key only when webhook verification is requested.
    pub async fn webhook_verifier(
        &self,
        resolver: &dyn SecretResolver,
    ) -> Result<WaffoWebhookVerifier, WaffoError> {
        let reference = self
            .configuration
            .webhook_public_key_reference()
            .ok_or(WaffoError::MissingWebhookConfiguration)?;
        let public_key = resolver.resolve(reference).await?;
        WaffoWebhookVerifier::from_pem(
            self.configuration.environment(),
            public_key.expose_for_verification(),
            self.configuration.webhook_tolerance(),
            self.configuration.webhook_max_bytes(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_core::{PluginManager, PluginSelection};
    use minco_plugin_idempotency::IdempotencyPlugin;
    use serde_json::json;

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
                "merchant_id": "MER_ABC123",
                "private_key": "env:WAFFO_PRIVATE_KEY"
            }),
        );

        let application = manager.compose(&selection).unwrap();
        let service = application.services.get::<WaffoService>().unwrap();

        assert_eq!(service.configuration().merchant_id(), "MER_ABC123");
    }
}
