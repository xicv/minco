use async_trait::async_trait;
use aws_sdk_sesv2::types::{Body, Content, Destination, EmailContent, Message};
use minco_plugin_notifications::{
    Notification, NotificationChannel, NotificationError, NotificationSink,
};

#[derive(Debug, Clone)]
pub struct SesNotificationSink {
    client: aws_sdk_sesv2::Client,
    from_address: String,
    from_identity_arn: Option<String>,
}

impl SesNotificationSink {
    pub fn new(
        client: aws_sdk_sesv2::Client,
        from_address: impl Into<String>,
        from_identity_arn: Option<String>,
    ) -> Result<Self, NotificationError> {
        let from_address = from_address.into();
        validate_email(&from_address)?;
        if from_identity_arn
            .as_deref()
            .is_some_and(|arn| !arn.starts_with("arn:") || arn.chars().any(char::is_control))
        {
            return Err(NotificationError::Delivery(
                "SES identity ARN is invalid".into(),
            ));
        }
        Ok(Self {
            client,
            from_address,
            from_identity_arn,
        })
    }
}

#[async_trait]
impl NotificationSink for SesNotificationSink {
    async fn send(&self, notification: Notification) -> Result<(), NotificationError> {
        if notification.channel != NotificationChannel::Email {
            return Err(NotificationError::Delivery(
                "SES adapter accepts only email notifications".into(),
            ));
        }
        validate_notification(&notification)?;

        let body_text = email_body(&notification);
        if body_text.len() > 1_000_000 || body_text.chars().any(|character| character == '\0') {
            return Err(NotificationError::Delivery(
                "rendered email body exceeds the Minco delivery boundary".into(),
            ));
        }
        let subject = Content::builder()
            .data(notification.title)
            .charset("UTF-8")
            .build()
            .map_err(|error| NotificationError::Delivery(error.to_string()))?;
        let body = Content::builder()
            .data(body_text)
            .charset("UTF-8")
            .build()
            .map_err(|error| NotificationError::Delivery(error.to_string()))?;
        let message = Message::builder()
            .subject(subject)
            .body(Body::builder().text(body).build())
            .build();
        let mut request = self
            .client
            .send_email()
            .from_email_address(&self.from_address)
            .destination(
                Destination::builder()
                    .to_addresses(notification.recipient)
                    .build(),
            )
            .content(EmailContent::builder().simple(message).build());
        if let Some(identity_arn) = &self.from_identity_arn {
            request = request.from_email_address_identity_arn(identity_arn);
        }
        request.send().await.map_err(|error| {
            NotificationError::Delivery(format!("SES SendEmail failed: {error}"))
        })?;
        Ok(())
    }
}

fn validate_notification(notification: &Notification) -> Result<(), NotificationError> {
    validate_email(&notification.recipient)?;
    if notification.title.trim().is_empty()
        || notification.title.len() > 200
        || notification.title.chars().any(char::is_control)
        || notification.body.len() > 1_000_000
        || notification
            .link
            .as_deref()
            .is_some_and(|link| link.len() > 2048 || link.chars().any(char::is_control))
    {
        return Err(NotificationError::Delivery(
            "email subject or body exceeds the Minco delivery boundary".into(),
        ));
    }
    Ok(())
}

fn validate_email(value: &str) -> Result<(), NotificationError> {
    let Some((local, domain)) = value.split_once('@') else {
        return Err(NotificationError::InvalidRecipient);
    };
    if local.is_empty()
        || domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || value.matches('@').count() != 1
        || value.len() > 320
        || !value.is_ascii()
        || value
            .chars()
            .any(|character| character.is_control() || character.is_ascii_whitespace())
        || domain.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(NotificationError::InvalidRecipient);
    }
    Ok(())
}

fn email_body(notification: &Notification) -> String {
    match notification.link.as_deref() {
        Some(link) => format!("{}\n\n{}", notification.body, link),
        None => notification.body.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_header_injection_and_non_email_channels() {
        assert!(validate_email("person@example.com").is_ok());
        assert!(validate_email("person@example.com\nBcc: attacker@example.com").is_err());
        let notification = Notification::new(
            "topic",
            NotificationChannel::Webhook,
            "person@example.com",
            "Title",
            "Body",
        );
        assert_ne!(notification.channel, NotificationChannel::Email);
    }
}
