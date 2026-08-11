use crate::{FeedbackAccessToken, FeedbackId, FeedbackListFilter, FeedbackSummary, FeedbackThread};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;

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
