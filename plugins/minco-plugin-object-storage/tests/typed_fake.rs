use minco_plugin_object_storage::{
    FakeObjectStore, ObjectKey, ObjectStore, ObjectStoreAttempt, ObjectStoreError,
    ObjectStoreOperation, PutObject,
};
use std::collections::BTreeMap;

fn object(key: &ObjectKey) -> PutObject {
    PutObject {
        key: key.clone(),
        bytes: b"private-object-bytes".to_vec(),
        content_type: "application/octet-stream".to_owned(),
        attributes: BTreeMap::from([("token".to_owned(), "attribute-secret".to_owned())]),
    }
}

#[tokio::test]
async fn fake_store_records_attempts_and_failed_put_does_not_persist() {
    let store = FakeObjectStore::default();
    let key = ObjectKey::parse("orders/order-1/export").unwrap();
    store
        .fail_next(ObjectStoreOperation::Put, "temporarily unavailable")
        .await;

    assert!(matches!(
        store.put(object(&key)).await,
        Err(ObjectStoreError::Store(message)) if message == "temporarily unavailable"
    ));
    assert!(store.get(&key).await.unwrap().is_none());
    store.put(object(&key)).await.unwrap();
    assert_eq!(
        store.get(&key).await.unwrap().unwrap().bytes,
        b"private-object-bytes"
    );

    let attempts = store.attempts().await;
    assert!(matches!(attempts.as_slice(), [
        ObjectStoreAttempt::Put(first),
        ObjectStoreAttempt::Get(first_read),
        ObjectStoreAttempt::Put(second),
        ObjectStoreAttempt::Get(second_read),
    ] if first == &object(&key) && first_read == &key && second == &object(&key) && second_read == &key));

    let attempt_debug = format!("{:?}", attempts[0]);
    assert!(!attempt_debug.contains("private-object-bytes"));
    assert!(!attempt_debug.contains("attribute-secret"));

    let debug = format!("{store:?}");
    assert!(!debug.contains("private-object-bytes"));
    assert!(!debug.contains("attribute-secret"));
}
