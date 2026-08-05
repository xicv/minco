//! Standalone third-party-style plugin used to prove the public conformance API.
#![forbid(unsafe_code)]

use minco_core::{Plugin, PluginContext, PluginDescriptor, PluginError, PluginId};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ExampleService;

#[derive(Debug, Clone, Default)]
pub struct ThirdPartyExamplePlugin;

impl Plugin for ThirdPartyExamplePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("third-party-example").expect("static plugin ID"),
            "0.1.0".parse().expect("static plugin version"),
            "Standalone third-party conformance example",
        );
        descriptor.core_compatibility = "^1.0.0".parse().expect("static Minco compatibility");
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        context.services().insert(Arc::new(ExampleService))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_test::{ConformanceStatus, PluginConformance};

    #[test]
    fn passes_the_same_public_conformance_kit_as_official_plugins() {
        let report = PluginConformance::for_package(env!("CARGO_MANIFEST_DIR"))
            .with_plugin(ThirdPartyExamplePlugin)
            .run();

        report.assert_passed();
        assert_eq!(report.assurance.plugin_lifecycle, ConformanceStatus::Passed);
        assert_eq!(report.assurance.provider_live, ConformanceStatus::NotRun);
    }
}
