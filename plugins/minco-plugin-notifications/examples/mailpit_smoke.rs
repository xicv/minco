use minco_plugin_notifications::{
    MailAddress, MailAttachment, MailMessage, MailTransport, MailpitTransport,
    MailpitTransportConfig,
};
use std::net::SocketAddr;

#[derive(Debug, thiserror::Error)]
enum SmokeError {
    #[error("MINCO_MAILPIT_SMTP_PORT must be a valid TCP port")]
    InvalidPort,
    #[error(transparent)]
    Mail(#[from] minco_plugin_notifications::MailError),
}

#[tokio::main]
async fn main() -> Result<(), SmokeError> {
    let smtp_port = std::env::var("MINCO_MAILPIT_SMTP_PORT")
        .unwrap_or_else(|_| "1025".into())
        .parse::<u16>()
        .map_err(|_| SmokeError::InvalidPort)?;
    let endpoint = SocketAddr::from(([127, 0, 0, 1], smtp_port));
    let transport = MailpitTransport::new(MailpitTransportConfig::new(
        endpoint,
        MailAddress::named("minco@example.test", "Minco local mail")?,
    )?)?;
    let message = MailMessage::builder("mailpit.smoke", "Minco Mailpit smoke ✓")
        .to(MailAddress::named("person@example.test", "Example Person")?)
        .cc(MailAddress::new("accounts@example.test")?)
        .bcc(MailAddress::new("audit@example.test")?)
        .reply_to(MailAddress::new("support@example.test")?)
        .text("Minco Mailpit plain-text smoke body")
        .html("<p>Minco <strong>Mailpit</strong> HTML smoke body</p><img src=\"cid:logo\">")
        .attachment(MailAttachment::attachment(
            "evidence.txt",
            "text/plain",
            b"bounded local evidence".to_vec(),
        )?)
        .attachment(MailAttachment::inline(
            "logo.svg",
            "image/svg+xml",
            br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#.to_vec(),
            "logo",
        )?)
        .header("X-Minco-Smoke", "mailpit-local")
        .tag("test_kind", "mailpit_smoke")
        .build()?;

    let receipt = transport.send(&message, 1).await?;
    println!(
        "Mailpit accepted the bounded local smoke message {} on attempt {}",
        receipt.message_id, receipt.attempt
    );
    Ok(())
}
