use async_trait::async_trait;
use minco_plugin_notifications::{
    Notification, NotificationChannel, NotificationError, NotificationSink,
};
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct RoutingNotificationSink {
    email: Option<Arc<dyn NotificationSink>>,
    webhook: Option<Arc<dyn NotificationSink>>,
    fallback: Option<Arc<dyn NotificationSink>>,
}

impl std::fmt::Debug for RoutingNotificationSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RoutingNotificationSink")
            .field("email", &self.email.is_some())
            .field("webhook", &self.webhook.is_some())
            .field("fallback", &self.fallback.is_some())
            .finish()
    }
}

impl RoutingNotificationSink {
    #[must_use]
    pub fn with_email(mut self, sink: Arc<dyn NotificationSink>) -> Self {
        self.email = Some(sink);
        self
    }

    #[must_use]
    pub fn with_webhook(mut self, sink: Arc<dyn NotificationSink>) -> Self {
        self.webhook = Some(sink);
        self
    }

    #[must_use]
    pub fn with_fallback(mut self, sink: Arc<dyn NotificationSink>) -> Self {
        self.fallback = Some(sink);
        self
    }
}

#[async_trait]
impl NotificationSink for RoutingNotificationSink {
    async fn send(&self, notification: Notification) -> Result<(), NotificationError> {
        let sink = match notification.channel {
            NotificationChannel::Email => self.email.as_ref(),
            NotificationChannel::Webhook => self.webhook.as_ref(),
            _ => self.fallback.as_ref(),
        }
        .ok_or_else(|| {
            NotificationError::Delivery(
                "no delivery adapter is configured for the notification channel".into(),
            )
        })?;
        sink.send(notification).await
    }
}
