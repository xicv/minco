---
title: Practical Recipes
description: Choose a complete Minco workflow by the application outcome you need.
---

# Practical Recipes

Recipes combine several framework features around one outcome. Each states the
strongest evidence it can produce; none turns a local dry run into a live AWS
claim.

## Standard JSON CRUD API

Use OpenAPI resource metadata, idempotent create, bounded cursor pagination,
strong ETags, conditional update/delete, Problem Details, fake-port application
tests, and real adapter tests.

Start with [Orders API end to end](./orders-api) and the
[resource API guide](../guides/resource-api).

## Authenticated Web Application

1. Enable `identity` and `sessions`.
2. Choose the verified ingress claims and session store at composition.
3. Authorize inside each application use case.
4. Add exact CORS/cookie/header policy.
5. Test missing, insufficient, expired, revoked, and valid principals.

Guide: [Identity and sessions](../guides/identity-and-sessions).

## File Upload with Metadata

1. Add an application use case owning media, size, tenant, and retention policy.
2. Inject the `object-storage` port and a metadata persistence port.
3. Store the object and metadata with explicit recovery for partial failure.
4. Serve through a reviewed proxy or signed-access adapter.
5. Test malicious names/types, size limits, authorization, and deletion.

Guide: [Files and static sites](../guides/files-and-static-sites).

## Transactional Notification

1. Write domain state and an outbox event in one database transaction.
2. Add an explicit worker and queue mapping.
3. Map the event to a typed notification intent.
4. Inject the selected email/webhook/in-app adapter.
5. Bound retries, DLQ, concurrency, and database connections in Plan IR.

Guides: [Events and notifications](../guides/events-and-notifications) and
[Queues and workers](../guides/background-work).

## Client Review Environment

1. Enable Feedback and its concrete data/storage/notification adapters.
2. Package an exact release and optional static frontend.
3. Create an application-owned review manifest with owner and expiry.
4. Collect untrusted, bounded feedback and clarify it in the thread.
5. Export only explicitly development-ready context.
6. Plan cleanup against the exact review identity; expiry alone does not apply
   deletion.

Guide: [Client feedback loop](../guides/feedback).

## Local to AWS

```text
dev dry run -> local test -> inspect -> package -> release verify
            -> change-set review -> target migration -> deploy apply
            -> hosted verify -> promote exact artifact -> observe
```

Use [Plan an AWS deployment](../guides/deployment). Account, Region,
environment, change set, migration, and destructive actions fail closed.

## Static Frontend plus API

Build frontend and API once, bind their digests into one release, review private
S3/CloudFront/domain intent, publish the exact assets, then verify API and
representative frontend bytes together.

Guide: [Files and static sites](../guides/files-and-static-sites).

## Choose the Smallest Stack

| Need | Suggested components |
|---|---|
| Local CRUD prototype | defaults + `sqlx-sqlite` + `test` |
| Lambda JSON API | defaults + `sqlx-postgres` + `aws-lambda` + `plan` + `release` |
| Signed-in app | previous + `plugin-identity` + `plugin-sessions` |
| Uploads | previous + `plugin-object-storage` + selected storage adapter |
| Async delivery | `plugin-events` + `plugin-notifications` + `aws-worker` |
| Review loop | `plugin-feedback` plus its concrete adapters |
| Frontend hosting | `plugin-static-site` + AWS deployment inputs |

See [Cargo feature flags](../reference/feature-flags) and the
[built-in component catalog](../plugins/).
