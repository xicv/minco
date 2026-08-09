use minco_core::{
    CapabilityProvision, CapabilityRequirement, IdleCostClass, Plugin, PluginContext,
    PluginDescriptor, PluginError, PluginId, PluginStability, ResourceIntent, ResourceKind,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
// These booleans are independent composition-root selections, not state.
#[allow(clippy::struct_excessive_bools)]
pub struct AwsAdapterSelection {
    pub object_storage: bool,
    pub event_publication: bool,
    pub email_notifications: bool,
    pub identity_administration: bool,
    pub static_site: bool,
    pub realtime_publication: bool,
}

#[derive(Debug, Clone)]
pub struct AwsAdaptersPlugin {
    selection: AwsAdapterSelection,
}

impl AwsAdaptersPlugin {
    pub fn new(selection: AwsAdapterSelection) -> Result<Self, PluginError> {
        if selection == AwsAdapterSelection::default() {
            return Err(PluginError::Installation(
                "aws-adapters requires at least one explicitly selected provider".into(),
            ));
        }
        Ok(Self { selection })
    }
}

impl Plugin for AwsAdaptersPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("aws-adapters").expect("static plugin ID"),
            "1.0.0".parse().expect("static version"),
            "Explicit AWS provider selections, resource intents, and IAM derivation markers",
        );
        descriptor.documentation = Some("https://docs.rs/minco-aws-adapters".into());
        descriptor.core_compatibility = concat!("^", env!("CARGO_PKG_VERSION"))
            .parse()
            .expect("package version");
        descriptor.stability = PluginStability::Beta;
        if self.selection.object_storage {
            add_provider(
                &mut descriptor,
                "storage.object",
                "aws.s3.object-storage",
                Some(ResourceIntent {
                    id: "aws-object-bucket".into(),
                    kind: ResourceKind::S3Bucket,
                    idle_cost: IdleCostClass::StorageOnly,
                    wake_sources: Vec::new(),
                    dependencies: Vec::new(),
                }),
            );
        }
        if self.selection.event_publication {
            add_provider(
                &mut descriptor,
                "events.publish",
                "aws.sqs.event-publication",
                Some(ResourceIntent {
                    id: "aws-events-queue".into(),
                    kind: ResourceKind::SqsQueue,
                    idle_cost: IdleCostClass::ProviderManaged,
                    wake_sources: Vec::new(),
                    dependencies: Vec::new(),
                }),
            );
        }
        if self.selection.email_notifications {
            add_provider(
                &mut descriptor,
                "notifications.send",
                "aws.ses.email-notifications",
                Some(ResourceIntent {
                    id: "aws-ses-identity".into(),
                    kind: ResourceKind::Custom("ses-identity".into()),
                    idle_cost: IdleCostClass::ProviderManaged,
                    wake_sources: Vec::new(),
                    dependencies: Vec::new(),
                }),
            );
        }
        if self.selection.identity_administration {
            add_provider(
                &mut descriptor,
                "identity.admin",
                "aws.cognito.identity-administration",
                Some(ResourceIntent {
                    id: "aws-cognito-user-pool".into(),
                    kind: ResourceKind::Custom("cognito-user-pool".into()),
                    idle_cost: IdleCostClass::ProviderManaged,
                    wake_sources: Vec::new(),
                    dependencies: Vec::new(),
                }),
            );
        }
        if self.selection.static_site {
            add_provider(
                &mut descriptor,
                "static-site.provider",
                "aws.cloudfront.static-site",
                None,
            );
        }
        if self.selection.realtime_publication {
            add_provider(
                &mut descriptor,
                "realtime.publish",
                "aws.appsync-events.realtime-publication",
                None,
            );
        }
        descriptor
    }

    fn install(&self, _context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }
}

fn add_provider(
    descriptor: &mut PluginDescriptor,
    requirement: &str,
    provision: &str,
    resource: Option<ResourceIntent>,
) {
    descriptor.requires.push(CapabilityRequirement {
        name: requirement.into(),
        version: "^1.0".parse().expect("static capability requirement"),
    });
    descriptor.provides.push(CapabilityProvision {
        name: provision.into(),
        version: "1.0.0".parse().expect("static capability version"),
    });
    descriptor.resources.extend(resource);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_providers_have_explicit_non_fixed_cost_intents() {
        let descriptor = AwsAdaptersPlugin::new(AwsAdapterSelection {
            object_storage: true,
            event_publication: true,
            email_notifications: true,
            identity_administration: true,
            static_site: true,
            realtime_publication: true,
        })
        .unwrap()
        .descriptor();
        assert!(
            descriptor
                .provides
                .iter()
                .any(|capability| capability.name == "aws.sqs.event-publication")
        );
        assert!(
            descriptor
                .provides
                .iter()
                .any(|capability| capability.name == "aws.appsync-events.realtime-publication")
        );
        assert!(
            descriptor
                .resources
                .iter()
                .all(|resource| resource.idle_cost != IdleCostClass::FixedCapacity)
        );
        assert!(
            descriptor
                .resources
                .iter()
                .all(|resource| resource.wake_sources.is_empty())
        );
    }
}

#[derive(Debug, Clone, Default)]
pub struct AwsSesMailPlugin;

impl Plugin for AwsSesMailPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("aws-ses-mail").expect("static plugin ID"),
            "1.0.0".parse().expect("static version"),
            "Explicit Amazon SES v2 rich-mail transport selection",
        );
        descriptor.documentation = Some("https://docs.rs/minco-aws-adapters".into());
        descriptor.core_compatibility = concat!("^", env!("CARGO_PKG_VERSION"))
            .parse()
            .expect("package version");
        descriptor.stability = PluginStability::Beta;
        add_provider(
            &mut descriptor,
            "mail.send",
            "aws.ses.mail-delivery",
            Some(ResourceIntent {
                id: "aws-ses-mail-identity".into(),
                kind: ResourceKind::Custom("ses-identity".into()),
                idle_cost: IdleCostClass::ProviderManaged,
                wake_sources: Vec::new(),
                dependencies: Vec::new(),
            }),
        );
        descriptor
    }

    fn install(&self, _context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }
}
