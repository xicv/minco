#![cfg(all(feature = "s3", feature = "sqs"))]

use aws_credential_types::{Credentials, provider::SharedCredentialsProvider};
use chrono::TimeDelta;
use minco_aws_adapters::{s3::S3Addressing, s3_storage::S3ObjectStorage, sqs::SqsEventPublisher};
use minco_core::{PluginId, PluginManager, PluginSelection};
use minco_plugin_events::{DomainEvent, EventPublisher};
use minco_plugin_object_storage::{
    IssueObjectUpload, ObjectAccessSigner, ObjectKey, ObjectStore, ObjectUploadError,
    ObjectUploadPolicy, ObjectUploadService, PresignGetObject, PresignPutObject, PresignedMethod,
    PutObject,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires the bounded Rustack smoke environment"]
async fn s3_and_sqs_adapters_use_standard_sdk_endpoints() {
    let endpoint = std::env::var("AWS_ENDPOINT_URL").expect("AWS_ENDPOINT_URL");
    let bucket = std::env::var("MINCO_RUSTACK_BUCKET").expect("MINCO_RUSTACK_BUCKET");
    let queue_url = std::env::var("MINCO_RUSTACK_QUEUE_URL").expect("MINCO_RUSTACK_QUEUE_URL");
    let region = std::env::var("AWS_DEFAULT_REGION").expect("AWS_DEFAULT_REGION");
    let access_key = std::env::var("AWS_ACCESS_KEY_ID").expect("AWS_ACCESS_KEY_ID");
    let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY").expect("AWS_SECRET_ACCESS_KEY");
    let credentials = SharedCredentialsProvider::new(Credentials::new(
        access_key,
        secret_key,
        None,
        None,
        "minco-rustack-smoke",
    ));
    let shared = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new(region.clone()))
        .credentials_provider(credentials.clone())
        .endpoint_url(&endpoint)
        .load()
        .await;
    let storage = S3ObjectStorage::from_sdk_builder(
        aws_sdk_s3::config::Builder::from(&shared),
        credentials,
        &bucket,
        "adapter",
        S3Addressing::new(region, Some(endpoint)).unwrap(),
    )
    .unwrap();
    let adapter = storage.adapter();

    let server_key = ObjectKey::parse("server.txt").unwrap();
    let metadata = adapter
        .put(PutObject {
            key: server_key.clone(),
            bytes: b"rustack-server-adapter".to_vec(),
            content_type: "text/plain".into(),
            attributes: BTreeMap::from([("source".into(), "rustack".into())]),
        })
        .await
        .unwrap();
    assert_eq!(metadata.attributes["source"], "rustack");
    assert_eq!(
        adapter.get(&server_key).await.unwrap().unwrap().bytes,
        b"rustack-server-adapter"
    );

    let direct_key = ObjectKey::parse("direct.txt").unwrap();
    let signed_post = adapter
        .sign_put(PresignPutObject {
            key: direct_key.clone(),
            content_type: "text/plain".into(),
            maximum_size_bytes: 1024,
            expires_in: TimeDelta::minutes(5),
            attributes: BTreeMap::from([("source".into(), "direct".into())]),
        })
        .await
        .unwrap();
    assert_eq!(signed_post.method, PresignedMethod::Post);
    let mut form = reqwest::multipart::Form::new();
    for (name, value) in signed_post.form_fields {
        form = form.text(name, value);
    }
    form = form.part(
        "file",
        reqwest::multipart::Part::bytes(b"rustack-direct-adapter".to_vec())
            .mime_str("text/plain")
            .unwrap(),
    );
    let response = reqwest::Client::new()
        .post(signed_post.url)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success(), "{}", response.status());
    assert_eq!(
        adapter.get(&direct_key).await.unwrap().unwrap().bytes,
        b"rustack-direct-adapter"
    );

    let managed_body = b"rustack-managed-upload";
    let managed_sha256 = hex::encode(Sha256::digest(managed_body));
    let policy =
        ObjectUploadPolicy::new(ObjectKey::parse("managed").unwrap(), 1024, ["text/plain"])
            .unwrap();
    let mut manager = PluginManager::default();
    manager.register(storage.plugin(policy).unwrap()).unwrap();
    let mut selection = PluginSelection::default();
    selection
        .enabled
        .insert(PluginId::new("object-storage").unwrap());
    let application = manager.compose(&selection).unwrap();
    let uploads = application.services.get::<ObjectUploadService>().unwrap();
    let issued = uploads
        .issue(IssueObjectUpload {
            content_type: "text/plain".into(),
            size_bytes: managed_body.len() as u64,
            sha256: managed_sha256.clone(),
            attributes: BTreeMap::from([("source".into(), "managed-rustack".into())]),
        })
        .await
        .unwrap();
    assert_eq!(issued.grant.request.method, PresignedMethod::Post);
    assert!(
        !issued
            .grant
            .key
            .as_str()
            .rsplit('/')
            .next()
            .unwrap()
            .contains('.'),
        "managed upload keys must not inherit a client file extension"
    );
    let mut managed_form = reqwest::multipart::Form::new();
    for (name, value) in issued.grant.request.form_fields.clone() {
        managed_form = managed_form.text(name, value);
    }
    managed_form = managed_form.part(
        "file",
        reqwest::multipart::Part::bytes(managed_body.to_vec())
            .mime_str("text/plain")
            .unwrap(),
    );
    let managed_response = reqwest::Client::new()
        .post(&issued.grant.request.url)
        .multipart(managed_form)
        .send()
        .await
        .unwrap();
    assert!(
        managed_response.status().is_success(),
        "{}",
        managed_response.status()
    );
    match uploads.verify(&issued.pending).await {
        Ok(verified) => {
            assert_eq!(verified.key, issued.grant.key);
            assert_eq!(verified.metadata.content_type, "text/plain");
            assert_eq!(verified.metadata.size_bytes, managed_body.len() as u64);
            assert_eq!(
                verified.metadata.sha256.as_deref(),
                Some(managed_sha256.as_str())
            );
            assert_eq!(verified.metadata.attributes["source"], "managed-rustack");
        }
        // Rustack currently accepts the signed multipart POST but does not
        // reproduce S3's checksum metadata contract. The managed path must
        // fail closed; exact policy/signature units plus the ignored real-S3
        // conformance test cover successful provider verification.
        Err(ObjectUploadError::ChecksumMismatch) => {}
        Err(error) => panic!("unexpected managed verification result: {error}"),
    }

    let signed_get = adapter
        .sign_get(PresignGetObject {
            key: direct_key.clone(),
            expires_in: TimeDelta::minutes(5),
            download_file_name: Some("direct.txt".into()),
        })
        .await
        .unwrap();
    let mut request = reqwest::Client::new().get(signed_get.url);
    for (name, value) in signed_get.headers {
        request = request.header(name, value);
    }
    assert_eq!(
        request
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .as_ref(),
        b"rustack-direct-adapter"
    );
    assert!(adapter.delete(&server_key).await.unwrap());
    assert!(adapter.delete(&direct_key).await.unwrap());
    assert!(adapter.delete(&issued.grant.key).await.unwrap());

    let sqs = aws_sdk_sqs::Client::new(&shared);
    let publisher = SqsEventPublisher::new(sqs.clone(), &queue_url, false).unwrap();
    let event = DomainEvent::new(
        "feedback.created",
        "feedback",
        "rustack",
        Uuid::now_v7(),
        serde_json::json!({"provider": "rustack"}),
    );
    publisher.publish(&event).await.unwrap();
    let messages = sqs
        .receive_message()
        .queue_url(queue_url)
        .max_number_of_messages(10)
        .wait_time_seconds(1)
        .send()
        .await
        .unwrap();
    assert!(messages.messages().iter().any(|message| {
        message
            .body()
            .and_then(|body| serde_json::from_str::<DomainEvent>(body).ok())
            .is_some_and(|received| received.id == event.id)
    }));
}
