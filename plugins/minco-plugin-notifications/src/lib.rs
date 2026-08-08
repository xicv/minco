//! Provider-neutral notifications, first-class mail delivery, and deterministic test adapters.
#![forbid(unsafe_code)]

pub mod mail;
pub mod mailpit;

pub use mail::*;
pub use mailpit::MailpitTransport;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use minco_core::{
    CapabilityProvision, DataClass, Plugin, PluginContext, PluginDescriptor, PluginError, PluginId,
    PluginStability,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannel {
    Email,
    Webhook,
    InApp,
    DeveloperInbox,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub topic: String,
    pub channel: NotificationChannel,
    pub recipient: String,
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl Notification {
    pub fn new(
        topic: impl Into<String>,
        channel: NotificationChannel,
        recipient: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            topic: topic.into(),
            channel,
            recipient: recipient.into(),
            title: title.into(),
            body: body.into(),
            link: None,
            metadata: BTreeMap::new(),
            created_at: Utc::now(),
        }
    }
}

#[async_trait]
pub trait NotificationSink: Send + Sync + std::fmt::Debug {
    async fn send(&self, notification: Notification) -> Result<(), NotificationError>;
}

#[derive(Clone)]
pub struct NotificationService(pub Arc<dyn NotificationSink>);

impl std::fmt::Debug for NotificationService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("NotificationService").finish()
    }
}

impl NotificationService {
    pub fn new(sink: Arc<dyn NotificationSink>) -> Self {
        Self(sink)
    }

    pub async fn send(&self, notification: Notification) -> Result<(), NotificationError> {
        self.0.send(notification).await
    }
}

#[derive(Debug, Default)]
pub struct MemoryNotificationSink {
    notifications: RwLock<Vec<Notification>>,
}

impl MemoryNotificationSink {
    pub async fn all(&self) -> Vec<Notification> {
        self.notifications.read().await.clone()
    }
}

#[async_trait]
impl NotificationSink for MemoryNotificationSink {
    async fn send(&self, notification: Notification) -> Result<(), NotificationError> {
        if notification.recipient.trim().is_empty() {
            return Err(NotificationError::InvalidRecipient);
        }
        self.notifications.write().await.push(notification);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct NotificationsPlugin {
    service: NotificationService,
    mail_service: MailService,
}

impl NotificationsPlugin {
    pub fn new(sink: Arc<dyn NotificationSink>) -> Self {
        let mail_transport = Arc::new(LegacyNotificationMailTransport::new(sink.clone()));
        let mail_service = MailService::single(mail_transport, Arc::new(NoopMailObserver))
            .expect("static notification compatibility transport");
        Self {
            service: NotificationService::new(sink),
            mail_service,
        }
    }

    pub fn with_mail_service(mut self, mail_service: MailService) -> Self {
        self.mail_service = mail_service;
        self
    }

    pub fn memory() -> (Self, Arc<MemoryNotificationSink>) {
        let sink = Arc::new(MemoryNotificationSink::default());
        let mail_transport = Arc::new(MemoryMailTransport::default());
        let mail_service = MailService::single(mail_transport, Arc::new(NoopMailObserver))
            .expect("static memory mail transport");
        (
            Self::new(sink.clone()).with_mail_service(mail_service),
            sink,
        )
    }

    pub fn memory_with_mail() -> (
        Self,
        Arc<MemoryNotificationSink>,
        Arc<MemoryMailTransport>,
        Arc<MemoryMailObserver>,
    ) {
        let notification_sink = Arc::new(MemoryNotificationSink::default());
        let mail_transport = Arc::new(MemoryMailTransport::default());
        let mail_observer = Arc::new(MemoryMailObserver::default());
        let mail_service = MailService::single(mail_transport.clone(), mail_observer.clone())
            .expect("static memory mail transport");
        (
            Self::new(notification_sink.clone()).with_mail_service(mail_service),
            notification_sink,
            mail_transport,
            mail_observer,
        )
    }
}

impl Plugin for NotificationsPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("notifications").expect("static plugin ID"),
            Version::new(1, 0, 0),
            "Provider-neutral email, webhook, in-app, and developer notifications",
        );
        descriptor.documentation = Some("https://docs.rs/minco-plugin-notifications".into());
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Beta;
        descriptor
            .data_classes
            .extend([DataClass::Personal, DataClass::Confidential]);
        descriptor.provides.extend([
            CapabilityProvision {
                name: "notifications.send".into(),
                version: Version::new(1, 0, 0),
            },
            CapabilityProvision {
                name: "mail.send".into(),
                version: Version::new(1, 0, 0),
            },
        ]);
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        context.services().insert(Arc::new(self.service.clone()))?;
        context
            .services()
            .insert(Arc::new(self.mail_service.clone()))?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("notification recipient must not be empty")]
    InvalidRecipient,
    #[error("notification delivery failed: {0}")]
    Delivery(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_sink_keeps_delivery_order() {
        let sink = MemoryNotificationSink::default();
        sink.send(Notification::new(
            "feedback.created",
            NotificationChannel::DeveloperInbox,
            "team",
            "New feedback",
            "The client reported a problem",
        ))
        .await
        .unwrap();
        assert_eq!(sink.all().await[0].topic, "feedback.created");
    }

    #[tokio::test]
    async fn memory_plugin_exposes_first_class_mail_service() {
        let (plugin, _, mail_transport, mail_observer) = NotificationsPlugin::memory_with_mail();
        let message = MailMessage::builder("account.welcome", "Welcome")
            .to(MailAddress::new("person@example.com").unwrap())
            .text("Welcome")
            .build()
            .unwrap();

        plugin.mail_service.send(message).await.unwrap();

        mail_transport.assert_sent_count(1).await;
        assert_eq!(mail_observer.events().await.len(), 3);
        assert!(
            plugin
                .descriptor()
                .provides
                .iter()
                .any(|capability| capability.name == "mail.send")
        );
    }
}
