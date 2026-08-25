//! Local Rustack seam proof for the ticketing inbound mail chain
//! (Stage D2, slice 3b part 2 / ADR-0064).
//!
//! Three explicit modes, orchestrated by `scripts/dev/ticketing-mail-seam.sh`:
//!
//! - `seed`: open the `SQLite` database (ticketing + plugin-storage
//!   migrations), create one ticket and ingest one previously known
//!   external message as the threading anchor.
//! - `poll`: poll the real SQS queue once per round, feed every delivery
//!   through `TicketingMailWakeHandler` (real S3 `Records` envelope,
//!   raw MIME fetched from the real S3 bucket), delete on success, and
//!   stop when the queue drains.
//! - `verify`: read the durable `minco_jobs` rows straight from `SQLite`
//!   and emit a JSON verdict; exit code 0 only when exactly one
//!   `ticketing.process-inbound-email` job exists.
//!
//! No AWS provider is contacted: every SDK client points at the local
//! Rustack endpoint from `AWS_ENDPOINT_URL`.

use aws_credential_types::provider::SharedCredentialsProvider;
use aws_sdk_s3::config::Credentials;
use minco_aws_adapters::s3_storage::S3ObjectStorage;
use minco_aws_worker::ticketing_wake::TicketingMailWakeHandler;
use minco_aws_worker::{MessageHandler as _, WorkerMessage};
use minco_plugin_identity::Identity;
use minco_plugin_jobs::{
    FailClosedDispatcher, JobExecutor, JobHandlerRegistry, JobsServices, SystemJobClock,
};
use minco_plugin_object_storage::ObjectStoreService;
use minco_plugin_ticketing::{
    CreateTicketInput, ExternalMessageIdentity, SqliteTicketingStore, TicketChannel,
    TicketPriority, TicketRequester, TicketingConfig, TicketingPortalServices, TicketingService,
    TicketingStoreService,
};
use minco_sqlx_sqlite::jobs::SqliteJobStore;
use minco_sqlx_sqlite::plugin_adapters::migrate_plugin_storage;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::{BTreeMap, BTreeSet};
use std::{env, process::exit, sync::Arc};
use uuid::Uuid;

fn required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

async fn open_pool(database: &str) -> sqlx::SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(database)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .expect("sqlite pool");
    migrate_plugin_storage(&pool)
        .await
        .expect("plugin-storage migrations");
    let ticketing = SqliteTicketingStore::new(pool.clone());
    ticketing.migrate().await.expect("ticketing migrations");
    pool
}

async fn compose() -> (
    Arc<TicketingService>,
    minco_plugin_object_storage::ObjectStoreService,
    sqlx::SqlitePool,
) {
    let pool = open_pool(&required("SEAM_DB")).await;
    let jobs_store = Arc::new(SqliteJobStore::new(pool.clone()));
    let registry = Arc::new(JobHandlerRegistry::new());
    let jobs = JobsServices::new(
        jobs_store.clone(),
        jobs_store.clone(),
        Arc::new(FailClosedDispatcher),
        jobs_store,
        Arc::new(SystemJobClock),
        Arc::new(JobExecutor::new(registry)),
    );
    let endpoint = env::var("AWS_ENDPOINT_URL").ok();
    let region = env::var("AWS_DEFAULT_REGION").unwrap_or_else(|_| "ap-southeast-2".into());
    let credentials = SharedCredentialsProvider::new(Credentials::new(
        "test",
        "test",
        None,
        None,
        "rustack-seam",
    ));
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest());
    let config = if let Some(endpoint) = endpoint.as_deref() {
        config.endpoint_url(endpoint)
    } else {
        config
    };
    let settings = config
        .credentials_provider(credentials.clone())
        .region(aws_config::Region::new(region.clone()))
        .load()
        .await;
    // Path-style addressing: the local Rustack endpoint is an IP literal,
    // and virtual-host style would resolve to an unroutable host.
    let s3_client = aws_sdk_s3::Client::from_conf(
        aws_sdk_s3::Config::from(&settings)
            .to_builder()
            .force_path_style(true)
            .build(),
    );
    let objects = S3ObjectStorage::new(
        s3_client,
        credentials,
        required("SEAM_BUCKET"),
        "",
        region,
        endpoint,
    )
    .expect("S3 object storage adapter")
    .adapter();
    let object_service = ObjectStoreService::new(objects);
    let service = Arc::new(
        TicketingService::new(
            TicketingStoreService::new(Arc::new(SqliteTicketingStore::new(pool.clone()))),
            TicketingConfig {
                project_id: "project-a".into(),
                ..TicketingConfig::default()
            },
        )
        .expect("ticketing service")
        .with_portal_services(TicketingPortalServices {
            jobs: Some(Arc::new(jobs)),
            objects: Some(Arc::new(object_service.clone())),
            ..TicketingPortalServices::default()
        }),
    );
    (service, object_service, pool)
}

fn seam_identity() -> Identity {
    Identity {
        subject: "seam".into(),
        permissions: BTreeSet::from([
            "ticketing.create".into(),
            "ticketing.ingest".into(),
            "ticketing.manage".into(),
        ]),
        scopes: BTreeSet::new(),
        claims: BTreeMap::new(),
    }
}

async fn seed() {
    let (service, _objects, _pool) = compose().await;
    let now = chrono::Utc::now();
    let identity = seam_identity();
    let created = TicketingService::clone(&service)
        .create_ticket(
            &identity,
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Help".into(),
                description: "Seam ticket".into(),
                requester: TicketRequester {
                    subject: "user-1".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Email,
                priority: TicketPriority::default(),
                ticket_type: minco_plugin_ticketing::TicketType::default(),
                form_answers: Vec::new(),
                resource_references: Vec::new(),
            },
            Uuid::new_v4(),
            now,
        )
        .await
        .expect("seed ticket");
    TicketingService::clone(&service)
        .ingest_external_message(
            &identity,
            ExternalMessageIdentity {
                project_id: "project-a".into(),
                provider: "ses".into(),
                mailbox_scope: "support@example.test".into(),
                external_id: "original-1".into(),
                content_sha256: "a".repeat(64),
                raw_message_object_key: None,
                internet_message_id: Some("<original-1@example.test>".into()),
                in_reply_to: None,
                references: Vec::new(),
            },
            created.ticket.id,
            "It broke".into(),
            0,
            Uuid::new_v4(),
            now,
        )
        .await
        .expect("seed threading anchor");
    println!(
        "{{\"mode\":\"seed\",\"ticket\":\"{}\"}}",
        created.ticket.display_reference
    );
}

async fn poll() {
    let (service, _objects, _pool) = compose().await;
    let scope = env::var("SEAM_MAILBOX_SCOPE").unwrap_or_else(|_| "support@example.test".into());
    let handler = TicketingMailWakeHandler::new(service, scope);
    let endpoint = env::var("AWS_ENDPOINT_URL").ok();
    let region = env::var("AWS_DEFAULT_REGION").unwrap_or_else(|_| "ap-southeast-2".into());
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest());
    let config = if let Some(endpoint) = endpoint.as_deref() {
        config.endpoint_url(endpoint)
    } else {
        config
    };
    let settings = config
        .credentials_provider(SharedCredentialsProvider::new(Credentials::new(
            "test",
            "test",
            None,
            None,
            "rustack-seam",
        )))
        .region(aws_config::Region::new(region))
        .load()
        .await;
    let sqs = aws_sdk_sqs::Client::new(&settings);
    let queue_url = required("SEAM_QUEUE_URL");
    let mut handled_count = 0usize;
    let mut failures = Vec::new();
    for _round in 0..10 {
        let received = sqs
            .receive_message()
            .queue_url(&queue_url)
            .max_number_of_messages(10)
            .wait_time_seconds(1)
            .send()
            .await
            .expect("sqs receive");
        let Some(messages) = received.messages else {
            break;
        };
        if messages.is_empty() {
            break;
        }
        for message in messages {
            let worker_message = WorkerMessage {
                message_id: message.message_id.clone().unwrap_or_default(),
                body: message.body.clone().unwrap_or_default(),
                attributes: BTreeMap::new(),
                message_group_id: None,
            };
            match handler.handle(worker_message).await {
                Ok(()) => {
                    sqs.delete_message()
                        .queue_url(&queue_url)
                        .receipt_handle(message.receipt_handle.clone().expect("receipt"))
                        .send()
                        .await
                        .expect("sqs delete");
                    handled_count += 1;
                }
                Err(failure) => failures.push(failure.code().to_owned()),
            }
        }
    }
    println!(
        "{{\"mode\":\"poll\",\"handled\":{handled_count},\"failures\":{}}}",
        serde_json::to_string(&failures).expect("encode failures")
    );
    if !failures.is_empty() {
        exit(1);
    }
}

async fn verify() {
    let pool = open_pool(&required("SEAM_DB")).await;
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT json_extract(envelope, '$.job_name'), status FROM minco_jobs")
            .fetch_all(&pool)
            .await
            .expect("job rows");
    let inbound: Vec<&(String, String)> = rows
        .iter()
        .filter(|(name, _)| name == "ticketing.process-inbound-email")
        .collect();
    println!(
        "{{\"mode\":\"verify\",\"total_jobs\":{},\"inbound_jobs\":{},\"ok\":{}}}",
        rows.len(),
        inbound.len(),
        inbound.len() == 1
    );
    if inbound.len() != 1 {
        exit(1);
    }
}

#[tokio::main]
async fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("seed") => seed().await,
        Some("poll") => poll().await,
        Some("verify") => verify().await,
        other => panic!("usage: ticketing_mail_seam seed|poll|verify (got {other:?})"),
    }
}
