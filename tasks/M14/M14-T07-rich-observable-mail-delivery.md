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

The recovery and hardening pass started from exact implementation head
`5f214207f5b440f1cc2f6d3696a37d761e268c6f` on current `main`
`80c0bc71dc21252e853e960745ed984a1a4fe9f5`. The locally qualified product
commit is `8aeb714c6fb128f04053ba8cbb058368e57b48b1`; the following evidence-only
task update is separately requalified before remote handoff.

Current local evidence on macOS 26.5.2 arm64 uses Rust/Cargo 1.97.1, JJ 0.43.0,
GitHub CLI 2.97.0, uv 0.11.32, Node 26.5.1, npm 11.17.0, Docker Desktop 4.84.0,
Docker Engine 29.6.2 arm64, and Compose 5.3.1:

- `cargo minco task verify M14-T07 --json` passes all nine declared checks,
  including 17 notification tests, 21 SES-feature AWS adapter tests, strict
  Clippy, warning-denying rustdoc, Compose validation, static validation, and
  source-manifest verification;
- all-feature facade check/tests, all-feature workspace check/Clippy/tests,
  the 51-test full AWS adapter suite, generated application tests, workspace
  rustdoc, `cargo deny`, `cargo audit`, ShellCheck, diff whitespace validation,
  and Gitleaks pass. Credential-gated AWS, S3, Rustack, and PostgreSQL tests
  remain ignored by their explicit environment contracts;
- generated reference checks and the 1,088-file source manifest are current,
  static validation reports zero errors and warnings across 88 tasks and 180
  Rust files, and 328 documentation snippets pass;
- the pinned Mailpit index digest
  `sha256:0059ef81e492a7192af3816281eed6859eb078bd7bdc58b76757c13e10e53a7d`
  resolves to the arm64 manifest
  `sha256:60d1dbefeabfec01540dade90a3dc39c8e85e4086b94e8dcff85eaa939f20dbd`.
  A loopback-only SMTP/API smoke captured one rich message and verified To, CC,
  BCC envelope delivery, Reply-To, Unicode headers, text/HTML alternatives,
  attachment and inline-content SHA-256 values, and the safe custom header.
  The byte-level SMTP test separately proves BCC is absent from MIME. The
  task-created container, network, inbox, and volume were removed afterward;
- the near-25-MiB attachment test stays within the provider request bound and
  measured a 170,885,672-byte peak memory footprint (176,537,600-byte maximum
  resident set) on this Mac, below the reference Lambda's 512-MiB allocation;
  documentation recommends object-storage links for large files; and
- `./scripts/quality.sh`, `scripts/docs/check-links.sh`, and
  `scripts/docs/build.sh` stop before documentation rendering on the unchanged
  `docs-site` dependency `nanoid <3.3.17` advisory
  `GHSA-2v37-7h3g-55p8`. That lockfile is outside this task's product scope, so
  this task remains `in_progress` and its replacement pull request remains
  draft rather than weakening or hiding the gate.

No live AWS request, SES identity or configuration mutation, deployment, real
email, release, tag, crate publication, queue, worker, schedule, database, NAT
Gateway, provisioned concurrency, or dedicated IP was created by this work.
