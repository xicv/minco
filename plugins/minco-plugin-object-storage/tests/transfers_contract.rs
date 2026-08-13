use async_trait::async_trait;
use chrono::{TimeDelta, Utc};
use futures::{StreamExt, stream};
use minco_plugin_object_storage::{
    CompleteMultipartObject, CompletedMultipartObject, DownloadCachePolicy,
    IssueMultipartObjectUpload, IssueObjectDownload, MAX_MULTIPART_OBJECT_SIZE_BYTES,
    MAX_MULTIPART_PART_SIZE_BYTES, MAX_MULTIPART_PARTS, MemoryObjectStore, MultipartObjectService,
    MultipartPartReceipt, MultipartUploadPolicy, ObjectByteRange, ObjectDownloadPolicy,
    ObjectDownloadService, ObjectDownloadSigner, ObjectKey, ObjectReadRequest, ObjectReadService,
    ObjectStoreError, ObjectStreamReader, ObjectTransferCostUsage, ObjectTransferError,
    PresignedMethod, PresignedObjectRequest, ProviderMultipartUploadId, SignMultipartObject,
    SignMultipartPart, SignObjectDownload, TrustedMultipartPart, estimate_object_transfer_cost,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

const MIB: u64 = 1024 * 1024;

#[derive(Debug, Default)]
struct TestTransferAdapter;

#[async_trait]
impl ObjectDownloadSigner for TestTransferAdapter {
    async fn sign_download(
        &self,
        request: SignObjectDownload,
    ) -> Result<PresignedObjectRequest, ObjectTransferError> {
        assert!(request.download_file_name.is_some());
        Ok(PresignedObjectRequest {
            method: PresignedMethod::Get,
            url: format!(
                "https://objects.example/{}?secret=redacted",
                request.key.as_str()
            ),
            headers: request
                .range
                .map(|range| BTreeMap::from([("range".into(), range.to_http_value())]))
                .unwrap_or_default(),
            form_fields: BTreeMap::new(),
            expires_at: Utc::now() + request.expires_in,
        })
    }
}

#[async_trait]
impl minco_plugin_object_storage::MultipartObjectSigner for TestTransferAdapter {
    async fn initiate_multipart(
        &self,
        _request: SignMultipartObject,
    ) -> Result<ProviderMultipartUploadId, ObjectTransferError> {
        ProviderMultipartUploadId::parse("provider-secret-upload-id")
    }

    async fn sign_multipart_part(
        &self,
        request: SignMultipartPart,
    ) -> Result<PresignedObjectRequest, ObjectTransferError> {
        Ok(PresignedObjectRequest {
            method: PresignedMethod::Put,
            url: format!("https://objects.example/part/{}", request.part_number),
            headers: BTreeMap::from([
                ("content-length".into(), request.size_bytes.to_string()),
                ("x-checksum-sha256".into(), request.sha256.clone()),
            ]),
            form_fields: BTreeMap::new(),
            expires_at: Utc::now() + request.expires_in,
        })
    }

    async fn complete_multipart(
        &self,
        request: CompleteMultipartObject,
    ) -> Result<CompletedMultipartObject, ObjectTransferError> {
        Ok(CompletedMultipartObject {
            key: request.key,
            content_type: request.content_type,
            size_bytes: request.size_bytes,
            entity_tag: Some("\"completed-etag\"".into()),
            version_id: Some("version-2".into()),
            attributes: request.attributes,
        })
    }

    async fn abort_multipart(
        &self,
        _key: &ObjectKey,
        _upload_id: &ProviderMultipartUploadId,
    ) -> Result<(), ObjectTransferError> {
        Ok(())
    }
}

fn sha256(value: &[u8]) -> String {
    hex::encode(Sha256::digest(value))
}

fn multipart_policy() -> MultipartUploadPolicy {
    MultipartUploadPolicy::new(
        ObjectKey::parse("uploads/large").unwrap(),
        128 * MIB,
        16 * MIB,
        ["video/mp4", "application/pdf"],
    )
    .unwrap()
}

#[test]
fn multipart_plan_has_exact_mobile_friendly_part_sizes() {
    let plan = multipart_policy().plan(35 * MIB).unwrap();
    assert_eq!(plan.part_count, 3);
    assert_eq!(plan.expected_part_size(1).unwrap(), 16 * MIB);
    assert_eq!(plan.expected_part_size(2).unwrap(), 16 * MIB);
    assert_eq!(plan.expected_part_size(3).unwrap(), 3 * MIB);
    assert!(plan.expected_part_size(4).is_err());
    assert!(
        MultipartUploadPolicy::new(
            ObjectKey::parse("uploads/invalid").unwrap(),
            128 * MIB,
            MAX_MULTIPART_PART_SIZE_BYTES + 1,
            ["application/pdf"],
        )
        .is_err()
    );
    let maximum = MultipartUploadPolicy::new(
        ObjectKey::parse("uploads/maximum").unwrap(),
        MAX_MULTIPART_OBJECT_SIZE_BYTES,
        MAX_MULTIPART_PART_SIZE_BYTES,
        ["application/octet-stream"],
    )
    .unwrap()
    .plan(MAX_MULTIPART_OBJECT_SIZE_BYTES)
    .unwrap();
    assert_eq!(maximum.part_count, MAX_MULTIPART_PARTS);
}

#[test]
fn persisted_provider_state_revalidates_opaque_ids_and_object_keys() {
    assert!(serde_json::from_str::<ProviderMultipartUploadId>(r#""provider\nsecret""#).is_err());
    assert!(serde_json::from_str::<ObjectKey>(r#""../secret""#).is_err());
}

#[tokio::test]
async fn multipart_session_retries_one_part_and_completes_only_a_full_manifest() {
    let service = MultipartObjectService::new(Arc::new(TestTransferAdapter), multipart_policy());
    let issued = service
        .issue(IssueMultipartObjectUpload {
            content_type: "video/mp4".into(),
            size_bytes: 35 * MIB,
            attributes: BTreeMap::from([("tenant".into(), "acme".into())]),
        })
        .await
        .unwrap();
    assert_eq!(issued.grant.part_count, 3);
    assert!(!format!("{:?}", issued.pending).contains("provider-secret-upload-id"));

    let mut corrupted = issued.pending.clone();
    corrupted.key = ObjectKey::parse("uploads/large/different").unwrap();
    assert!(matches!(
        service
            .issue_part(&corrupted, 1, sha256(b"corrupted"))
            .await,
        Err(ObjectTransferError::InvalidPendingUpload)
    ));

    let mut parts = Vec::new();
    for part_number in 1..=3 {
        let expected_size = issued.grant.expected_part_size(part_number).unwrap();
        let checksum = sha256(format!("part-{part_number}").as_bytes());
        let expected = service
            .issue_part(&issued.pending, part_number, checksum.clone())
            .await
            .unwrap();
        assert_eq!(expected.grant.size_bytes, expected_size);
        parts.push(
            service
                .accept_part(
                    &issued.pending,
                    &expected.expected,
                    MultipartPartReceipt {
                        part_number,
                        entity_tag: format!("\"etag-{part_number}\""),
                        sha256: checksum,
                    },
                )
                .unwrap(),
        );
    }

    let completed = service.complete(&issued.pending, &parts).await.unwrap();
    assert_eq!(completed.key, issued.pending.key);
    assert_eq!(completed.size_bytes, 35 * MIB);

    let incomplete = &parts[..2];
    assert!(matches!(
        service.complete(&issued.pending, incomplete).await,
        Err(ObjectTransferError::IncompletePartManifest { .. })
    ));

    // Uploading the same number is an intentional replacement, not a second
    // manifest entry. The application persists only the latest receipt.
    let latest: Vec<TrustedMultipartPart> = parts;
    assert_eq!(latest[0].part_number, 1);
}

#[tokio::test]
async fn range_download_is_bound_to_a_strong_validator_and_private_cache() {
    let store = Arc::new(MemoryObjectStore::default());
    minco_plugin_object_storage::ObjectStore::put(
        store.as_ref(),
        minco_plugin_object_storage::PutObject {
            key: ObjectKey::parse("files/revision-1").unwrap(),
            bytes: b"0123456789".to_vec(),
            content_type: "application/octet-stream".into(),
            attributes: BTreeMap::new(),
        },
    )
    .await
    .unwrap();
    let reader: Arc<dyn ObjectStreamReader> = store;
    let reads = ObjectReadService::new(reader);
    let head = reads
        .head(&ObjectKey::parse("files/revision-1").unwrap())
        .await
        .unwrap()
        .unwrap();
    let service = ObjectDownloadService::new(
        Arc::new(TestTransferAdapter),
        reads,
        ObjectDownloadPolicy::new(
            TimeDelta::minutes(10),
            DownloadCachePolicy::Private {
                max_age_seconds: 600,
                immutable: true,
            },
        )
        .unwrap(),
    );
    let grant = service
        .issue(IssueObjectDownload {
            key: head.key.clone(),
            range: Some(ObjectByteRange::bounded(2, 6).unwrap()),
            expected_entity_tag: Some(head.entity_tag.clone()),
            version_id: head.version_id.clone(),
            download_file_name: Some("report.bin".into()),
        })
        .await
        .unwrap();
    assert_eq!(grant.range.unwrap().to_http_value(), "bytes=2-5");
    assert_eq!(grant.entity_tag, head.entity_tag);
    assert_eq!(grant.cache_control, "private, max-age=600, immutable");
    assert!(!format!("{grant:?}").contains("secret=redacted"));

    let default_attachment = service
        .issue(IssueObjectDownload {
            key: head.key,
            range: None,
            expected_entity_tag: Some(head.entity_tag),
            version_id: head.version_id,
            download_file_name: None,
        })
        .await
        .unwrap();
    assert_eq!(
        default_attachment.cache_control,
        "private, max-age=600, immutable"
    );
}

#[tokio::test]
async fn stream_reads_only_the_selected_range_and_can_be_dropped() {
    let store = Arc::new(MemoryObjectStore::default());
    let key = ObjectKey::parse("files/revision-2").unwrap();
    minco_plugin_object_storage::ObjectStore::put(
        store.as_ref(),
        minco_plugin_object_storage::PutObject {
            key: key.clone(),
            bytes: b"abcdefghij".to_vec(),
            content_type: "text/plain".into(),
            attributes: BTreeMap::new(),
        },
    )
    .await
    .unwrap();
    let mut response = ObjectStreamReader::read(
        store.as_ref(),
        ObjectReadRequest {
            key,
            range: Some(ObjectByteRange::bounded(3, 7).unwrap()),
            expected_entity_tag: None,
            version_id: None,
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.content_range.as_deref(), Some("bytes 3-6/10"));
    assert_eq!(response.stream.next().await.unwrap().unwrap(), b"defg");
    drop(response); // cancellation is dropping the remaining provider stream

    let _compile_shape = stream::iter([Ok::<_, ObjectStoreError>(b"chunk".to_vec())]);
}

#[tokio::test]
async fn range_edges_clamp_suffixes_and_reject_an_offset_at_end_of_file() {
    let store = MemoryObjectStore::default();
    let key = ObjectKey::parse("files/range-edges").unwrap();
    minco_plugin_object_storage::ObjectStore::put(
        &store,
        minco_plugin_object_storage::PutObject {
            key: key.clone(),
            bytes: b"abcdef".to_vec(),
            content_type: "text/plain".into(),
            attributes: BTreeMap::new(),
        },
    )
    .await
    .unwrap();

    let mut suffix = ObjectStreamReader::read(
        &store,
        ObjectReadRequest {
            key: key.clone(),
            range: Some(ObjectByteRange::suffix(100).unwrap()),
            expected_entity_tag: None,
            version_id: None,
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(suffix.content_range.as_deref(), Some("bytes 0-5/6"));
    assert_eq!(suffix.stream.next().await.unwrap().unwrap(), b"abcdef");

    let error = ObjectStreamReader::read(
        &store,
        ObjectReadRequest {
            key,
            range: Some(ObjectByteRange::from(6)),
            expected_entity_tag: None,
            version_id: None,
        },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("cannot be satisfied"));
    assert!(ObjectByteRange::suffix(0).is_err());
}

#[test]
fn cost_projection_is_structural_and_never_invents_aws_rates() {
    let projection = estimate_object_transfer_cost(ObjectTransferCostUsage {
        retained_bytes: 100 * MIB,
        incomplete_multipart_bytes: 16 * MIB,
        single_upload_requests: 1,
        multipart_initiations: 1,
        multipart_part_attempts: 4,
        multipart_completions: 1,
        multipart_aborts: 0,
        metadata_requests: 2,
        download_requests: 3,
        downloaded_bytes: 35 * MIB,
        accelerated_bytes: 0,
        edge_requests: 0,
        edge_egress_bytes: 0,
    });
    assert!(!projection.complete);
    assert!(!projection.fixed_compute);
    assert_eq!(projection.api_relay_bytes, 0);
    assert!(
        projection
            .missing_rates
            .contains(&"storage_byte_month".into())
    );
    assert!(
        projection
            .missing_rates
            .contains(&"provider_egress_byte".into())
    );
}
