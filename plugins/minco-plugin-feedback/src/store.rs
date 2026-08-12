use crate::{FeedbackAccessToken, FeedbackId, FeedbackListFilter, FeedbackSummary, FeedbackThread};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    sync::Arc,
};
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, RwLock};

#[async_trait]
pub trait FeedbackStore: Send + Sync + std::fmt::Debug {
    async fn create(
        &self,
        thread: FeedbackThread,
        client_token_hash: String,
    ) -> Result<(), FeedbackStoreError>;

    async fn get(&self, id: FeedbackId) -> Result<Option<FeedbackThread>, FeedbackStoreError>;

    async fn get_for_client(
        &self,
        id: FeedbackId,
        client_token_hash: &str,
    ) -> Result<Option<FeedbackThread>, FeedbackStoreError>;

    async fn list(
        &self,
        filter: FeedbackListFilter,
    ) -> Result<Vec<FeedbackSummary>, FeedbackStoreError>;

    async fn save(
        &self,
        thread: FeedbackThread,
        expected_revision: u64,
    ) -> Result<(), FeedbackStoreError>;

    /// Bounded readiness check for deployment health reporting.
    async fn ready(&self) -> Result<(), FeedbackStoreError> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct FeedbackStoreService(pub Arc<dyn FeedbackStore>);

impl std::fmt::Debug for FeedbackStoreService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("FeedbackStoreService").finish()
    }
}

impl FeedbackStoreService {
    pub fn new(store: Arc<dyn FeedbackStore>) -> Self {
        Self(store)
    }

    pub async fn create(
        &self,
        thread: FeedbackThread,
        client_token_hash: String,
    ) -> Result<(), FeedbackStoreError> {
        self.0.create(thread, client_token_hash).await
    }

    pub async fn get(&self, id: FeedbackId) -> Result<Option<FeedbackThread>, FeedbackStoreError> {
        self.0.get(id).await
    }

    pub async fn get_for_client(
        &self,
        id: FeedbackId,
        client_token_hash: &str,
    ) -> Result<Option<FeedbackThread>, FeedbackStoreError> {
        self.0.get_for_client(id, client_token_hash).await
    }

    pub async fn list(
        &self,
        filter: FeedbackListFilter,
    ) -> Result<Vec<FeedbackSummary>, FeedbackStoreError> {
        self.0.list(filter).await
    }

    pub async fn save(
        &self,
        thread: FeedbackThread,
        expected_revision: u64,
    ) -> Result<(), FeedbackStoreError> {
        self.0.save(thread, expected_revision).await
    }

    pub async fn ready(&self) -> Result<(), FeedbackStoreError> {
        self.0.ready().await
    }
}

#[derive(Debug, Clone)]
struct MemoryFeedbackEntry {
    thread: FeedbackThread,
    client_token_hash: String,
}

#[derive(Debug, Default)]
pub struct MemoryFeedbackStore {
    entries: RwLock<BTreeMap<FeedbackId, MemoryFeedbackEntry>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FeedbackStoreOperation {
    Create,
    Get,
    GetForClient,
    List,
    Save,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedbackStoreAttempt {
    Create {
        id: FeedbackId,
    },
    Get {
        id: FeedbackId,
    },
    GetForClient {
        id: FeedbackId,
    },
    List,
    Save {
        id: FeedbackId,
        expected_revision: u64,
    },
    Ready,
}

/// Deterministic feedback-store fake with privacy-bounded attempt evidence.
///
/// The fake delegates successful behavior to [`MemoryFeedbackStore`]. Recorded
/// attempts deliberately omit feedback bodies and client-token hashes.
#[derive(Default)]
pub struct FakeFeedbackStore {
    inner: MemoryFeedbackStore,
    attempts: RwLock<Vec<FeedbackStoreAttempt>>,
    failures: Mutex<BTreeMap<FeedbackStoreOperation, VecDeque<String>>>,
}

impl FakeFeedbackStore {
    pub async fn fail_next(&self, operation: FeedbackStoreOperation, message: impl Into<String>) {
        self.failures
            .lock()
            .await
            .entry(operation)
            .or_default()
            .push_back(message.into());
    }

    pub async fn attempts(&self) -> Vec<FeedbackStoreAttempt> {
        self.attempts.read().await.clone()
    }

    async fn take_failure(&self, operation: FeedbackStoreOperation) -> Option<String> {
        let mut failures = self.failures.lock().await;
        let failure = failures.get_mut(&operation).and_then(VecDeque::pop_front);
        if failures.get(&operation).is_some_and(VecDeque::is_empty) {
            failures.remove(&operation);
        }
        drop(failures);
        failure
    }
}

impl fmt::Debug for FakeFeedbackStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FakeFeedbackStore")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl FeedbackStore for FakeFeedbackStore {
    async fn create(
        &self,
        thread: FeedbackThread,
        client_token_hash: String,
    ) -> Result<(), FeedbackStoreError> {
        self.attempts
            .write()
            .await
            .push(FeedbackStoreAttempt::Create { id: thread.id });
        if let Some(message) = self.take_failure(FeedbackStoreOperation::Create).await {
            return Err(FeedbackStoreError::Infrastructure(message));
        }
        self.inner.create(thread, client_token_hash).await
    }

    async fn get(&self, id: FeedbackId) -> Result<Option<FeedbackThread>, FeedbackStoreError> {
        self.attempts
            .write()
            .await
            .push(FeedbackStoreAttempt::Get { id });
        if let Some(message) = self.take_failure(FeedbackStoreOperation::Get).await {
            return Err(FeedbackStoreError::Infrastructure(message));
        }
        self.inner.get(id).await
    }

    async fn get_for_client(
        &self,
        id: FeedbackId,
        client_token_hash: &str,
    ) -> Result<Option<FeedbackThread>, FeedbackStoreError> {
        self.attempts
            .write()
            .await
            .push(FeedbackStoreAttempt::GetForClient { id });
        if let Some(message) = self
            .take_failure(FeedbackStoreOperation::GetForClient)
            .await
        {
            return Err(FeedbackStoreError::Infrastructure(message));
        }
        self.inner.get_for_client(id, client_token_hash).await
    }

    async fn list(
        &self,
        filter: FeedbackListFilter,
    ) -> Result<Vec<FeedbackSummary>, FeedbackStoreError> {
        self.attempts.write().await.push(FeedbackStoreAttempt::List);
        if let Some(message) = self.take_failure(FeedbackStoreOperation::List).await {
            return Err(FeedbackStoreError::Infrastructure(message));
        }
        self.inner.list(filter).await
    }

    async fn save(
        &self,
        thread: FeedbackThread,
        expected_revision: u64,
    ) -> Result<(), FeedbackStoreError> {
        self.attempts
            .write()
            .await
            .push(FeedbackStoreAttempt::Save {
                id: thread.id,
                expected_revision,
            });
        if let Some(message) = self.take_failure(FeedbackStoreOperation::Save).await {
            return Err(FeedbackStoreError::Infrastructure(message));
        }
        self.inner.save(thread, expected_revision).await
    }

    async fn ready(&self) -> Result<(), FeedbackStoreError> {
        self.attempts
            .write()
            .await
            .push(FeedbackStoreAttempt::Ready);
        if let Some(message) = self.take_failure(FeedbackStoreOperation::Ready).await {
            return Err(FeedbackStoreError::Infrastructure(message));
        }
        self.inner.ready().await
    }
}

#[async_trait]
impl FeedbackStore for MemoryFeedbackStore {
    async fn create(
        &self,
        thread: FeedbackThread,
        client_token_hash: String,
    ) -> Result<(), FeedbackStoreError> {
        let mut entries = self.entries.write().await;
        if entries.contains_key(&thread.id) {
            return Err(FeedbackStoreError::AlreadyExists(thread.id));
        }
        entries.insert(
            thread.id,
            MemoryFeedbackEntry {
                thread,
                client_token_hash,
            },
        );
        drop(entries);
        Ok(())
    }

    async fn get(&self, id: FeedbackId) -> Result<Option<FeedbackThread>, FeedbackStoreError> {
        Ok(self
            .entries
            .read()
            .await
            .get(&id)
            .map(|entry| entry.thread.clone()))
    }

    async fn get_for_client(
        &self,
        id: FeedbackId,
        client_token_hash: &str,
    ) -> Result<Option<FeedbackThread>, FeedbackStoreError> {
        Ok(self.entries.read().await.get(&id).and_then(|entry| {
            if constant_time_equals(&entry.client_token_hash, client_token_hash) {
                Some(entry.thread.clone())
            } else {
                None
            }
        }))
    }

    async fn list(
        &self,
        filter: FeedbackListFilter,
    ) -> Result<Vec<FeedbackSummary>, FeedbackStoreError> {
        let limit = filter.limit.clamp(1, 200);
        let mut threads = self
            .entries
            .read()
            .await
            .values()
            .map(|entry| entry.thread.clone())
            .filter(|thread| {
                filter.status.is_none_or(|status| thread.status == status)
                    && filter
                        .project_id
                        .as_deref()
                        .is_none_or(|project_id| thread.project_id == project_id)
            })
            .collect::<Vec<_>>();
        threads.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(threads
            .iter()
            .take(limit)
            .map(FeedbackSummary::from)
            .collect())
    }

    async fn save(
        &self,
        thread: FeedbackThread,
        expected_revision: u64,
    ) -> Result<(), FeedbackStoreError> {
        let mut entries = self.entries.write().await;
        {
            let entry = entries
                .get_mut(&thread.id)
                .ok_or(FeedbackStoreError::NotFound(thread.id))?;
            if entry.thread.revision != expected_revision {
                return Err(FeedbackStoreError::ConcurrentModification {
                    id: thread.id,
                    expected_revision,
                    actual_revision: entry.thread.revision,
                });
            }
            entry.thread = thread;
        }
        drop(entries);
        Ok(())
    }
}

pub fn hash_access_token(token: &FeedbackAccessToken) -> String {
    hex::encode(Sha256::digest(token.expose().as_bytes()))
}

fn constant_time_equals(left: &str, right: &str) -> bool {
    left.as_bytes().ct_eq(right.as_bytes()).into()
}

#[derive(Debug, thiserror::Error)]
pub enum FeedbackStoreError {
    #[error("feedback already exists: {0}")]
    AlreadyExists(FeedbackId),
    #[error("feedback was not found: {0}")]
    NotFound(FeedbackId),
    #[error(
        "feedback {id} changed concurrently: expected revision {expected_revision}, actual {actual_revision}"
    )]
    ConcurrentModification {
        id: FeedbackId,
        expected_revision: u64,
        actual_revision: u64,
    },
    #[error("feedback store failed: {0}")]
    Infrastructure(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CreateFeedbackInput, FeedbackContext, FeedbackKind, FeedbackPriority};
    use std::collections::BTreeSet;

    fn thread() -> FeedbackThread {
        FeedbackThread::create(CreateFeedbackInput {
            project_id: "example".into(),
            kind: FeedbackKind::Bug,
            priority: FeedbackPriority::Normal,
            title: "Problem".into(),
            description: "Something did not work.".into(),
            context: FeedbackContext {
                page_url: "https://example.test".into(),
                route_name: None,
                release_id: None,
                environment: None,
                request_id: None,
                user_agent: None,
                viewport: None,
                client_subject: None,
            },
            tags: BTreeSet::new(),
        })
        .unwrap()
    }

    #[tokio::test]
    async fn access_token_is_required_for_client_reads() {
        let store = MemoryFeedbackStore::default();
        let feedback = thread();
        let id = feedback.id;
        let token = FeedbackAccessToken::generate();
        store
            .create(feedback, hash_access_token(&token))
            .await
            .unwrap();
        assert!(
            store
                .get_for_client(id, &hash_access_token(&token))
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            store
                .get_for_client(id, &hash_access_token(&FeedbackAccessToken::generate()))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn optimistic_revision_prevents_lost_updates() {
        let store = MemoryFeedbackStore::default();
        let feedback = thread();
        let id = feedback.id;
        store
            .create(feedback.clone(), "token".into())
            .await
            .unwrap();
        let mut updated = feedback;
        updated.append_message(crate::FeedbackMessage::client("More detail").unwrap());
        store.save(updated.clone(), 1).await.unwrap();
        assert!(matches!(
            store.save(updated, 1).await,
            Err(FeedbackStoreError::ConcurrentModification { id: conflict_id, .. })
                if conflict_id == id
        ));
    }
}
