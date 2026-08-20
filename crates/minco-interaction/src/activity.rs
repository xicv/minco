use minco_plugin_audit::{AuditEvent, AuditService};
use minco_plugin_events::{DomainEvent, EventServices, OutboxRecord};
use minco_plugin_notifications::{Notification, NotificationService};
use serde::{Deserialize, Serialize};

/// Stable public warning emitted by the explicitly post-commit recorder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostCommitActivityWarning {
    pub code: String,
    pub detail: String,
}

/// Compatibility helper for best-effort work after an operational commit.
///
/// This helper does not provide transactional audit durability. An adapter may
/// claim that only when it atomically commits the mutation and local audit or
/// outbox intent in its own store transaction.
#[derive(Debug, Clone)]
pub struct PostCommitActivityRecorder {
    pub audit: AuditService,
    pub events: EventServices,
    pub notifications: NotificationService,
}

impl PostCommitActivityRecorder {
    pub async fn record(
        &self,
        audit: AuditEvent,
        event: DomainEvent,
        notification: Option<Notification>,
    ) -> Vec<PostCommitActivityWarning> {
        let mut warnings = Vec::new();
        if self.audit.append(audit).await.is_err() {
            warnings.push(warning(
                "interaction_post_commit_audit_failed",
                "The operation completed, but its external audit record was not appended.",
            ));
        }
        if self
            .events
            .outbox
            .enqueue(OutboxRecord::pending(event))
            .await
            .is_err()
        {
            warnings.push(warning(
                "interaction_post_commit_event_failed",
                "The operation completed, but its external event was not queued.",
            ));
        }
        if let Some(notification) = notification
            && self.notifications.send(notification).await.is_err()
        {
            warnings.push(warning(
                "interaction_post_commit_notification_failed",
                "The operation completed, but its notification was not sent.",
            ));
        }
        warnings
    }
}

fn warning(code: &str, detail: &str) -> PostCommitActivityWarning {
    PostCommitActivityWarning {
        code: code.into(),
        detail: detail.into(),
    }
}
