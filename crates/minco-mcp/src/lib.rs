//! Local read-only MCP access to bounded Minco project views.
#![forbid(unsafe_code)]

use minco_project_view::{EvidenceLane, PROJECT_VIEW_SCHEMA_VERSION, ProjectView};
use rmcp::{
    ErrorData, ServiceExt,
    handler::server::wrapper::{Json, Parameters},
    model::Tool,
    schemars, tool, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, ReadBuf};

pub const DEFAULT_MAX_MCP_MESSAGE_BYTES: usize = 256 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum StdioServerError {
    #[error("MCP stdio initialization failed: {0}")]
    Initialize(#[source] Box<rmcp::service::ServerInitializeError>),
    #[error("MCP stdio task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl From<rmcp::service::ServerInitializeError> for StdioServerError {
    fn from(error: rmcp::service::ServerInitializeError) -> Self {
        Self::Initialize(Box::new(error))
    }
}

#[derive(Debug)]
pub struct BoundedMessageReader<R> {
    inner: R,
    max_line_bytes: usize,
    current_line_bytes: usize,
}

impl<R> BoundedMessageReader<R> {
    #[must_use]
    pub const fn new(inner: R, max_line_bytes: usize) -> Self {
        Self {
            inner,
            max_line_bytes,
            current_line_bytes: 0,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for BoundedMessageReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        destination: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if destination.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        let mut storage = [0_u8; 8 * 1024];
        let capacity = destination.remaining().min(storage.len());
        let mut bounded = ReadBuf::new(&mut storage[..capacity]);
        match Pin::new(&mut self.inner).poll_read(context, &mut bounded) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                for byte in bounded.filled() {
                    self.current_line_bytes = self.current_line_bytes.saturating_add(1);
                    if self.current_line_bytes > self.max_line_bytes {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "MCP message exceeds max_message_bytes={}",
                                self.max_line_bytes
                            ),
                        )));
                    }
                    if *byte == b'\n' {
                        self.current_line_bytes = 0;
                    }
                }
                destination.put_slice(bounded.filled());
                Poll::Ready(Ok(()))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct MincoMcp {
    view: Arc<ProjectView>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperationExplainInput {
    /// Exact `OpenAPI` `operationId` declared by the project.
    pub operation_id: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmptyInput {}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskReadinessInput {
    /// Exact roadmap task ID. Omit to return every task readiness record.
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLaneInput {
    Source,
    LocalVerification,
    HostedVerification,
    Deployment,
    Runtime,
    Review,
}

impl From<EvidenceLaneInput> for EvidenceLane {
    fn from(value: EvidenceLaneInput) -> Self {
        match value {
            EvidenceLaneInput::Source => Self::Source,
            EvidenceLaneInput::LocalVerification => Self::LocalVerification,
            EvidenceLaneInput::HostedVerification => Self::HostedVerification,
            EvidenceLaneInput::Deployment => Self::Deployment,
            EvidenceLaneInput::Runtime => Self::Runtime,
            EvidenceLaneInput::Review => Self::Review,
        }
    }
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceInput {
    /// Evidence lane to return. Omit to return all six independent lanes.
    pub lane: Option<EvidenceLaneInput>,
}

#[tool_router(server_handler)]
impl MincoMcp {
    #[must_use]
    pub fn new(view: ProjectView) -> Self {
        Self {
            view: Arc::new(view),
        }
    }

    #[must_use]
    pub fn tool_catalog() -> Vec<Tool> {
        Self::tool_router().list_all()
    }

    fn bounded<T: Serialize>(&self, kind: &'static str, data: T) -> Result<Json<Value>, ErrorData> {
        let value = json!({
            "schema_version": PROJECT_VIEW_SCHEMA_VERSION,
            "kind": kind,
            "data": data,
        });
        let prospective_result = rmcp::model::CallToolResult::structured(value.clone());
        let bytes = serde_json::to_vec(&prospective_result).map_err(|_| {
            ErrorData::internal_error("Minco MCP response serialization failed", None)
        })?;
        if bytes.len().saturating_add(DEFAULT_MAX_MCP_MESSAGE_BYTES)
            > self.view.limits.max_response_bytes
        {
            return Err(ErrorData::internal_error(
                "Minco MCP response exceeds its configured byte limit",
                None,
            ));
        }
        Ok(Json(value))
    }

    #[tool(
        name = "minco.project_view",
        description = "Return the complete bounded repository-native Minco ProjectView.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn project_view(
        &self,
        Parameters(_input): Parameters<EmptyInput>,
    ) -> Result<Json<Value>, ErrorData> {
        self.bounded("project_view", self.view.as_ref())
    }

    #[tool(
        name = "minco.project_summary",
        description = "Return the ProjectView identity, derived summary, limits, input usage, and diagnostics.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn project_summary(
        &self,
        Parameters(_input): Parameters<EmptyInput>,
    ) -> Result<Json<Value>, ErrorData> {
        self.bounded(
            "project_summary",
            json!({
                "project": &self.view.project,
                "summary": &self.view.summary,
                "limits": &self.view.limits,
                "input_usage": &self.view.input_usage,
                "diagnostics": &self.view.diagnostics,
            }),
        )
    }

    #[tool(
        name = "minco.operation_explain",
        description = "Explain one declared OpenAPI operation by exact operationId without invoking it.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn operation_explain(
        &self,
        Parameters(input): Parameters<OperationExplainInput>,
    ) -> Result<Json<Value>, ErrorData> {
        self.bounded(
            "operation_explain",
            json!({
                "operation_id": input.operation_id,
                "found": self.view.operation(&input.operation_id).is_some(),
                "operation": self.view.operation(&input.operation_id),
            }),
        )
    }

    #[tool(
        name = "minco.task_readiness",
        description = "Return derived readiness for one exact task ID or all repository tasks.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn task_readiness(
        &self,
        Parameters(input): Parameters<TaskReadinessInput>,
    ) -> Result<Json<Value>, ErrorData> {
        match input.task_id {
            Some(task_id) => self.bounded(
                "task_readiness",
                json!({
                    "task_id": task_id,
                    "found": self.view.task(&task_id).is_some(),
                    "task": self.view.task(&task_id),
                }),
            ),
            None => self.bounded("task_readiness", &self.view.task_readiness),
        }
    }

    #[tool(
        name = "minco.evidence",
        description = "Return ProjectView evidence while preserving source, local, hosted, deployment, runtime, and review as independent lanes.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn evidence(
        &self,
        Parameters(input): Parameters<EvidenceInput>,
    ) -> Result<Json<Value>, ErrorData> {
        match input.lane {
            Some(lane) => {
                let lane = EvidenceLane::from(lane);
                self.bounded(
                    "evidence",
                    json!({
                        "lane": lane,
                        "items": self.view.evidence.get(&lane).cloned().unwrap_or_default(),
                    }),
                )
            }
            None => self.bounded("evidence", &self.view.evidence),
        }
    }

    #[tool(
        name = "minco.feedback_context",
        description = "Return bounded Feedback capability metadata and operation IDs; never records, attachments, credentials, or provider access.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn feedback_context(
        &self,
        Parameters(_input): Parameters<EmptyInput>,
    ) -> Result<Json<Value>, ErrorData> {
        self.bounded("feedback_context", &self.view.feedback)
    }
}

pub async fn serve_stdio(view: ProjectView) -> Result<(), StdioServerError> {
    let transport = (
        BoundedMessageReader::new(tokio::io::stdin(), DEFAULT_MAX_MCP_MESSAGE_BYTES),
        tokio::io::stdout(),
    );
    let service = MincoMcp::new(view).serve(transport).await?;
    service.waiting().await?;
    Ok(())
}
