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
  -> server-stamped release/deployment identity remains internal
  -> `cargo minco feedback pull` emits deterministic AI context
  -> `cargo minco feedback task` verifies the exact release/deployment
  -> exact plan-digest approval creates a task and immutable receipt
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
- The server rejects page URLs with query, fragment or user information; widget
  redaction is defense in depth rather than the authority boundary.
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

Clarification uses durable message identities. A client-visible developer reply
is explicitly followed by `needs_clarification`; that transition records its
message ID. A client reply records the resolving message ID. Punctuation never
opens or closes clarification state.

## Exact release binding and task conversion

Applications that want feedback-to-delivery traceability construct a
`FeedbackReleaseBinding` from the exact release manifest, successful deployment
receipt, environment, and optional UI build. The server overwrites client
release labels and stores the complete binding as an internal system message. A
missing, malformed, duplicate, or inconsistent marker is an error.

A development-ready thread can be inspected without mutation:

```bash
cargo minco feedback task FEEDBACK_ID \
  --task-id M15-T01 \
  --milestone M15 \
  --release-manifest target/minco/release.json \
  --deployment-receipt target/minco/deployment-receipt.json
```

The output includes an exact plan digest. Apply only the reviewed plan:

```bash
cargo minco feedback task FEEDBACK_ID \
  --task-id M15-T01 \
  --milestone M15 \
  --release-manifest target/minco/release.json \
  --deployment-receipt target/minco/deployment-receipt.json \
  --approve-plan-digest EXACT_DIGEST
```

The command requires `ready_for_development`, no unresolved questions, existing
dependency task IDs, an optional operation ID present in ProjectView, and an
exact matching successful deployment. Task and receipt paths are normalized,
project-contained, fd-relative, parent-replacement safe, transactional,
create-only, and byte-idempotent. Exact release-manifest bytes and direct current
source authority are bound. Feedback remains untrusted, task metadata is
neutral, secret-shaped content is rejected, export size is bounded, and the
generated task remains `planned`.

`cargo minco feedback pull` and attachment downloads use the same containment
and create-only rules. Raw downloads are further confined below
`target/minco/feedback-attachments/`, so untrusted attachment bytes cannot be
created as active source or repository configuration. They never silently
overwrite operator files.
