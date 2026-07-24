use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use minco_plugin_feedback::{
    DeveloperReplyInput, FeedbackAiContext, FeedbackApiClient, FeedbackId, FeedbackListFilter,
    FeedbackStatus,
};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};
use uuid::Uuid;

#[derive(Debug, Args)]
pub struct FeedbackArgs {
    /// Feedback plugin base URL, ending in `/_minco/feedback`.
    #[arg(long, env = "MINCO_FEEDBACK_URL")]
    pub url: String,
    /// Developer bearer token configured by the Feedback plugin.
    #[arg(long, env = "MINCO_FEEDBACK_DEVELOPER_TOKEN", hide_env_values = true)]
    pub token: String,
    #[command(subcommand)]
    pub command: FeedbackCommand,
}

#[derive(Debug, Subcommand)]
pub enum FeedbackCommand {
    /// List the developer feedback inbox.
    Inbox {
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Show one feedback thread as JSON or AI-ready Markdown.
    Show {
        id: String,
        #[arg(long, value_enum, default_value_t = FeedbackFormat::Markdown)]
        format: FeedbackFormat,
    },
    /// Reply to the client or add an internal developer note.
    Reply {
        id: String,
        #[arg(long, alias = "message")]
        body: String,
        #[arg(long)]
        internal: bool,
        #[arg(long)]
        author: Option<String>,
    },
    /// Move a feedback thread through its explicit workflow.
    Status {
        id: String,
        status: String,
        #[arg(long)]
        resolution: Option<String>,
        #[arg(long)]
        author: Option<String>,
    },
    /// Materialize an AI-ready feedback file in the repository task area.
    Pull {
        id: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Download a screenshot, voice note, or other attachment.
    Attachment {
        id: String,
        attachment_id: String,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum FeedbackFormat {
    Json,
    Markdown,
}

pub async fn execute(root: &Path, args: FeedbackArgs, as_json: bool) -> Result<()> {
    let client = FeedbackApiClient::new(args.url, args.token)?;
    match args.command {
        FeedbackCommand::Inbox { status, limit } => {
            let status = status
                .map(|value| FeedbackStatus::from_str(&value))
                .transpose()?;
            let values = client
                .inbox(FeedbackListFilter {
                    status,
                    project_id: None,
                    limit,
                })
                .await?;
            print(&values, as_json)
        }
        FeedbackCommand::Show { id, format } => {
            let id = parse_feedback_id(&id)?;
            match format {
                FeedbackFormat::Json => print(&client.get(id).await?, as_json),
                FeedbackFormat::Markdown => {
                    let markdown = client.ai_context_markdown(id).await?;
                    if as_json {
                        print(
                            &serde_json::json!({"feedback_id": id, "markdown": markdown}),
                            true,
                        )
                    } else {
                        println!("{markdown}");
                        Ok(())
                    }
                }
            }
        }
        FeedbackCommand::Reply {
            id,
            body,
            internal,
            author,
        } => {
            let result = client
                .reply(
                    parse_feedback_id(&id)?,
                    DeveloperReplyInput {
                        body,
                        visible_to_client: !internal,
                        author_display: author,
                    },
                )
                .await?;
            print(&result, as_json)
        }
        FeedbackCommand::Status {
            id,
            status,
            resolution,
            author,
        } => {
            let result = client
                .transition(
                    parse_feedback_id(&id)?,
                    FeedbackStatus::from_str(&status)?,
                    resolution,
                    author,
                )
                .await?;
            print(&result, as_json)
        }
        FeedbackCommand::Pull { id, output } => {
            let id = parse_feedback_id(&id)?;
            let thread = client.get(id).await?;
            let markdown = client
                .ai_context_markdown(id)
                .await
                .unwrap_or_else(|_| FeedbackAiContext::from_thread(thread).to_markdown());
            let output = root
                .join(output.unwrap_or_else(|| PathBuf::from(format!("tasks/feedback/{id}.md"))));
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output, markdown)?;
            print(
                &serde_json::json!({
                    "feedback_id": id,
                    "output": output,
                    "ready_for_agent": true
                }),
                as_json,
            )
        }
        FeedbackCommand::Attachment {
            id,
            attachment_id,
            output,
        } => {
            let id = parse_feedback_id(&id)?;
            let attachment_id =
                Uuid::parse_str(&attachment_id).context("attachment ID must be a UUID")?;
            let bytes = client.attachment(id, attachment_id).await?;
            let output = root.join(output);
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output, &bytes)?;
            print(
                &serde_json::json!({
                    "feedback_id": id,
                    "attachment_id": attachment_id,
                    "output": output,
                    "size_bytes": bytes.len()
                }),
                as_json,
            )
        }
    }
}

fn parse_feedback_id(value: &str) -> Result<FeedbackId> {
    FeedbackId::from_str(value).with_context(|| format!("invalid feedback ID {value:?}"))
}

fn print<T: Serialize + ?Sized>(value: &T, as_json: bool) -> Result<()> {
    let serialized = serde_json::to_value(value)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&serialized)?);
        return Ok(());
    }
    match serialized {
        serde_json::Value::String(value) => println!("{value}"),
        other => println!("{}", serde_json::to_string_pretty(&other)?),
    }
    Ok(())
}
