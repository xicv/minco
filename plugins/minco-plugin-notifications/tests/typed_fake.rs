use chrono::{DateTime, Utc};
use minco_plugin_notifications::{
    FakeMailTransport, MailAddress, MailErrorKind, MailMessage, MailService, NoopMailObserver,
};
use std::sync::Arc;

fn message() -> MailMessage {
    MailMessage::builder("order.ready", "private subject")
        .to(MailAddress::new("person@example.com").unwrap())
        .text("private mail body")
        .build()
        .unwrap()
}

#[tokio::test]
async fn fake_transport_drives_real_fallback_and_consumes_failure_once() {
    let primary = Arc::new(FakeMailTransport::named("primary").unwrap());
    let fallback = Arc::new(FakeMailTransport::named("fallback").unwrap());
    primary
        .fail_next(MailErrorKind::Unavailable, "temporarily unavailable")
        .await;
    let service = MailService::new(
        vec![primary.clone(), fallback.clone()],
        Arc::new(NoopMailObserver),
    )
    .unwrap();

    let first = service.send(message()).await.unwrap();
    assert_eq!(first.transport, "fallback");
    assert_eq!(first.accepted_at, DateTime::<Utc>::UNIX_EPOCH);
    assert_eq!(primary.attempts().await[0].attempt, 1);
    assert_eq!(fallback.attempts().await[0].attempt, 2);

    let second = service.send(message()).await.unwrap();
    assert_eq!(second.transport, "primary");
    assert_eq!(second.accepted_at, DateTime::<Utc>::UNIX_EPOCH);
    assert_eq!(primary.attempts().await.len(), 2);
    assert_eq!(fallback.attempts().await.len(), 1);

    let attempt_debug = format!("{:?}", primary.attempts().await[0]);
    for private in ["person@example.com", "private subject", "private mail body"] {
        assert!(!attempt_debug.contains(private));
    }

    let debug = format!("{primary:?}");
    for private in ["person@example.com", "private subject", "private mail body"] {
        assert!(!debug.contains(private));
    }
}
