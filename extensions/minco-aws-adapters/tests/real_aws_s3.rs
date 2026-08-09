#![cfg(feature = "s3")]

use aws_sdk_s3::{primitives::ByteStream, types::ServerSideEncryption};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use minco_aws_adapters::{s3::S3Addressing, s3_storage::S3ObjectStorage};
use minco_core::{PluginId, PluginManager, PluginSelection};
use minco_plugin_object_storage::{
    IssueObjectUpload, IssuedObjectUpload, ObjectKey, ObjectMetadataReader, ObjectStore,
    ObjectUploadError, ObjectUploadPolicy, ObjectUploadService,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, error::Error, fs::OpenOptions, io::Write, path::Path, sync::Arc};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

#[tokio::test]
#[ignore = "requires a bounded pre-existing S3 bucket and journaled real-AWS credentials"]
async fn managed_uploads_conform_on_bounded_real_s3() {
    let region = required("AWS_REGION");
    let bucket = required("MINCO_AWS_BUCKET");
    let run_id = required("MINCO_AWS_RUN_ID");
    let journal = required("MINCO_AWS_TOUCH_LOG");
    let shared = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new(region.clone()))
        .load()
        .await;
    let credentials = shared
        .credentials_provider()
        .expect("bounded profile must supply credentials");
    let storage_prefix = format!("minco-managed-conformance/{run_id}");
    let storage = S3ObjectStorage::from_sdk_builder(
        aws_sdk_s3::config::Builder::from(&shared),
        credentials,
        &bucket,
        &storage_prefix,
        S3Addressing::new(region, None).unwrap(),
    )
    .unwrap();
    let policy =
        ObjectUploadPolicy::new(ObjectKey::parse("uploads").unwrap(), 1024, ["text/plain"])
            .unwrap();
    let mut manager = PluginManager::default();
    manager.register(storage.plugin(policy).unwrap()).unwrap();
    let mut selection = PluginSelection::default();
    selection
        .enabled
        .insert(PluginId::new("object-storage").unwrap());
    let application = manager.compose(&selection).unwrap();
    let uploads = application.services.get::<ObjectUploadService>().unwrap();
    let adapter = storage.adapter();
    let client = aws_sdk_s3::Client::new(&shared);
    let mut cleanup = Vec::new();

    let result = run_conformance(
        &bucket,
        &storage_prefix,
        &run_id,
        &journal,
        &uploads,
        &storage,
        &client,
        &mut cleanup,
    )
    .await;

    let cleanup_result: TestResult = async {
        for key in cleanup {
            touch(&journal, "DeleteObject", "remove conformance-owned object");
            adapter.delete(&key).await?;
        }
        Ok(())
    }
    .await;
    result.and(cleanup_result).unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn run_conformance(
    bucket: &str,
    storage_prefix: &str,
    run_id: &str,
    journal: &str,
    uploads: &Arc<ObjectUploadService>,
    storage: &S3ObjectStorage,
    client: &aws_sdk_s3::Client,
    cleanup: &mut Vec<ObjectKey>,
) -> TestResult {
    let positive_body = b"minco-managed-real-s3";
    let positive = issue(uploads, positive_body, run_id, "positive").await?;
    cleanup.push(positive.grant.key.clone());
    ensure(
        !positive
            .grant
            .key
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .contains('.'),
        "managed keys must be extensionless",
    )?;
    touch(journal, "PresignedPost", "upload exact managed object");
    let response = post_form(
        &positive.grant.request.url,
        positive.grant.request.form_fields.clone(),
        positive_body,
        "text/plain",
    )
    .await?;
    ensure(
        response.status().is_success(),
        format!("positive POST returned {}", response.status()),
    )?;
    touch(
        journal,
        "HeadObject",
        "verify managed object without GetObject",
    );
    let verified = uploads.verify(&positive.pending).await?;
    ensure(verified.key == positive.grant.key, "verified key changed")?;
    ensure(
        verified.metadata.size_bytes == positive_body.len() as u64,
        "verified size changed",
    )?;
    ensure(
        verified.metadata.content_type == "text/plain",
        "verified content type changed",
    )?;
    ensure(
        verified.metadata.sha256.as_deref() == Some(sha256(positive_body).as_str()),
        "provider checksum was not verified",
    )?;
    ensure(
        verified.metadata.attributes.get("case").map(String::as_str) == Some("positive"),
        "signed attributes changed",
    )?;

    let changed_bytes = issue(uploads, b"same-size-a", run_id, "changed-bytes").await?;
    cleanup.push(changed_bytes.grant.key.clone());
    touch(
        journal,
        "PresignedPost",
        "reject body with changed checksum",
    );
    let response = post_form(
        &changed_bytes.grant.request.url,
        changed_bytes.grant.request.form_fields.clone(),
        b"same-size-b",
        "text/plain",
    )
    .await?;
    ensure(
        response.status().is_client_error(),
        format!("changed-checksum POST returned {}", response.status()),
    )?;
    assert_absent(storage, journal, &changed_bytes.grant.key).await?;

    let wrong_size = issue(uploads, b"three", run_id, "wrong-size").await?;
    cleanup.push(wrong_size.grant.key.clone());
    touch(journal, "PresignedPost", "reject changed byte count");
    let response = post_form(
        &wrong_size.grant.request.url,
        wrong_size.grant.request.form_fields.clone(),
        b"three-more",
        "text/plain",
    )
    .await?;
    ensure(
        response.status().is_client_error(),
        format!("wrong-size POST returned {}", response.status()),
    )?;
    assert_absent(storage, journal, &wrong_size.grant.key).await?;

    let changed_type = issue(uploads, b"type", run_id, "changed-type").await?;
    cleanup.push(changed_type.grant.key.clone());
    let mut fields = changed_type.grant.request.form_fields.clone();
    fields.insert("Content-Type".into(), "application/json".into());
    touch(
        journal,
        "PresignedPost",
        "reject changed signed content type",
    );
    let response = post_form(
        &changed_type.grant.request.url,
        fields,
        b"type",
        "application/json",
    )
    .await?;
    ensure(
        response.status().is_client_error(),
        format!("changed-content-type POST returned {}", response.status()),
    )?;
    assert_absent(storage, journal, &changed_type.grant.key).await?;

    let changed_attribute = issue(uploads, b"attribute", run_id, "changed-attribute").await?;
    cleanup.push(changed_attribute.grant.key.clone());
    let mut fields = changed_attribute.grant.request.form_fields.clone();
    fields.insert("x-amz-meta-minco-attributes".into(), "changed".into());
    touch(journal, "PresignedPost", "reject changed signed attributes");
    let response = post_form(
        &changed_attribute.grant.request.url,
        fields,
        b"attribute",
        "text/plain",
    )
    .await?;
    ensure(
        response.status().is_client_error(),
        format!("changed-attribute POST returned {}", response.status()),
    )?;
    assert_absent(storage, journal, &changed_attribute.grant.key).await?;

    let missing_checksum = issue(uploads, b"missing-checksum", run_id, "missing-checksum").await?;
    cleanup.push(missing_checksum.grant.key.clone());
    touch(
        journal,
        "PutObject",
        "create metadata-only checksum object without provider checksum",
    );
    put_fixture(
        client,
        bucket,
        storage_prefix,
        &missing_checksum,
        b"missing-checksum",
        Some(&missing_checksum.pending.expected_sha256),
        None,
    )
    .await?;
    touch(journal, "HeadObject", "reject missing provider checksum");
    ensure(
        matches!(
            uploads.verify(&missing_checksum.pending).await,
            Err(ObjectUploadError::ChecksumMismatch)
        ),
        "metadata-only checksum did not fail closed",
    )?;

    let conflicting = issue(uploads, b"conflicting", run_id, "conflicting").await?;
    cleanup.push(conflicting.grant.key.clone());
    let provider_checksum = STANDARD.encode(Sha256::digest(b"conflicting"));
    touch(
        journal,
        "PutObject",
        "create object with conflicting metadata and provider checksums",
    );
    put_fixture(
        client,
        bucket,
        storage_prefix,
        &conflicting,
        b"conflicting",
        Some(&"00".repeat(32)),
        Some(&provider_checksum),
    )
    .await?;
    touch(journal, "HeadObject", "reject conflicting checksums");
    ensure(
        matches!(
            uploads.verify(&conflicting.pending).await,
            Err(ObjectUploadError::ObjectStore(_))
        ),
        "conflicting provider and metadata checksums did not fail closed",
    )?;

    Ok(())
}

async fn issue(
    uploads: &ObjectUploadService,
    body: &[u8],
    run_id: &str,
    case: &str,
) -> Result<IssuedObjectUpload, ObjectUploadError> {
    uploads
        .issue(IssueObjectUpload {
            content_type: "text/plain".into(),
            size_bytes: body.len() as u64,
            sha256: sha256(body),
            attributes: BTreeMap::from([
                ("case".into(), case.into()),
                ("run".into(), run_id.into()),
            ]),
        })
        .await
}

async fn post_form(
    url: &str,
    fields: BTreeMap<String, String>,
    body: &[u8],
    content_type: &str,
) -> TestResult<reqwest::Response> {
    let mut form = reqwest::multipart::Form::new();
    for (name, value) in fields {
        form = form.text(name, value);
    }
    let file = reqwest::multipart::Part::bytes(body.to_vec()).mime_str(content_type)?;
    Ok(reqwest::Client::new()
        .post(url)
        .multipart(form.part("file", file))
        .send()
        .await?)
}

async fn assert_absent(storage: &S3ObjectStorage, journal: &str, key: &ObjectKey) -> TestResult {
    touch(journal, "HeadObject", "prove rejected object is absent");
    ensure(
        storage.metadata_reader().head(key).await?.is_none(),
        "provider persisted an object from a rejected POST",
    )
}

async fn put_fixture(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    storage_prefix: &str,
    issued: &IssuedObjectUpload,
    body: &[u8],
    metadata_sha256: Option<&str>,
    provider_checksum: Option<&str>,
) -> TestResult {
    let attributes = STANDARD.encode(serde_json::to_vec(&issued.pending.expected_attributes)?);
    let mut request = client
        .put_object()
        .bucket(bucket)
        .key(format!("{storage_prefix}/{}", issued.grant.key.as_str()))
        .body(ByteStream::from(body.to_vec()))
        .content_type("text/plain")
        .server_side_encryption(ServerSideEncryption::Aes256)
        .metadata("minco-created-at", chrono::Utc::now().to_rfc3339())
        .metadata("minco-attributes", attributes);
    if let Some(value) = metadata_sha256 {
        request = request.metadata("minco-sha256", value);
    }
    if let Some(value) = provider_checksum {
        request = request.checksum_sha256(value);
    }
    request.send().await?;
    Ok(())
}

fn sha256(body: &[u8]) -> String {
    format!("{:x}", Sha256::digest(body))
}

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

fn ensure(condition: bool, message: impl Into<String>) -> TestResult {
    if condition {
        Ok(())
    } else {
        Err(std::io::Error::other(message.into()).into())
    }
}

fn touch(journal: &str, action: &str, detail: &str) {
    let entry = json!({
        "at": chrono::Utc::now().to_rfc3339(),
        "run_id": required("MINCO_AWS_RUN_ID"),
        "service": "aws:s3",
        "action": action,
        "detail": detail,
    });
    let mut output = OpenOptions::new()
        .append(true)
        .open(Path::new(journal))
        .unwrap();
    serde_json::to_writer(&mut output, &entry).unwrap();
    output.write_all(b"\n").unwrap();
}
