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
    pub rich_mail: bool,
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
                None,
            );
        }
        if self.selection.rich_mail {
            add_provider(&mut descriptor, "mail.send", "aws.ses.mail-delivery", None);
        }
        if self.selection.email_notifications || self.selection.rich_mail {
            descriptor.resources.push(ResourceIntent {
                id: "aws-ses-identity".into(),
                kind: ResourceKind::Custom("ses-identity".into()),
                idle_cost: IdleCostClass::ProviderManaged,
                wake_sources: Vec::new(),
                dependencies: Vec::new(),
            });
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
    use minco_core::{PluginManager, PluginSelection};
    use minco_plugin_notifications::NotificationsPlugin;

    #[test]
    fn selected_providers_have_explicit_non_fixed_cost_intents() {
        let descriptor = AwsAdaptersPlugin::new(AwsAdapterSelection {
            object_storage: true,
            event_publication: true,
            email_notifications: true,
            rich_mail: true,
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

    #[test]
    fn rich_mail_capabilities_are_selected_together() {
        let plain = AwsAdaptersPlugin::new(AwsAdapterSelection {
            email_notifications: true,
            ..AwsAdapterSelection::default()
        })
        .unwrap()
        .descriptor();
        assert!(plain.requires.iter().all(|item| item.name != "mail.send"));
        assert!(
            plain
                .provides
                .iter()
                .all(|item| item.name != "aws.ses.mail-delivery")
        );

        let rich = AwsAdaptersPlugin::new(AwsAdapterSelection {
            rich_mail: true,
            ..AwsAdapterSelection::default()
        })
        .unwrap()
        .descriptor();
        assert!(rich.requires.iter().any(|item| item.name == "mail.send"));
        assert!(
            rich.provides
                .iter()
                .any(|item| item.name == "aws.ses.mail-delivery")
        );
        assert_eq!(rich.resources.len(), 1);
    }

    #[test]
    fn graph_requires_explicit_rich_mail_on_both_sides() {
        let aws = AwsAdaptersPlugin::new(AwsAdapterSelection {
            rich_mail: true,
            ..AwsAdapterSelection::default()
        })
        .unwrap();
        let mut selection = PluginSelection::default();
        selection.enabled.extend([
            PluginId::new("notifications").unwrap(),
            PluginId::new("aws-adapters").unwrap(),
        ]);

        let mut plain_manager = PluginManager::default();
        plain_manager
            .register(NotificationsPlugin::memory().0)
            .unwrap();
        plain_manager.register(aws.clone()).unwrap();
        assert!(plain_manager.compose(&selection).is_err());

        let mut rich_manager = PluginManager::default();
        rich_manager
            .register(NotificationsPlugin::memory_with_mail().0)
            .unwrap();
        rich_manager.register(aws).unwrap();
        let application = rich_manager.compose(&selection).unwrap();
        assert!(application.graph.capabilities.contains_key("mail.send"));
        assert!(
            application
                .graph
                .capabilities
                .contains_key("aws.ses.mail-delivery")
        );
    }
}
