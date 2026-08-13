# Configuration and Plan schema reference

<!-- @generated; do not edit by hand -->
<!-- generated-reference-schema: 1 -->

Generator: `scripts/docs/generate_reference.py` schema `1`.

Authorities:

- `cargo minco config schema --json`
- `crates/minco-plan/src/model.rs DeploymentPlan`
- `cargo minco deploy plan --stdout --json reference output`

Regenerate with `scripts/docs/generate-reference.sh`; use `--check` to verify byte-for-byte freshness.

Plan model source SHA-256: `807e00d17f1d8198b4d4f0a6dbc72e3e6a13f0649f7727ed1a40ae5247357830`.

## Composed configuration schema

Schema version: `1`. Secret fields expose names, kinds, and descriptions only; defaults are never rendered for secret fields.

| Path | Kind | Required | Secret | Default | Description |
|---|---|:---:|:---:|---|---|
| `application.name` | `string` | yes | no | minco-framework | Stable application service name |
| `database.url` | `string` | yes | yes | — | Opaque database connection secret reference |
| `plugins.feedback.allow_anonymous` | `boolean` | no | no | no | Explicitly allow unauthenticated feedback when neither identity nor a project key is available |
| `plugins.feedback.auto_transcribe_audio` | `boolean` | no | no | no | Transcribe uploaded voice recordings automatically |
| `plugins.feedback.developer_link_base` | `string` | no | no | — | Optional base URL included in developer notifications |
| `plugins.feedback.developer_recipient` | `string` | no | no | developers | Recipient understood by the configured notification sink |
| `plugins.feedback.developer_token` | `string` | no | yes | — | Fallback bearer token for local/operator access; prefer an identity principal with feedback.manage |
| `plugins.feedback.include_url_query` | `boolean` | no | no | no | Include URL query parameters in captured context after redaction |
| `plugins.feedback.max_attachments` | `integer` | no | no | 3 | Maximum screenshot, audio, and file attachments per submission; zero disables all attachments |
| `plugins.feedback.max_audio_bytes` | `integer` | no | no | 5242880 | Maximum voice recording upload size |
| `plugins.feedback.max_file_bytes` | `integer` | no | no | 5242880 | Maximum general attachment upload size |
| `plugins.feedback.max_http_body_bytes` | `integer` | no | no | 7340032 | Maximum complete multipart request size for the default serverless HTTP path |
| `plugins.feedback.max_recording_seconds` | `integer` | no | no | 90 | Maximum browser voice-note recording duration |
| `plugins.feedback.max_screenshot_bytes` | `integer` | no | no | 4194304 | Maximum screenshot upload size |
| `plugins.feedback.notify_client_updates` | `boolean` | no | no | yes | Send in-app notifications for developer replies and status changes |
| `plugins.feedback.offset_x_px` | `integer` | no | no | 24 | Horizontal viewport offset in CSS pixels |
| `plugins.feedback.offset_y_px` | `integer` | no | no | 24 | Vertical viewport offset in CSS pixels |
| `plugins.feedback.poll_interval_ms` | `integer` | no | no | 15000 | Client discussion refresh interval in milliseconds |
| `plugins.feedback.privacy_notice` | `string` | no | no | — | Optional client-visible privacy and retention notice |
| `plugins.feedback.project_id` | `string` | yes | no | — | Stable product or application identifier |
| `plugins.feedback.project_key` | `string` | no | no | — | Optional browser-visible submission key used for basic abuse controls |
| `plugins.feedback.publish_events_inline` | `boolean` | no | no | no | Publish outbox events on the request path instead of leaving them for a worker |
| `plugins.feedback.redact_query_parameters` | `string_list` | no | no | ["access_token","api_key","code","key","password","secret","signature","token"] | Case-insensitive query parameter names replaced with [REDACTED] |
| `plugins.feedback.screenshot_enabled` | `boolean` | no | no | yes | Allow browser screen capture and image attachments |
| `plugins.feedback.theme` | `string` | no | no | auto | Widget theme: light, dark, or auto |
| `plugins.feedback.token_storage` | `string` | no | no | session | Opaque client-token storage: session (default) or local |
| `plugins.feedback.transcription_enabled` | `boolean` | no | no | no | Expose voice transcription for authenticated feedback.create principals when a TranscriptionService is configured |
| `plugins.feedback.voice_enabled` | `boolean` | no | no | no | Allow microphone recording when the browser supports MediaRecorder |
| `plugins.feedback.widget_label` | `string` | no | no | Share feedback | Accessible label shown on the feedback action |
| `plugins.feedback.widget_position` | `string` | no | no | bottom_right | FAB position: top_left, top_right, bottom_left, or bottom_right |
| `plugins.idempotency.claim_timeout_seconds` | `integer` | no | no | 300 | Time after which an abandoned in-progress claim may be recovered |
| `plugins.identity.group_permissions` | `object` | no | no | {} | Map of provider groups to application permissions |
| `plugins.identity.groups_claim` | `string` | no | no | groups | Verified claim containing provider group names |
| `plugins.identity.permission_claim` | `string` | no | no | permissions | Verified claim containing permissions |
| `plugins.identity.scope_claim` | `string` | no | no | scope | Verified claim containing OAuth/OIDC scopes |
| `plugins.identity.separator` | `string` | no | no | " " | Single character separating claim values |
| `plugins.observability.default_filter` | `string` | no | no | info,tower_http=info | Fallback tracing filter when RUST_LOG is unset |
| `plugins.observability.json` | `boolean` | no | no | yes | Emit structured JSON suitable for CloudWatch Logs |
| `plugins.observability.service_name` | `string` | no | no | minco-app | Stable service name included in operational telemetry |
| `plugins.payments-waffo.allow_custom_api_base_url` | `boolean` | no | no | no | Permit an explicitly configured compatible HTTPS endpoint for test credentials only |
| `plugins.payments-waffo.allow_production_writes` | `boolean` | no | no | no | Explicit persisted guard required before production actions can mutate Waffo |
| `plugins.payments-waffo.api_base_url` | `string` | no | no | https://api.waffo.ai | HTTPS Waffo API origin; override only for an explicitly trusted compatible endpoint |
| `plugins.payments-waffo.environment` | `string` | no | no | test | Waffo API-key environment: test or production |
| `plugins.payments-waffo.merchant_id` | `string` | yes | no | — | Waffo merchant short ID |
| `plugins.payments-waffo.private_key` | `string` | yes | yes | — | Opaque env: or ssm: reference to the unencrypted RSA private key |
| `plugins.payments-waffo.request_max_bytes` | `integer` | no | no | 1048576 | Maximum provider request body retained in memory |
| `plugins.payments-waffo.request_timeout_seconds` | `integer` | no | no | 30 | Bounded timeout for one provider request |
| `plugins.payments-waffo.response_max_bytes` | `integer` | no | no | 2097152 | Maximum provider response body retained in memory |
| `plugins.payments-waffo.store_id` | `string` | no | no | — | Store short ID used by webhook automation |
| `plugins.payments-waffo.webhook_events` | `string_list` | no | no | [] | Waffo event types registered by the webhook-add CLI command |
| `plugins.payments-waffo.webhook_future_tolerance_seconds` | `integer` | no | no | 60 | Maximum accepted future clock skew for a signed webhook delivery |
| `plugins.payments-waffo.webhook_max_bytes` | `integer` | no | no | 1048576 | Maximum raw webhook body accepted for verification |
| `plugins.payments-waffo.webhook_past_tolerance_seconds` | `integer` | no | no | 2700 | Maximum accepted age for a signed webhook delivery |
| `plugins.payments-waffo.webhook_public_key` | `string` | no | yes | — | Opaque env: or ssm: reference to Waffo's environment-specific webhook public key |
| `plugins.payments-waffo.webhook_url` | `string` | no | no | — | HTTPS endpoint registered by the webhook-add CLI command |
| `plugins.realtime.max_event_bytes` | `integer` | no | no | 5120 | Maximum encoded envelope size; 5120 bytes keeps one billing unit |
| `plugins.realtime.namespace` | `string` | no | no | minco | Portable namespace prepended to subscriber channels |
| `plugins.realtime.subscriber_claim` | `string` | no | no | sub | OIDC claim that must equal the first channel segment after the namespace |
| `plugins.static-site.custom_domain` | `string` | no | no | — | Optional application hostname |
| `plugins.static-site.html_cache_seconds` | `integer` | no | no | 0 | Cache lifetime for HTML entrypoints |
| `plugins.static-site.immutable_cache_seconds` | `integer` | no | no | 31536000 | Cache lifetime for fingerprinted immutable assets |
| `plugins.static-site.index_document` | `string` | no | no | index.html | Default document served at the site root |
| `plugins.static-site.ipv6_enabled` | `boolean` | no | no | yes | Enable IPv6 on the CDN distribution |
| `plugins.static-site.manage_dns_alias` | `boolean` | no | no | no | Create a DNS alias for custom_domain in the selected deployment renderer |
| `plugins.static-site.price_class` | `string` | no | no | price_class100 | CloudFront price class: price_class100, price_class200, or price_class_all |
| `plugins.static-site.source_directory` | `string` | no | no | dist | Directory containing the built static artifact |
| `plugins.static-site.spa_fallback` | `boolean` | no | no | yes | Rewrite missing browser routes to the index document |
| `runtime.log_level` | `string` | yes | no | info | Default structured logging filter |

## DeploymentPlan top-level schema

Rust types are shown exactly as declared. Serde attributes may omit empty or optional fields from a particular serialized plan.

| Field | Rust type | Present in reference plan |
|---|---|:---:|
| `schema_version` | `u32` | yes |
| `application` | `String` | yes |
| `environment` | `String` | yes |
| `region` | `String` | yes |
| `runtime` | `RuntimePlan` | yes |
| `ingress` | `IngressPlan` | yes |
| `auth` | `AuthPlan` | yes |
| `database` | `DatabaseDeployment` | yes |
| `functions` | `Vec<FunctionPlan>` | yes |
| `queues` | `Vec<QueuePlan>` | no |
| `triggers` | `Vec<TriggerPlan>` | no |
| `iam_intents` | `Vec<IamIntent>` | no |
| `routes` | `Vec<RoutePlan>` | yes |
| `application_graph` | `ApplicationGraph` | yes |
| `static_site` | `Option<StaticSiteDeployment>` | no |
| `realtime` | `Option<RealtimeDeployment>` | no |
| `preview` | `Option<PreviewLifecyclePlan>` | no |
| `local_aws_services` | `Vec<String>` | yes |
| `scheduled_wakeups` | `Vec<String>` | yes |
| `uses_nat_gateway` | `bool` | yes |
| `allowed_origins` | `Vec<String>` | yes |
| `allowed_headers` | `Vec<String>` | yes |
| `exposed_headers` | `Vec<String>` | yes |
| `log_retention_days` | `u32` | yes |
| `cost_policy` | `CostPolicy` | yes |
| `performance_policy` | `PerformancePolicy` | yes |

## Reference serialized Plan paths

Reference schema version: `1`. This inventory records the checked-in reference application's selected profile; omitted optional schema 2 topology remains visible in the Rust type table above.

| JSON path | Observed type |
|---|---|
| `allowed_headers` | `array` |
| `allowed_headers[]` | `string` |
| `allowed_origins` | `array` |
| `allowed_origins[]` | `string` |
| `application` | `string` |
| `application_graph` | `object` |
| `application_graph.capabilities` | `object` |
| `application_graph.capabilities.health.registry` | `string` |
| `application_graph.capabilities.http.idempotency` | `string` |
| `application_graph.capabilities.idempotency.claim` | `string` |
| `application_graph.capabilities.idempotency.store` | `string` |
| `application_graph.capabilities.observability.tracing` | `string` |
| `application_graph.health_checks` | `object` |
| `application_graph.health_checks.minco-core` | `object` |
| `application_graph.health_checks.minco-core.critical` | `boolean` |
| `application_graph.health_checks.minco-core.id` | `string` |
| `application_graph.migrations` | `object` |
| `application_graph.operations` | `object` |
| `application_graph.plugins` | `array` |
| `application_graph.plugins[]` | `object` |
| `application_graph.plugins[].configuration` | `array` |
| `application_graph.plugins[].configuration_namespace` | `string` |
| `application_graph.plugins[].core_compatibility` | `string` |
| `application_graph.plugins[].data_classes` | `array` |
| `application_graph.plugins[].default_enabled` | `boolean` |
| `application_graph.plugins[].description` | `string` |
| `application_graph.plugins[].documentation` | `string` |
| `application_graph.plugins[].health_checks` | `array` |
| `application_graph.plugins[].health_checks[]` | `object` |
| `application_graph.plugins[].health_checks[].critical` | `boolean` |
| `application_graph.plugins[].health_checks[].id` | `string` |
| `application_graph.plugins[].id` | `string` |
| `application_graph.plugins[].migrations` | `array` |
| `application_graph.plugins[].operations` | `array` |
| `application_graph.plugins[].plugin_dependencies` | `array` |
| `application_graph.plugins[].provides` | `array` |
| `application_graph.plugins[].provides[]` | `object` |
| `application_graph.plugins[].provides[].name` | `string` |
| `application_graph.plugins[].provides[].version` | `string` |
| `application_graph.plugins[].requires` | `array` |
| `application_graph.plugins[].resources` | `array` |
| `application_graph.plugins[].stability` | `string` |
| `application_graph.plugins[].version` | `string` |
| `application_graph.resources` | `object` |
| `auth` | `object` |
| `auth.audiences` | `array` |
| `auth.audiences[]` | `string` |
| `auth.issuer` | `string` |
| `auth.kind` | `string` |
| `cost_policy` | `object` |
| `cost_policy.deny_fixed_compute` | `boolean` |
| `cost_policy.deny_nat_gateway` | `boolean` |
| `cost_policy.deny_provisioned_concurrency` | `boolean` |
| `cost_policy.deny_scheduled_wakeups` | `boolean` |
| `cost_policy.max_database_connections` | `integer` |
| `cost_policy.max_reserved_concurrency` | `integer` |
| `database` | `object` |
| `database.compute_unit_hours` | `number` |
| `database.history_storage_gb_month` | `number` |
| `database.kind` | `string` |
| `database.plan` | `string` |
| `database.storage_gb_month` | `number` |
| `environment` | `string` |
| `exposed_headers` | `array` |
| `exposed_headers[]` | `string` |
| `functions` | `array` |
| `functions[]` | `object` |
| `functions[].artifact_path` | `string` |
| `functions[].database_connections_per_instance` | `integer` |
| `functions[].memory_mb` | `integer` |
| `functions[].name` | `string` |
| `functions[].provisioned_concurrency` | `integer` |
| `functions[].reserved_concurrency` | `integer` |
| `functions[].timeout_seconds` | `integer` |
| `ingress` | `string` |
| `local_aws_services` | `array` |
| `local_aws_services[]` | `string` |
| `log_retention_days` | `integer` |
| `performance_policy` | `object` |
| `performance_policy.max_lambda_memory_mb` | `integer` |
| `performance_policy.max_lambda_timeout_seconds` | `integer` |
| `performance_policy.max_request_body_bytes` | `integer` |
| `performance_policy.target_artifact_bytes` | `integer` |
| `region` | `string` |
| `routes` | `array` |
| `routes[]` | `object` |
| `routes[].authenticated` | `boolean` |
| `routes[].method` | `string` |
| `routes[].operation_id` | `string` |
| `routes[].path` | `string` |
| `runtime` | `string` |
| `scheduled_wakeups` | `array` |
| `schema_version` | `integer` |
| `uses_nat_gateway` | `boolean` |
