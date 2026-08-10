#![cfg(any(feature = "postgres", feature = "sqlite"))]

use minco_plugin_feedback::{
    CreateFeedbackInput, FeedbackAccessToken, FeedbackAiContext, FeedbackAttachment,
    FeedbackAttachmentKind, FeedbackContext, FeedbackId, FeedbackKind, FeedbackListFilter,
    FeedbackMessage, FeedbackPriority, FeedbackStatus, FeedbackStore, FeedbackStoreError,
    FeedbackThread, hash_access_token,
};
use std::collections::BTreeSet;
use uuid::Uuid;

fn thread(project_id: &str, title: &str) -> FeedbackThread {
    FeedbackThread::create(CreateFeedbackInput {
        project_id: project_id.into(),
        kind: FeedbackKind::Bug,
        priority: FeedbackPriority::High,
        title: title.into(),
        description: "The feedback persistence adapter must preserve this report.".into(),
        context: FeedbackContext {
            page_url: "https://example.test/review".into(),
            route_name: Some("review".into()),
            release_id: Some("test-release".into()),
            environment: Some("test".into()),
            request_id: Some("request-persistence".into()),
            user_agent: None,
            viewport: None,
            client_subject: None,
        },
        tags: BTreeSet::from(["persistence".into()]),
    })
    .expect("the persistence test fixture must be valid")
}

async fn verify_store(store: &impl FeedbackStore) -> Vec<FeedbackId> {
    store.ready().await.expect("the database must be ready");

    let access_token = FeedbackAccessToken::generate();
    let token_hash = hash_access_token(&access_token);
    let first = thread("persistence-test", "Durable feedback");
    let first_id = first.id;
    store
        .create(first.clone(), token_hash.clone())
        .await
        .expect("creating feedback must succeed");

    assert!(matches!(
        store.create(first.clone(), token_hash.clone()).await,
        Err(FeedbackStoreError::AlreadyExists(id)) if id == first_id
    ));
    assert_eq!(
        store.get(first_id).await.expect("read must succeed"),
        Some(first.clone())
    );
    assert_eq!(
        store
            .get_for_client(first_id, &token_hash)
            .await
            .expect("authorized client read must succeed"),
        Some(first.clone())
    );
    assert!(
        store
            .get_for_client(
                first_id,
                &hash_access_token(&FeedbackAccessToken::generate()),
            )
            .await
            .expect("unauthorized client read must fail closed without an error")
            .is_none()
    );

    let second = thread("other-project", "Filtered feedback");
    let second_id = second.id;
    store
        .create(second, "other-token-hash".into())
        .await
        .expect("creating the filter control record must succeed");

    let filtered = store
        .list(FeedbackListFilter {
            status: Some(FeedbackStatus::New),
            project_id: Some("persistence-test".into()),
            limit: 10,
        })
        .await
        .expect("listing feedback must succeed");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, first_id);

    let mut updated = first;
    updated.append_message(
        FeedbackMessage::client("The report still reproduces.")
            .expect("the update fixture must be valid"),
    );
    updated.append_message(
        FeedbackMessage::developer(Some("developer".into()), "Private triage note", false)
            .expect("the internal-note fixture must be valid"),
    );
    updated.append_message(
        FeedbackMessage::developer(
            Some("developer".into()),
            "Please provide the affected order identifier.",
            true,
        )
        .expect("the clarification fixture must be valid"),
    );
    updated.add_attachment(FeedbackAttachment {
        id: Uuid::now_v7(),
        kind: FeedbackAttachmentKind::Screenshot,
        object_key: format!("feedback/{first_id}/screenshot.png"),
        file_name: "screenshot.png".into(),
        content_type: "image/png".into(),
        size_bytes: 3,
        sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        created_at: chrono::Utc::now(),
        transcript: None,
    });
    updated
        .transition(FeedbackStatus::NeedsClarification, None)
        .expect("the persisted workflow transition must be valid");
    store
        .save(updated.clone(), 1)
        .await
        .expect("saving the expected revision must succeed");
    let persisted = store
        .get(first_id)
        .await
        .expect("updated read must succeed")
        .expect("updated feedback must remain present");
    assert_eq!(persisted, updated);
    assert_eq!(persisted.status, FeedbackStatus::NeedsClarification);
    assert_eq!(persisted.attachments.len(), 1);
    assert_eq!(persisted.attachments[0].content_type, "image/png");
    assert_eq!(persisted.client_view().messages.len(), 2);
    assert_eq!(persisted.messages.len(), 3);

    let ai_context = FeedbackAiContext::from_thread(persisted.clone());
    assert_eq!(ai_context, FeedbackAiContext::from_thread(persisted));
    assert!(ai_context.to_markdown().contains("Private triage note"));
    assert_eq!(ai_context.unresolved_questions.len(), 1);

    let actual_revision = updated.revision;
    assert!(matches!(
        store.save(updated, 1).await,
        Err(FeedbackStoreError::ConcurrentModification {
            id,
            expected_revision: 1,
            actual_revision: observed_revision,
        }) if id == first_id && observed_revision == actual_revision
    ));

    vec![first_id, second_id]
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn sqlite_adapter_runs_migrations_and_store_contract_against_sqlite() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("the in-memory SQLite database must connect");
    let store = minco_plugin_feedback::SqliteFeedbackStore::new(pool);
    store
        .migrate()
        .await
        .expect("the SQLite feedback migration must apply");
    let feedback_history_exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_minco_feedback_migrations'",
    )
    .fetch_one(store.pool())
    .await
    .expect("the SQLite migration history table must be inspectable");
    assert_eq!(feedback_history_exists, 1);
    let _ = verify_store(&store).await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_adapter_runs_migrations_and_store_contract_when_configured() {
    let Ok(database_url) = std::env::var("MINCO_FEEDBACK_TEST_POSTGRES_URL") else {
        eprintln!(
            "skipping PostgreSQL adapter contract; set MINCO_FEEDBACK_TEST_POSTGRES_URL to run it"
        );
        return;
    };

    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("the PostgreSQL test database must connect");
    let store = minco_plugin_feedback::PostgresFeedbackStore::new(pool);
    store
        .migrate()
        .await
        .expect("the PostgreSQL feedback migration must apply");
    let feedback_history_exists = sqlx::query_scalar::<_, bool>(
        "SELECT to_regclass('_minco_feedback_migrations') IS NOT NULL",
    )
    .fetch_one(store.pool())
    .await
    .expect("the PostgreSQL migration history table must be inspectable");
    assert!(feedback_history_exists);
    let ids = verify_store(&store).await;

    for id in ids {
        sqlx::query("DELETE FROM minco_feedback_threads WHERE id = $1")
            .bind(id.0)
            .execute(store.pool())
            .await
            .expect("the PostgreSQL test record must be removed");
    }
}
