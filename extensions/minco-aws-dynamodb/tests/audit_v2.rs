use aws_sdk_dynamodb::types::{AttributeValue, Put, TransactWriteItem};
use minco_aws_dynamodb::{DynamoDbConfig, audit_v2::DynamoDbAuditLedger};
use minco_plugin_audit::{
    AuditActor, AuditLedgerWriter, AuditQuery, AuditReader, AuditRecordV2, AuditResourceRef,
    AuditStorageInspector,
};
use std::collections::HashMap;
use uuid::Uuid;

async fn ledger() -> (minco_aws_dynamodb::DynamoDbProvider, DynamoDbAuditLedger) {
    let source_table = std::env::var("MINCO_AUDIT_TEST_DYNAMODB_SOURCE_TABLE")
        .expect("MINCO_AUDIT_TEST_DYNAMODB_SOURCE_TABLE must name a disposable Rustack table");
    let audit_table = std::env::var("MINCO_AUDIT_TEST_DYNAMODB_AUDIT_TABLE")
        .expect("MINCO_AUDIT_TEST_DYNAMODB_AUDIT_TABLE must name a disposable Rustack table");
    let endpoint = std::env::var("MINCO_AUDIT_TEST_DYNAMODB_ENDPOINT")
        .expect("MINCO_AUDIT_TEST_DYNAMODB_ENDPOINT must name a loopback Rustack endpoint");
    let provider = DynamoDbConfig::new(source_table, "ap-southeast-2", Some(endpoint))
        .build()
        .await
        .expect("source provider");
    let ledger = DynamoDbAuditLedger::from_provider(&provider, audit_table).expect("audit ledger");
    (provider, ledger)
}

fn record(resource_id: String) -> AuditRecordV2 {
    AuditRecordV2::new(
        "rustack-tenant",
        "order.created",
        AuditResourceRef::new("order", resource_id),
        AuditActor::human("rustack-subject"),
        "placeOrder",
        Uuid::now_v7(),
    )
}

fn source_put(table: &str, id: Uuid) -> TransactWriteItem {
    let item = HashMap::from([
        ("pk".into(), AttributeValue::S(format!("ORDER#{id}"))),
        ("sk".into(), AttributeValue::S("ORDER".into())),
        ("entity".into(), AttributeValue::S("test_order".into())),
    ]);
    let put = Put::builder()
        .table_name(table)
        .set_item(Some(item))
        .condition_expression("attribute_not_exists(#pk)")
        .expression_attribute_names("#pk", "pk")
        .build()
        .expect("source put");
    TransactWriteItem::builder().put(put).build()
}

#[tokio::test]
#[ignore = "requires two disposable Rustack DynamoDB tables"]
async fn source_and_audit_commit_atomically_and_retry_is_idempotent() {
    let (provider, ledger) = ledger().await;
    let source_id = Uuid::now_v7();
    let audit = record(source_id.to_string());
    let mut items = vec![source_put(provider.table_name(), source_id)];
    items.extend(ledger.transact_items(&audit).expect("audit items"));
    provider
        .client()
        .transact_write_items()
        .set_transact_items(Some(items))
        .client_request_token(Uuid::now_v7().simple().to_string())
        .send()
        .await
        .expect("atomic source and audit write");

    let page = ledger
        .list_resource_history(&AuditQuery::for_resource(
            "rustack-tenant",
            audit.resource.clone(),
        ))
        .await
        .expect("resource history");
    assert_eq!(page.records.as_slice(), std::slice::from_ref(&audit));
    let replay = ledger
        .append_batch(&[audit])
        .await
        .expect("idempotent replay");
    assert_eq!((replay.inserted, replay.duplicates), (0, 1));
    let health = ledger.storage_health().await.expect("table health");
    assert_eq!(health.snapshot.provider, "dynamodb");

    let canceled = record(Uuid::now_v7().to_string());
    let mut items = vec![source_put(provider.table_name(), source_id)];
    items.extend(
        ledger
            .transact_items(&canceled)
            .expect("canceled audit items"),
    );
    assert!(
        provider
            .client()
            .transact_write_items()
            .set_transact_items(Some(items))
            .client_request_token(Uuid::now_v7().simple().to_string())
            .send()
            .await
            .is_err()
    );
    let page = ledger
        .list_resource_history(&AuditQuery::for_resource(
            "rustack-tenant",
            canceled.resource,
        ))
        .await
        .expect("canceled history query");
    assert!(page.records.is_empty());
}
