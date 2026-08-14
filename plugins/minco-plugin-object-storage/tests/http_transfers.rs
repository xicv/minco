#![cfg(feature = "http")]

use async_trait::async_trait;
use chrono::Utc;
use http::{Request, StatusCode, header};
use minco_core::{PluginId, PluginManager, PluginSelection};
use minco_http::Principal;
use minco_plugin_object_storage::{
    CompleteTransferUpload, CompletedTransferUpload, InitiateTransferUpload, IssueTransferDownload,
    IssueTransferPart, ManagedObjectStoragePlugin, MemoryObjectStore, MultipartPartGrant,
    MultipartUploadGrant, ObjectAccessSigner, ObjectDownloadGrant, ObjectKey, ObjectMetadataReader,
    ObjectStore, ObjectStoreError, ObjectTransferApiError, ObjectTransferHttpService,
    ObjectTransferHttpUseCases, ObjectTransferMetadata, ObjectTransferRequestContext,
    ObjectUploadError, ObjectUploadPolicy, ObjectUploadSigner, ObjectValidationState,
    PresignGetObject, PresignPutObject, PresignedMethod, PresignedObjectRequest, SignObjectUpload,
    TransferUploadGrant, TransferUploadResponse, object_transfer_router,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tokio::sync::Mutex;
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Debug, Default)]
struct FakeUseCases {
    calls: Mutex<Vec<String>>,
}

#[derive(Debug)]
struct UnusedSigner;

#[async_trait]
impl ObjectAccessSigner for UnusedSigner {
    async fn sign_put(
        &self,
        _request: PresignPutObject,
    ) -> Result<PresignedObjectRequest, ObjectStoreError> {
        unreachable!("composition test does not issue provider capabilities")
    }

    async fn sign_get(
        &self,
        _request: PresignGetObject,
    ) -> Result<PresignedObjectRequest, ObjectStoreError> {
        unreachable!("composition test does not issue provider capabilities")
    }
}

#[async_trait]
impl ObjectUploadSigner for UnusedSigner {
    async fn sign_upload(
        &self,
        _request: SignObjectUpload,
    ) -> Result<PresignedObjectRequest, ObjectUploadError> {
        unreachable!("composition test does not issue provider capabilities")
    }
}

impl FakeUseCases {
    async fn calls(&self) -> Vec<String> {
        self.calls.lock().await.clone()
    }
}

#[async_trait]
impl ObjectTransferHttpUseCases for FakeUseCases {
    async fn initiate_upload(
        &self,
        context: ObjectTransferRequestContext,
        request: InitiateTransferUpload,
    ) -> Result<TransferUploadResponse, ObjectTransferApiError> {
        assert_eq!(context.principal.subject, "user-1");
        assert_eq!(context.request_id, "request-1");
        assert_eq!(context.idempotency_key.as_deref(), Some("upload-once"));
        assert_eq!(request.if_match.as_deref(), Some("\"revision-1\""));
        self.calls.lock().await.push("initiate".into());
        Ok(TransferUploadResponse {
            upload: TransferUploadGrant::Multipart(MultipartUploadGrant {
                upload_id: Uuid::nil(),
                key: ObjectKey::parse("uploads/revision-2").unwrap(),
                size_bytes: 10,
                part_size_bytes: 5,
                part_count: 2,
            }),
            validation: ObjectValidationState::Quarantined,
        })
    }

    async fn issue_part(
        &self,
        _context: ObjectTransferRequestContext,
        upload_id: Uuid,
        part_number: u32,
        _request: IssueTransferPart,
    ) -> Result<MultipartPartGrant, ObjectTransferApiError> {
        self.calls.lock().await.push("part".into());
        Ok(MultipartPartGrant {
            upload_id,
            part_number,
            size_bytes: 5,
            request: signed(PresignedMethod::Put),
        })
    }

    async fn complete_upload(
        &self,
        _context: ObjectTransferRequestContext,
        _upload_id: Uuid,
        _request: CompleteTransferUpload,
    ) -> Result<CompletedTransferUpload, ObjectTransferApiError> {
        self.calls.lock().await.push("complete".into());
        Ok(CompletedTransferUpload {
            object_id: "document-1".into(),
            revision: "2".into(),
            entity_tag: "\"revision-2\"".into(),
            validation: ObjectValidationState::Quarantined,
        })
    }

    async fn abort_upload(
        &self,
        _context: ObjectTransferRequestContext,
        _upload_id: Uuid,
    ) -> Result<(), ObjectTransferApiError> {
        self.calls.lock().await.push("abort".into());
        Ok(())
    }

    async fn issue_download(
        &self,
        _context: ObjectTransferRequestContext,
        _request: IssueTransferDownload,
    ) -> Result<ObjectDownloadGrant, ObjectTransferApiError> {
        self.calls.lock().await.push("download".into());
        Ok(ObjectDownloadGrant {
            key: ObjectKey::parse("objects/revision-2").unwrap(),
            request: signed(PresignedMethod::Get),
            content_type: "application/pdf".into(),
            size_bytes: 10,
            entity_tag: "\"revision-2\"".into(),
            version_id: None,
            last_modified: Utc::now(),
            range: None,
            cache_control: "private, no-store".into(),
        })
    }

    async fn get_metadata(
        &self,
        _context: ObjectTransferRequestContext,
        object_id: String,
    ) -> Result<ObjectTransferMetadata, ObjectTransferApiError> {
        self.calls.lock().await.push("metadata".into());
        Ok(ObjectTransferMetadata {
            object_id,
            revision: "2".into(),
            content_type: "application/pdf".into(),
            size_bytes: 10,
            entity_tag: "\"revision-2\"".into(),
            last_modified: Utc::now(),
            validation: ObjectValidationState::Accepted {
                inspector: "safe-pdf".into(),
                inspected_at: Utc::now(),
            },
        })
    }
}

fn signed(method: PresignedMethod) -> PresignedObjectRequest {
    PresignedObjectRequest {
        method,
        url: "https://objects.example/?signature=secret".into(),
        headers: BTreeMap::new(),
        form_fields: BTreeMap::new(),
        expires_at: Utc::now(),
    }
}

fn principal() -> Principal {
    Principal {
        subject: "user-1".into(),
        permissions: BTreeSet::from(["objects.write".into()]),
        claims: BTreeMap::new(),
    }
}

#[tokio::test]
async fn transfer_routes_fail_closed_without_a_principal() {
    let app = object_transfer_router(ObjectTransferHttpService::new(Arc::new(
        FakeUseCases::default(),
    )));
    let response = app
        .oneshot(
            Request::post("/_minco/objects/downloads")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(r#"{"object_id":"document-1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
}

#[tokio::test]
async fn update_initiation_binds_if_match_and_calls_one_application_use_case() {
    let use_cases = Arc::new(FakeUseCases::default());
    let app = object_transfer_router(ObjectTransferHttpService::new(use_cases.clone()))
        .layer(axum::Extension(principal()));
    let response = app
        .oneshot(
            Request::post("/_minco/objects/uploads")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-request-id", "request-1")
                .header("idempotency-key", "upload-once")
                .header(header::IF_MATCH, "\"revision-1\"")
                .body(axum::body::Body::from(
                    r#"{
                        "purpose":"documents",
                        "content_type":"application/pdf",
                        "size_bytes":10,
                        "sha256":null,
                        "file_name":"report.pdf",
                        "replaces_object_id":"document-1",
                        "attributes":{}
                    }"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(use_cases.calls().await, ["initiate"]);
}

#[tokio::test]
async fn update_initiation_requires_a_conditional_revision() {
    let use_cases = Arc::new(FakeUseCases::default());
    let app = object_transfer_router(ObjectTransferHttpService::new(use_cases.clone()))
        .layer(axum::Extension(principal()));
    let response = app
        .oneshot(
            Request::post("/_minco/objects/uploads")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    r#"{
                        "purpose":"documents",
                        "content_type":"application/pdf",
                        "size_bytes":10,
                        "replaces_object_id":"document-1",
                        "attributes":{}
                    }"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
    assert!(use_cases.calls().await.is_empty());
}

#[tokio::test]
async fn update_initiation_rejects_weak_or_ambiguous_etags_before_the_use_case() {
    let use_cases = Arc::new(FakeUseCases::default());
    let app = object_transfer_router(ObjectTransferHttpService::new(use_cases.clone()))
        .layer(axum::Extension(principal()));
    let response = app
        .oneshot(
            Request::post("/_minco/objects/uploads")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::IF_MATCH, "W/\"revision-1\"")
                .body(axum::body::Body::from(
                    r#"{
                        "purpose":"documents",
                        "content_type":"application/pdf",
                        "size_bytes":10,
                        "replaces_object_id":"document-1",
                        "attributes":{}
                    }"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(use_cases.calls().await.is_empty());
}

#[tokio::test]
async fn duplicate_idempotency_keys_fail_before_the_use_case() {
    let use_cases = Arc::new(FakeUseCases::default());
    let app = object_transfer_router(ObjectTransferHttpService::new(use_cases.clone()))
        .layer(axum::Extension(principal()));
    let mut request = Request::post("/_minco/objects/uploads")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-request-id", "request-1")
        .header("idempotency-key", "first")
        .body(axum::body::Body::from(
            r#"{
                "purpose":"documents",
                "content_type":"application/pdf",
                "size_bytes":10,
                "attributes":{}
            }"#,
        ))
        .unwrap();
    request
        .headers_mut()
        .append("idempotency-key", http::HeaderValue::from_static("second"));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(use_cases.calls().await, Vec::<String>::new());
}

#[tokio::test]
async fn conditional_metadata_avoids_reissuing_or_redownloading_unchanged_bytes() {
    let use_cases = Arc::new(FakeUseCases::default());
    let app = object_transfer_router(ObjectTransferHttpService::new(use_cases.clone()))
        .layer(axum::Extension(principal()));
    let response = app
        .oneshot(
            Request::get("/_minco/objects/document-1")
                .header(header::IF_NONE_MATCH, "\"revision-2\"")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(response.headers()[header::ETAG], "\"revision-2\"");
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, no-cache"
    );
    assert_eq!(response.headers()[header::VARY], "Authorization");
    assert_eq!(use_cases.calls().await, ["metadata"]);
}

#[test]
fn managed_plugin_http_descriptor_and_module_own_the_same_operations() {
    let store = Arc::new(MemoryObjectStore::default());
    let object_store: Arc<dyn ObjectStore> = store.clone();
    let metadata: Arc<dyn ObjectMetadataReader> = store;
    let policy = ObjectUploadPolicy::new(
        ObjectKey::parse("uploads/documents").unwrap(),
        1024,
        ["application/pdf"],
    )
    .unwrap();
    let plugin = ManagedObjectStoragePlugin::new_with_signers(
        object_store,
        Arc::new(UnusedSigner),
        Arc::new(UnusedSigner),
        metadata,
        policy,
    )
    .with_http_api(ObjectTransferHttpService::new(Arc::new(
        FakeUseCases::default(),
    )));
    let mut manager = PluginManager::default();
    manager.register(plugin).unwrap();
    let mut selection = PluginSelection::default();
    selection
        .enabled
        .insert(PluginId::new("object-storage").unwrap());
    let application = manager.compose(&selection).unwrap();
    minco_http::validate_plugin_http_modules(&application.graph, &application.contributions)
        .unwrap();
    let module = application.contributions.get::<minco_http::HttpModule>();
    assert_eq!(module.len(), 1);
    assert_eq!(module[0].operation_ids.len(), 6);
}
