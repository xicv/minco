# minco-plugin-feedback

Official Minco plugin for a short, structured client-feedback loop.

It provides:

- a configurable floating action button with no frontend-framework dependency;
- browser-authorized screen capture plus screenshot and file attachments;
- browser voice recording with optional OpenAI-compatible or local-command transcription;
- client/developer threaded discussion and explicit workflow states;
- memory, PostgreSQL, SQLite, and application-provided persistence adapters;
- provider-neutral object storage, notifications, audit, and event/outbox integration;
- a protected developer inbox API and typed HTTP client;
- deterministic JSON and Markdown context for AI-assisted implementation;
- explicit upload limits, privacy controls, authorization, and optimistic concurrency.

## Embed the widget

```html
<script
  src="/_minco/feedback/widget.js"
  data-endpoint="/_minco/feedback"
  data-position="bottom-right"
  data-environment="review"
  data-release="2026-07-24.abc123"
  defer
></script>
```

The element can also be configured programmatically or with attributes. Supported
positions are `top-left`, `top-right`, `bottom-left`, and `bottom-right`.
The widget is isolated in a Shadow DOM and requires no React, Vue, or other
frontend runtime.

The browser Screen Capture and MediaRecorder APIs always retain their normal user
consent surfaces. Minco does not capture a screen or microphone without the
browser and user granting access. Query strings are excluded from captured page
context by default and configured secret-like parameter names are redacted when
query capture is enabled.

Client access tokens are bearer credentials. The default widget stores them in
`sessionStorage`; applications with a different threat model can configure local
storage or provide their own client. A same-origin script compromise can read
browser storage, so Content Security Policy, dependency hygiene, retention, and
data classification remain part of the application security review.

Anonymous submission is disabled by default. Production deployments should grant
authenticated principals `feedback.create`. `project_key` is browser-visible and
provides only a basic abuse-control boundary. Enabling `allow_anonymous` therefore
also requires ingress rate limits, upload-cost monitoring, and a deliberate data-
retention policy.

Voice transcription can invoke a paid provider or a local process, so it is
available only to authenticated principals with `feedback.create`. A deployment
cannot combine `transcription_enabled` with `project_key` or `allow_anonymous`.
Ingress rate limits and provider spend alerts remain required defense in depth
for authenticated callers.

## Compose the plugin

The plugin declares its dependencies explicitly. They must be registered, but
Minco automatically enables them when Feedback is selected unless an operator
explicitly disables one.

```rust,no_run
use axum::Router;
use minco_core::{PluginId, PluginManager, PluginSelection};
use minco_http::{HttpRuntimeConfig, compose_plugin_http};
use minco_plugin_audit::AuditPlugin;
use minco_plugin_events::EventsPlugin;
use minco_plugin_feedback::{FeedbackConfig, FeedbackPlugin};
use minco_plugin_health::HealthPlugin;
use minco_plugin_identity::IdentityPlugin;
use minco_plugin_notifications::NotificationsPlugin;
use minco_plugin_object_storage::ObjectStoragePlugin;

let mut manager = PluginManager::default();
manager.register(HealthPlugin)?;
manager.register(IdentityPlugin::default())?;
manager.register(ObjectStoragePlugin::memory())?;
manager.register(EventsPlugin::memory().0)?;
manager.register(NotificationsPlugin::memory().0)?;
manager.register(AuditPlugin::memory().0)?;
manager.register(FeedbackPlugin::memory())?;

let feedback = PluginId::new("feedback")?;
let mut selection = PluginSelection::default();
selection.enabled.insert(feedback.clone());
selection.set_configuration(
    feedback,
    &FeedbackConfig {
        project_id: "review-app".into(),
        developer_token: Some("replace-with-secret-provider-value".into()),
        ..FeedbackConfig::default()
    },
)?;

let application = manager.compose(&selection)?;
let router = compose_plugin_http(
    Router::new(),
    &HttpRuntimeConfig::default(),
    &application.graph,
    &application.contributions,
)?;
# let _ = router;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`compose_plugin_http` is important: it merges the statically contributed routes
and raises the global body ceiling to the plugin's configured aggregate upload
limit. Route handlers still enforce attachment count, media class, and per-file
limits. Set `max_attachments` to zero for a text-only profile: the bundled widget
hides screenshot, file, and voice controls, and the server rejects multipart
attachment fields.

Production applications should replace memory implementations with durable
adapters. `developer_token` is an operator fallback for local or narrowly
controlled environments and must come from a secret provider. A normal production
application should instead inject a verified `minco_http::Principal` containing
the `feedback.manage` permission.

## Persistence and delivery semantics

The PostgreSQL and SQLite features persist feedback threads and hashed client
tokens. Object bytes are stored through `ObjectStoreService`; the feedback table
contains metadata and stable object keys rather than large blobs.

Feedback persistence is authoritative. Notification, audit, and event-delivery
failures are returned as warnings after the feedback mutation has committed, so
a temporary downstream outage does not discard client input. Client responses
contain stable warning codes and public-safe summaries; provider diagnostics stay
in access-controlled server logs. The bundled event integration writes to the
configured outbox service, but it cannot make the feedback-store transaction and
an independently configured outbox transaction atomic. Applications requiring
that guarantee must provide a transaction-integrated feedback store/outbox adapter.
Minco does not hide a scheduled polling worker.

## Transcription

Two official adapters are feature-gated:

- `openai-transcription`: multipart audio transcription through a configured API;
- `command-transcription`: a direct-process adapter suitable for `whisper.cpp`,
  `faster-whisper`, or another audited local transcription command.

The command adapter invokes an executable directly, never through a shell. Use
`{input}` in an argument to place the temporary audio path, or omit the placeholder
to append it as the final argument.

The core trait remains provider-neutral, so teams can implement an on-premises,
regional, or privacy-specific transcriber without modifying the plugin.

## Contract and developer workflow

The plugin contract is committed at
[`openapi/feedback.openapi.yaml`](openapi/feedback.openapi.yaml). The developer
CLI supports:

```text
cargo minco feedback inbox
cargo minco feedback show <id>
cargo minco feedback reply <id> --body "Could you clarify the expected result?"
cargo minco feedback status <id> needs_clarification
cargo minco feedback pull <id> --output tasks/feedback/<id>.md
cargo minco feedback attachment <feedback-id> <attachment-id> --output screenshot.png
```

`feedback pull` produces deterministic, repository-friendly context for a coding
agent. Status transitions keep the clarification loop explicit rather than
turning every raw comment immediately into implementation work.
