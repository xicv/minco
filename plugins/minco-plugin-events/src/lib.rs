//! Domain event publishing and explicit transactional-outbox primitives.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
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
pub struct DomainEvent {
    pub id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub correlation_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl DomainEvent {
    pub fn new(
        event_type: impl Into<String>,
        aggregate_type: impl Into<String>,
        aggregate_id: impl Into<String>,
        correlation_id: Uuid,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            event_type: event_type.into(),
            aggregate_type: aggregate_type.into(),
            aggregate_id: aggregate_id.into(),
            correlation_id,
            occurred_at: Utc::now(),
            payload,
            metadata: BTreeMap::new(),
        }
    }
}

#[async_trait]
pub trait EventPublisher: Send + Sync + std::fmt::Debug {
    async fn publish(&self, event: &DomainEvent) -> Result<(), EventError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxStatus {
    Pending,
    Claimed,
    Published,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxRecord {
    pub event: DomainEvent,
    pub status: OutboxStatus,
    pub attempt_count: u32,
    pub available_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl OutboxRecord {
    pub fn pending(event: DomainEvent) -> Self {
        Self {
            event,
            status: OutboxStatus::Pending,
            attempt_count: 0,
            available_at: Utc::now(),
            claimed_by: None,
            claim_expires_at: None,
            last_error: None,
        }
    }
}

/// Persistence boundary for a transactional outbox.
///
/// Implementations must claim records atomically. Reading pending rows and updating them in a
/// second statement is not a conforming implementation because multiple workers could publish the
/// same event concurrently.
#[async_trait]
pub trait OutboxStore: Send + Sync + std::fmt::Debug {
    async fn enqueue(&self, record: OutboxRecord) -> Result<(), EventError>;

    async fn claim_pending(
        &self,
        worker_id: &str,
        limit: usize,
        claim_expires_at: DateTime<Utc>,
    ) -> Result<Vec<OutboxRecord>, EventError>;

    /// Atomically claims one known event for request-assisted publication.
    async fn claim_event(
        &self,
        event_id: Uuid,
        worker_id: &str,
        claim_expires_at: DateTime<Utc>,
    ) -> Result<Option<OutboxRecord>, EventError>;

    async fn mark_published(&self, event_id: Uuid, worker_id: &str) -> Result<(), EventError>;

    async fn mark_failed(
        &self,
        event_id: Uuid,
        worker_id: &str,
        error: String,
        retry_at: DateTime<Utc>,
    ) -> Result<(), EventError>;

    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, EventError>;
}

#[derive(Clone)]
pub struct EventServices {
    pub publisher: Arc<dyn EventPublisher>,
    pub outbox: Arc<dyn OutboxStore>,
}

impl std::fmt::Debug for EventServices {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EventServices")
            .finish_non_exhaustive()
    }
}

impl EventServices {
    /// Executes one bounded, explicit outbox-dispatch pass.
    ///
    /// Minco never schedules this automatically. Applications choose request-assisted delivery,
    /// an SQS-triggered worker, an operator command, or an explicitly costed recovery schedule.
    pub async fn dispatch_once(
        &self,
        worker_id: &str,
        limit: usize,
        lease: TimeDelta,
    ) -> Result<DispatchReport, EventError> {
        validate_worker(worker_id, limit, lease)?;
        let now = Utc::now();
        self.outbox.recover_expired_claims(now).await?;
        let claimed = self
            .outbox
            .claim_pending(worker_id, limit, now + lease)
            .await?;
        let mut report = DispatchReport {
            claimed: claimed.len(),
            ..DispatchReport::default()
        };
        for record in claimed {
            match self.publisher.publish(&record.event).await {
                Ok(()) => {
                    self.outbox
                        .mark_published(record.event.id, worker_id)
                        .await?;
                    report.published += 1;
                }
                Err(error) => {
                    self.outbox
                        .mark_failed(
                            record.event.id,
                            worker_id,
                            error.to_string(),
                            Utc::now() + TimeDelta::seconds(30),
                        )
                        .await?;
                    report.failed += 1;
                }
            }
        }
        Ok(report)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchReport {
    pub claimed: usize,
    pub published: usize,
    pub failed: usize,
}

#[derive(Debug, Default)]
pub struct MemoryEventBus {
    published: RwLock<Vec<DomainEvent>>,
    outbox: RwLock<BTreeMap<Uuid, OutboxRecord>>,
}

impl MemoryEventBus {
    pub async fn published(&self) -> Vec<DomainEvent> {
        self.published.read().await.clone()
    }

    pub async fn outbox_records(&self) -> Vec<OutboxRecord> {
        self.outbox.read().await.values().cloned().collect()
    }
}

#[async_trait]
impl EventPublisher for MemoryEventBus {
    async fn publish(&self, event: &DomainEvent) -> Result<(), EventError> {
        validate_event(event)?;
        self.published.write().await.push(event.clone());
        Ok(())
    }
}

#[async_trait]
impl OutboxStore for MemoryEventBus {
    async fn enqueue(&self, record: OutboxRecord) -> Result<(), EventError> {
        validate_event(&record.event)?;
        if record.status != OutboxStatus::Pending {
            return Err(EventError::InvalidOutboxState);
        }
        let mut outbox = self.outbox.write().await;
        if outbox.contains_key(&record.event.id) {
            return Err(EventError::DuplicateEvent(record.event.id));
        }
        outbox.insert(record.event.id, record);
        drop(outbox);
        Ok(())
    }

    async fn claim_pending(
        &self,
        worker_id: &str,
        limit: usize,
        claim_expires_at: DateTime<Utc>,
    ) -> Result<Vec<OutboxRecord>, EventError> {
        if worker_id.trim().is_empty() || limit == 0 || claim_expires_at <= Utc::now() {
            return Err(EventError::InvalidClaim);
        }
        let now = Utc::now();
        let mut outbox = self.outbox.write().await;
        let ids = outbox
            .values()
            .filter(|record| {
                matches!(record.status, OutboxStatus::Pending | OutboxStatus::Failed)
                    && record.available_at <= now
            })
            .take(limit)
            .map(|record| record.event.id)
            .collect::<Vec<_>>();
        let mut claimed = Vec::with_capacity(ids.len());
        for id in ids {
            let record = outbox.get_mut(&id).ok_or(EventError::MissingEvent(id))?;
            record.status = OutboxStatus::Claimed;
            record.claimed_by = Some(worker_id.to_owned());
            record.claim_expires_at = Some(claim_expires_at);
            record.attempt_count = record.attempt_count.saturating_add(1);
            claimed.push(record.clone());
        }
        drop(outbox);
        Ok(claimed)
    }

    async fn claim_event(
        &self,
        event_id: Uuid,
        worker_id: &str,
        claim_expires_at: DateTime<Utc>,
    ) -> Result<Option<OutboxRecord>, EventError> {
        if worker_id.trim().is_empty() || claim_expires_at <= Utc::now() {
            return Err(EventError::InvalidClaim);
        }
        let now = Utc::now();
        let mut outbox = self.outbox.write().await;
        let claimed = outbox.get_mut(&event_id).and_then(|record| {
            if !matches!(record.status, OutboxStatus::Pending | OutboxStatus::Failed)
                || record.available_at > now
            {
                return None;
            }
            record.status = OutboxStatus::Claimed;
            record.claimed_by = Some(worker_id.to_owned());
            record.claim_expires_at = Some(claim_expires_at);
            record.attempt_count = record.attempt_count.saturating_add(1);
            Some(record.clone())
        });
        drop(outbox);
        Ok(claimed)
    }

    async fn mark_published(&self, event_id: Uuid, worker_id: &str) -> Result<(), EventError> {
        let mut outbox = self.outbox.write().await;
        {
            let record = claimed_by(&mut outbox, event_id, worker_id)?;
            record.status = OutboxStatus::Published;
            record.claimed_by = None;
            record.claim_expires_at = None;
            record.last_error = None;
        }
        drop(outbox);
        Ok(())
    }

    async fn mark_failed(
        &self,
        event_id: Uuid,
        worker_id: &str,
        error: String,
        retry_at: DateTime<Utc>,
    ) -> Result<(), EventError> {
        let mut outbox = self.outbox.write().await;
        {
            let record = claimed_by(&mut outbox, event_id, worker_id)?;
            record.status = OutboxStatus::Failed;
            record.claimed_by = None;
            record.claim_expires_at = None;
            record.available_at = retry_at;
            record.last_error = Some(error);
        }
        drop(outbox);
        Ok(())
    }

    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, EventError> {
        let mut recovered = 0;
        let mut outbox = self.outbox.write().await;
        for record in outbox.values_mut() {
            if record.status == OutboxStatus::Claimed
                && record
                    .claim_expires_at
                    .is_some_and(|expires| expires <= now)
            {
                record.status = OutboxStatus::Pending;
                record.claimed_by = None;
                record.claim_expires_at = None;
                recovered += 1;
            }
        }
        drop(outbox);
        Ok(recovered)
    }
}

fn claimed_by<'a>(
    outbox: &'a mut BTreeMap<Uuid, OutboxRecord>,
    event_id: Uuid,
    worker_id: &str,
) -> Result<&'a mut OutboxRecord, EventError> {
    let record = outbox
        .get_mut(&event_id)
        .ok_or(EventError::MissingEvent(event_id))?;
    if record.status != OutboxStatus::Claimed || record.claimed_by.as_deref() != Some(worker_id) {
        return Err(EventError::ClaimOwnership {
            event_id,
            worker_id: worker_id.to_owned(),
        });
    }
    Ok(record)
}

#[derive(Debug, Clone)]
pub struct EventsPlugin {
    services: EventServices,
}

impl EventsPlugin {
    pub fn new(publisher: Arc<dyn EventPublisher>, outbox: Arc<dyn OutboxStore>) -> Self {
        Self {
            services: EventServices { publisher, outbox },
        }
    }

    pub fn memory() -> (Self, Arc<MemoryEventBus>) {
        let bus = Arc::new(MemoryEventBus::default());
        (Self::new(bus.clone(), bus.clone()), bus)
    }
}

impl Plugin for EventsPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("events").expect("static plugin ID"),
            Version::new(1, 0, 0),
            "Domain event publisher and transactional outbox ports without hidden schedules",
        );
        descriptor.documentation = Some("https://docs.rs/minco-plugin-events".into());
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Beta;
        descriptor
            .data_classes
            .extend([DataClass::Internal, DataClass::CustomerProvided]);
        descriptor.provides.extend([
            CapabilityProvision {
                name: "events.publish".into(),
                version: Version::new(1, 0, 0),
            },
            CapabilityProvision {
                name: "events.outbox".into(),
                version: Version::new(1, 0, 0),
            },
        ]);
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        context.services().insert(Arc::new(self.services.clone()))?;
        Ok(())
    }
}

fn validate_event(event: &DomainEvent) -> Result<(), EventError> {
    if event.event_type.trim().is_empty()
        || event.aggregate_type.trim().is_empty()
        || event.aggregate_id.trim().is_empty()
    {
        return Err(EventError::InvalidEvent);
    }
    Ok(())
}

fn validate_worker(worker_id: &str, limit: usize, lease: TimeDelta) -> Result<(), EventError> {
    if worker_id.trim().is_empty() || limit == 0 || lease <= TimeDelta::zero() {
        Err(EventError::InvalidClaim)
    } else {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("event type, aggregate type, and aggregate ID are required")]
    InvalidEvent,
    #[error("outbox records must be enqueued in pending state")]
    InvalidOutboxState,
    #[error("outbox claim requires a worker ID, positive limit, and future lease")]
    InvalidClaim,
    #[error("event already exists in the outbox: {0}")]
    DuplicateEvent(Uuid),
    #[error("event does not exist in the outbox: {0}")]
    MissingEvent(Uuid),
    #[error("worker {worker_id} does not own the claim for event {event_id}")]
    ClaimOwnership { event_id: Uuid, worker_id: String },
    #[error("event infrastructure failed: {0}")]
    Infrastructure(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event() -> DomainEvent {
        DomainEvent::new(
            "order.placed",
            "order",
            "order-1",
            Uuid::now_v7(),
            serde_json::json!({"total": 10}),
        )
    }

    #[tokio::test]
    async fn memory_outbox_claims_atomically_and_requires_claim_ownership() {
        let bus = MemoryEventBus::default();
        let event = event();
        bus.enqueue(OutboxRecord::pending(event.clone()))
            .await
            .unwrap();

        let first = bus
            .claim_pending("worker-a", 10, Utc::now() + TimeDelta::minutes(1))
            .await
            .unwrap();
        let second = bus
            .claim_pending("worker-b", 10, Utc::now() + TimeDelta::minutes(1))
            .await
            .unwrap();
        assert_eq!(first.len(), 1);
        assert!(second.is_empty());
        assert!(matches!(
            bus.mark_published(event.id, "worker-b").await,
            Err(EventError::ClaimOwnership { .. })
        ));
        bus.mark_published(event.id, "worker-a").await.unwrap();
        assert!(
            bus.claim_pending("worker-b", 10, Utc::now() + TimeDelta::minutes(1))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn dispatch_is_explicit_and_bounded() {
        let bus = Arc::new(MemoryEventBus::default());
        bus.enqueue(OutboxRecord::pending(event())).await.unwrap();
        let services = EventServices {
            publisher: bus.clone(),
            outbox: bus.clone(),
        };
        let report = services
            .dispatch_once("worker-a", 10, TimeDelta::minutes(1))
            .await
            .unwrap();
        assert_eq!(
            report,
            DispatchReport {
                claimed: 1,
                published: 1,
                failed: 0,
            }
        );
        assert_eq!(bus.published().await.len(), 1);
    }
}
