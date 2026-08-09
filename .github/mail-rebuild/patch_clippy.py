from pathlib import Path


def replace_exact(path: str, old: str, new: str, expected: int = 1) -> None:
    file = Path(path)
    text = file.read_text(encoding='utf-8')
    count = text.count(old)
    if count != expected:
        raise SystemExit(f'{path}: expected {expected} matches, found {count}: {old!r}')
    file.write_text(text.replace(old, new), encoding='utf-8')


mail = 'plugins/minco-plugin-notifications/src/mail.rs'
lib = 'plugins/minco-plugin-notifications/src/lib.rs'
mailpit = 'plugins/minco-plugin-notifications/src/mailpit.rs'

replace_exact(mail, 'name.chars().any(|character| character.is_control())', 'name.chars().any(char::is_control)')
replace_exact(mail, '#[derive(Clone, PartialEq)]\npub struct MailMessage {', '#[derive(Clone, PartialEq, Eq)]\npub struct MailMessage {')
replace_exact(mail, '#[derive(Debug, Clone)]\npub struct MailMessageBuilder {', '#[must_use]\n#[derive(Debug, Clone)]\npub struct MailMessageBuilder {')
replace_exact(mail, '    pub fn id(mut self, id: Uuid) -> Self {', '    pub const fn id(mut self, id: Uuid) -> Self {')
replace_exact(mail, '    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {', '    pub const fn created_at(mut self, created_at: DateTime<Utc>) -> Self {')
replace_exact(mail, '    pub fn retry_advice(&self) -> MailRetryAdvice {', '    pub const fn retry_advice(&self) -> MailRetryAdvice {')
replace_exact(
    mail,
    '''        match event.kind {\n            MailSubmissionEventKind::AttemptFailed => tracing::warn!(\n                target: "minco.mail",\n                mail_event_id = %event.event_id,\n                mail_message_id = %event.message_id,\n                mail_topic = %event.topic,\n                mail_transport = %event.transport,\n                mail_event = ?event.kind,\n                mail_attempt = event.attempt,\n                mail_failure_kind = ?event.failure_kind,\n                mail_duration_ms = event.duration_ms,\n                "mail submission event"\n            ),\n            _ => tracing::info!(\n                target: "minco.mail",\n                mail_event_id = %event.event_id,\n                mail_message_id = %event.message_id,\n                mail_topic = %event.topic,\n                mail_transport = %event.transport,\n                mail_event = ?event.kind,\n                mail_attempt = event.attempt,\n                mail_failure_kind = ?event.failure_kind,\n                mail_duration_ms = event.duration_ms,\n                "mail submission event"\n            ),\n        }''',
    '''        if event.kind == MailSubmissionEventKind::AttemptFailed {\n            tracing::warn!(\n                target: "minco.mail",\n                mail_event_id = %event.event_id,\n                mail_message_id = %event.message_id,\n                mail_topic = %event.topic,\n                mail_transport = %event.transport,\n                mail_event = ?event.kind,\n                mail_attempt = event.attempt,\n                mail_failure_kind = ?event.failure_kind,\n                mail_duration_ms = event.duration_ms,\n                "mail submission event"\n            );\n        } else {\n            tracing::info!(\n                target: "minco.mail",\n                mail_event_id = %event.event_id,\n                mail_message_id = %event.message_id,\n                mail_topic = %event.topic,\n                mail_transport = %event.transport,\n                mail_event = ?event.kind,\n                mail_attempt = event.attempt,\n                mail_failure_kind = ?event.failure_kind,\n                mail_duration_ms = event.duration_ms,\n                "mail submission event"\n            );\n        }'''
)
replace_exact(
    mail,
    '''        let mut messages = self.messages.write().await;\n        messages.push(message.clone());\n        Ok(MailReceipt {\n            message_id: message.id,\n            transport: self.name.clone(),\n            provider_message_id: format!("memory:{}:{}", message.id, messages.len()),''',
    '''        let sequence = {\n            let mut messages = self.messages.write().await;\n            messages.push(message.clone());\n            messages.len()\n        };\n        Ok(MailReceipt {\n            message_id: message.id,\n            transport: self.name.clone(),\n            provider_message_id: format!("memory:{}:{sequence}", message.id),'''
)
replace_exact(mail, '    fn name(&self) -> &str {\n        "legacy-notification"', '    fn name(&self) -> &\'static str {\n        "legacy-notification"')
replace_exact(
    mail,
    '''        let mut source_ids = self.source_ids.write().await;\n        if !source_ids.insert(event.source_event_id.clone()) {\n            return Ok(MailDeliveryDisposition::Duplicate);\n        }\n        self.events.write().await.push(event);''',
    '''        {\n            let mut source_ids = self.source_ids.write().await;\n            if !source_ids.insert(event.source_event_id.clone()) {\n                return Ok(MailDeliveryDisposition::Duplicate);\n            }\n        }\n        self.events.write().await.push(event);'''
)
replace_exact(mail, 'fn valid_local_byte(byte: u8) -> bool {', 'const fn valid_local_byte(byte: u8) -> bool {')
replace_exact(
    mail,
    '''            match self.outcomes.lock().await.pop_front().unwrap_or(Ok(())) {''',
    '''            let outcome = {\n                let mut outcomes = self.outcomes.lock().await;\n                outcomes.pop_front().unwrap_or(Ok(()))\n            };\n            match outcome {'''
)
replace_exact(mailpit, '    fn name(&self) -> &str {\n        "mailpit"', '    fn name(&self) -> &\'static str {\n        "mailpit"')
replace_exact(lib, '#[derive(Debug, Clone)]\npub struct NotificationsPlugin {', '#[must_use]\n#[derive(Debug, Clone)]\npub struct NotificationsPlugin {')
