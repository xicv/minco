use crate::{
    ConsumeHandoffRequest, ConsumedHandoff, CreateTicketInput, ExternalMessageIngestResult,
    IngestExternalMessageRequest, MAX_TICKET_LIST_FETCH_LIMIT, Ticket, TicketActivityIntent,
    TicketId, TicketListFilter, TicketRequester, TicketStatus, TicketStoreError, TicketSummary,
    TicketSummaryFilter, TicketingStore,
};
use async_trait::async_trait;
use minco_interaction::{SupportHandoff, SupportHandoffResult};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/sqlite");

#[derive(Debug, Clone)]
pub struct SqliteTicketingStore {
    pool: SqlitePool,
}

impl SqliteTicketingStore {
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn migrate(&self) -> Result<(), TicketStoreError> {
        MIGRATOR
            .run(&self.pool)
            .await
            .map_err(|error| TicketStoreError::Infrastructure(error.to_string()))
    }

    #[must_use]
    pub const fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl TicketingStore for SqliteTicketingStore {
    async fn create(
        &self,
        ticket: Ticket,
        intent: TicketActivityIntent,
    ) -> Result<(), TicketStoreError> {
        let mut transaction = self.pool.begin().await.map_err(infrastructure)?;
        insert_ticket(&mut transaction, &ticket).await?;
        insert_children(&mut transaction, &ticket).await?;
        insert_activity(&mut transaction, &intent).await?;
        transaction.commit().await.map_err(infrastructure)
    }

    async fn get(
        &self,
        project_id: &str,
        id: TicketId,
    ) -> Result<Option<Ticket>, TicketStoreError> {
        let row = sqlx::query(
            "SELECT ticket_json FROM ticketing_tickets WHERE project_id = ? AND id = ?",
        )
        .bind(project_id)
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(infrastructure)?;
        row.map(|row| decode_ticket(row.get::<String, _>("ticket_json")))
            .transpose()
    }

    async fn list(&self, filter: TicketListFilter) -> Result<Vec<Ticket>, TicketStoreError> {
        if !(1..=MAX_TICKET_LIST_FETCH_LIMIT).contains(&filter.limit) {
            return Err(TicketStoreError::InvalidListLimit);
        }
        if filter.after_updated_at.is_some() != filter.after_id.is_some() {
            return Err(TicketStoreError::InvalidListCursor);
        }
        let statuses = filter
            .statuses
            .iter()
            .map(enum_json)
            .collect::<Result<Vec<_>, _>>()?;
        let no_status_filter = statuses.is_empty();
        let status = |index: usize| statuses.get(index).map(String::as_str);
        let after_updated_at = filter.after_updated_at.map(|value| value.to_rfc3339());
        let after_id = filter.after_id.map(|value| value.to_string());
        let rows = sqlx::query(
            "SELECT ticket_json FROM ticketing_tickets
             WHERE project_id = ?
               AND (? OR status IN (?, ?, ?, ?, ?, ?, ?))
               AND (? IS NULL OR queue_id = ?)
               AND (? IS NULL OR assignee_subject = ?)
               AND (? IS NULL OR requester_subject = ?)
               AND (? IS NULL OR updated_at > ? OR (updated_at = ? AND id > ?))
             ORDER BY updated_at, id
             LIMIT ?",
        )
        .bind(&filter.project_id)
        .bind(no_status_filter)
        .bind(status(0))
        .bind(status(1))
        .bind(status(2))
        .bind(status(3))
        .bind(status(4))
        .bind(status(5))
        .bind(status(6))
        .bind(filter.queue_id.as_deref())
        .bind(filter.queue_id.as_deref())
        .bind(filter.assignee_subject.as_deref())
        .bind(filter.assignee_subject.as_deref())
        .bind(filter.requester_subject.as_deref())
        .bind(filter.requester_subject.as_deref())
        .bind(after_updated_at.as_deref())
        .bind(after_updated_at.as_deref())
        .bind(after_updated_at.as_deref())
        .bind(after_id.as_deref())
        .bind(i64::try_from(filter.limit).map_err(infrastructure)?)
        .fetch_all(&self.pool)
        .await
        .map_err(infrastructure)?;
        rows.into_iter()
            .map(|row| decode_ticket(row.get::<String, _>("ticket_json")))
            .collect::<Result<Vec<_>, _>>()
    }

    async fn list_summaries(
        &self,
        filter: TicketSummaryFilter,
    ) -> Result<Vec<TicketSummary>, TicketStoreError> {
        if !(1..=MAX_TICKET_LIST_FETCH_LIMIT).contains(&filter.limit) {
            return Err(TicketStoreError::InvalidListLimit);
        }
        if filter.before_updated_at.is_some() != filter.before_id.is_some() {
            return Err(TicketStoreError::InvalidListCursor);
        }
        let statuses = filter
            .statuses
            .iter()
            .map(enum_json)
            .collect::<Result<Vec<_>, _>>()?;
        let no_status_filter = statuses.is_empty();
        let status = |index: usize| statuses.get(index).map(String::as_str);
        let before_updated_at = filter.before_updated_at.map(|value| value.to_rfc3339());
        let before_id = filter.before_id.map(|value| value.to_string());
        // Compact projection: projection columns and child-table counts only;
        // this query must never read ticket_json.
        let rows = sqlx::query(
            "SELECT t.id, t.display_reference, t.subject, t.status, t.priority, t.queue_id,
                    t.assignee_subject, t.requester_subject, t.created_at, t.updated_at, t.revision,
                    (SELECT COUNT(*) FROM ticketing_messages m
                      WHERE m.project_id = t.project_id AND m.ticket_id = t.id) AS message_count,
                    (SELECT COUNT(*) FROM ticketing_attachments a
                      WHERE a.project_id = t.project_id AND a.ticket_id = t.id) AS attachment_count,
                    (SELECT MAX(m.created_at) FROM ticketing_messages m
                      WHERE m.project_id = t.project_id AND m.ticket_id = t.id) AS last_activity_at
             FROM ticketing_tickets t
             WHERE t.project_id = ?
               AND (? OR t.status IN (?, ?, ?, ?, ?, ?, ?))
               AND (? IS NULL OR t.queue_id = ?)
               AND (? IS NULL OR t.assignee_subject = ?)
               AND (? IS NULL OR t.requester_subject = ?)
               AND (? IS NULL OR t.updated_at < ? OR (t.updated_at = ? AND t.id < ?))
             ORDER BY t.updated_at DESC, t.id DESC
             LIMIT ?",
        )
        .bind(&filter.project_id)
        .bind(no_status_filter)
        .bind(status(0))
        .bind(status(1))
        .bind(status(2))
        .bind(status(3))
        .bind(status(4))
        .bind(status(5))
        .bind(status(6))
        .bind(filter.queue_id.as_deref())
        .bind(filter.queue_id.as_deref())
        .bind(filter.assignee_subject.as_deref())
        .bind(filter.assignee_subject.as_deref())
        .bind(filter.requester_subject.as_deref())
        .bind(filter.requester_subject.as_deref())
        .bind(before_updated_at.as_deref())
        .bind(before_updated_at.as_deref())
        .bind(before_updated_at.as_deref())
        .bind(before_id.as_deref())
        .bind(i64::try_from(filter.limit).map_err(infrastructure)?)
        .fetch_all(&self.pool)
        .await
        .map_err(infrastructure)?;
        rows.into_iter()
            .map(|row| {
                let status = parse_enum("status", row.get::<String, _>("status"))?;
                let priority = parse_enum("priority", row.get::<String, _>("priority"))?;
                let updated_at = parse_timestamp(&row.get::<String, _>("updated_at"))?;
                let created_at = parse_timestamp(&row.get::<String, _>("created_at"))?;
                Ok(TicketSummary {
                    id: TicketId(Uuid::parse_str(&row.get::<String, _>("id")).map_err(|_| {
                        TicketStoreError::Infrastructure("ticket id is not a UUID".into())
                    })?),
                    project_id: filter.project_id.clone(),
                    display_reference: row.get("display_reference"),
                    subject: row.get("subject"),
                    requester_subject: row.get("requester_subject"),
                    status,
                    clock_state: status.clock_state(),
                    priority,
                    queue_id: row.get("queue_id"),
                    assignee_subject: row.get("assignee_subject"),
                    message_count: usize::try_from(row.get::<i64, _>("message_count")).map_err(
                        |_| TicketStoreError::Infrastructure("message count overflow".into()),
                    )?,
                    attachment_count: usize::try_from(row.get::<i64, _>("attachment_count"))
                        .map_err(|_| {
                            TicketStoreError::Infrastructure("attachment count overflow".into())
                        })?,
                    last_activity_at: row
                        .get::<Option<String>, _>("last_activity_at")
                        .map(|value| parse_timestamp(&value))
                        .transpose()?,
                    needs_attention: matches!(
                        status,
                        TicketStatus::New | TicketStatus::PendingInternal
                    ),
                    created_at,
                    updated_at,
                    revision: u64::try_from(row.get::<i64, _>("revision")).map_err(|_| {
                        TicketStoreError::Infrastructure("revision overflow".into())
                    })?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    async fn save(
        &self,
        ticket: Ticket,
        expected_revision: u64,
        intent: TicketActivityIntent,
    ) -> Result<(), TicketStoreError> {
        let mut transaction = self.pool.begin().await.map_err(infrastructure)?;
        update_ticket(&mut transaction, &ticket, expected_revision).await?;
        replace_children(&mut transaction, &ticket).await?;
        insert_activity(&mut transaction, &intent).await?;
        transaction.commit().await.map_err(infrastructure)
    }

    async fn insert_handoff(&self, handoff: SupportHandoff) -> Result<(), TicketStoreError> {
        let json = serde_json::to_string(&handoff).map_err(encoding)?;
        sqlx::query(
            "INSERT INTO ticketing_handoffs (digest, handoff_id, project_id, portal_origin, expires_at, handoff_json) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(handoff.digest.as_str())
        .bind(handoff.id.to_string())
        .bind(&handoff.project_id)
        .bind(&handoff.portal_origin)
        .bind(handoff.expires_at.to_rfc3339())
        .bind(json)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            if is_unique(&error) { TicketStoreError::DuplicateHandoff } else { infrastructure(error) }
        })?;
        Ok(())
    }

    async fn consume_and_create_ticket(
        &self,
        request: ConsumeHandoffRequest,
    ) -> Result<ConsumedHandoff, TicketStoreError> {
        let digest = request.token.digest();
        let mut transaction = self.pool.begin().await.map_err(infrastructure)?;
        let claimed = sqlx::query(
            "UPDATE ticketing_handoffs SET completed_fingerprint = ? WHERE digest = ? AND consumed_result_json IS NULL AND completed_fingerprint IS NULL",
        )
            .bind(&request.request_fingerprint)
            .bind(digest.as_str())
            .execute(&mut *transaction)
            .await
            .map_err(infrastructure)?;
        let row = sqlx::query(
            "SELECT handoff_json, consumed_result_json, completed_fingerprint FROM ticketing_handoffs WHERE digest = ?",
        )
        .bind(digest.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(infrastructure)?
        .ok_or(TicketStoreError::UnknownHandoff)?;
        let handoff: SupportHandoff =
            serde_json::from_str(row.get("handoff_json")).map_err(encoding)?;
        if !handoff.digest.matches_token(&request.token) {
            return Err(TicketStoreError::UnknownHandoff);
        }
        if handoff.project_id != request.project_id {
            return Err(TicketStoreError::WrongHandoffProject);
        }
        if handoff.portal_origin != request.portal_origin {
            return Err(TicketStoreError::WrongHandoffPortal);
        }
        let completed: Option<String> = row.get("consumed_result_json");
        let completed_fingerprint: Option<String> = row.get("completed_fingerprint");
        if let Some(completed) = completed {
            if completed_fingerprint.as_deref() != Some(&request.request_fingerprint) {
                return Err(TicketStoreError::HandoffAlreadyConsumed);
            }
            let result: SupportHandoffResult =
                serde_json::from_str(&completed).map_err(encoding)?;
            let ticket = load_ticket_tx(
                &mut transaction,
                &request.project_id,
                TicketId(result.ticket_id),
            )
            .await?
            .ok_or_else(|| {
                TicketStoreError::Infrastructure("completed handoff ticket is missing".into())
            })?;
            transaction.commit().await.map_err(infrastructure)?;
            return Ok(ConsumedHandoff {
                ticket,
                result,
                repeated: true,
            });
        }
        if claimed.rows_affected() == 0 {
            return Err(TicketStoreError::Infrastructure(
                "handoff claim completed without an authoritative result".into(),
            ));
        }
        if handoff.expires_at <= request.now {
            return Err(TicketStoreError::ExpiredHandoff);
        }

        let ticket_uuid = uuid::Uuid::now_v7();
        let ticket = Ticket::create(
            CreateTicketInput {
                project_id: handoff.project_id.clone(),
                subject: request.input.subject,
                description: request.input.description,
                requester: TicketRequester {
                    subject: handoff.requester_subject.clone(),
                    display_name: None,
                    email: None,
                },
                channel: request.input.channel,
                priority: request.input.priority,
                resource_references: handoff.context.resource_references.clone(),
            },
            format!("TKT-{}", &ticket_uuid.simple().to_string()[..12]),
            request.now,
        )?;
        let result = SupportHandoffResult {
            ticket_id: ticket.id.0,
            requester_session_id: uuid::Uuid::now_v7(),
        };
        insert_ticket(&mut transaction, &ticket).await?;
        insert_children(&mut transaction, &ticket).await?;
        insert_activity(
            &mut transaction,
            &TicketActivityIntent::new(
                ticket.project_id.clone(),
                ticket.id,
                "ticketing.created_from_handoff",
                handoff.correlation_id,
                serde_json::json!({ "ticket_id": ticket.id, "handoff_id": handoff.id }),
                request.now,
            ),
        )
        .await?;
        let completed = sqlx::query(
            "UPDATE ticketing_handoffs SET consumed_result_json = ? WHERE digest = ? AND completed_fingerprint = ? AND consumed_result_json IS NULL",
        )
        .bind(serde_json::to_string(&result).map_err(encoding)?)
        .bind(digest.as_str())
        .bind(&request.request_fingerprint)
        .execute(&mut *transaction)
        .await
        .map_err(infrastructure)?;
        if completed.rows_affected() != 1 {
            return Err(TicketStoreError::Infrastructure(
                "handoff completion lost its atomic claim".into(),
            ));
        }
        transaction.commit().await.map_err(infrastructure)?;
        Ok(ConsumedHandoff {
            ticket,
            result,
            repeated: false,
        })
    }

    async fn ingest_external_message(
        &self,
        request: IngestExternalMessageRequest,
    ) -> Result<ExternalMessageIngestResult, TicketStoreError> {
        let mut transaction = self.pool.begin().await.map_err(infrastructure)?;
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO ticketing_external_messages
             (project_id, provider, mailbox_scope, external_id, content_sha256, identity_json, ticket_id, created_at)
             SELECT ?, ?, ?, ?, ?, ?, ?, ?
             WHERE EXISTS (
               SELECT 1 FROM ticketing_tickets WHERE project_id = ? AND id = ?
             )",
        )
        .bind(&request.identity.project_id)
        .bind(&request.identity.provider)
        .bind(&request.identity.mailbox_scope)
        .bind(&request.identity.external_id)
        .bind(&request.identity.content_sha256)
        .bind(serde_json::to_string(&request.identity).map_err(encoding)?)
        .bind(request.ticket_id.to_string())
        .bind(request.now.to_rfc3339())
        .bind(&request.identity.project_id)
        .bind(request.ticket_id.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(infrastructure)?;
        if inserted.rows_affected() == 0 {
            let row = sqlx::query(
                "SELECT content_sha256, ticket_id FROM ticketing_external_messages WHERE project_id = ? AND provider = ? AND mailbox_scope = ? AND external_id = ?",
            )
            .bind(&request.identity.project_id)
            .bind(&request.identity.provider)
            .bind(&request.identity.mailbox_scope)
            .bind(&request.identity.external_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(infrastructure)?;
            let Some(row) = row else {
                return Err(TicketStoreError::NotFound(request.ticket_id));
            };
            return if row.get::<String, _>("content_sha256") == request.identity.content_sha256 {
                let ticket_id = row
                    .get::<String, _>("ticket_id")
                    .parse::<TicketId>()
                    .map_err(encoding)?;
                let ticket =
                    load_ticket_tx(&mut transaction, &request.identity.project_id, ticket_id)
                        .await?
                        .ok_or_else(|| {
                            TicketStoreError::Infrastructure(
                                "external message authoritative ticket is missing".into(),
                            )
                        })?;
                transaction.commit().await.map_err(infrastructure)?;
                Ok(ExternalMessageIngestResult {
                    ticket,
                    repeated: true,
                })
            } else {
                Err(TicketStoreError::ExternalIdentityConflict)
            };
        }
        let mut ticket = load_ticket_tx(
            &mut transaction,
            &request.identity.project_id,
            request.ticket_id,
        )
        .await?
        .ok_or(TicketStoreError::NotFound(request.ticket_id))?;
        if ticket.revision != request.expected_revision {
            return Err(TicketStoreError::StaleRevision {
                expected: request.expected_revision,
                actual: ticket.revision,
            });
        }
        ticket.reply_as_requester(request.body, request.now)?;
        let intent = TicketActivityIntent::new(
            ticket.project_id.clone(),
            ticket.id,
            "ticketing.external_message_ingested",
            request.correlation_id,
            serde_json::json!({ "ticket_id": ticket.id }),
            request.now,
        );
        update_ticket(&mut transaction, &ticket, request.expected_revision).await?;
        replace_children(&mut transaction, &ticket).await?;
        insert_activity(&mut transaction, &intent).await?;
        transaction.commit().await.map_err(infrastructure)?;
        Ok(ExternalMessageIngestResult {
            ticket,
            repeated: false,
        })
    }

    async fn ready(&self) -> Result<(), TicketStoreError> {
        sqlx::query("SELECT 1 FROM ticketing_tickets LIMIT 1")
            .execute(&self.pool)
            .await
            .map_err(infrastructure)?;
        Ok(())
    }
}

async fn insert_ticket(
    transaction: &mut Transaction<'_, Sqlite>,
    ticket: &Ticket,
) -> Result<(), TicketStoreError> {
    sqlx::query(
        "INSERT INTO ticketing_tickets (project_id, id, display_reference, subject, priority, status, queue_id, assignee_subject, requester_subject, created_at, updated_at, revision, ticket_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&ticket.project_id)
    .bind(ticket.id.to_string())
    .bind(&ticket.display_reference)
    .bind(&ticket.subject)
    .bind(enum_json(&ticket.priority)?)
    .bind(enum_json(&ticket.status)?)
    .bind(&ticket.queue_id)
    .bind(&ticket.assignee_subject)
    .bind(&ticket.requester.subject)
    .bind(ticket.created_at.to_rfc3339())
    .bind(ticket.updated_at.to_rfc3339())
    .bind(i64::try_from(ticket.revision).map_err(|_| TicketStoreError::Infrastructure("revision exceeds SQLite integer".into()))?)
    .bind(serde_json::to_string(ticket).map_err(encoding)?)
    .execute(&mut **transaction)
    .await
    .map_err(|error| if is_unique(&error) { TicketStoreError::DuplicateDisplayReference } else { infrastructure(error) })?;
    Ok(())
}

async fn update_ticket(
    transaction: &mut Transaction<'_, Sqlite>,
    ticket: &Ticket,
    expected_revision: u64,
) -> Result<(), TicketStoreError> {
    if ticket.revision <= expected_revision {
        return Err(TicketStoreError::StaleRevision {
            expected: expected_revision,
            actual: ticket.revision,
        });
    }
    let result = sqlx::query(
        "UPDATE ticketing_tickets SET subject = ?, priority = ?, status = ?, queue_id = ?, assignee_subject = ?, requester_subject = ?, created_at = ?, updated_at = ?, revision = ?, ticket_json = ? WHERE project_id = ? AND id = ? AND revision = ?",
    )
    .bind(&ticket.subject)
    .bind(enum_json(&ticket.priority)?)
    .bind(enum_json(&ticket.status)?)
    .bind(&ticket.queue_id)
    .bind(&ticket.assignee_subject)
    .bind(&ticket.requester.subject)
    .bind(ticket.created_at.to_rfc3339())
    .bind(ticket.updated_at.to_rfc3339())
    .bind(i64::try_from(ticket.revision).map_err(|_| TicketStoreError::Infrastructure("revision exceeds SQLite integer".into()))?)
    .bind(serde_json::to_string(ticket).map_err(encoding)?)
    .bind(&ticket.project_id)
    .bind(ticket.id.to_string())
    .bind(i64::try_from(expected_revision).map_err(|_| TicketStoreError::Infrastructure("revision exceeds SQLite integer".into()))?)
    .execute(&mut **transaction)
    .await
    .map_err(infrastructure)?;
    if result.rows_affected() == 0 {
        let actual =
            sqlx::query("SELECT revision FROM ticketing_tickets WHERE project_id = ? AND id = ?")
                .bind(&ticket.project_id)
                .bind(ticket.id.to_string())
                .fetch_optional(&mut **transaction)
                .await
                .map_err(infrastructure)?
                .ok_or(TicketStoreError::NotFound(ticket.id))?
                .get::<i64, _>("revision");
        let actual = u64::try_from(actual).map_err(|_| {
            TicketStoreError::Infrastructure("stored ticket revision is negative".into())
        })?;
        return Err(TicketStoreError::StaleRevision {
            expected: expected_revision,
            actual,
        });
    }
    Ok(())
}

async fn insert_children(
    transaction: &mut Transaction<'_, Sqlite>,
    ticket: &Ticket,
) -> Result<(), TicketStoreError> {
    for message in &ticket.messages {
        sqlx::query("INSERT INTO ticketing_messages (project_id, ticket_id, id, created_at, message_json) VALUES (?, ?, ?, ?, ?)")
            .bind(&ticket.project_id).bind(ticket.id.to_string()).bind(message.id.to_string())
            .bind(message.created_at.to_rfc3339()).bind(serde_json::to_string(message).map_err(encoding)?)
            .execute(&mut **transaction).await.map_err(infrastructure)?;
    }
    insert_non_message_children(transaction, ticket).await
}

async fn replace_children(
    transaction: &mut Transaction<'_, Sqlite>,
    ticket: &Ticket,
) -> Result<(), TicketStoreError> {
    for query in [
        "DELETE FROM ticketing_messages WHERE project_id = ? AND ticket_id = ?",
        "DELETE FROM ticketing_attachments WHERE project_id = ? AND ticket_id = ?",
        "DELETE FROM ticketing_followers WHERE project_id = ? AND ticket_id = ?",
        "DELETE FROM ticketing_tags WHERE project_id = ? AND ticket_id = ?",
        "DELETE FROM ticketing_source_references WHERE project_id = ? AND ticket_id = ?",
        "DELETE FROM ticketing_resource_references WHERE project_id = ? AND ticket_id = ?",
    ] {
        sqlx::query(query)
            .bind(&ticket.project_id)
            .bind(ticket.id.to_string())
            .execute(&mut **transaction)
            .await
            .map_err(infrastructure)?;
    }
    insert_children(transaction, ticket).await
}

async fn insert_non_message_children(
    transaction: &mut Transaction<'_, Sqlite>,
    ticket: &Ticket,
) -> Result<(), TicketStoreError> {
    for attachment in &ticket.attachments {
        sqlx::query("INSERT INTO ticketing_attachments (project_id, ticket_id, id, object_key, attachment_json) VALUES (?, ?, ?, ?, ?)")
            .bind(&ticket.project_id).bind(ticket.id.to_string()).bind(attachment.id.to_string()).bind(&attachment.object_key)
            .bind(serde_json::to_string(attachment).map_err(encoding)?).execute(&mut **transaction).await.map_err(infrastructure)?;
    }
    for follower in &ticket.followers {
        sqlx::query(
            "INSERT INTO ticketing_followers (project_id, ticket_id, subject) VALUES (?, ?, ?)",
        )
        .bind(&ticket.project_id)
        .bind(ticket.id.to_string())
        .bind(follower)
        .execute(&mut **transaction)
        .await
        .map_err(infrastructure)?;
    }
    for tag in &ticket.tags {
        sqlx::query("INSERT INTO ticketing_tags (project_id, ticket_id, tag) VALUES (?, ?, ?)")
            .bind(&ticket.project_id)
            .bind(ticket.id.to_string())
            .bind(tag)
            .execute(&mut **transaction)
            .await
            .map_err(infrastructure)?;
    }
    for reference in &ticket.source_references {
        sqlx::query("INSERT INTO ticketing_source_references (project_id, ticket_id, provider, scope, external_id) VALUES (?, ?, ?, ?, ?)")
            .bind(&ticket.project_id).bind(ticket.id.to_string()).bind(&reference.provider).bind(&reference.scope).bind(&reference.external_id)
            .execute(&mut **transaction).await.map_err(infrastructure)?;
    }
    for reference in &ticket.resource_references {
        sqlx::query("INSERT INTO ticketing_resource_references (project_id, ticket_id, system, resource_type, resource_id) VALUES (?, ?, ?, ?, ?)")
            .bind(&ticket.project_id).bind(ticket.id.to_string()).bind(&reference.system).bind(&reference.resource_type).bind(&reference.resource_id)
            .execute(&mut **transaction).await.map_err(infrastructure)?;
    }
    Ok(())
}

async fn insert_activity(
    transaction: &mut Transaction<'_, Sqlite>,
    intent: &TicketActivityIntent,
) -> Result<(), TicketStoreError> {
    sqlx::query("INSERT INTO ticketing_activity_intents (id, project_id, ticket_id, kind, correlation_id, payload_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(intent.id.to_string()).bind(&intent.project_id).bind(intent.ticket_id.to_string()).bind(&intent.kind)
        .bind(intent.correlation_id.to_string()).bind(serde_json::to_string(&intent.payload).map_err(encoding)?)
        .bind(intent.created_at.to_rfc3339()).execute(&mut **transaction).await.map_err(infrastructure)?;
    Ok(())
}

async fn load_ticket_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    project_id: &str,
    id: TicketId,
) -> Result<Option<Ticket>, TicketStoreError> {
    sqlx::query("SELECT ticket_json FROM ticketing_tickets WHERE project_id = ? AND id = ?")
        .bind(project_id)
        .bind(id.to_string())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(infrastructure)?
        .map(|row| decode_ticket(row.get::<String, _>("ticket_json")))
        .transpose()
}

#[allow(clippy::needless_pass_by_value)]
fn decode_ticket(value: String) -> Result<Ticket, TicketStoreError> {
    serde_json::from_str(&value).map_err(encoding)
}

fn enum_json<T: serde::Serialize>(value: &T) -> Result<String, TicketStoreError> {
    serde_json::to_string(value)
        .map(|value| value.trim_matches('"').to_owned())
        .map_err(encoding)
}

fn parse_enum<T: serde::de::DeserializeOwned>(
    field: &'static str,
    value: String,
) -> Result<T, TicketStoreError> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(|_| {
        TicketStoreError::Infrastructure(format!("stored {field} is not a valid enum value"))
    })
}

fn parse_timestamp(value: &str) -> Result<chrono::DateTime<chrono::Utc>, TicketStoreError> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&chrono::Utc))
        .map_err(|_| TicketStoreError::Infrastructure("stored timestamp is not RFC 3339".into()))
}

fn infrastructure(error: impl std::fmt::Display) -> TicketStoreError {
    TicketStoreError::Infrastructure(error.to_string())
}

fn encoding(error: impl std::fmt::Display) -> TicketStoreError {
    TicketStoreError::Infrastructure(format!("ticketing persisted JSON is invalid: {error}"))
}

fn is_unique(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        TicketChannel, TicketClockState, TicketFromHandoffInput, TicketPriority, TicketStatus,
    };
    use chrono::{TimeDelta, Utc};
    use minco_interaction::{
        SupportContext, SupportLocationPolicy, SupportSurface, issue_support_handoff,
    };
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
    use std::{collections::BTreeMap, str::FromStr, sync::Arc};

    async fn store() -> (tempfile::TempDir, Arc<SqliteTicketingStore>) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ticketing.sqlite");
        let options = SqliteConnectOptions::from_str(path.to_str().unwrap())
            .unwrap()
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .unwrap();
        let store = Arc::new(SqliteTicketingStore::new(pool));
        store.migrate().await.unwrap();
        (directory, store)
    }

    fn handoff(
        now: chrono::DateTime<Utc>,
    ) -> (SupportHandoff, minco_interaction::SupportHandoffToken) {
        let (handoff, grant) = issue_support_handoff(
            "project-a",
            "user-1",
            vec!["ticketing.create".into()],
            SupportSurface::Widget,
            SupportContext {
                page_url: "https://app.example.test/orders/1".into(),
                ..SupportContext::default()
            },
            "https://app.example.test/orders/1",
            uuid::Uuid::now_v7(),
            &SupportLocationPolicy {
                portal_origin: "https://support.example.test".into(),
                allowed_return_paths: BTreeMap::from([(
                    "https://app.example.test".into(),
                    vec!["/orders".into()],
                )]),
            },
            now,
            TimeDelta::minutes(5),
        )
        .unwrap();
        (handoff, grant.token)
    }

    fn consume(
        token: minco_interaction::SupportHandoffToken,
        now: chrono::DateTime<Utc>,
    ) -> ConsumeHandoffRequest {
        ConsumeHandoffRequest::new(
            token,
            "project-a",
            "https://support.example.test",
            TicketFromHandoffInput {
                subject: "Help".into(),
                description: "Broken".into(),
                channel: TicketChannel::Portal,
                priority: TicketPriority::Normal,
            },
            now,
        )
        .unwrap()
    }

    fn ingress(
        ticket_id: TicketId,
        content_sha256: &str,
        now: chrono::DateTime<Utc>,
    ) -> IngestExternalMessageRequest {
        IngestExternalMessageRequest {
            identity: crate::ExternalMessageIdentity {
                project_id: "project-a".into(),
                provider: "example-mail".into(),
                mailbox_scope: "support@example.test".into(),
                external_id: "message-1".into(),
                content_sha256: content_sha256.into(),
                raw_message_object_key: Some("mail/project-a/message-1".into()),
                internet_message_id: Some("<message-1@example.test>".into()),
                in_reply_to: None,
                references: Vec::new(),
            },
            ticket_id,
            body: "External reply".into(),
            expected_revision: 0,
            correlation_id: uuid::Uuid::now_v7(),
            now,
        }
    }

    #[tokio::test]
    async fn concurrent_handoff_exchange_is_atomic_and_idempotent() {
        let (_directory, store) = store().await;
        let now = Utc::now();
        let (handoff, token) = handoff(now);
        store.insert_handoff(handoff).await.unwrap();
        let left = {
            let store = store.clone();
            let token = token.clone();
            tokio::spawn(async move { store.consume_and_create_ticket(consume(token, now)).await })
        };
        let right = {
            let store = store.clone();
            let token = token.clone();
            tokio::spawn(async move { store.consume_and_create_ticket(consume(token, now)).await })
        };
        let (left, right) = tokio::join!(left, right);
        let left = left.unwrap().unwrap();
        let right = right.unwrap().unwrap();
        assert_eq!(left.result, right.result);
        assert_ne!(left.repeated, right.repeated);
        let after_expiry = store
            .consume_and_create_ticket(consume(token, now + TimeDelta::minutes(10)))
            .await
            .unwrap();
        assert!(after_expiry.repeated);
        assert_eq!(after_expiry.result, left.result);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ticketing_tickets")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn ticket_and_activity_roll_back_together() {
        let (_directory, store) = store().await;
        sqlx::query("CREATE TRIGGER fail_ticket_activity BEFORE INSERT ON ticketing_activity_intents BEGIN SELECT RAISE(ABORT, 'injected'); END")
            .execute(store.pool()).await.unwrap();
        let now = Utc::now();
        let ticket = Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Help".into(),
                description: "Broken".into(),
                requester: TicketRequester {
                    subject: "user".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Api,
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "TKT-ROLLBACK",
            now,
        )
        .unwrap();
        let intent = TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            uuid::Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        assert!(matches!(
            store.create(ticket, intent).await,
            Err(TicketStoreError::Infrastructure(_))
        ));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ticketing_tickets")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn save_rejects_a_non_advancing_revision() {
        let (_directory, store) = store().await;
        let now = Utc::now();
        let ticket = Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Help".into(),
                description: "Broken".into(),
                requester: TicketRequester {
                    subject: "user".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Api,
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "TKT-REVISION",
            now,
        )
        .unwrap();
        store
            .create(
                ticket.clone(),
                TicketActivityIntent::new(
                    "project-a",
                    ticket.id,
                    "created",
                    uuid::Uuid::now_v7(),
                    serde_json::json!({}),
                    now,
                ),
            )
            .await
            .unwrap();

        let error = store
            .save(
                ticket.clone(),
                ticket.revision,
                TicketActivityIntent::new(
                    "project-a",
                    ticket.id,
                    "unchanged",
                    uuid::Uuid::now_v7(),
                    serde_json::json!({}),
                    now,
                ),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, TicketStoreError::StaleRevision { .. }));
        let activity_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ticketing_activity_intents")
                .fetch_one(store.pool())
                .await
                .unwrap();
        assert_eq!(activity_count, 1);
    }

    #[tokio::test]
    async fn external_ingress_is_atomic_idempotent_and_conflict_safe() {
        let (_directory, store) = store().await;
        let now = Utc::now();
        let ticket = Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Help".into(),
                description: "Broken".into(),
                requester: TicketRequester {
                    subject: "user".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Api,
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "TKT-EXTERNAL",
            now,
        )
        .unwrap();
        store
            .create(
                ticket.clone(),
                TicketActivityIntent::new(
                    "project-a",
                    ticket.id,
                    "created",
                    uuid::Uuid::now_v7(),
                    serde_json::json!({}),
                    now,
                ),
            )
            .await
            .unwrap();

        let digest = "a".repeat(64);
        let first = store
            .ingest_external_message(ingress(ticket.id, &digest, now))
            .await
            .unwrap();
        let repeated = store
            .ingest_external_message(ingress(ticket.id, &digest, now))
            .await
            .unwrap();
        assert!(!first.repeated);
        assert!(repeated.repeated);
        assert_eq!(first.ticket, repeated.ticket);

        assert!(matches!(
            store
                .ingest_external_message(ingress(ticket.id, &"b".repeat(64), now))
                .await,
            Err(TicketStoreError::ExternalIdentityConflict)
        ));
        let messages: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ticketing_messages WHERE project_id = ? AND ticket_id = ?",
        )
        .bind("project-a")
        .bind(ticket.id.to_string())
        .fetch_one(store.pool())
        .await
        .unwrap();
        assert_eq!(messages, 2);
    }

    #[tokio::test]
    async fn external_ingress_reports_a_missing_ticket_before_any_identity_is_stored() {
        let (_directory, store) = store().await;
        let missing = TicketId::new();
        assert!(matches!(
            store
                .ingest_external_message(ingress(missing, &"a".repeat(64), Utc::now()))
                .await,
            Err(TicketStoreError::NotFound(id)) if id == missing
        ));
        let identities: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ticketing_external_messages")
                .fetch_one(store.pool())
                .await
                .unwrap();
        assert_eq!(identities, 0);
    }

    #[tokio::test]
    async fn list_filters_before_lookahead_limit_without_a_thousand_row_cutoff() {
        let (_directory, store) = store().await;
        let base = chrono::DateTime::from_timestamp(1_777_777_777, 0).unwrap();
        let mut transaction = store.pool().begin().await.unwrap();
        let mut expected_id = None;
        for index in 0..=1_000 {
            let now = base + TimeDelta::milliseconds(index);
            let mut ticket = Ticket::create(
                CreateTicketInput {
                    project_id: "project-a".into(),
                    subject: format!("Ticket {index}"),
                    description: "Bounded listing".into(),
                    requester: TicketRequester {
                        subject: "user".into(),
                        display_name: None,
                        email: None,
                    },
                    channel: TicketChannel::Api,
                    priority: TicketPriority::Normal,
                    resource_references: Vec::new(),
                },
                format!("TKT-LIST-{index:04}"),
                now,
            )
            .unwrap();
            if index < 1_000 {
                ticket.status = TicketStatus::Open;
                ticket.clock_state = TicketClockState::Open;
            } else {
                expected_id = Some(ticket.id);
            }
            insert_ticket(&mut transaction, &ticket).await.unwrap();
        }
        transaction.commit().await.unwrap();

        let tickets = store
            .list(TicketListFilter {
                project_id: "project-a".into(),
                statuses: std::collections::BTreeSet::from([TicketStatus::New]),
                limit: MAX_TICKET_LIST_FETCH_LIMIT,
                ..TicketListFilter::default()
            })
            .await
            .unwrap();

        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].id, expected_id.unwrap());
    }

    #[tokio::test]
    async fn summary_list_matches_memory_projection_newest_first() {
        let (_directory, sqlite) = store().await;
        let memory = crate::MemoryTicketingStore::default();
        let base = chrono::DateTime::from_timestamp(1_778_000_000, 0).unwrap();
        let tied = base + TimeDelta::seconds(10);
        for (index, (instant, reference)) in
            [(base, "TKT-OLD"), (tied, "TKT-TIE-A"), (tied, "TKT-TIE-B")]
                .into_iter()
                .enumerate()
        {
            let mut ticket = crate::Ticket::create(
                crate::CreateTicketInput {
                    project_id: "project-a".into(),
                    subject: format!("Help {reference}"),
                    description: "Broken".into(),
                    requester: crate::TicketRequester {
                        subject: "user-1".into(),
                        display_name: None,
                        email: None,
                    },
                    channel: TicketChannel::Api,
                    priority: if index == 1 {
                        TicketPriority::High
                    } else {
                        TicketPriority::Normal
                    },
                    resource_references: Vec::new(),
                },
                reference,
                instant,
            )
            .unwrap();
            ticket.updated_at = instant;
            if index == 2 {
                ticket
                    .add_internal_note("agent", "private note", instant + TimeDelta::seconds(1))
                    .unwrap();
            }
            let intent = crate::TicketActivityIntent::new(
                "project-a",
                ticket.id,
                "created",
                Uuid::now_v7(),
                serde_json::json!({}),
                instant,
            );
            sqlite.create(ticket.clone(), intent.clone()).await.unwrap();
            memory.create(ticket, intent).await.unwrap();
        }

        let filter = |limit: usize| crate::TicketSummaryFilter {
            project_id: "project-a".into(),
            limit,
            ..crate::TicketSummaryFilter::default()
        };
        let from_sqlite = sqlite.list_summaries(filter(10)).await.unwrap();
        let from_memory = memory.list_summaries(filter(10)).await.unwrap();
        assert_eq!(from_sqlite, from_memory);
        assert_eq!(
            from_sqlite
                .iter()
                .map(|summary| summary.display_reference.clone())
                .collect::<Vec<_>>(),
            vec!["TKT-TIE-B", "TKT-TIE-A", "TKT-OLD"]
        );
        assert_eq!(from_sqlite[0].message_count, 2);
        assert!(from_sqlite[0].needs_attention);
        assert_eq!(from_sqlite[0].status, TicketStatus::New);
        assert!(from_sqlite[0].last_activity_at.is_some());

        let mut paged = filter(10);
        paged.before_updated_at = Some(from_sqlite[0].updated_at);
        paged.before_id = Some(from_sqlite[0].id);
        let rest = sqlite.list_summaries(paged).await.unwrap();
        assert_eq!(
            rest.iter()
                .map(|summary| summary.display_reference.clone())
                .collect::<Vec<_>>(),
            vec!["TKT-TIE-A", "TKT-OLD"]
        );
    }
}
