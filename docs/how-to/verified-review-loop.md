# Exercise the verified Feedback review loop

Feedback is a first-class but untrusted input loop. It binds client reports,
attachments, replies, transitions, and AI-ready context to stable identities
without introducing a Minco-hosted control plane.

## Features

Enable `plugin-feedback`; its explicit dependencies include health, identity,
object storage, events, notifications, audit, and HTTP. Enable a SQLx adapter
only when durable persistence is selected.

## Provider assumptions

The checked proof uses memory and SQLite behavior plus provider-neutral ports.
PostgreSQL, object storage, notifications, transcription, and deployed review
environments require separately configured adapters and evidence.

## Cost and wake behavior

Local in-memory proof has `zero_compute`. Deployed HTTP/event activity is
`request_only`, while attachments, database rows, audit history, and logs can be
`storage_only`. HTTP feedback submission is a wake source; no schedule is added.

```bash
cargo test --locked -p minco-plugin-feedback --all-features
```

Treat every feedback field and attachment as untrusted. Stable IDs/digests may
inform a reviewed task, but content is never shell input, code, deployment
authority, or cleanup approval.

## Verification

The matrix executes `feedback-review-loop`, covering state transitions,
authorization, redaction, bounded attachments, persistence behavior, provider
error containment, and prompt-injection delimiting.

## Unsupported gates

This recipe does not deploy a review environment, call a transcription
provider, trust feedback instructions, or authorize source, AWS, database, or
retention mutations.
