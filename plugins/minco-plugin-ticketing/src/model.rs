use chrono::{DateTime, Utc};
use minco_interaction::{
    AttachmentKind, AttachmentMetadata, SupportResourceReference, TransitionRule,
    transition_allowed,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt, str::FromStr};
use uuid::Uuid;

macro_rules! ticket_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                value.parse().map(Self)
            }
        }
    };
}

ticket_id!(TicketId);
ticket_id!(TicketMessageId);
ticket_id!(TicketAttachmentId);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketPriority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketChannel {
    Portal,
    Email,
    Api,
    Voice,
    Internal,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    New,
    Open,
    PendingRequester,
    PendingInternal,
    OnHold,
    Resolved,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketClockState {
    Open,
    Paused,
    Stopped,
}

impl TicketStatus {
    #[must_use]
    pub const fn clock_state(self) -> TicketClockState {
        match self {
            Self::New | Self::Open | Self::PendingInternal => TicketClockState::Open,
            Self::PendingRequester | Self::OnHold => TicketClockState::Paused,
            Self::Resolved | Self::Closed => TicketClockState::Stopped,
        }
    }

    #[must_use]
    pub fn can_transition_to(self, target: Self) -> bool {
        use TicketStatus::{
            Closed, New, OnHold, Open, PendingInternal, PendingRequester, Resolved,
        };
        const RULES: &[TransitionRule<TicketStatus>] = &[
            TransitionRule::new(New, Open),
            TransitionRule::new(New, PendingRequester),
            TransitionRule::new(New, PendingInternal),
            TransitionRule::new(New, OnHold),
            TransitionRule::new(New, Resolved),
            TransitionRule::new(Open, PendingRequester),
            TransitionRule::new(Open, PendingInternal),
            TransitionRule::new(Open, OnHold),
            TransitionRule::new(Open, Resolved),
            TransitionRule::new(PendingRequester, Open),
            TransitionRule::new(PendingRequester, PendingInternal),
            TransitionRule::new(PendingRequester, OnHold),
            TransitionRule::new(PendingRequester, Resolved),
            TransitionRule::new(PendingInternal, Open),
            TransitionRule::new(PendingInternal, PendingRequester),
            TransitionRule::new(PendingInternal, OnHold),
            TransitionRule::new(PendingInternal, Resolved),
            TransitionRule::new(OnHold, Open),
            TransitionRule::new(OnHold, PendingRequester),
            TransitionRule::new(OnHold, PendingInternal),
            TransitionRule::new(OnHold, Resolved),
            TransitionRule::new(Resolved, Open),
            TransitionRule::new(Resolved, Closed),
            TransitionRule::new(Closed, Open),
        ];
        transition_allowed(&self, &target, RULES)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketMessageKind {
    PublicReply,
    InternalNote,
    SystemEvent,
    VoiceTranscript,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketMessageDirection {
    Inbound,
    Outbound,
    Internal,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TicketRequester {
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// The bounded helpdesk ticket taxonomy (ADR-0066). `Question` is the
/// default so existing requesters keep a valid home.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketType {
    #[default]
    Question,
    Incident,
    Problem,
    Task,
}

/// Which slot of a [`TicketFormAnswer`] carries the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketFormValueKind {
    Text,
    Number,
    Boolean,
    DateTime,
}

/// One typed form answer captured at creation (ADR-0066).
///
/// `kind` selects the meaningful slot — exactly one slot is set;
/// `date_time` answers carry an RFC 3339 string in `text_value`. Numbers
/// are bounded integers; floating point is deliberately out of contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TicketFormAnswer {
    pub field_id: String,
    pub kind: TicketFormValueKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_value: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boolean_value: Option<bool>,
}

/// Bound on form answers per ticket; the taxonomy stays small and
/// reviewable, not an arbitrary form registry.
pub const MAX_FORM_ANSWERS: usize = 16;

fn validate_form_answers(answers: &[TicketFormAnswer]) -> Result<(), TicketValidationError> {
    if answers.len() > MAX_FORM_ANSWERS {
        return Err(TicketValidationError::InvalidField {
            field: "form_answers",
            detail: format!("must not contain more than {MAX_FORM_ANSWERS} answers"),
        });
    }
    let mut seen = BTreeSet::new();
    for answer in answers {
        if answer.field_id.is_empty()
            || answer.field_id.len() > 64
            || !answer.field_id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
            })
        {
            return Err(TicketValidationError::InvalidField {
                field: "form_answers.field_id",
                detail: "must be 1..=64 characters of [a-z0-9_-]".into(),
            });
        }
        if !seen.insert(answer.field_id.clone()) {
            return Err(TicketValidationError::InvalidField {
                field: "form_answers.field_id",
                detail: "must be unique".into(),
            });
        }
        let slots = [
            answer.text_value.is_some(),
            answer.number_value.is_some(),
            answer.boolean_value.is_some(),
        ]
        .into_iter()
        .filter(|set| *set)
        .count();
        if slots != 1 {
            return Err(TicketValidationError::InvalidField {
                field: "form_answers.value",
                detail: "exactly one value slot must be set".into(),
            });
        }
        match answer.kind {
            TicketFormValueKind::Text => {
                if let Some(text) = answer.text_value.as_deref()
                    && (text.is_empty() || text.chars().count() > 2_000)
                {
                    return Err(TicketValidationError::InvalidField {
                        field: "form_answers.text_value",
                        detail: "must be 1..=2000 characters".into(),
                    });
                }
            }
            TicketFormValueKind::DateTime => {
                if let Some(text) = answer.text_value.as_deref()
                    && chrono::DateTime::parse_from_rfc3339(text).is_err()
                {
                    return Err(TicketValidationError::InvalidField {
                        field: "form_answers.text_value",
                        detail: "date_time answers must be RFC 3339".into(),
                    });
                }
            }
            TicketFormValueKind::Number => {
                if !(-9_007_199_254_740_991..=9_007_199_254_740_991)
                    .contains(&answer.number_value.unwrap_or(0))
                {
                    return Err(TicketValidationError::InvalidField {
                        field: "form_answers.number_value",
                        detail: "must be within the f64-safe integer range".into(),
                    });
                }
            }
            TicketFormValueKind::Boolean => {}
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TicketSourceReference {
    pub provider: String,
    pub scope: String,
    pub external_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketMessage {
    pub id: TicketMessageId,
    pub kind: TicketMessageKind,
    pub direction: TicketMessageDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_subject: Option<String>,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketAttachment {
    pub id: TicketAttachmentId,
    pub kind: AttachmentKind,
    pub object_key: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
}

impl From<AttachmentMetadata> for TicketAttachment {
    fn from(value: AttachmentMetadata) -> Self {
        Self {
            id: TicketAttachmentId(value.id),
            kind: value.kind,
            object_key: value.object_key.as_str().to_owned(),
            file_name: value.file_name,
            content_type: value.content_type,
            size_bytes: value.size_bytes,
            sha256: value.sha256,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    pub id: TicketId,
    pub project_id: String,
    pub display_reference: String,
    pub subject: String,
    pub description: String,
    pub requester: TicketRequester,
    pub channel: TicketChannel,
    pub priority: TicketPriority,
    pub ticket_type: TicketType,
    #[serde(default)]
    pub form_answers: Vec<TicketFormAnswer>,
    pub status: TicketStatus,
    pub clock_state: TicketClockState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee_subject: Option<String>,
    #[serde(default)]
    pub followers: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: BTreeSet<String>,
    #[serde(default)]
    pub source_references: Vec<TicketSourceReference>,
    #[serde(default)]
    pub resource_references: Vec<SupportResourceReference>,
    #[serde(default)]
    pub messages: Vec<TicketMessage>,
    #[serde(default)]
    pub attachments: Vec<TicketAttachment>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_public_response_at: Option<DateTime<Utc>>,
    /// SLA snapshots (ADR-0068): fixed at creation when an SLA is
    /// configured; never recomputed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_response_deadline: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_deadline: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_since: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_reason: Option<String>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTicketInput {
    pub project_id: String,
    pub subject: String,
    pub description: String,
    pub requester: TicketRequester,
    pub channel: TicketChannel,
    pub priority: TicketPriority,
    #[serde(default)]
    pub ticket_type: TicketType,
    #[serde(default)]
    pub form_answers: Vec<TicketFormAnswer>,
    #[serde(default)]
    pub resource_references: Vec<SupportResourceReference>,
}

/// How an assignment decision picks its agent (ADR-0068): manual
/// carries the subject; the pool modes select from the configured
/// assignment pool deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentMode {
    Manual,
    RoundRobin,
    LeastWorkload,
}

/// The closed set of curated agent views (ADR-0067). Server-defined
/// predicates over ticket summaries — never an ad-hoc query surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentCuratedView {
    NewUnassigned,
    PendingRequester,
    PendingInternal,
    Mine,
    RecentlyResolved,
}

impl AgentCuratedView {
    /// The stable path identifier of the view.
    #[must_use]
    pub const fn as_slug(self) -> &'static str {
        match self {
            Self::NewUnassigned => "new-unassigned",
            Self::PendingRequester => "pending-requester",
            Self::PendingInternal => "pending-internal",
            Self::Mine => "mine",
            Self::RecentlyResolved => "recently-resolved",
        }
    }

    /// Parse a path identifier; unknown views are rejected, not guessed.
    #[must_use]
    pub fn from_slug(value: &str) -> Option<Self> {
        Some(match value {
            "new-unassigned" => Self::NewUnassigned,
            "pending-requester" => Self::PendingRequester,
            "pending-internal" => Self::PendingInternal,
            "mine" => Self::Mine,
            "recently-resolved" => Self::RecentlyResolved,
            _ => return None,
        })
    }
}

/// How long a ticket view indicates the viewer, for advisory collision
/// indication (ADR-0067).
pub const TICKET_VIEW_WINDOW: chrono::TimeDelta = chrono::TimeDelta::minutes(5);
/// At most this many other viewers are ever surfaced.
pub const MAX_OTHER_VIEWERS: usize = 8;

/// One shared saved reply (ADR-0067).
///
/// Plain text an agent can edit before submitting. The library is
/// revision-aware; applying a macro to a draft is a client-side text
/// insertion, never a server submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMacro {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
}

impl AgentMacro {
    /// Validates one macro decision's bounds.
    pub fn validate_decision(title: &str, body: &str) -> Result<(), TicketValidationError> {
        validate_text("macro.title", title, 300)?;
        validate_text("macro.body", body, 20_000)
    }

    /// Validates one macro decision and returns the next revision's
    /// record; nothing is persisted by the domain.
    pub fn new_decision(
        id: Uuid,
        title: &str,
        body: &str,
        now: DateTime<Utc>,
    ) -> Result<Self, TicketValidationError> {
        Self::validate_decision(title, body)?;
        Ok(Self {
            id,
            title: title.to_owned(),
            body: body.to_owned(),
            updated_at: now,
            revision: 0,
        })
    }

    #[must_use]
    #[allow(clippy::assigning_clones)]
    pub fn with_next_revision(mut self, title: &str, body: &str, now: DateTime<Utc>) -> Self {
        self.title = title.to_owned();
        self.body = body.to_owned();
        self.updated_at = now;
        self.revision += 1;
        self
    }
}

impl Ticket {
    pub fn create(
        input: CreateTicketInput,
        display_reference: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, TicketValidationError> {
        validate_text("project_id", &input.project_id, 100)?;
        validate_text("subject", &input.subject, 300)?;
        validate_text("description", &input.description, 20_000)?;
        validate_text("requester.subject", &input.requester.subject, 300)?;
        validate_optional_text(
            "requester.display_name",
            input.requester.display_name.as_deref(),
            300,
        )?;
        validate_optional_text("requester.email", input.requester.email.as_deref(), 320)?;
        let display_reference = display_reference.into();
        validate_text("display_reference", &display_reference, 100)?;
        if input.resource_references.len() > 32 {
            return Err(TicketValidationError::InvalidField {
                field: "resource_references",
                detail: "must not contain more than 32 values".into(),
            });
        }
        validate_form_answers(&input.form_answers)?;
        let id = TicketId::new();
        let mut ticket = Self {
            id,
            project_id: input.project_id,
            display_reference,
            subject: input.subject,
            description: input.description.clone(),
            requester: input.requester,
            channel: input.channel,
            priority: input.priority,
            ticket_type: input.ticket_type,
            form_answers: input.form_answers.clone(),
            status: TicketStatus::New,
            clock_state: TicketClockState::Open,
            queue_id: None,
            assignee_subject: None,
            followers: BTreeSet::new(),
            category: None,
            tags: BTreeSet::new(),
            source_references: Vec::new(),
            resource_references: input.resource_references,
            messages: Vec::new(),
            attachments: Vec::new(),
            created_at: now,
            updated_at: now,
            first_public_response_at: None,
            first_response_deadline: None,
            resolution_deadline: None,
            waiting_since: None,
            resolved_at: None,
            closed_at: None,
            resolution: None,
            close_reason: None,
            revision: 0,
        };
        ticket.messages.push(TicketMessage {
            id: TicketMessageId::new(),
            kind: TicketMessageKind::PublicReply,
            direction: TicketMessageDirection::Inbound,
            author_subject: Some(ticket.requester.subject.clone()),
            body: input.description,
            created_at: now,
        });
        Ok(ticket)
    }

    pub fn reply_as_requester(
        &mut self,
        body: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        self.reply_as_requester_message(body, now).map(|_| ())
    }

    /// Applies the requester-reply domain mutation and returns the appended
    /// message so persistence can commit a single-row append (ADR-0052).
    pub fn reply_as_requester_message(
        &mut self,
        body: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<TicketMessage, TicketValidationError> {
        let body = body.into();
        validate_text("body", &body, 20_000)?;
        if matches!(
            self.status,
            TicketStatus::PendingRequester | TicketStatus::Resolved | TicketStatus::Closed
        ) {
            self.apply_status(TicketStatus::Open, None, None, now)?;
        }
        let message = TicketMessage {
            id: TicketMessageId::new(),
            kind: TicketMessageKind::PublicReply,
            direction: TicketMessageDirection::Inbound,
            author_subject: Some(self.requester.subject.clone()),
            body,
            created_at: now,
        };
        self.messages.push(message.clone());
        self.touch(now);
        Ok(message)
    }

    pub fn reply_as_agent(
        &mut self,
        actor_subject: impl Into<String>,
        body: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        let actor_subject = actor_subject.into();
        self.reply_as_agent_message(&actor_subject, body, now)
            .map(|_| ())
    }

    /// Applies the agent-reply domain mutation and returns the appended
    /// message for a single-row append commit.
    pub fn reply_as_agent_message(
        &mut self,
        actor_subject: &str,
        body: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<TicketMessage, TicketValidationError> {
        let actor_subject = actor_subject.to_owned();
        let body = body.into();
        validate_text("actor_subject", &actor_subject, 300)?;
        validate_text("body", &body, 20_000)?;
        self.first_public_response_at.get_or_insert(now);
        if self.status == TicketStatus::New {
            self.apply_status(TicketStatus::Open, None, None, now)?;
        }
        let message = TicketMessage {
            id: TicketMessageId::new(),
            kind: TicketMessageKind::PublicReply,
            direction: TicketMessageDirection::Outbound,
            author_subject: Some(actor_subject),
            body,
            created_at: now,
        };
        self.messages.push(message.clone());
        self.touch(now);
        Ok(message)
    }

    pub fn add_internal_note(
        &mut self,
        actor_subject: impl Into<String>,
        body: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        let actor_subject = actor_subject.into();
        self.internal_note_message(&actor_subject, body, now)
            .map(|_| ())
    }

    /// Applies the internal-note domain mutation and returns the appended
    /// message for a single-row append commit.
    pub fn internal_note_message(
        &mut self,
        actor_subject: &str,
        body: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<TicketMessage, TicketValidationError> {
        let actor_subject = actor_subject.to_owned();
        let body = body.into();
        validate_text("actor_subject", &actor_subject, 300)?;
        validate_text("body", &body, 20_000)?;
        let message = TicketMessage {
            id: TicketMessageId::new(),
            kind: TicketMessageKind::InternalNote,
            direction: TicketMessageDirection::Internal,
            author_subject: Some(actor_subject),
            body,
            created_at: now,
        };
        self.messages.push(message.clone());
        self.touch(now);
        Ok(message)
    }

    pub fn change_status(
        &mut self,
        target: TicketStatus,
        resolution: Option<String>,
        close_reason: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        self.apply_status(target, resolution, close_reason, now)?;
        self.messages.push(TicketMessage {
            id: TicketMessageId::new(),
            kind: TicketMessageKind::SystemEvent,
            direction: TicketMessageDirection::System,
            author_subject: None,
            body: format!("status changed to {target:?}").to_ascii_lowercase(),
            created_at: now,
        });
        self.touch(now);
        Ok(())
    }

    fn apply_status(
        &mut self,
        target: TicketStatus,
        resolution: Option<String>,
        close_reason: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        if !self.status.can_transition_to(target) {
            return Err(TicketValidationError::InvalidTransition {
                current: self.status,
                target,
            });
        }
        if let Some(value) = resolution.as_deref() {
            validate_text("resolution", value, 20_000)?;
        }
        if let Some(value) = close_reason.as_deref() {
            validate_text("close_reason", value, 500)?;
        }
        if target == TicketStatus::Resolved
            && resolution.as_ref().or(self.resolution.as_ref()).is_none()
        {
            return Err(TicketValidationError::ResolutionRequired);
        }
        if target == TicketStatus::Closed {
            if resolution.as_ref().or(self.resolution.as_ref()).is_none() {
                return Err(TicketValidationError::ResolutionRequired);
            }
            if close_reason.is_none() {
                return Err(TicketValidationError::CloseReasonRequired);
            }
        }
        self.status = target;
        self.clock_state = target.clock_state();
        self.waiting_since = matches!(
            target,
            TicketStatus::PendingRequester | TicketStatus::OnHold
        )
        .then_some(now);
        if target == TicketStatus::Resolved {
            self.resolved_at = Some(now);
            self.resolution = resolution.clone().or_else(|| self.resolution.clone());
        } else if !matches!(target, TicketStatus::Closed) {
            self.resolved_at = None;
            if target == TicketStatus::Open {
                self.closed_at = None;
                self.close_reason = None;
            }
        }
        if target == TicketStatus::Closed {
            self.closed_at = Some(now);
            self.resolution = resolution.or_else(|| self.resolution.clone());
            self.close_reason = close_reason;
        }
        Ok(())
    }

    pub fn assign(
        &mut self,
        subject: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        validate_optional_text("assignee_subject", subject.as_deref(), 300)?;
        self.assignee_subject = subject;
        self.touch(now);
        Ok(())
    }

    pub fn transfer_queue(
        &mut self,
        queue_id: String,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        validate_text("queue_id", &queue_id, 200)?;
        self.queue_id = Some(queue_id);
        self.touch(now);
        Ok(())
    }

    pub const fn change_priority(&mut self, priority: TicketPriority, now: DateTime<Utc>) {
        self.priority = priority;
        self.touch(now);
    }

    pub fn add_attachment(&mut self, attachment: TicketAttachment, now: DateTime<Utc>) {
        self.attachments.push(attachment);
        self.touch(now);
    }

    const fn touch(&mut self, now: DateTime<Utc>) {
        self.updated_at = now;
        self.revision = self.revision.saturating_add(1);
    }

    #[must_use]
    pub fn requester_projection(&self) -> RequesterTicket {
        RequesterTicket {
            id: self.id,
            project_id: self.project_id.clone(),
            display_reference: self.display_reference.clone(),
            subject: self.subject.clone(),
            description: self.description.clone(),
            requester: self.requester.clone(),
            channel: self.channel.clone(),
            priority: self.priority,
            ticket_type: self.ticket_type,
            form_answers: self.form_answers.clone(),
            status: self.status.into(),
            messages: self
                .messages
                .iter()
                .filter(|message| message.kind != TicketMessageKind::InternalNote)
                .map(|message| Self::public_message(message, &self.requester.subject))
                .collect(),
            attachments: self
                .attachments
                .iter()
                .map(RequesterTicketAttachment::from)
                .collect(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            revision: self.revision,
        }
    }

    /// System-event bodies record internal status vocabulary; requesters see
    /// the public label only.
    /// Public projection of one message; the internal actor subject never
    /// crosses the requester boundary.
    #[must_use]
    pub fn public_message(message: &TicketMessage, requester_subject: &str) -> PublicTicketMessage {
        PublicTicketMessage {
            id: message.id,
            author: match &message.author_subject {
                None => PublicMessageAuthor::System,
                Some(subject) if *subject == requester_subject => PublicMessageAuthor::Requester,
                Some(_) => PublicMessageAuthor::Support,
            },
            kind: match message.kind {
                TicketMessageKind::SystemEvent => PublicMessageKind::Status,
                TicketMessageKind::PublicReply | TicketMessageKind::VoiceTranscript => {
                    PublicMessageKind::Reply
                }
                TicketMessageKind::InternalNote => PublicMessageKind::Reply,
            },
            body: Self::public_message_body(message),
            created_at: message.created_at,
        }
    }

    fn public_message_body(message: &TicketMessage) -> String {
        if message.kind != TicketMessageKind::SystemEvent {
            return message.body.clone();
        }
        let internal = message
            .body
            .rsplit(' ')
            .next()
            .map(|word| word.replace('_', ""))
            .and_then(|word| match word.as_str() {
                "new" => Some(TicketStatus::New),
                "open" => Some(TicketStatus::Open),
                "pendingrequester" => Some(TicketStatus::PendingRequester),
                "pendinginternal" => Some(TicketStatus::PendingInternal),
                "onhold" => Some(TicketStatus::OnHold),
                "resolved" => Some(TicketStatus::Resolved),
                "closed" => Some(TicketStatus::Closed),
                _ => None,
            });
        match internal {
            Some(status) => format!("status changed to {}", PublicTicketStatus::from(status)),
            None => "status updated".into(),
        }
    }

    #[must_use]
    pub fn export_ai_context(&self) -> TicketAiContext {
        TicketAiContext {
            schema_version: 1,
            project_id: self.project_id.clone(),
            ticket_id: self.id,
            display_reference: self.display_reference.clone(),
            subject: self.subject.clone(),
            description: self.description.clone(),
            status: self.status,
            priority: self.priority,
            resource_references: self.resource_references.clone(),
            messages: self.messages.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequesterTicketAttachment {
    pub id: TicketAttachmentId,
    pub kind: AttachmentKind,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
}

impl From<&TicketAttachment> for RequesterTicketAttachment {
    fn from(value: &TicketAttachment) -> Self {
        Self {
            id: value.id,
            kind: value.kind,
            file_name: value.file_name.clone(),
            content_type: value.content_type.clone(),
            size_bytes: value.size_bytes,
            created_at: value.created_at,
        }
    }
}

/// Public requester-facing message author. The internal actor subject is
/// never serialized across the requester boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicMessageAuthor {
    Requester,
    Support,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicMessageKind {
    Reply,
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicTicketMessage {
    pub id: TicketMessageId,
    pub author: PublicMessageAuthor,
    pub kind: PublicMessageKind,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// Public requester-facing status vocabulary. Internal workflow statuses
/// never cross the requester boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicTicketStatus {
    Open,
    InProgress,
    WaitingForYou,
    OnHold,
    Resolved,
    Closed,
}

impl fmt::Display for PublicTicketStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::WaitingForYou => "waiting_for_you",
            Self::OnHold => "on_hold",
            Self::Resolved => "resolved",
            Self::Closed => "closed",
        })
    }
}

impl From<TicketStatus> for PublicTicketStatus {
    fn from(value: TicketStatus) -> Self {
        match value {
            TicketStatus::New | TicketStatus::Open => Self::Open,
            TicketStatus::PendingInternal => Self::InProgress,
            TicketStatus::PendingRequester => Self::WaitingForYou,
            TicketStatus::OnHold => Self::OnHold,
            TicketStatus::Resolved => Self::Resolved,
            TicketStatus::Closed => Self::Closed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequesterTicket {
    pub id: TicketId,
    pub project_id: String,
    pub display_reference: String,
    pub subject: String,
    pub description: String,
    pub requester: TicketRequester,
    pub channel: TicketChannel,
    pub priority: TicketPriority,
    pub ticket_type: TicketType,
    #[serde(default)]
    pub form_answers: Vec<TicketFormAnswer>,
    pub status: PublicTicketStatus,
    pub messages: Vec<PublicTicketMessage>,
    pub attachments: Vec<RequesterTicketAttachment>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
}

/// Compact requester-facing ticket summary of the requester's own ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicTicketSummary {
    pub id: TicketId,
    pub display_reference: String,
    pub subject: String,
    pub ticket_type: TicketType,
    pub status: PublicTicketStatus,
    pub message_count: usize,
    pub needs_attention: bool,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
}

impl From<&TicketSummary> for PublicTicketSummary {
    fn from(value: &TicketSummary) -> Self {
        Self {
            id: value.id,
            display_reference: value.display_reference.clone(),
            subject: value.subject.clone(),
            ticket_type: value.ticket_type,
            status: value.status.into(),
            message_count: value.message_count,
            needs_attention: value.needs_attention,
            updated_at: value.updated_at,
            revision: value.revision,
        }
    }
}

/// Compact agent-facing ticket summary. Deliberately excludes descriptions,
/// message bodies, object keys, digests, audit, AI context and provider data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketSummary {
    pub id: TicketId,
    pub project_id: String,
    pub display_reference: String,
    pub subject: String,
    pub requester_subject: String,
    pub ticket_type: TicketType,
    pub status: TicketStatus,
    pub clock_state: TicketClockState,
    pub priority: TicketPriority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee_subject: Option<String>,
    pub message_count: usize,
    pub attachment_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_response_deadline: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_deadline: Option<DateTime<Utc>>,
    pub needs_attention: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
}

impl Ticket {
    /// Snapshots the SLA deadlines from the creation time (ADR-0068);
    /// `0` hours disables that single deadline.
    #[must_use]
    pub fn with_sla_snapshots(mut self, sla: crate::TicketSlaConfig) -> Self {
        let created = self.created_at;
        if sla.first_response_hours > 0 {
            self.first_response_deadline =
                Some(created + chrono::TimeDelta::hours(i64::from(sla.first_response_hours)));
        }
        if sla.resolution_hours > 0 {
            self.resolution_deadline =
                Some(created + chrono::TimeDelta::hours(i64::from(sla.resolution_hours)));
        }
        self
    }

    /// Sets precomputed SLA snapshots (ADR-0068) on handoff-created
    /// tickets; creation-time snapshots come from `with_sla_snapshots`.
    #[must_use]
    pub const fn with_deadlines(
        mut self,
        first_response_deadline: Option<DateTime<Utc>>,
        resolution_deadline: Option<DateTime<Utc>>,
    ) -> Self {
        self.first_response_deadline = first_response_deadline;
        self.resolution_deadline = resolution_deadline;
        self
    }

    #[must_use]
    pub const fn needs_attention(&self) -> bool {
        matches!(
            self.status,
            TicketStatus::New | TicketStatus::PendingInternal
        )
    }

    #[must_use]
    pub fn agent_summary(&self) -> TicketSummary {
        TicketSummary {
            id: self.id,
            project_id: self.project_id.clone(),
            display_reference: self.display_reference.clone(),
            subject: self.subject.clone(),
            requester_subject: self.requester.subject.clone(),
            ticket_type: self.ticket_type,
            status: self.status,
            clock_state: self.clock_state,
            priority: self.priority,
            queue_id: self.queue_id.clone(),
            assignee_subject: self.assignee_subject.clone(),
            message_count: self.messages.len(),
            attachment_count: self.attachments.len(),
            last_activity_at: self.messages.last().map(|message| message.created_at),
            first_response_deadline: self.first_response_deadline,
            resolution_deadline: self.resolution_deadline,
            needs_attention: self.needs_attention(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            revision: self.revision,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketAiContext {
    pub schema_version: u32,
    pub project_id: String,
    pub ticket_id: TicketId,
    pub display_reference: String,
    pub subject: String,
    pub description: String,
    pub status: TicketStatus,
    pub priority: TicketPriority,
    pub resource_references: Vec<SupportResourceReference>,
    pub messages: Vec<TicketMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketFromHandoffInput {
    pub subject: String,
    pub description: String,
    pub channel: TicketChannel,
    pub priority: TicketPriority,
    pub ticket_type: TicketType,
    pub form_answers: Vec<TicketFormAnswer>,
    /// SLA snapshots (ADR-0068) computed by the service from config;
    /// `None` when no SLA is configured.
    pub first_response_deadline: Option<DateTime<Utc>>,
    pub resolution_deadline: Option<DateTime<Utc>>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TicketValidationError {
    #[error("invalid {field}: {detail}")]
    InvalidField { field: &'static str, detail: String },
    #[error("ticket cannot transition from {current:?} to {target:?}")]
    InvalidTransition {
        current: TicketStatus,
        target: TicketStatus,
    },
    #[error("a resolution is required")]
    ResolutionRequired,
    #[error("a close reason is required")]
    CloseReasonRequired,
}

fn validate_text(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), TicketValidationError> {
    if value.trim().is_empty()
        || value.chars().count() > maximum
        || value.chars().any(char::is_control)
    {
        Err(TicketValidationError::InvalidField {
            field,
            detail: format!("must contain 1-{maximum} visible characters"),
        })
    } else {
        Ok(())
    }
}

fn validate_optional_text(
    field: &'static str,
    value: Option<&str>,
    maximum: usize,
) -> Result<(), TicketValidationError> {
    value.map_or(Ok(()), |value| validate_text(field, value, maximum))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    fn typed_input() -> CreateTicketInput {
        CreateTicketInput {
            ticket_type: TicketType::Incident,
            form_answers: vec![
                TicketFormAnswer {
                    field_id: "order-id".into(),
                    kind: TicketFormValueKind::Text,
                    text_value: Some("ord-91".into()),
                    number_value: None,
                    boolean_value: None,
                },
                TicketFormAnswer {
                    field_id: "seen-at".into(),
                    kind: TicketFormValueKind::DateTime,
                    text_value: Some("2026-08-25T10:00:00Z".into()),
                    number_value: None,
                    boolean_value: None,
                },
            ],
            ..ticket_input()
        }
    }

    #[test]
    fn typed_tickets_carry_the_taxonomy_and_answers() {
        let ticket = Ticket::create(typed_input(), "TKT-T", Utc::now()).unwrap();
        assert_eq!(ticket.ticket_type, TicketType::Incident);
        assert_eq!(ticket.form_answers.len(), 2);
        assert_eq!(ticket.agent_summary().ticket_type, TicketType::Incident);
        assert_eq!(ticket.requester_projection().form_answers.len(), 2);
    }

    #[test]
    fn form_answers_fail_closed_on_broken_shapes() {
        for broken in [
            // duplicate field ids
            vec![answer("a"), answer("a")],
            // no slot set
            vec![TicketFormAnswer {
                field_id: "a".into(),
                kind: TicketFormValueKind::Text,
                text_value: None,
                number_value: None,
                boolean_value: None,
            }],
            // two slots set
            vec![TicketFormAnswer {
                field_id: "a".into(),
                kind: TicketFormValueKind::Text,
                text_value: Some("x".into()),
                number_value: Some(1),
                boolean_value: None,
            }],
            // non-RFC3339 date_time
            vec![TicketFormAnswer {
                field_id: "a".into(),
                kind: TicketFormValueKind::DateTime,
                text_value: Some("yesterday".into()),
                number_value: None,
                boolean_value: None,
            }],
            // invalid field id charset
            vec![TicketFormAnswer {
                field_id: "Not Ok!".into(),
                kind: TicketFormValueKind::Boolean,
                text_value: None,
                number_value: None,
                boolean_value: Some(true),
            }],
        ] {
            let input = CreateTicketInput {
                form_answers: broken,
                ..ticket_input()
            };
            assert!(
                Ticket::create(input, "TKT-T", Utc::now()).is_err(),
                "broken form answers must fail closed"
            );
        }
    }

    fn ticket_input() -> CreateTicketInput {
        CreateTicketInput {
            project_id: "project-a".into(),
            subject: "Need help".into(),
            description: "It broke".into(),
            requester: TicketRequester {
                subject: "user-1".into(),
                display_name: None,
                email: None,
            },
            channel: TicketChannel::Portal,
            priority: TicketPriority::Normal,
            ticket_type: TicketType::Question,
            form_answers: Vec::new(),
            resource_references: Vec::new(),
        }
    }

    fn answer(field_id: &str) -> TicketFormAnswer {
        TicketFormAnswer {
            field_id: field_id.into(),
            kind: TicketFormValueKind::Text,
            text_value: Some("v".into()),
            number_value: None,
            boolean_value: None,
        }
    }

    fn ticket() -> Ticket {
        Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Need help".into(),
                description: "It broke".into(),
                requester: TicketRequester {
                    subject: "user-1".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Portal,
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "SUP-1",
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn clock_mapping_keeps_pending_internal_open() {
        assert_eq!(
            TicketStatus::PendingInternal.clock_state(),
            TicketClockState::Open
        );
        assert_eq!(
            TicketStatus::PendingRequester.clock_state(),
            TicketClockState::Paused
        );
        assert_eq!(
            TicketStatus::Closed.clock_state(),
            TicketClockState::Stopped
        );
    }

    #[test]
    fn requester_reply_reopens_and_internal_note_stays_private() {
        let mut ticket = ticket();
        let now = ticket.created_at + TimeDelta::minutes(1);
        ticket
            .change_status(TicketStatus::PendingRequester, None, None, now)
            .unwrap();
        ticket.add_internal_note("agent", "private", now).unwrap();
        assert_eq!(ticket.status, TicketStatus::PendingRequester);
        ticket.reply_as_requester("more detail", now).unwrap();
        assert_eq!(ticket.status, TicketStatus::Open);
        assert!(
            !serde_json::to_string(&ticket.requester_projection())
                .unwrap()
                .contains("private")
        );
    }

    #[test]
    fn first_response_and_resolution_timestamps_are_stable() {
        let mut ticket = ticket();
        let first = ticket.created_at + TimeDelta::minutes(1);
        ticket.reply_as_agent("agent", "hello", first).unwrap();
        ticket
            .reply_as_agent("agent", "again", first + TimeDelta::minutes(1))
            .unwrap();
        assert_eq!(ticket.first_public_response_at, Some(first));
        let resolved = first + TimeDelta::minutes(2);
        ticket
            .change_status(TicketStatus::Resolved, Some("fixed".into()), None, resolved)
            .unwrap();
        assert_eq!(ticket.resolved_at, Some(resolved));
        assert_eq!(
            ticket.change_status(TicketStatus::Closed, None, None, resolved),
            Err(TicketValidationError::CloseReasonRequired)
        );
        ticket
            .change_status(
                TicketStatus::Closed,
                None,
                Some("confirmed".into()),
                resolved,
            )
            .unwrap();
        ticket.reply_as_requester("regressed", resolved).unwrap();
        assert_eq!(ticket.status, TicketStatus::Open);
        assert!(ticket.closed_at.is_none());
    }

    #[test]
    fn requester_projection_never_serializes_internal_identity_or_vocabulary() {
        let mut ticket = ticket();
        let now = ticket.created_at + TimeDelta::minutes(1);
        ticket
            .reply_as_agent("agent-internal-subject", "We are checking.", now)
            .unwrap();
        ticket
            .add_internal_note("agent-internal-subject", "secret internal note", now)
            .unwrap();
        ticket
            .change_status(TicketStatus::PendingInternal, None, None, now)
            .unwrap();

        let projection = ticket.requester_projection();
        assert_eq!(projection.status, PublicTicketStatus::InProgress);
        let encoded = serde_json::to_string(&projection).unwrap();
        assert!(!encoded.contains("agent-internal-subject"));
        assert!(!encoded.contains("author_subject"));
        assert!(!encoded.contains("secret internal note"));
        assert!(!encoded.contains("pending_internal"));
        assert!(encoded.contains("in_progress"));
        assert!(encoded.contains("\"system\""));
        assert!(encoded.contains("status changed to in_progress"));

        let authors = projection
            .messages
            .iter()
            .map(|message| message.author)
            .collect::<Vec<_>>();
        assert_eq!(
            authors,
            vec![
                PublicMessageAuthor::Requester,
                PublicMessageAuthor::Support,
                PublicMessageAuthor::System
            ]
        );
    }

    #[test]
    fn public_status_mapping_is_total_and_deterministic() {
        let mapped = [
            TicketStatus::New,
            TicketStatus::Open,
            TicketStatus::PendingInternal,
            TicketStatus::PendingRequester,
            TicketStatus::OnHold,
            TicketStatus::Resolved,
            TicketStatus::Closed,
        ]
        .map(PublicTicketStatus::from);
        assert_eq!(
            mapped,
            [
                PublicTicketStatus::Open,
                PublicTicketStatus::Open,
                PublicTicketStatus::InProgress,
                PublicTicketStatus::WaitingForYou,
                PublicTicketStatus::OnHold,
                PublicTicketStatus::Resolved,
                PublicTicketStatus::Closed
            ]
        );
    }
}
