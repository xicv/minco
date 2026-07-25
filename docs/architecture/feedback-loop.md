# Fast client feedback loop

The Feedback plugin shortens the loop from review environment to a development-
ready task without treating unclarified comments as specifications.

```text
client opens widget
  -> writes feedback
  -> optionally attaches browser-authorized screen capture/files/voice
  -> server persists thread and opaque client-token hash
  -> developer inbox notification
  -> developer asks questions or changes status
  -> client continues the same token-scoped thread
  -> developer marks ready_for_development
  -> `cargo minco feedback pull` emits deterministic AI context
  -> implementation and release ID return to the thread
```

## Design rules

- Feedback persistence is authoritative; notification/audit/event failures are
  surfaced as warnings and never erase client input.
- Warning responses expose stable codes and public-safe summaries; raw downstream
  diagnostics remain in access-controlled server logs.
- Client tokens are random bearer credentials and only their hashes are stored.
- Anonymous submission is disabled by default. Production uses a principal with
  `feedback.create`; explicit anonymous mode requires ingress abuse controls.
- Developer endpoints require `feedback.manage` or an explicitly configured
  operator-token fallback.
- Every mutation uses optimistic revision checking.
- Internal developer notes never appear in the client projection.
- Attachments are stored through the object-storage plugin and validated by
  media class, count, content type, and configured size.
- The aggregate HTTP body ceiling is explicit and automatically contributes to
  Minco's HTTP composition.
- Query strings are excluded from captured context by default.
- Voice transcription is optional, provider-neutral, and disabled unless both
  configuration and an adapter are present.
- No hidden scheduler is installed. Event dispatch is request-assisted or run by
  an explicit worker selected by the application.
- AI export is deterministic Markdown/JSON, not an autonomous change trigger.

## Privacy and operations

Screenshots and voice can contain personal or confidential information. The
plugin declares those data classes in its descriptor. Deployments must set
retention, encryption, access-control, deletion, incident-response, and data-
residency policy for the chosen database, object store, notification sink, and
transcription provider.

The browser retains native screen/microphone consent. The widget never attempts
DOM or session replay. Applications that add replay tooling must independently
mask sensitive fields and obtain the required consent.

## Development threshold

A thread becomes development-ready only after the developer explicitly moves it
to `ready_for_development`. The AI context includes unresolved questions and
suggested next actions so a coding agent can refuse to invent missing product
requirements.
