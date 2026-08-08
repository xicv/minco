---
title: Client Feedback Loop
description: Turn review-environment feedback into bounded, traceable development context.
---

# Client Feedback Loop

The stable Feedback plugin shortens the path from client review to a
development-ready task. It supports threads, screenshots/files, optional voice
transcription, discussion, status transitions, notifications, audit, and a
deterministic AI handoff.

## Compose the plugin

`plugin-feedback` brings the capabilities it needs: health, identity, object
storage, events, notifications, audit, and HTTP. The application must still
inject concrete persistence, storage, notification, clock, and optional
transcription adapters.

```bash
cargo minco plugin enable feedback --dry-run --json
cargo minco plugin doctor --json
cargo minco plugin test feedback --json
```

## Review workflow

```text
client submits feedback and optional attachments
  -> server persists the thread and opaque client-token hash
  -> developer reviews, asks questions, and changes status
  -> client continues the token-scoped discussion
  -> developer explicitly marks ready_for_development
  -> cargo minco feedback pull emits deterministic Markdown or JSON
  -> implementation and exact release identity return to the thread
```

Browse and inspect from the CLI:

```bash
cargo minco feedback inbox --json
cargo minco feedback show FEEDBACK_ID --json
cargo minco feedback pull FEEDBACK_ID --json
```

The export is context, not an autonomous instruction or deployment trigger.
Unresolved questions and suggested next actions remain visible so a coding
agent can refuse to invent missing requirements.

## Security and privacy

- client bearer tokens are random and only their hashes are stored;
- anonymous submission is off by default and needs separate ingress abuse
  controls if enabled;
- developer actions require `feedback.manage` or an explicitly configured
  operator fallback;
- every mutation uses optimistic revision checking;
- internal developer notes never appear in the client projection;
- attachment types, counts, and sizes are bounded;
- screen and microphone capture retain browser-native consent;
- query strings are excluded from captured context by default;
- downstream failures expose public-safe warning codes, not raw diagnostics.

Screenshots and voice may contain personal or confidential information. The
deployment must state retention, encryption, access, deletion, incident, and
residency policy for every selected database, object store, notification sink,
and transcription provider.

## Verified review environments

An optional review manifest binds the review ID to source revision, release
manifest digest, artifact digests, target, owner, and expiry. Expiry is not
permission to delete. Preview teardown still needs exact identity, a reviewed
plan, environment guards, and explicit apply approval.

Local widget/browser tests do not prove production storage, live notification,
provider transcription, preview deployment, or cleanup.
