use crate::{
    AgentMacro, ConsumeHandoffRequest, ConsumeSessionRequest, ConsumedHandoff,
    ConsumedSessionIdentity, CreateTicketInput, DeliveryFeedbackKind, ExternalMessageIngestResult,
    IngestExternalMessageRequest, MAX_TICKET_LIST_FETCH_LIMIT, OutboundDeliveryEvidence,
    OutboundEvidenceKind, Ticket, TicketActivityIntent, TicketId, TicketListFilter,
    TicketMessageId, TicketRequester, TicketStatus, TicketStoreError, TicketSummary,
    TicketSummaryFilter, TicketingStore,
};
use async_trait::async_trait;
use minco_interaction::{SupportHandoff, SupportHandoffResult};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/sqlite");

#[derive(Debug, Clone)]
pub struct SqliteTicketingStore {
    pool: SqlitePool,
    #[cfg(feature = "jobs")]
    job_enqueue: Option<Arc<dyn crate::TicketingJobEnqueue>>,
}

impl SqliteTicketingStore {
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            #[cfg(feature = "jobs")]
            job_enqueue: None,
        }
    }

    /// Pattern A (ADR-0054): with an enqueue adapter sharing this pool,
    /// job records attached to a mutation commit in its transaction.
    #[must_use]
    #[cfg(feature = "jobs")]
    pub fn with_job_enqueue(mut self, enqueue: Arc<dyn crate::TicketingJobEnqueue>) -> Self {
        self.job_enqueue = Some(enqueue);
        self
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
        let mut connection = self.pool.acquire().await.map_err(infrastructure)?;
        load_ticket_row(&mut connection, project_id, &id.to_string()).await
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
            "SELECT id FROM ticketing_tickets
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
        let mut connection = self.pool.acquire().await.map_err(infrastructure)?;
        let mut tickets = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.get::<String, _>("id");
            tickets.push(
                load_ticket_row(&mut connection, &filter.project_id, &id)
                    .await?
                    .ok_or_else(|| {
                        TicketStoreError::Infrastructure(
                            "ticket row disappeared during list reconstruction".into(),
                        )
                    })?,
            );
        }
        Ok(tickets)
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
            "SELECT t.id, t.display_reference, t.subject, t.status, t.priority, t.ticket_type, t.first_response_deadline, t.resolution_deadline, t.queue_id,
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
               AND (? = 0 OR t.assignee_subject IS NULL)
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
        .bind(i64::from(filter.unassigned))
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
                    ticket_type: parse_enum("ticket_type", row.get::<String, _>("ticket_type"))?,
                    first_response_deadline: row
                        .get::<Option<String>, _>("first_response_deadline")
                        .map(|value| parse_timestamp(&value))
                        .transpose()?,
                    resolution_deadline: row
                        .get::<Option<String>, _>("resolution_deadline")
                        .map(|value| parse_timestamp(&value))
                        .transpose()?,
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

    async fn append_ticket_message(
        &self,
        request: crate::AppendTicketMessageRequest,
    ) -> Result<(), TicketStoreError> {
        #[cfg(feature = "jobs")]
        if request.job_records.len() > crate::MAX_JOB_RECORDS_PER_MUTATION {
            return Err(TicketStoreError::InvalidJobRecords);
        }
        let mut transaction = self.pool.begin().await.map_err(infrastructure)?;
        let updated = sqlx::query(
            "UPDATE ticketing_tickets
                SET status = ?, first_public_response_at = ?, waiting_since = ?,
                    resolved_at = ?, updated_at = ?, revision = ?
              WHERE project_id = ? AND id = ? AND revision = ?",
        )
        .bind(enum_json(&request.status)?)
        .bind(request.first_public_response_at.map(|v| v.to_rfc3339()))
        .bind(request.waiting_since.map(|v| v.to_rfc3339()))
        .bind(request.resolved_at.map(|v| v.to_rfc3339()))
        .bind(request.updated_at.to_rfc3339())
        .bind(i64::try_from(request.expected_revision + 1).map_err(|_| {
            TicketStoreError::Infrastructure("revision exceeds SQLite integer".into())
        })?)
        .bind(&request.project_id)
        .bind(request.ticket_id.to_string())
        .bind(i64::try_from(request.expected_revision).map_err(|_| {
            TicketStoreError::Infrastructure("revision exceeds SQLite integer".into())
        })?)
        .execute(&mut *transaction)
        .await
        .map_err(infrastructure)?;
        if updated.rows_affected() == 0 {
            let actual = sqlx::query(
                "SELECT revision FROM ticketing_tickets WHERE project_id = ? AND id = ?",
            )
            .bind(&request.project_id)
            .bind(request.ticket_id.to_string())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(infrastructure)?;
            return match actual {
                Some(row) => Err(TicketStoreError::StaleRevision {
                    expected: request.expected_revision,
                    actual: u64::try_from(row.get::<i64, _>("revision")).map_err(|_| {
                        TicketStoreError::Infrastructure("revision overflow".into())
                    })?,
                }),
                None => Err(TicketStoreError::NotFound(request.ticket_id)),
            };
        }
        sqlx::query(
            "INSERT INTO ticketing_messages (project_id, ticket_id, id, created_at, message_json) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&request.project_id)
        .bind(request.ticket_id.to_string())
        .bind(request.message.id.to_string())
        .bind(request.message.created_at.to_rfc3339())
        .bind(serde_json::to_string(&request.message).map_err(encoding)?)
        .execute(&mut *transaction)
        .await
        .map_err(infrastructure)?;
        insert_activity(&mut transaction, &request.intent).await?;
        #[cfg(feature = "jobs")]
        if !request.job_records.is_empty() {
            let sink = self.job_enqueue.as_ref().ok_or_else(|| {
                TicketStoreError::Infrastructure(
                    "job records require a configured TicketingJobEnqueue adapter".into(),
                )
            })?;
            for record in &request.job_records {
                sink.enqueue_in(&mut transaction, record.clone()).await?;
            }
        }
        transaction.commit().await.map_err(infrastructure)
    }

    async fn list_ticket_messages(
        &self,
        filter: crate::MessageListFilter,
    ) -> Result<Vec<crate::TicketMessage>, TicketStoreError> {
        if !(1..=MAX_TICKET_LIST_FETCH_LIMIT).contains(&filter.limit) {
            return Err(TicketStoreError::InvalidListLimit);
        }
        if filter.before_created_at.is_some() != filter.before_id.is_some() {
            return Err(TicketStoreError::InvalidListCursor);
        }
        let before_created_at = filter.before_created_at.map(|value| value.to_rfc3339());
        let before_id = filter.before_id.map(|value| value.to_string());
        let rows = sqlx::query(
            "SELECT message_json FROM ticketing_messages
              WHERE project_id = ? AND ticket_id = ?
                AND (? OR json_extract(message_json, '$.kind') <> 'internal_note')
                AND (? IS NULL OR created_at < ? OR (created_at = ? AND id < ?))
              ORDER BY created_at DESC, id DESC
              LIMIT ?",
        )
        .bind(&filter.project_id)
        .bind(filter.ticket_id.to_string())
        .bind(filter.include_internal)
        .bind(before_created_at.as_deref())
        .bind(before_created_at.as_deref())
        .bind(before_created_at.as_deref())
        .bind(before_id.as_deref())
        .bind(i64::try_from(filter.limit).map_err(infrastructure)?)
        .fetch_all(&self.pool)
        .await
        .map_err(infrastructure)?;
        rows.into_iter()
            .map(|row| {
                serde_json::from_str(&row.get::<String, _>("message_json")).map_err(encoding)
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
                ticket_type: request.input.ticket_type,
                form_answers: request.input.form_answers.clone(),
                resource_references: handoff.context.resource_references.clone(),
            },
            format!("TKT-{}", &ticket_uuid.simple().to_string()[..12]),
            request.now,
        )?;
        let ticket = ticket.with_deadlines(
            request.input.first_response_deadline,
            request.input.resolution_deadline,
        );
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

    async fn consume_handoff_identity(
        &self,
        request: ConsumeSessionRequest,
    ) -> Result<(ConsumedSessionIdentity, bool), TicketStoreError> {
        let digest = request.token.digest();
        let mut transaction = self.pool.begin().await.map_err(infrastructure)?;
        let claimed = sqlx::query(
            "UPDATE ticketing_handoffs SET completed_identity_fingerprint = ? WHERE digest = ? AND consumed_identity_json IS NULL AND completed_identity_fingerprint IS NULL",
        )
        .bind(&request.request_fingerprint)
        .bind(digest.as_str())
        .execute(&mut *transaction)
        .await
        .map_err(infrastructure)?;
        let row = sqlx::query(
            "SELECT handoff_json, consumed_identity_json, completed_identity_fingerprint FROM ticketing_handoffs WHERE digest = ?",
        )
        .bind(digest.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(infrastructure)?
        .ok_or(TicketStoreError::UnknownHandoff)?;
        let handoff: SupportHandoff =
            serde_json::from_str(&row.get::<String, _>("handoff_json")).map_err(encoding)?;
        if !handoff.digest.matches_token(&request.token) {
            return Err(TicketStoreError::UnknownHandoff);
        }
        if handoff.project_id != request.project_id {
            return Err(TicketStoreError::WrongHandoffProject);
        }
        if handoff.portal_origin != request.portal_origin {
            return Err(TicketStoreError::WrongHandoffPortal);
        }
        let identity = ConsumedSessionIdentity {
            requester_subject: handoff.requester_subject.clone(),
            requester_permissions: handoff.requester_permissions.clone(),
            correlation_id: handoff.correlation_id,
        };
        let consumed: Option<String> = row.get("consumed_identity_json");
        let completed_fingerprint: Option<String> = row.get("completed_identity_fingerprint");
        if let Some(consumed) = consumed {
            if completed_fingerprint.as_deref() != Some(&request.request_fingerprint) {
                return Err(TicketStoreError::HandoffAlreadyConsumed);
            }
            let identity: ConsumedSessionIdentity =
                serde_json::from_str(&consumed).map_err(encoding)?;
            return Ok((identity, true));
        }
        if claimed.rows_affected() == 0 {
            return Err(TicketStoreError::Infrastructure(
                "session handoff claim completed without an authoritative identity".into(),
            ));
        }
        if handoff.expires_at <= request.now {
            return Err(TicketStoreError::ExpiredHandoff);
        }
        let completed = sqlx::query(
            "UPDATE ticketing_handoffs SET consumed_identity_json = ? WHERE digest = ? AND completed_identity_fingerprint = ? AND consumed_identity_json IS NULL",
        )
        .bind(serde_json::to_string(&identity).map_err(encoding)?)
        .bind(digest.as_str())
        .bind(&request.request_fingerprint)
        .execute(&mut *transaction)
        .await
        .map_err(infrastructure)?;
        if completed.rows_affected() != 1 {
            return Err(TicketStoreError::Infrastructure(
                "session handoff completion lost its atomic claim".into(),
            ));
        }
        transaction.commit().await.map_err(infrastructure)?;
        Ok((identity, false))
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

    async fn pending_activity_intents(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<TicketActivityIntent>, TicketStoreError> {
        let rows = sqlx::query(
            "SELECT id, project_id, ticket_id, kind, correlation_id, payload_json, created_at
               FROM ticketing_activity_intents
              WHERE project_id = ? AND published_at IS NULL
              ORDER BY created_at, id
              LIMIT ?",
        )
        .bind(project_id)
        .bind(i64::try_from(limit).map_err(infrastructure)?)
        .fetch_all(&self.pool)
        .await
        .map_err(infrastructure)?;
        rows.into_iter()
            .map(|row| {
                Ok(TicketActivityIntent {
                    id: Uuid::parse_str(&row.get::<String, _>("id")).map_err(|_| {
                        TicketStoreError::Infrastructure("stored intent id is not a UUID".into())
                    })?,
                    project_id: row.get("project_id"),
                    ticket_id: TicketId(
                        Uuid::parse_str(&row.get::<String, _>("ticket_id")).map_err(|_| {
                            TicketStoreError::Infrastructure(
                                "stored intent ticket id is not a UUID".into(),
                            )
                        })?,
                    ),
                    kind: row.get("kind"),
                    correlation_id: Uuid::parse_str(&row.get::<String, _>("correlation_id"))
                        .map_err(|_| {
                            TicketStoreError::Infrastructure(
                                "stored correlation id is not a UUID".into(),
                            )
                        })?,
                    payload: serde_json::from_str(&row.get::<String, _>("payload_json"))
                        .map_err(encoding)?,
                    created_at: parse_timestamp(&row.get::<String, _>("created_at"))?,
                })
            })
            .collect()
    }

    async fn mark_activity_published(
        &self,
        intent_id: Uuid,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, TicketStoreError> {
        let result = sqlx::query(
            "UPDATE ticketing_activity_intents SET published_at = ?
              WHERE id = ? AND published_at IS NULL",
        )
        .bind(at.to_rfc3339())
        .bind(intent_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(infrastructure)?;
        Ok(result.rows_affected() == 1)
    }

    async fn find_ticket_by_message_identity(
        &self,
        project_id: &str,
        provider: &str,
        internet_message_id: &str,
    ) -> Result<Option<(TicketId, u64)>, TicketStoreError> {
        let row = sqlx::query(
            "SELECT m.ticket_id, t.revision
               FROM ticketing_external_messages m
               JOIN ticketing_tickets t
                 ON t.project_id = m.project_id AND t.id = m.ticket_id
              WHERE m.project_id = ? AND m.provider = ?
                AND json_extract(m.identity_json, '$.internet_message_id') = ?
              LIMIT 1",
        )
        .bind(project_id)
        .bind(provider)
        .bind(internet_message_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(infrastructure)?;
        row.map(|row| {
            Ok((
                TicketId(
                    Uuid::parse_str(&row.get::<String, _>("ticket_id")).map_err(|_| {
                        TicketStoreError::Infrastructure(
                            "stored external ticket id is not a UUID".into(),
                        )
                    })?,
                ),
                u64::try_from(row.get::<i64, _>("revision"))
                    .map_err(|_| TicketStoreError::Infrastructure("revision overflow".into()))?,
            ))
        })
        .transpose()
    }

    async fn append_outbound_evidence(
        &self,
        evidence: OutboundDeliveryEvidence,
    ) -> Result<(), TicketStoreError> {
        let kind = match evidence.kind {
            OutboundEvidenceKind::Accepted => "accepted",
            OutboundEvidenceKind::Ambiguous => "ambiguous",
            OutboundEvidenceKind::PermanentFailure => "permanent_failure",
            OutboundEvidenceKind::Feedback => "feedback",
        };
        let feedback = evidence.feedback.map(|feedback| match feedback {
            DeliveryFeedbackKind::Bounce => "bounce",
            DeliveryFeedbackKind::Complaint => "complaint",
            DeliveryFeedbackKind::Delay => "delay",
        });
        sqlx::query(
            "INSERT INTO ticketing_delivery_evidence
                 (project_id, ticket_id, message_id, kind, provider, provider_message_id,
                  feedback, failure_kind, recorded_at, evidence_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (project_id, ticket_id, message_id, recorded_at, kind, provider_message_id)
                 DO NOTHING",
        )
        .bind(&evidence.project_id)
        .bind(evidence.ticket_id.to_string())
        .bind(evidence.message_id.to_string())
        .bind(kind)
        .bind(&evidence.provider)
        .bind(&evidence.provider_message_id)
        .bind(feedback)
        .bind(&evidence.failure_kind)
        .bind(evidence.recorded_at.to_rfc3339())
        .bind(serde_json::to_string(&evidence).map_err(encoding)?)
        .execute(&self.pool)
        .await
        .map_err(infrastructure)?;
        Ok(())
    }

    async fn outbound_evidence(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        message_id: TicketMessageId,
    ) -> Result<Vec<OutboundDeliveryEvidence>, TicketStoreError> {
        let rows = sqlx::query(
            "SELECT evidence_json FROM ticketing_delivery_evidence
              WHERE project_id = ? AND ticket_id = ? AND message_id = ?
              ORDER BY recorded_at, kind, provider_message_id",
        )
        .bind(project_id)
        .bind(ticket_id.to_string())
        .bind(message_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(infrastructure)?;
        rows.into_iter()
            .map(|row| {
                serde_json::from_str(&row.get::<String, _>("evidence_json")).map_err(encoding)
            })
            .collect()
    }

    async fn record_ticket_view(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        subject: &str,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), TicketStoreError> {
        sqlx::query(
            "INSERT INTO ticketing_ticket_views (project_id, ticket_id, subject, viewed_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (project_id, ticket_id, subject) DO UPDATE SET viewed_at = excluded.viewed_at",
        )
        .bind(project_id)
        .bind(ticket_id.to_string())
        .bind(subject)
        .bind(at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|_| TicketStoreError::NotFound(ticket_id))?;
        Ok(())
    }

    async fn recent_ticket_viewers(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        excluding: &str,
        within: chrono::TimeDelta,
        now: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<Vec<String>, TicketStoreError> {
        let cutoff = (now - within).to_rfc3339();
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT subject FROM ticketing_ticket_views
              WHERE project_id = ? AND ticket_id = ? AND subject != ? AND viewed_at >= ?
              ORDER BY viewed_at DESC LIMIT ?",
        )
        .bind(project_id)
        .bind(ticket_id.to_string())
        .bind(excluding)
        .bind(cutoff)
        .bind(i64::try_from(limit).map_err(infrastructure)?)
        .fetch_all(&self.pool)
        .await
        .map_err(infrastructure)?;
        Ok(rows.into_iter().map(|row| row.0).collect())
    }

    async fn list_macros(&self, project_id: &str) -> Result<Vec<AgentMacro>, TicketStoreError> {
        let rows: Vec<(String, String, String, String, i64)> = sqlx::query_as(
            "SELECT id, title, body, updated_at, revision FROM ticketing_macros
              WHERE project_id = ? ORDER BY title, id",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(infrastructure)?;
        rows.into_iter()
            .map(|(id, title, body, updated_at, revision)| {
                Ok(AgentMacro {
                    id: Uuid::parse_str(&id).map_err(|_| {
                        TicketStoreError::Infrastructure("stored macro id is not a UUID".into())
                    })?,
                    title,
                    body,
                    updated_at: parse_timestamp(&updated_at)?,
                    revision: u64::try_from(revision).map_err(|_| {
                        TicketStoreError::Infrastructure("stored macro revision is negative".into())
                    })?,
                })
            })
            .collect()
    }

    async fn insert_macro(
        &self,
        project_id: &str,
        macro_: AgentMacro,
    ) -> Result<(), TicketStoreError> {
        sqlx::query(
            "INSERT INTO ticketing_macros (project_id, id, title, body, updated_at, revision)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(project_id)
        .bind(macro_.id.to_string())
        .bind(&macro_.title)
        .bind(&macro_.body)
        .bind(macro_.updated_at.to_rfc3339())
        .bind(i64::try_from(macro_.revision).map_err(infrastructure)?)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            if is_unique(&error) {
                TicketStoreError::DuplicateMacroTitle
            } else {
                infrastructure(error)
            }
        })?;
        Ok(())
    }

    async fn update_macro(
        &self,
        project_id: &str,
        id: Uuid,
        expected_revision: u64,
        title: &str,
        body: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<AgentMacro, TicketStoreError> {
        let result = sqlx::query(
            "UPDATE ticketing_macros
                SET title = ?, body = ?, updated_at = ?, revision = revision + 1
              WHERE project_id = ? AND id = ? AND revision = ?",
        )
        .bind(title)
        .bind(body)
        .bind(now.to_rfc3339())
        .bind(project_id)
        .bind(id.to_string())
        .bind(i64::try_from(expected_revision).map_err(infrastructure)?)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            if is_unique(&error) {
                TicketStoreError::DuplicateMacroTitle
            } else {
                infrastructure(error)
            }
        })?;
        if result.rows_affected() == 0 {
            let existing: Option<(i64,)> = sqlx::query_as(
                "SELECT revision FROM ticketing_macros WHERE project_id = ? AND id = ?",
            )
            .bind(project_id)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(infrastructure)?;
            return match existing {
                None => Err(TicketStoreError::MacroNotFound(id)),
                Some((revision,)) => Err(TicketStoreError::StaleRevision {
                    expected: expected_revision,
                    actual: u64::try_from(revision).unwrap_or(0),
                }),
            };
        }
        let row: (String, String, String, String, i64) = sqlx::query_as(
            "SELECT id, title, body, updated_at, revision FROM ticketing_macros
              WHERE project_id = ? AND id = ?",
        )
        .bind(project_id)
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(infrastructure)?;
        Ok(AgentMacro {
            id,
            title: row.1,
            body: row.2,
            updated_at: parse_timestamp(&row.3)?,
            revision: u64::try_from(row.4).map_err(|_| {
                TicketStoreError::Infrastructure("stored macro revision is negative".into())
            })?,
        })
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn advance_assignment_cursor(
        &self,
        project_id: &str,
        pool_len: usize,
    ) -> Result<usize, TicketStoreError> {
        if pool_len == 0 {
            return Err(TicketStoreError::Infrastructure(
                "assignment pool is empty".into(),
            ));
        }
        let pool_len = i64::try_from(pool_len)
            .map_err(|_| TicketStoreError::Infrastructure("overflow".into()))?;
        let mut transaction = self.pool.begin().await.map_err(infrastructure)?;
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT next_index FROM ticketing_assignment_cursor WHERE project_id = ?",
        )
        .bind(project_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(infrastructure)?;
        let next = row.map_or(0, |(value,)| value);
        let index = next % pool_len;
        sqlx::query(
            "INSERT INTO ticketing_assignment_cursor (project_id, next_index)
             VALUES (?, ( ? + 1 ) % ?)
             ON CONFLICT (project_id) DO UPDATE SET next_index = ( ? + 1 ) % ?",
        )
        .bind(project_id)
        .bind(next)
        .bind(pool_len)
        .bind(next)
        .bind(pool_len)
        .execute(&mut *transaction)
        .await
        .map_err(infrastructure)?;
        transaction.commit().await.map_err(infrastructure)?;
        Ok(usize::try_from(index)
            .map_err(|_| TicketStoreError::Infrastructure("cursor index is negative".into()))?)
    }

    async fn assignee_workload(
        &self,
        project_id: &str,
        subjects: &[String],
    ) -> Result<BTreeMap<String, u64>, TicketStoreError> {
        let mut workload = subjects
            .iter()
            .map(|subject| (subject.clone(), 0u64))
            .collect::<BTreeMap<_, _>>();
        if subjects.is_empty() {
            return Ok(workload);
        }
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT assignee_subject, COUNT(*) FROM ticketing_tickets
              WHERE project_id = ?
                AND assignee_subject IS NOT NULL
                AND status NOT IN ('resolved', 'closed')
              GROUP BY assignee_subject",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(infrastructure)?;
        for (subject, count) in rows {
            if let Some(entry) = workload.get_mut(&subject) {
                *entry = u64::try_from(count).unwrap_or(0);
            }
        }
        Ok(workload)
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
        "INSERT INTO ticketing_tickets (project_id, id, display_reference, subject, description, channel, priority, ticket_type, form_answers_json, status, queue_id, assignee_subject, requester_subject, requester_display_name, requester_email, created_at, updated_at, revision, first_public_response_at, first_response_deadline, resolution_deadline, waiting_since, resolved_at, closed_at, resolution, close_reason, ticket_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&ticket.project_id)
    .bind(ticket.id.to_string())
    .bind(&ticket.display_reference)
    .bind(&ticket.subject)
    .bind(&ticket.description)
    .bind(enum_json(&ticket.channel)?)
    .bind(enum_json(&ticket.priority)?)
    .bind(enum_json(&ticket.ticket_type)?)
    .bind(serde_json::to_string(&ticket.form_answers).map_err(encoding)?)
    .bind(enum_json(&ticket.status)?)
    .bind(&ticket.queue_id)
    .bind(&ticket.assignee_subject)
    .bind(&ticket.requester.subject)
    .bind(&ticket.requester.display_name)
    .bind(&ticket.requester.email)
    .bind(ticket.created_at.to_rfc3339())
    .bind(ticket.updated_at.to_rfc3339())
    .bind(i64::try_from(ticket.revision).map_err(|_| TicketStoreError::Infrastructure("revision exceeds SQLite integer".into()))?)
    .bind(ticket.first_public_response_at.map(|v| v.to_rfc3339()))
    .bind(ticket.first_response_deadline.map(|v| v.to_rfc3339()))
    .bind(ticket.resolution_deadline.map(|v| v.to_rfc3339()))
    .bind(ticket.waiting_since.map(|v| v.to_rfc3339()))
    .bind(ticket.resolved_at.map(|v| v.to_rfc3339()))
    .bind(ticket.closed_at.map(|v| v.to_rfc3339()))
    .bind(&ticket.resolution)
    .bind(&ticket.close_reason)
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
        "UPDATE ticketing_tickets SET subject = ?, description = ?, channel = ?, priority = ?, ticket_type = ?, form_answers_json = ?, status = ?, queue_id = ?, assignee_subject = ?, requester_subject = ?, requester_display_name = ?, requester_email = ?, created_at = ?, updated_at = ?, revision = ?, first_public_response_at = ?, first_response_deadline = ?, resolution_deadline = ?, waiting_since = ?, resolved_at = ?, closed_at = ?, resolution = ?, close_reason = ?, ticket_json = ? WHERE project_id = ? AND id = ? AND revision = ?",
    )
    .bind(&ticket.subject)
    .bind(&ticket.description)
    .bind(enum_json(&ticket.channel)?)
    .bind(enum_json(&ticket.priority)?)
    .bind(enum_json(&ticket.ticket_type)?)
    .bind(serde_json::to_string(&ticket.form_answers).map_err(encoding)?)
    .bind(enum_json(&ticket.status)?)
    .bind(&ticket.queue_id)
    .bind(&ticket.assignee_subject)
    .bind(&ticket.requester.subject)
    .bind(&ticket.requester.display_name)
    .bind(&ticket.requester.email)
    .bind(ticket.created_at.to_rfc3339())
    .bind(ticket.updated_at.to_rfc3339())
    .bind(i64::try_from(ticket.revision).map_err(|_| TicketStoreError::Infrastructure("revision exceeds SQLite integer".into()))?)
    .bind(ticket.first_public_response_at.map(|v| v.to_rfc3339()))
    .bind(ticket.first_response_deadline.map(|v| v.to_rfc3339()))
    .bind(ticket.resolution_deadline.map(|v| v.to_rfc3339()))
    .bind(ticket.waiting_since.map(|v| v.to_rfc3339()))
    .bind(ticket.resolved_at.map(|v| v.to_rfc3339()))
    .bind(ticket.closed_at.map(|v| v.to_rfc3339()))
    .bind(&ticket.resolution)
    .bind(&ticket.close_reason)
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
    load_ticket_row(transaction, project_id, &id.to_string()).await
}

/// Authoritative columnar read (ADR-0052): the aggregate is reconstructed
/// from projection columns and child tables; `ticket_json` is never read.
async fn load_ticket_row(
    connection: &mut sqlx::SqliteConnection,
    project_id: &str,
    id: &str,
) -> Result<Option<Ticket>, TicketStoreError> {
    let Some(row) = sqlx::query(
        "SELECT project_id, id, display_reference, subject, description, channel, priority, ticket_type, form_answers_json, status, queue_id, assignee_subject, requester_subject, requester_display_name, requester_email, created_at, updated_at, revision, first_public_response_at, first_response_deadline, resolution_deadline, waiting_since, resolved_at, closed_at, resolution, close_reason FROM ticketing_tickets WHERE project_id = ? AND id = ?",
    )
    .bind(project_id)
    .bind(id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(infrastructure)?
    else {
        return Ok(None);
    };
    let messages = sqlx::query(
        "SELECT message_json FROM ticketing_messages WHERE project_id = ? AND ticket_id = ? ORDER BY created_at, id",
    )
    .bind(project_id)
    .bind(id)
    .fetch_all(&mut *connection)
    .await
    .map_err(infrastructure)?
    .into_iter()
    .map(|row| {
        serde_json::from_str(&row.get::<String, _>("message_json")).map_err(encoding)
    })
    .collect::<Result<Vec<crate::TicketMessage>, _>>()?;
    let attachments = sqlx::query(
        "SELECT attachment_json FROM ticketing_attachments WHERE project_id = ? AND ticket_id = ? ORDER BY rowid",
    )
    .bind(project_id)
    .bind(id)
    .fetch_all(&mut *connection)
    .await
    .map_err(infrastructure)?
    .into_iter()
    .map(|row| {
        serde_json::from_str(&row.get::<String, _>("attachment_json")).map_err(encoding)
    })
    .collect::<Result<Vec<crate::TicketAttachment>, _>>()?;
    let followers = sqlx::query(
        "SELECT subject FROM ticketing_followers WHERE project_id = ? AND ticket_id = ?",
    )
    .bind(project_id)
    .bind(id)
    .fetch_all(&mut *connection)
    .await
    .map_err(infrastructure)?
    .into_iter()
    .map(|row| row.get::<String, _>("subject"))
    .collect::<std::collections::BTreeSet<_>>();
    let tags = sqlx::query("SELECT tag FROM ticketing_tags WHERE project_id = ? AND ticket_id = ?")
        .bind(project_id)
        .bind(id)
        .fetch_all(&mut *connection)
        .await
        .map_err(infrastructure)?
        .into_iter()
        .map(|row| row.get::<String, _>("tag"))
        .collect::<std::collections::BTreeSet<_>>();
    let source_references = sqlx::query(
        "SELECT provider, scope, external_id FROM ticketing_source_references WHERE project_id = ? AND ticket_id = ?",
    )
    .bind(project_id)
    .bind(id)
    .fetch_all(&mut *connection)
    .await
    .map_err(infrastructure)?
    .into_iter()
    .map(|row| crate::TicketSourceReference {
        provider: row.get("provider"),
        scope: row.get("scope"),
        external_id: row.get("external_id"),
    })
    .collect::<Vec<_>>();
    let resource_references = sqlx::query(
        "SELECT system, resource_type, resource_id FROM ticketing_resource_references WHERE project_id = ? AND ticket_id = ?",
    )
    .bind(project_id)
    .bind(id)
    .fetch_all(&mut *connection)
    .await
    .map_err(infrastructure)?
    .into_iter()
    .map(|row| minco_interaction::SupportResourceReference {
        system: row.get("system"),
        resource_type: row.get("resource_type"),
        resource_id: row.get("resource_id"),
    })
    .collect::<Vec<_>>();

    let status: TicketStatus = parse_enum("status", row.get::<String, _>("status"))?;
    let channel: crate::TicketChannel = parse_enum("channel", row.get::<String, _>("channel"))?;
    let priority: crate::TicketPriority = parse_enum("priority", row.get::<String, _>("priority"))?;
    let optional_timestamp =
        |column: &str| -> Result<Option<chrono::DateTime<chrono::Utc>>, TicketStoreError> {
            row.get::<Option<String>, _>(column)
                .map(|value| parse_timestamp(&value))
                .transpose()
        };
    Ok(Some(Ticket {
        id: TicketId(Uuid::parse_str(id).map_err(|_| {
            TicketStoreError::Infrastructure("stored ticket id is not a UUID".into())
        })?),
        project_id: row.get("project_id"),
        display_reference: row.get("display_reference"),
        subject: row.get("subject"),
        description: row.get("description"),
        requester: crate::TicketRequester {
            subject: row.get("requester_subject"),
            display_name: row.get("requester_display_name"),
            email: row.get("requester_email"),
        },
        channel,
        priority,
        ticket_type: parse_enum("ticket_type", row.get::<String, _>("ticket_type"))?,
        form_answers: serde_json::from_str(&row.get::<String, _>("form_answers_json")).map_err(
            |_| {
                TicketStoreError::Infrastructure(
                    "stored ticket form answers are not valid JSON".into(),
                )
            },
        )?,
        status,
        clock_state: status.clock_state(),
        queue_id: row.get("queue_id"),
        assignee_subject: row.get("assignee_subject"),
        followers,
        category: None,
        tags,
        source_references,
        resource_references,
        messages,
        attachments,
        created_at: parse_timestamp(&row.get::<String, _>("created_at"))?,
        updated_at: parse_timestamp(&row.get::<String, _>("updated_at"))?,
        first_public_response_at: optional_timestamp("first_public_response_at")?,
        first_response_deadline: optional_timestamp("first_response_deadline")?,
        resolution_deadline: optional_timestamp("resolution_deadline")?,
        waiting_since: optional_timestamp("waiting_since")?,
        resolved_at: optional_timestamp("resolved_at")?,
        closed_at: optional_timestamp("closed_at")?,
        resolution: row.get("resolution"),
        close_reason: row.get("close_reason"),
        revision: u64::try_from(row.get::<i64, _>("revision")).map_err(|_| {
            TicketStoreError::Infrastructure("stored ticket revision is negative".into())
        })?,
    }))
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
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),

                first_response_deadline: None,
                resolution_deadline: None,
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
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
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
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
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
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
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
                    ticket_type: crate::TicketType::default(),
                    form_answers: Vec::new(),
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
                    ticket_type: crate::TicketType::default(),
                    form_answers: Vec::new(),
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

    #[tokio::test]
    async fn sqlite_session_handoff_identity_is_atomic_one_time_and_replayable() {
        let (_directory, sqlite) = store().await;
        let now = Utc::now();
        let (handoff, token) = handoff(now);
        sqlite.insert_handoff(handoff).await.unwrap();

        let request = |fingerprint: &str| ConsumeSessionRequest {
            token: token.clone(),
            project_id: "project-a".into(),
            portal_origin: "https://support.example.test".into(),
            request_fingerprint: fingerprint.into(),
            now,
        };

        let (first, repeated_flag) = sqlite
            .consume_handoff_identity(request("fp-1"))
            .await
            .unwrap();
        assert!(!repeated_flag);
        assert_eq!(first.requester_subject, "user-1");
        let (replayed, replay_flag) = sqlite
            .consume_handoff_identity(request("fp-1"))
            .await
            .unwrap();
        assert!(replay_flag);
        assert_eq!(replayed, first);
        assert!(matches!(
            sqlite.consume_handoff_identity(request("fp-2")).await,
            Err(TicketStoreError::HandoffAlreadyConsumed)
        ));

        // Ticket creation remains independently consumable exactly once.
        let created = sqlite
            .consume_and_create_ticket(consume(token, now))
            .await
            .unwrap();
        assert!(!created.repeated);
    }

    fn seeded_conversation_ticket(now: chrono::DateTime<Utc>) -> Ticket {
        let mut ticket = crate::Ticket::create(
            crate::CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Conversation".into(),
                description: "It broke and the requester needs help.".into(),
                requester: crate::TicketRequester {
                    subject: "user-1".into(),
                    display_name: Some("User One".into()),
                    email: Some("user-1@example.test".into()),
                },
                channel: TicketChannel::Portal,
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "TKT-CONV",
            now,
        )
        .unwrap();
        ticket.queue_id = Some("tier-1".into());
        ticket.tags.insert("alpha".into());
        ticket.tags.insert("beta".into());
        ticket.followers.insert("watcher".into());
        ticket.source_references.push(crate::TicketSourceReference {
            provider: "mail".into(),
            scope: "support@example.test".into(),
            external_id: "ext-1".into(),
        });
        ticket
    }

    async fn append_all(
        store: &SqliteTicketingStore,
        ticket: &mut Ticket,
        bodies: &[(&str, bool)],
        now: chrono::DateTime<Utc>,
    ) {
        for (body, internal) in bodies {
            let message = if *internal {
                ticket.internal_note_message("agent-1", *body, now).unwrap()
            } else {
                ticket
                    .reply_as_agent_message("agent-1", *body, now)
                    .unwrap()
            };
            let intent = TicketActivityIntent::new(
                "project-a",
                ticket.id,
                "appended",
                uuid::Uuid::now_v7(),
                serde_json::json!({}),
                now,
            );
            store
                .append_ticket_message(crate::AppendTicketMessageRequest {
                    project_id: "project-a".into(),
                    ticket_id: ticket.id,
                    message,
                    status: ticket.status,
                    first_public_response_at: ticket.first_public_response_at,
                    waiting_since: ticket.waiting_since,
                    resolved_at: ticket.resolved_at,
                    updated_at: ticket.updated_at,
                    expected_revision: ticket.revision - 1,
                    intent,
                    #[cfg(feature = "jobs")]
                    job_records: Vec::new(),
                })
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn delivery_evidence_round_trips_and_reconciles_through_sqlite() {
        let (_directory, sqlite) = store().await;
        let now = Utc::now();
        let ticket = seeded_conversation_ticket(now);
        let intent = TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            uuid::Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        sqlite.create(ticket.clone(), intent).await.unwrap();
        let message_id = TicketMessageId::new();
        sqlite
            .append_outbound_evidence(OutboundDeliveryEvidence {
                project_id: "project-a".into(),
                ticket_id: ticket.id,
                message_id,
                kind: OutboundEvidenceKind::Ambiguous,
                provider: "scripted".into(),
                provider_message_id: String::new(),
                feedback: None,
                failure_kind: Some("ambiguous".into()),
                recorded_at: now,
            })
            .await
            .unwrap();
        let accepted = OutboundDeliveryEvidence {
            project_id: "project-a".into(),
            ticket_id: ticket.id,
            message_id,
            kind: OutboundEvidenceKind::Accepted,
            provider: "scripted".into(),
            provider_message_id: "provider-1".into(),
            feedback: None,
            failure_kind: None,
            recorded_at: now + TimeDelta::seconds(1),
        };
        sqlite
            .append_outbound_evidence(accepted.clone())
            .await
            .unwrap();
        // The natural key makes a redelivered acceptance append idempotent.
        sqlite.append_outbound_evidence(accepted).await.unwrap();

        let rows = sqlite
            .outbound_evidence("project-a", ticket.id, message_id)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, OutboundEvidenceKind::Ambiguous);
        assert_eq!(rows[0].failure_kind.as_deref(), Some("ambiguous"));
        assert_eq!(rows[1].kind, OutboundEvidenceKind::Accepted);
        assert_eq!(rows[1].provider_message_id, "provider-1");
        // Evidence is scoped to the exact message.
        assert!(
            sqlite
                .outbound_evidence("project-a", ticket.id, TicketMessageId::new())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn ticket_type_and_form_answers_round_trip_columnar() {
        let (_directory, sqlite) = store().await;
        let now = Utc::now();
        let ticket = Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Typed".into(),
                description: "Typed ticket".into(),
                requester: TicketRequester {
                    subject: "user-1".into(),
                    display_name: None,
                    email: None,
                },
                channel: crate::TicketChannel::Portal,
                priority: crate::TicketPriority::Normal,
                ticket_type: crate::TicketType::Problem,
                form_answers: vec![crate::TicketFormAnswer {
                    field_id: "order-id".into(),
                    kind: crate::TicketFormValueKind::Text,
                    text_value: Some("ord-91".into()),
                    number_value: None,
                    boolean_value: None,
                }],
                resource_references: Vec::new(),
            },
            "TKT-TYPED",
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
        sqlite.create(ticket.clone(), intent).await.unwrap();
        let loaded = sqlite.get("project-a", ticket.id).await.unwrap().unwrap();
        assert_eq!(loaded.ticket_type, crate::TicketType::Problem);
        assert_eq!(loaded.form_answers, ticket.form_answers);
        let summaries = sqlite
            .list_summaries(crate::TicketSummaryFilter {
                project_id: "project-a".into(),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(
            summaries
                .iter()
                .all(|summary| summary.ticket_type == crate::TicketType::Problem)
        );
    }

    #[tokio::test]
    async fn columnar_reads_reconstruct_every_field_and_appends_do_not_rewrite_ticket_json() {
        let (_directory, sqlite) = store().await;
        let now = Utc::now();
        let mut ticket = seeded_conversation_ticket(now);
        let intent = TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            uuid::Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        sqlite.create(ticket.clone(), intent).await.unwrap();

        // ticket_json snapshot after create has zero appended messages.
        let snapshot_before: String = sqlx::query(
            "SELECT ticket_json FROM ticketing_tickets WHERE project_id = ? AND id = ?",
        )
        .bind("project-a")
        .bind(ticket.id.to_string())
        .fetch_one(sqlite.pool())
        .await
        .unwrap()
        .get("ticket_json");

        append_all(
            &sqlite,
            &mut ticket,
            &[
                ("Public message 0", false),
                ("private note body", true),
                ("Public message 2", false),
            ],
            now,
        )
        .await;

        // The append path must not rewrite the conversation snapshot.
        let snapshot_after: String = sqlx::query(
            "SELECT ticket_json FROM ticketing_tickets WHERE project_id = ? AND id = ?",
        )
        .bind("project-a")
        .bind(ticket.id.to_string())
        .fetch_one(sqlite.pool())
        .await
        .unwrap()
        .get("ticket_json");
        assert_eq!(snapshot_before, snapshot_after);

        // Columnar read reconstructs every field, including appends.
        let reconstructed = sqlite
            .get("project-a", ticket.id)
            .await
            .unwrap()
            .expect("ticket exists");
        assert_eq!(reconstructed.subject, "Conversation");
        assert_eq!(
            reconstructed.description,
            "It broke and the requester needs help."
        );
        assert_eq!(
            reconstructed.requester.display_name.as_deref(),
            Some("User One")
        );
        assert_eq!(
            reconstructed.requester.email.as_deref(),
            Some("user-1@example.test")
        );
        assert_eq!(reconstructed.channel, TicketChannel::Portal);
        assert_eq!(reconstructed.queue_id.as_deref(), Some("tier-1"));
        assert_eq!(reconstructed.tags, ticket.tags);
        assert_eq!(reconstructed.followers, ticket.followers);
        assert_eq!(reconstructed.source_references, ticket.source_references);
        assert_eq!(reconstructed.messages.len(), 4);
        assert_eq!(reconstructed.revision, ticket.revision);
        assert_eq!(reconstructed.status, ticket.status);
        assert_eq!(
            reconstructed.first_public_response_at,
            ticket.first_public_response_at
        );
    }

    #[tokio::test]
    async fn append_is_revision_checked_and_message_pagination_is_stable() {
        let (_directory, sqlite) = store().await;
        let memory = crate::MemoryTicketingStore::default();
        let now = Utc::now();
        let mut ticket = seeded_conversation_ticket(now);
        let intent = TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            uuid::Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        sqlite.create(ticket.clone(), intent.clone()).await.unwrap();
        memory.create(ticket.clone(), intent).await.unwrap();

        let message = ticket
            .reply_as_agent_message("agent-1", "Latest message", now)
            .unwrap();
        let stale = crate::AppendTicketMessageRequest {
            project_id: "project-a".into(),
            ticket_id: ticket.id,
            message: message.clone(),
            status: ticket.status,
            first_public_response_at: ticket.first_public_response_at,
            waiting_since: ticket.waiting_since,
            resolved_at: ticket.resolved_at,
            updated_at: ticket.updated_at,
            expected_revision: ticket.revision + 5,
            intent: TicketActivityIntent::new(
                "project-a",
                ticket.id,
                "appended",
                uuid::Uuid::now_v7(),
                serde_json::json!({}),
                now,
            ),
            #[cfg(feature = "jobs")]
            job_records: Vec::new(),
        };
        assert!(matches!(
            sqlite.append_ticket_message(stale.clone()).await,
            Err(TicketStoreError::StaleRevision { .. })
        ));
        assert!(matches!(
            memory.append_ticket_message(stale).await,
            Err(TicketStoreError::StaleRevision { .. })
        ));
        // Nothing was inserted by the rejected append.
        assert_eq!(
            sqlite
                .list_ticket_messages(crate::MessageListFilter {
                    project_id: "project-a".into(),
                    ticket_id: ticket.id,
                    include_internal: true,
                    before_created_at: None,
                    before_id: None,
                    limit: 10,
                })
                .await
                .unwrap()
                .len(),
            1
        );

        // Newest-first pagination across two pages, internal notes hidden
        // from the public filter, memory and SQLite identical.
        let public = crate::MessageListFilter {
            project_id: "project-a".into(),
            ticket_id: ticket.id,
            include_internal: false,
            before_created_at: None,
            before_id: None,
            limit: 10,
        };
        let sqlite_messages = sqlite.list_ticket_messages(public.clone()).await.unwrap();
        let memory_messages = memory.list_ticket_messages(public).await.unwrap();
        assert_eq!(sqlite_messages, memory_messages);
        assert_eq!(sqlite_messages.len(), 1);

        let mut conversation = seeded_conversation_ticket(now);
        conversation.id = ticket.id;
        append_all(
            &sqlite,
            &mut conversation,
            &[
                ("one", false),
                ("private", true),
                ("two", false),
                ("three", false),
            ],
            now,
        )
        .await;
        let page_one = sqlite
            .list_ticket_messages(crate::MessageListFilter {
                project_id: "project-a".into(),
                ticket_id: ticket.id,
                include_internal: false,
                before_created_at: None,
                before_id: None,
                limit: 2,
            })
            .await
            .unwrap();
        assert_eq!(page_one.len(), 2);
        let second = sqlite
            .list_ticket_messages(crate::MessageListFilter {
                project_id: "project-a".into(),
                ticket_id: ticket.id,
                include_internal: false,
                before_created_at: Some(page_one[1].created_at),
                before_id: Some(page_one[1].id),
                limit: 10,
            })
            .await
            .unwrap();
        assert!(!second.is_empty());
        let mut seen: Vec<String> = page_one
            .iter()
            .chain(second.iter())
            .map(|message| message.id.to_string())
            .collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), page_one.len() + second.len());

        assert!(matches!(
            sqlite
                .list_ticket_messages(crate::MessageListFilter {
                    project_id: "project-a".into(),
                    ticket_id: ticket.id,
                    include_internal: false,
                    before_created_at: Some(now),
                    before_id: None,
                    limit: 10,
                })
                .await,
            Err(TicketStoreError::InvalidListCursor)
        ));
    }

    #[cfg(all(feature = "jobs", feature = "sqlite"))]
    mod jobs_bridge {
        use super::*;
        use crate::{
            AppendTicketMessageRequest, MAX_JOB_RECORDS_PER_MUTATION, TicketActivityIntent,
            TicketingJobEnqueue,
        };
        use async_trait::async_trait;
        use minco_plugin_jobs::{JobEnvelope, JobRecord, pending_record};
        use std::sync::Arc;

        /// The real Pattern A adapter an application writes at the
        /// composition root: the released `SqliteJobStore` behind the
        /// ticketing-owned port, sharing one pool.
        #[derive(Debug)]
        struct SharedPoolEnqueue(Arc<minco_sqlx_sqlite::jobs::SqliteJobStore>);

        #[async_trait]
        impl TicketingJobEnqueue for SharedPoolEnqueue {
            async fn enqueue_in(
                &self,
                transaction: &mut Transaction<'_, Sqlite>,
                record: JobRecord,
            ) -> Result<(), TicketStoreError> {
                self.0
                    .enqueue_in(transaction, record)
                    .await
                    .map(|_| ())
                    .map_err(|error| TicketStoreError::Infrastructure(error.to_string()))
            }
        }

        fn notification_record(project: &str, ticket_id: TicketId) -> JobRecord {
            pending_record(
                JobEnvelope::for_parts(
                    "ticketing.deliver-public-notification",
                    1,
                    serde_json::json!({
                        "project_id": project,
                        "ticket_id": ticket_id.to_string(),
                        "message_id": uuid::Uuid::new_v4().to_string(),
                    }),
                    "ticketing-mail",
                    uuid::Uuid::now_v7(),
                )
                .unwrap(),
            )
        }

        async fn bridged_store() -> (tempfile::TempDir, SqliteTicketingStore) {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("ticketing.sqlite");
            let options = sqlx::sqlite::SqliteConnectOptions::from_str(path.to_str().unwrap())
                .unwrap()
                .create_if_missing(true)
                .foreign_keys(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .connect_with(options)
                .await
                .unwrap();
            let ticketing = SqliteTicketingStore::new(pool.clone());
            ticketing.migrate().await.unwrap();
            minco_sqlx_sqlite::plugin_adapters::migrate_plugin_storage(&pool)
                .await
                .unwrap();
            let jobs = Arc::new(minco_sqlx_sqlite::jobs::SqliteJobStore::new(pool.clone()));
            let store = ticketing.with_job_enqueue(Arc::new(SharedPoolEnqueue(jobs)));
            (directory, store)
        }

        fn append_request(
            ticket: &mut Ticket,
            body: &str,
            jobs: Vec<JobRecord>,
        ) -> AppendTicketMessageRequest {
            let message = ticket
                .reply_as_agent_message("agent-1", body, Utc::now())
                .unwrap();
            AppendTicketMessageRequest {
                project_id: "project-a".into(),
                ticket_id: ticket.id,
                message,
                status: ticket.status,
                first_public_response_at: ticket.first_public_response_at,
                waiting_since: ticket.waiting_since,
                resolved_at: ticket.resolved_at,
                updated_at: ticket.updated_at,
                expected_revision: ticket.revision - 1,
                intent: TicketActivityIntent::new(
                    "project-a",
                    ticket.id,
                    "appended",
                    uuid::Uuid::now_v7(),
                    serde_json::json!({}),
                    Utc::now(),
                ),
                job_records: jobs,
            }
        }

        #[tokio::test]
        async fn job_records_commit_and_roll_back_with_the_ticket_mutation() {
            let (_directory, store) = bridged_store().await;
            let now = Utc::now();
            let mut ticket = crate::Ticket::create(
                crate::CreateTicketInput {
                    project_id: "project-a".into(),
                    subject: "Jobs".into(),
                    description: "It broke and needs an agent.".into(),
                    requester: crate::TicketRequester {
                        subject: "user-1".into(),
                        display_name: None,
                        email: None,
                    },
                    channel: TicketChannel::Api,
                    ticket_type: crate::TicketType::default(),
                    form_answers: Vec::new(),
                    priority: TicketPriority::Normal,
                    resource_references: Vec::new(),
                },
                "TKT-JOBS",
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
            store.create(ticket.clone(), intent).await.unwrap();

            // Successful append enqueues its job record in the same commit.
            let first_jobs = vec![notification_record("project-a", ticket.id)];
            store
                .append_ticket_message(append_request(&mut ticket, "first reply", first_jobs))
                .await
                .unwrap();
            let jobs: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM minco_jobs WHERE json_extract(envelope, '$.job_name') = 'ticketing.deliver-public-notification'",
            )
            .fetch_one(store.pool())
            .await
            .unwrap();
            assert_eq!(jobs, 1, "the job row committed with the mutation");

            // A stale append rolls the whole transaction back: no second
            // job row, no second message.
            let stale_jobs = vec![notification_record("project-a", ticket.id)];
            let mut stale = append_request(&mut ticket, "stale reply", Vec::new());
            stale.expected_revision += 10;
            stale.job_records = stale_jobs;
            assert!(matches!(
                store.append_ticket_message(stale).await,
                Err(TicketStoreError::StaleRevision { .. })
            ));
            let jobs: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM minco_jobs WHERE json_extract(envelope, '$.job_name') = 'ticketing.deliver-public-notification'",
            )
            .fetch_one(store.pool())
            .await
            .unwrap();
            assert_eq!(jobs, 1, "the rolled-back append left no job row");

            // Over the bound fails closed before anything commits.
            let mut ticket = store
                .get("project-a", ticket.id)
                .await
                .unwrap()
                .expect("ticket survives the rolled-back attempt");
            let over_ticket_id = ticket.id;
            let over: Vec<JobRecord> = (0..=MAX_JOB_RECORDS_PER_MUTATION)
                .map(|_| notification_record("project-a", over_ticket_id))
                .collect();
            assert!(matches!(
                store
                    .append_ticket_message(append_request(&mut ticket, "too many", over))
                    .await,
                Err(TicketStoreError::InvalidJobRecords)
            ));
        }

        #[tokio::test]
        async fn job_records_without_a_sink_fail_closed() {
            let (_directory, pool_store) = store().await;
            // `store()` builds a sink-less sqlite store on a fresh pool.
            let now = Utc::now();
            let mut ticket = crate::Ticket::create(
                crate::CreateTicketInput {
                    project_id: "project-a".into(),
                    subject: "No sink".into(),
                    description: "It broke and needs an agent.".into(),
                    requester: crate::TicketRequester {
                        subject: "user-1".into(),
                        display_name: None,
                        email: None,
                    },
                    channel: TicketChannel::Api,
                    ticket_type: crate::TicketType::default(),
                    form_answers: Vec::new(),
                    priority: TicketPriority::Normal,
                    resource_references: Vec::new(),
                },
                "TKT-NOSINK",
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
            pool_store.create(ticket.clone(), intent).await.unwrap();
            let reply_jobs = vec![notification_record("project-a", ticket.id)];
            let request = append_request(&mut ticket, "reply", reply_jobs);
            let error = pool_store.append_ticket_message(request).await;
            assert!(matches!(
                error,
                Err(TicketStoreError::Infrastructure(ref detail))
                    if detail.contains("TicketingJobEnqueue")
            ));
        }
    }

    #[tokio::test]
    async fn activity_intents_dispatch_lifecycle_matches_memory() {
        let (_directory, sqlite) = store().await;
        let now = Utc::now();
        let ticket = Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Events".into(),
                description: "It broke and needs an agent.".into(),
                requester: TicketRequester {
                    subject: "user-1".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Api,
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "TKT-EVENTS",
            now,
        )
        .unwrap();
        let intent = TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "ticketing.created",
            uuid::Uuid::now_v7(),
            serde_json::json!({ "ticket_id": ticket.id.to_string() }),
            now,
        );
        sqlite.create(ticket, intent).await.unwrap();

        let pending = sqlite
            .pending_activity_intents("project-a", 10)
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].kind, "ticketing.created");

        assert!(
            sqlite
                .mark_activity_published(pending[0].id, now)
                .await
                .unwrap()
        );
        // Idempotent mark: a second mark reports false.
        assert!(
            !sqlite
                .mark_activity_published(pending[0].id, now)
                .await
                .unwrap()
        );
        assert!(
            sqlite
                .pending_activity_intents("project-a", 10)
                .await
                .unwrap()
                .is_empty()
        );

        let unpublished_row: Option<String> =
            sqlx::query_scalar("SELECT published_at FROM ticketing_activity_intents WHERE id = ?")
                .bind(pending[0].id.to_string())
                .fetch_one(sqlite.pool())
                .await
                .unwrap();
        assert!(unpublished_row.is_some(), "published_at is now recorded");
    }

    #[tokio::test]
    async fn message_identity_resolution_matches_memory_semantics() {
        let (_directory, sqlite) = store().await;
        let now = Utc::now();
        let ticket = Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Thread".into(),
                description: "It broke and needs an agent.".into(),
                requester: TicketRequester {
                    subject: "user-1".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Email,
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "TKT-THREAD",
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
        sqlite.create(ticket.clone(), intent).await.unwrap();
        let request = IngestExternalMessageRequest {
            identity: crate::ExternalMessageIdentity {
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
            ticket_id: ticket.id,
            body: "Original external reply".into(),
            expected_revision: 0,
            correlation_id: uuid::Uuid::now_v7(),
            now,
        };
        sqlite.ingest_external_message(request).await.unwrap();

        let resolved = sqlite
            .find_ticket_by_message_identity("project-a", "ses", "<original-1@example.test>")
            .await
            .unwrap()
            .expect("threading identity resolves");
        assert_eq!(resolved.0, ticket.id);
        assert_eq!(resolved.1, 1);

        // Unknown identity, foreign provider and foreign project all miss.
        assert!(
            sqlite
                .find_ticket_by_message_identity("project-a", "ses", "<unknown@example.test>")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            sqlite
                .find_ticket_by_message_identity("project-a", "mail", "<original-1@example.test>")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            sqlite
                .find_ticket_by_message_identity("project-b", "ses", "<original-1@example.test>")
                .await
                .unwrap()
                .is_none()
        );
    }
}
