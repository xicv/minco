---
id: M14-T07
title: Add rich observable low-cost outbound mail
milestone: M14
status: in_progress
priority: high
area: plugins/mail
depends_on: [M14-T02]
operations: []
owned_paths:
  - Cargo.lock
  - compose.mail.yml
  - docs/adrs/0034-outbound-mail-delivery.md
  - docs-site/next/guides/events-and-notifications.md
  - extensions/minco-aws-adapters/**
  - plugins/minco-plugin-notifications/**
  - roadmap/tasks.mmd
  - tasks/M14/M14-T07-rich-observable-mail-delivery.md
  - verification/source-manifest.json
checks:
  - cargo test -p minco-plugin-notifications --all-features --locked
  - cargo clippy -p minco-plugin-notifications --all-targets --all-features --locked -- -D warnings
  - cargo test -p minco-aws-adapters --features ses --locked
  - cargo clippy -p minco-aws-adapters --all-targets --features ses --locked -- -D warnings
  - RUSTDOCFLAGS='-D warnings' cargo doc -p minco-plugin-notifications --all-features --no-deps --locked
  - RUSTDOCFLAGS='-D warnings' cargo doc -p minco-aws-adapters --features ses --no-deps --locked
  - docker compose -f compose.mail.yml config --quiet
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Add a complete outbound transactional-mail foundation that is ergonomic for
ordinary applications, direct-SES and near-zero-idle-cost by default,
privacy-safe to observe, and easy to test either deterministically in memory or
visually through a loopback-only Mailpit inbox.

## Acceptance

- existing generic notification APIs and the legacy SES notification adapter
  remain compatible;
- `mail.send` is advertised only when a rich mail service is explicitly
  installed;
- the mail model supports To/CC/BCC/reply-to, text and HTML alternatives,
  bounded attachments and inline content, safe headers, tags, receipts, and
  deterministic tests;
- BCC is never rendered into MIME headers and address normalization preserves
  local-part semantics;
- ambiguous submission outcomes never retry or fail over automatically;
- the SES v2 transport uses one SDK attempt, bounded timeouts, a fixed sender,
  reserved correlation tags, raw MIME, and privacy-safe error classification;
- SES delivery events are normalized and deduplicable without projecting
  recipient data or raw provider payloads;
- direct SES introduces no queue, worker, schedule, database, NAT gateway,
  provisioned concurrency, dedicated IP, or fixed-capacity service;
- Mailpit is pinned, loopback-bound, resource-bounded, provider-free, and usable
  on macOS Docker-compatible runtimes; and
- Next documentation and package README files describe the exact implementation
  without changing the frozen 1.1.0 manual.

## Non-goals

- inbound email;
- a template, localization, Markdown, or CSS-inlining framework;
- automatic queue/outbox creation;
- exactly-once delivery;
- automatic provider failover after ambiguous outcomes;
- silent open/click tracking;
- dedicated IP or managed deliverability procurement;
- live AWS mutation, deployment, release, tag, or crate publication.

## Safety

No check or composition path contacts AWS without explicit runtime credentials
and an application send operation. Mailpit accepts plaintext SMTP only on a
loopback endpoint. Errors, traces, and normalized provider events exclude
addresses, subject/body content, attachment bytes or names, URLs, IP addresses,
user agents, credentials, and raw provider diagnostics.

## Evidence

Record exact-head package tests, strict Clippy, rustdoc, static validation,
source-manifest verification, Compose validation, and a bounded Mailpit SMTP/API
smoke before changing this task to complete. Provider acceptance must not be
reported as mailbox delivery. Any real SES smoke remains separately authorised
and must name the account, Region, verified identity, configuration set,
mailbox-simulator fixture, spend boundary, and cleanup/evidence path.
