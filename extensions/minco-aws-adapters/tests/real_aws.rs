#![cfg(all(
    feature = "cognito",
    feature = "s3",
    feature = "ses",
    feature = "sqs",
    feature = "static-site"
))]

use chrono::TimeDelta;
use minco_aws_adapters::{
    cognito::CognitoIdentityAdministrator, s3::S3ObjectAdapter, ses::SesNotificationSink,
    sqs::SqsEventPublisher, static_site::AwsStaticSitePublisher,
};
use minco_plugin_events::{DomainEvent, EventPublisher};
use minco_plugin_identity::{IdentityAdministrator, InviteIdentity};
use minco_plugin_notifications::{Notification, NotificationChannel, NotificationSink};
use minco_plugin_object_storage::{
    ObjectAccessSigner, ObjectKey, ObjectStore, PresignGetObject, PresignPutObject,
    PresignedMethod, PutObject,
};
use minco_plugin_static_site::{StaticSitePlan, StaticSitePublisher};
use serde_json::json;
use std::{collections::BTreeMap, fs::OpenOptions, io::Write, path::Path};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires the bounded, journaled real-AWS smoke environment"]
async fn adapters_conform_on_bounded_real_aws() {
    let region = required("AWS_REGION");
    let bucket = required("MINCO_AWS_BUCKET");
    let queue_url = required("MINCO_AWS_QUEUE_URL");
    let user_pool_id = required("MINCO_AWS_USER_POOL_ID");
    let run_id = required("MINCO_AWS_RUN_ID");
    let journal = required("MINCO_AWS_TOUCH_LOG");
    let shared = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new(region.clone()))
        .load()
        .await;
    let credentials = shared
        .credentials_provider()
        .expect("bounded profile must supply credentials");
    let s3_client = aws_sdk_s3::Client::new(&shared);
    let s3 =
        S3ObjectAdapter::new(s3_client, credentials, &bucket, "objects", &region, None).unwrap();

    let server_key = ObjectKey::parse(format!("{run_id}/server.txt")).unwrap();
    touch(
        &journal,
        "aws:s3",
        "PutObject",
        "adapter server-side object",
    );
    s3.put(PutObject {
        key: server_key.clone(),
        bytes: b"minco-real-aws-server".to_vec(),
        content_type: "text/plain".into(),
        attributes: BTreeMap::from([("run".into(), run_id.clone())]),
    })
    .await
    .unwrap();
    touch(
        &journal,
        "aws:s3",
        "GetObject",
        "verify adapter server-side object",
    );
    assert_eq!(
        s3.get(&server_key).await.unwrap().unwrap().bytes,
        b"minco-real-aws-server"
    );

    let direct_key = ObjectKey::parse(format!("{run_id}/direct.txt")).unwrap();
    let signed_post = s3
        .sign_put(PresignPutObject {
            key: direct_key.clone(),
            content_type: "text/plain".into(),
            maximum_size_bytes: 128,
            expires_in: TimeDelta::minutes(5),
            attributes: BTreeMap::from([("run".into(), run_id.clone())]),
        })
        .await
        .unwrap();
    assert_eq!(signed_post.method, PresignedMethod::Post);
    touch(
        &journal,
        "aws:s3",
        "PresignedPost",
        "upload within signed size policy",
    );
    let response = post_form(
        signed_post.url,
        signed_post.form_fields,
        b"minco-real-aws-direct".to_vec(),
    )
    .await;
    assert!(response.status().is_success(), "{}", response.status());

    let oversized_key = ObjectKey::parse(format!("{run_id}/oversized.txt")).unwrap();
    let oversized_post = s3
        .sign_put(PresignPutObject {
            key: oversized_key.clone(),
            content_type: "application/octet-stream".into(),
            maximum_size_bytes: 16,
            expires_in: TimeDelta::minutes(5),
            attributes: BTreeMap::new(),
        })
        .await
        .unwrap();
    touch(
        &journal,
        "aws:s3",
        "PresignedPost",
        "prove signed content-length-range rejects oversized body",
    );
    let response = post_form(oversized_post.url, oversized_post.form_fields, vec![7; 17]).await;
    assert!(
        response.status().is_client_error(),
        "oversized S3 POST unexpectedly returned {}",
        response.status()
    );
    touch(
        &journal,
        "aws:s3",
        "GetObject",
        "prove rejected object is absent",
    );
    assert!(s3.get(&oversized_key).await.unwrap().is_none());

    let signed_get = s3
        .sign_get(PresignGetObject {
            key: direct_key.clone(),
            expires_in: TimeDelta::minutes(5),
            download_file_name: Some("direct.txt".into()),
        })
        .await
        .unwrap();
    touch(
        &journal,
        "aws:s3",
        "PresignedGet",
        "download direct-upload object",
    );
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
        b"minco-real-aws-direct"
    );

    let sqs = SqsEventPublisher::new(aws_sdk_sqs::Client::new(&shared), &queue_url, false).unwrap();
    touch(
        &journal,
        "aws:sqs",
        "SendMessage",
        "publish one serialized domain event",
    );
    sqs.publish(&DomainEvent::new(
        "feedback.created",
        "feedback",
        &run_id,
        Uuid::now_v7(),
        json!({"provider": "aws"}),
    ))
    .await
    .unwrap();

    let cognito = CognitoIdentityAdministrator::new(
        aws_sdk_cognitoidentityprovider::Client::new(&shared),
        &user_pool_id,
    )
    .unwrap();
    let username = format!("minco-{run_id}");
    touch(
        &journal,
        "aws:cognito-idp",
        "AdminCreateUser",
        "create suppressed-invitation smoke identity",
    );
    cognito
        .invite(InviteIdentity {
            username: username.clone(),
            email: "success@simulator.amazonses.com".into(),
            attributes: BTreeMap::from([("name".into(), run_id.clone())]),
            send_invitation: false,
        })
        .await
        .unwrap();
    touch(
        &journal,
        "aws:cognito-idp",
        "AdminGetUser",
        "verify smoke identity",
    );
    assert!(cognito.get(&username).await.unwrap().is_some());
    touch(
        &journal,
        "aws:cognito-idp",
        "AdminDisableUser",
        "disable smoke identity",
    );
    assert!(cognito.disable(&username).await.unwrap());
    touch(
        &journal,
        "aws:cognito-idp",
        "AdminDeleteUser",
        "delete smoke identity",
    );
    assert!(cognito.delete(&username).await.unwrap());

    if let Ok(sender) = std::env::var("MINCO_AWS_SES_SENDER") {
        let identity_arn = required("MINCO_AWS_SES_IDENTITY_ARN");
        let ses = SesNotificationSink::new(
            aws_sdk_sesv2::Client::new(&shared),
            sender,
            Some(identity_arn),
        )
        .unwrap();
        touch(
            &journal,
            "aws:sesv2",
            "SendEmail",
            "send to AWS success simulator from pre-existing verified identity",
        );
        ses.send(Notification::new(
            "minco.adapter.smoke",
            NotificationChannel::Email,
            "success@simulator.amazonses.com",
            "Minco bounded adapter smoke",
            "Provider conformance only.",
        ))
        .await
        .unwrap();
    }

    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    std::fs::create_dir(&dist).unwrap();
    std::fs::write(
        dist.join("index.html"),
        "<!doctype html><title>Minco</title>",
    )
    .unwrap();
    std::fs::write(dist.join("app.0123abcd.js"), "console.log('minco')").unwrap();
    let static_publisher = AwsStaticSitePublisher::new(
        aws_sdk_s3::Client::new(&shared),
        aws_sdk_cloudfront::Client::new(&shared),
        &bucket,
        "site",
        None,
        "https://example.invalid",
        false,
    )
    .unwrap();
    touch(
        &journal,
        "aws:s3",
        "StaticSitePublish",
        "list and upload run-owned static-site prefix without CloudFront creation",
    );
    let publication = static_publisher
        .publish(&static_plan(), temp.path())
        .await
        .unwrap();
    assert_eq!(publication.uploaded, 2);

    touch(
        &journal,
        "aws:s3",
        "DeleteObject",
        "remove adapter server object",
    );
    assert!(s3.delete(&server_key).await.unwrap());
    touch(
        &journal,
        "aws:s3",
        "DeleteObject",
        "remove adapter direct object",
    );
    assert!(s3.delete(&direct_key).await.unwrap());
}

async fn post_form(
    url: String,
    fields: BTreeMap<String, String>,
    body: Vec<u8>,
) -> reqwest::Response {
    let mut form = reqwest::multipart::Form::new();
    for (name, value) in fields {
        form = form.text(name, value);
    }
    reqwest::Client::new()
        .post(url)
        .multipart(form.part("file", reqwest::multipart::Part::bytes(body)))
        .send()
        .await
        .unwrap()
}

fn static_plan() -> StaticSitePlan {
    StaticSitePlan {
        source_directory: "dist".into(),
        index_document: "index.html".into(),
        spa_fallback: true,
        immutable_cache_seconds: 31_536_000,
        html_cache_seconds: 0,
        price_class: "PriceClass_100".into(),
        ipv6_enabled: true,
        custom_domain: None,
        manage_dns_alias: false,
    }
}

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

fn touch(journal: &str, service: &str, action: &str, detail: &str) {
    let entry = json!({
        "at": chrono::Utc::now().to_rfc3339(),
        "run_id": required("MINCO_AWS_RUN_ID"),
        "service": service,
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
