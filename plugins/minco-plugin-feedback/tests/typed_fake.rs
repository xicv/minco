use minco_plugin_feedback::{
    CreateFeedbackInput, FakeFeedbackStore, FeedbackContext, FeedbackKind, FeedbackPriority,
    FeedbackStore, FeedbackStoreAttempt, FeedbackStoreError, FeedbackStoreOperation,
    FeedbackThread,
};
use std::collections::BTreeSet;

fn thread() -> FeedbackThread {
    FeedbackThread::create(CreateFeedbackInput {
        project_id: "example".to_owned(),
        kind: FeedbackKind::Bug,
        priority: FeedbackPriority::Normal,
        title: "private title".to_owned(),
        description: "private feedback body".to_owned(),
        context: FeedbackContext {
            page_url: "https://example.test/orders".to_owned(),
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
async fn fake_store_records_safe_attempts_and_failed_create_does_not_persist() {
    let store = FakeFeedbackStore::default();
    let thread = thread();
    let id = thread.id;
    store
        .fail_next(FeedbackStoreOperation::Create, "temporarily unavailable")
        .await;

    assert!(matches!(
        store.create(thread.clone(), "token-hash-secret".to_owned()).await,
        Err(FeedbackStoreError::Infrastructure(message)) if message == "temporarily unavailable"
    ));
    assert!(store.get(id).await.unwrap().is_none());
    store
        .create(thread, "token-hash-secret".to_owned())
        .await
        .unwrap();
    assert!(store.get(id).await.unwrap().is_some());

    assert!(matches!(store.attempts().await.as_slice(), [
        FeedbackStoreAttempt::Create { id: first },
        FeedbackStoreAttempt::Get { id: first_read },
        FeedbackStoreAttempt::Create { id: second },
        FeedbackStoreAttempt::Get { id: second_read },
    ] if first == &id && first_read == &id && second == &id && second_read == &id));

    let debug = format!("{store:?}");
    for private in [
        "private title",
        "private feedback body",
        "token-hash-secret",
    ] {
        assert!(!debug.contains(private));
    }
}
