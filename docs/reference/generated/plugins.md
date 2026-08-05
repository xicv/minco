# Plugin and adapter reference

<!-- @generated; do not edit by hand -->
<!-- generated-reference-schema: 1 -->

Generator: `scripts/docs/generate_reference.py` schema `1`.

Authorities:

- `plugins/catalog.toml`
- `package-root minco-plugin.json distribution manifests`
- `ADR 0027 authority split`

Regenerate with `scripts/docs/generate-reference.sh`; use `--check` to verify byte-for-byte freshness.

This is pre-link distribution metadata. Enabling remains an explicit Cargo feature plus typed constructor registration. Secret values and provider credentials have no field in this reference.

| ID | Crate | Kind | Facade feature | Default | Stability | Description | Runtimes | Databases | Idle cost | Wake sources | Metadata digests |
|---|---|---|---|:---:|---|---|---|---|---|---|---|
| `audit` | `minco-plugin-audit` | `plugin` | `plugin-audit` | no | `beta` | Append-only audit history independent of operational logs. | ["native"] | [] | [] | [] | `de0cbc853d02` / `402df393117b` |
| `aws-adapters` | `minco-aws-adapters` | `adapter` | `aws-adapters` | no | `beta` | Explicit opt-in AWS service adapters; the facade full feature is never enabled by default. | ["native","aws-lambda"] | [] | ["provider_managed","storage_only"] | [] | `13121203d5dc` / `f68bcbd958dd` |
| `aws-lambda` | `minco-aws-lambda` | `runtime` | `aws-lambda` | no | `beta` | Native Lambda HTTP runtime and SSM configuration loading. | ["aws-lambda"] | [] | ["zero_compute"] | ["http_request"] | `bb131ca819f0` / `98eac0ad3db1` |
| `aws-worker` | `minco-aws-worker` | `runtime` | `aws-worker` | no | `beta` | Explicit SQS Lambda partial-batch worker runtime without hidden schedules. | ["aws-lambda-sqs"] | [] | ["storage_only","zero_compute"] | ["queue_message"] | `73ca0663219a` / `b9c40be78d5e` |
| `events` | `minco-plugin-events` | `plugin` | `plugin-events` | no | `beta` | Domain events and explicit transactional-outbox ports without hidden schedules. | ["native"] | [] | [] | [] | `51c7c98aae29` / `a65e53ace3fd` |
| `feedback` | `minco-plugin-feedback` | `plugin` | `plugin-feedback` | no | `stable` | Client feedback widget, screenshots, voice transcription, discussion and AI handoff. | ["native"] | ["postgres","sqlite"] | [] | [] | `8483fd7aef00` / `c5509623955d` |
| `health` | `minco-plugin-health` | `plugin` | `plugin-health` | yes | `stable` | Liveness, readiness and dependency health registry. | ["native"] | [] | [] | [] | `bd91fff30ace` / `8253668f2c1c` |
| `idempotency` | `minco-plugin-idempotency` | `plugin` | `plugin-idempotency` | yes | `stable` | Idempotency keys, request fingerprints and a storage port. | ["native"] | [] | [] | [] | `90584252bb3d` / `93fb29e6982b` |
| `identity` | `minco-plugin-identity` | `plugin` | `plugin-identity` | no | `beta` | Verified claims, provider-neutral identities, scopes and permission mapping. | ["native"] | [] | [] | [] | `aea5926675d0` / `6f777ea39f61` |
| `notifications` | `minco-plugin-notifications` | `plugin` | `plugin-notifications` | no | `beta` | Provider-neutral email, webhook, in-app and developer notifications. | ["native"] | [] | [] | [] | `e4896b3c17c1` / `1ed558f80483` |
| `object-storage` | `minco-plugin-object-storage` | `plugin` | `plugin-object-storage` | no | `beta` | Provider-neutral object storage for uploads, exports and feedback attachments. | ["native"] | [] | [] | [] | `aca36e59c57a` / `5b91329ed28e` |
| `observability` | `minco-plugin-observability` | `plugin` | `plugin-observability` | yes | `stable` | Structured tracing and CloudWatch-compatible logging. | ["native"] | [] | [] | [] | `e4a9f253afe1` / `28cc56320f3e` |
| `sessions` | `minco-plugin-sessions` | `plugin` | `plugin-sessions` | no | `beta` | Provider-neutral session issuance, lookup, expiry, and revocation. | ["native"] | [] | [] | [] | `8816bcebe8dd` / `041bd1fb1b15` |
| `realtime` | `minco-plugin-realtime` | `plugin` | `plugin-realtime` | no | `beta` | Subscriber-only realtime invalidation with backend-owned publication and HTTP resynchronization. | ["native"] | [] | [] | [] | `aff66d7947bb` / `8a78fafaa6b3` |
| `sqlx-postgres` | `minco-sqlx-postgres` | `adapter` | `sqlx-postgres` | no | `beta` | Bounded PostgreSQL pools and explicit migrations. | ["native","aws-lambda"] | ["postgres"] | ["provider_managed"] | [] | `0e05c0b20f69` / `a30225dba7bd` |
| `sqlx-sqlite` | `minco-sqlx-sqlite` | `adapter` | `sqlx-sqlite` | no | `beta` | SQLite pools with explicit durability constraints. | ["native","aws-lambda"] | ["sqlite"] | ["storage_only"] | [] | `c6764ce095b1` / `baca86dbd173` |
| `static-site` | `minco-plugin-static-site` | `plugin` | `plugin-static-site` | no | `beta` | Private static assets, CDN caching, SPA fallback, and optional custom-domain deployment intent. | ["native"] | [] | [] | [] | `532190c9fd62` / `935c982004e9` |

## Catalog fields

| Field | Observed type | Present on every entry |
|---|---|:---:|
| `crate` | `string` | yes |
| `default_enabled` | `boolean` | yes |
| `description` | `string` | yes |
| `feature` | `string` | yes |
| `id` | `string` | yes |
| `kind` | `string` | yes |
| `path` | `string` | yes |
| `stability` | `string` | yes |

## Distribution fields

Unknown fields and unknown schema versions fail validation. Fields may be optional for a component with no behavior in that dimension.

| Field | Observed type | Present on every manifest |
|---|---|:---:|
| `configuration` | `array` | no |
| `conformance` | `object` | yes |
| `core_compatibility` | `string` | yes |
| `data_classes` | `array` | no |
| `databases` | `array` | no |
| `default_enabled` | `boolean` | yes |
| `documentation` | `object` | yes |
| `failure_policy` | `object` | yes |
| `feature` | `string` | yes |
| `health_checks` | `array` | no |
| `id` | `string` | yes |
| `kind` | `string` | yes |
| `migrations` | `array` | no |
| `operations` | `array` | no |
| `plugin_dependencies` | `array` | no |
| `plugin_version` | `string` | yes |
| `provides` | `array` | no |
| `requires` | `array` | no |
| `resources` | `array` | no |
| `retention` | `string` | yes |
| `runtimes` | `array` | yes |
| `schema` | `integer` | yes |
| `seeds` | `array` | no |
| `stability` | `string` | yes |
