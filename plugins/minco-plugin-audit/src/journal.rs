//! Transactional audit-intent journal and explicit bounded relay.

use crate::{AuditLedgerError, AuditLedgerWriter, AuditRecordV2, MAX_AUDIT_BATCH_RECORDS};
use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditJournalStatus {
    Pending,
    Claimed,
    Failed,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditJournalEntry {
    pub record: AuditRecordV2,
    pub status: AuditJournalStatus,
    pub attempt_count: u32,
    pub encoded_bytes: usize,
    pub available_at: DateTime<Utc>,
    pub claimed_by: Option<String>,
    pub claim_expires_at: Option<DateTime<Utc>>,
    pub failure_code: Option<String>,
}

impl AuditJournalEntry {
    pub fn pending(record: AuditRecordV2) -> Result<Self, AuditLedgerError> {
        let encoded_bytes = record.validate()?;
        Ok(Self {
            record,
            status: AuditJournalStatus::Pending,
            attempt_count: 0,
            encoded_bytes,
            available_at: Utc::now(),
            claimed_by: None,
            claim_expires_at: None,
            failure_code: None,
        })
    }
}

#[async_trait]
pub trait AuditJournalStore: Send + Sync + std::fmt::Debug {
    async fn enqueue(&self, entry: AuditJournalEntry) -> Result<(), AuditLedgerError>;

    async fn claim_pending(
        &self,
        worker_id: &str,
        limit: usize,
        claim_expires_at: DateTime<Utc>,
    ) -> Result<Vec<AuditJournalEntry>, AuditLedgerError>;

    async fn mark_delivered(
        &self,
        event_ids: &[Uuid],
        worker_id: &str,
    ) -> Result<(), AuditLedgerError>;

    async fn mark_retry(
        &self,
        event_ids: &[Uuid],
        worker_id: &str,
        failure_code: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<(), AuditLedgerError>;

    async fn quarantine(
        &self,
        event_ids: &[Uuid],
        worker_id: &str,
        failure_code: &str,
    ) -> Result<(), AuditLedgerError>;

    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, AuditLedgerError>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AuditRelayReport {
    pub claimed: usize,
    pub inserted: usize,
    pub duplicates: usize,
    pub retried: usize,
    pub quarantined: usize,
}

#[derive(Debug, Clone)]
pub struct AuditRelay {
    journal: Arc<dyn AuditJournalStore>,
    ledger: Arc<dyn AuditLedgerWriter>,
}

impl AuditRelay {
    pub fn new(journal: Arc<dyn AuditJournalStore>, ledger: Arc<dyn AuditLedgerWriter>) -> Self {
        Self { journal, ledger }
    }

    /// Executes one explicit bounded delivery pass. Minco never schedules it.
    pub async fn dispatch_once(
        &self,
        worker_id: &str,
        limit: usize,
        lease: TimeDelta,
    ) -> Result<AuditRelayReport, AuditLedgerError> {
        validate_claim(worker_id, limit, lease)?;
        let now = Utc::now();
        self.journal.recover_expired_claims(now).await?;
        let entries = self
            .journal
            .claim_pending(worker_id, limit, now + lease)
            .await?;
        let mut report = AuditRelayReport {
            claimed: entries.len(),
            ..AuditRelayReport::default()
        };
        if entries.is_empty() {
            return Ok(report);
        }
        let event_ids = entries
            .iter()
            .map(|entry| entry.record.event_id)
            .collect::<Vec<_>>();
        let records = entries
            .into_iter()
            .map(|entry| entry.record)
            .collect::<Vec<_>>();
        match self.ledger.append_batch(&records).await {
            Ok(appended) => {
                self.journal.mark_delivered(&event_ids, worker_id).await?;
                report.inserted = appended.inserted;
                report.duplicates = appended.duplicates;
            }
            Err(error) if error.is_permanent() => {
                self.journal
                    .quarantine(&event_ids, worker_id, error.stable_code())
                    .await?;
                report.quarantined = event_ids.len();
            }
            Err(error) => {
                self.journal
                    .mark_retry(
                        &event_ids,
                        worker_id,
                        error.stable_code(),
                        Utc::now() + TimeDelta::seconds(30),
                    )
                    .await?;
                report.retried = event_ids.len();
            }
        }
        Ok(report)
    }
}

fn validate_claim(worker_id: &str, limit: usize, lease: TimeDelta) -> Result<(), AuditLedgerError> {
    if worker_id.trim().is_empty()
        || worker_id.len() > 128
        || worker_id.chars().any(char::is_control)
        || limit == 0
        || limit > MAX_AUDIT_BATCH_RECORDS
        || lease <= TimeDelta::zero()
        || lease > TimeDelta::hours(1)
    {
        return Err(AuditLedgerError::InvalidJournalClaim);
    }
    Ok(())
}

fn validate_transition(event_ids: &[Uuid], worker_id: &str) -> Result<(), AuditLedgerError> {
    if event_ids.is_empty()
        || event_ids.len() > MAX_AUDIT_BATCH_RECORDS
        || worker_id.trim().is_empty()
    {
        return Err(AuditLedgerError::InvalidJournalClaim);
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct MemoryAuditJournal {
    entries: RwLock<BTreeMap<Uuid, AuditJournalEntry>>,
}

impl MemoryAuditJournal {
    pub async fn entries(&self) -> Vec<AuditJournalEntry> {
        self.entries.read().await.values().cloned().collect()
    }
}

#[async_trait]
impl AuditJournalStore for MemoryAuditJournal {
    async fn enqueue(&self, entry: AuditJournalEntry) -> Result<(), AuditLedgerError> {
        if entry.status != AuditJournalStatus::Pending
            || entry.encoded_bytes != entry.record.validate()?
        {
            return Err(AuditLedgerError::InvalidJournalEntry);
        }
        let mut entries = self.entries.write().await;
        let result = match entries.get(&entry.record.event_id) {
            Some(existing) if existing.record == entry.record => Ok(()),
            Some(_) => Err(AuditLedgerError::EventConflict(entry.record.event_id)),
            None => {
                entries.insert(entry.record.event_id, entry);
                Ok(())
            }
        };
        drop(entries);
        result
    }

    async fn claim_pending(
        &self,
        worker_id: &str,
        limit: usize,
        claim_expires_at: DateTime<Utc>,
    ) -> Result<Vec<AuditJournalEntry>, AuditLedgerError> {
        let lease = claim_expires_at - Utc::now();
        validate_claim(worker_id, limit, lease)?;
        let now = Utc::now();
        let mut entries = self.entries.write().await;
        let ids = entries
            .values()
            .filter(|entry| {
                matches!(
                    entry.status,
                    AuditJournalStatus::Pending | AuditJournalStatus::Failed
                ) && entry.available_at <= now
            })
            .take(limit)
            .map(|entry| entry.record.event_id)
            .collect::<Vec<_>>();
        let mut claimed = Vec::with_capacity(ids.len());
        for id in ids {
            let entry = entries.get_mut(&id).expect("selected memory entry");
            entry.status = AuditJournalStatus::Claimed;
            entry.attempt_count = entry.attempt_count.saturating_add(1);
            entry.claimed_by = Some(worker_id.into());
            entry.claim_expires_at = Some(claim_expires_at);
            claimed.push(entry.clone());
        }
        drop(entries);
        Ok(claimed)
    }

    async fn mark_delivered(
        &self,
        event_ids: &[Uuid],
        worker_id: &str,
    ) -> Result<(), AuditLedgerError> {
        validate_transition(event_ids, worker_id)?;
        let mut entries = self.entries.write().await;
        require_claims(&entries, event_ids, worker_id)?;
        for event_id in event_ids {
            entries.remove(event_id);
        }
        drop(entries);
        Ok(())
    }

    async fn mark_retry(
        &self,
        event_ids: &[Uuid],
        worker_id: &str,
        failure_code: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<(), AuditLedgerError> {
        validate_transition(event_ids, worker_id)?;
        validate_failure_code(failure_code)?;
        let mut entries = self.entries.write().await;
        require_claims(&entries, event_ids, worker_id)?;
        for event_id in event_ids {
            let entry = entries.get_mut(event_id).expect("validated memory claim");
            entry.status = AuditJournalStatus::Failed;
            entry.available_at = retry_at;
            entry.claimed_by = None;
            entry.claim_expires_at = None;
            entry.failure_code = Some(failure_code.into());
        }
        drop(entries);
        Ok(())
    }

    async fn quarantine(
        &self,
        event_ids: &[Uuid],
        worker_id: &str,
        failure_code: &str,
    ) -> Result<(), AuditLedgerError> {
        validate_transition(event_ids, worker_id)?;
        validate_failure_code(failure_code)?;
        let mut entries = self.entries.write().await;
        require_claims(&entries, event_ids, worker_id)?;
        for event_id in event_ids {
            let entry = entries.get_mut(event_id).expect("validated memory claim");
            entry.status = AuditJournalStatus::Quarantined;
            entry.claimed_by = None;
            entry.claim_expires_at = None;
            entry.failure_code = Some(failure_code.into());
        }
        drop(entries);
        Ok(())
    }

    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, AuditLedgerError> {
        let mut entries = self.entries.write().await;
        let mut recovered = 0;
        for entry in entries.values_mut() {
            if entry.status == AuditJournalStatus::Claimed
                && entry.claim_expires_at.is_some_and(|expires| expires <= now)
            {
                entry.status = AuditJournalStatus::Failed;
                entry.available_at = now;
                entry.claimed_by = None;
                entry.claim_expires_at = None;
                entry.failure_code = Some("AUDIT-CLAIM-EXPIRED".into());
                recovered += 1;
            }
        }
        drop(entries);
        Ok(recovered)
    }
}

fn require_claims(
    entries: &BTreeMap<Uuid, AuditJournalEntry>,
    event_ids: &[Uuid],
    worker_id: &str,
) -> Result<(), AuditLedgerError> {
    if event_ids.iter().all(|event_id| {
        entries.get(event_id).is_some_and(|entry| {
            entry.status == AuditJournalStatus::Claimed
                && entry.claimed_by.as_deref() == Some(worker_id)
        })
    }) {
        Ok(())
    } else {
        Err(AuditLedgerError::JournalClaimLost)
    }
}

fn validate_failure_code(value: &str) -> Result<(), AuditLedgerError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
    {
        Err(AuditLedgerError::InvalidJournalEntry)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AuditActor, AuditLedgerError, AuditLedgerWriter, AuditResourceRef, MemoryAuditLedger,
    };
    use async_trait::async_trait;

    fn record() -> AuditRecordV2 {
        AuditRecordV2::new(
            "tenant",
            "order.created",
            AuditResourceRef::new("order", "one"),
            AuditActor::human("subject"),
            "placeOrder",
            Uuid::now_v7(),
        )
    }

    #[derive(Debug)]
    struct FailingLedger(AuditLedgerError);

    #[async_trait]
    impl AuditLedgerWriter for FailingLedger {
        async fn append_batch(
            &self,
            _records: &[AuditRecordV2],
        ) -> Result<crate::AuditAppendReport, AuditLedgerError> {
            match self.0 {
                AuditLedgerError::Infrastructure => Err(AuditLedgerError::Infrastructure),
                _ => Err(AuditLedgerError::InvalidRecord("invalid".into())),
            }
        }
    }

    #[tokio::test]
    async fn relay_deletes_only_after_idempotent_ledger_commit() {
        let journal = Arc::new(MemoryAuditJournal::default());
        let ledger = Arc::new(MemoryAuditLedger::default());
        let action = record();
        journal
            .enqueue(AuditJournalEntry::pending(action).unwrap())
            .await
            .unwrap();
        let report = AuditRelay::new(journal.clone(), ledger)
            .dispatch_once("worker", 10, TimeDelta::minutes(1))
            .await
            .unwrap();
        assert_eq!(report.inserted, 1);
        assert!(journal.entries().await.is_empty());
    }

    #[tokio::test]
    async fn transient_failure_retries_and_permanent_failure_quarantines() {
        for (error, expected) in [
            (AuditLedgerError::Infrastructure, AuditJournalStatus::Failed),
            (
                AuditLedgerError::InvalidRecord("invalid".into()),
                AuditJournalStatus::Quarantined,
            ),
        ] {
            let journal = Arc::new(MemoryAuditJournal::default());
            journal
                .enqueue(AuditJournalEntry::pending(record()).unwrap())
                .await
                .unwrap();
            AuditRelay::new(journal.clone(), Arc::new(FailingLedger(error)))
                .dispatch_once("worker", 10, TimeDelta::minutes(1))
                .await
                .unwrap();
            assert_eq!(journal.entries().await[0].status, expected);
        }
    }
}
