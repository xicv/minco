# Diagnostic code reference

<!-- @generated; do not edit by hand -->
<!-- generated-reference-schema: 1 -->

Generator: `scripts/docs/generate_reference.py` schema `1`.

Authorities:

- `diagnostic string literals in crates/**/*.rs`
- `diagnostic string literals in plugins/**/*.rs and extensions/**/*.rs`
- `repository validation diagnostics in scripts/**/*.py`

Regenerate with `scripts/docs/generate-reference.sh`; use `--check` to verify byte-for-byte freshness.

This inventory lists source-declared stable code identities, not every possible runtime message. Messages may gain context while codes remain the automation contract. A code's presence does not claim that every profile can emit it.

Declared codes: `344`.

| Code | Family | First declaration | Additional references |
|---|---|---|---:|
| `MINCO-AGENT-CONTEXT-OPERATION-ABSENT` | agent | `crates/minco-cli/src/agent_cmd.rs:419` | 1 |
| `MINCO-AGENT-CONTEXT-TASK-ABSENT` | agent | `crates/minco-cli/src/agent_cmd.rs:461` | 1 |
| `MINCO-ARCH-001` | arch | `crates/minco-cli/src/architecture.rs:132` | 0 |
| `MINCO-ARCH-002` | arch | `crates/minco-cli/src/architecture.rs:133` | 0 |
| `MINCO-ARCH-003` | arch | `crates/minco-cli/src/architecture.rs:134` | 0 |
| `MINCO-AUTH-001` | auth | `crates/minco-plan/src/model.rs:545` | 0 |
| `MINCO-AWS-001` | aws | `crates/minco-plan/src/model.rs:1289` | 1 |
| `MINCO-AWS-002` | aws | `crates/minco-plan/src/model.rs:1305` | 1 |
| `MINCO-CONTRACT-001` | contract | `crates/minco-contract/src/validate.rs:64` | 0 |
| `MINCO-CONTRACT-002` | contract | `crates/minco-contract/src/validate.rs:72` | 0 |
| `MINCO-CONTRACT-003` | contract | `crates/minco-contract/src/validate.rs:84` | 0 |
| `MINCO-CONTRACT-004` | contract | `crates/minco-contract/src/validate.rs:98` | 0 |
| `MINCO-CONTRACT-005` | contract | `crates/minco-contract/src/validate.rs:111` | 0 |
| `MINCO-CONTRACT-006` | contract | `crates/minco-contract/src/validate.rs:120` | 0 |
| `MINCO-CONTRACT-007` | contract | `crates/minco-contract/src/validate.rs:152` | 0 |
| `MINCO-CONTRACT-008` | contract | `crates/minco-contract/src/validate.rs:196` | 0 |
| `MINCO-CONTRACT-009` | contract | `crates/minco-contract/src/validate.rs:1288` | 1 |
| `MINCO-CONTRACT-010` | contract | `crates/minco-contract/src/validate.rs:807` | 0 |
| `MINCO-CONTRACT-011` | contract | `crates/minco-contract/src/validate.rs:816` | 0 |
| `MINCO-CONTRACT-012` | contract | `crates/minco-contract/src/validate.rs:827` | 0 |
| `MINCO-CONTRACT-013` | contract | `crates/minco-contract/src/validate.rs:213` | 0 |
| `MINCO-CONTRACT-014` | contract | `crates/minco-contract/src/validate.rs:136` | 0 |
| `MINCO-CONTRACT-015` | contract | `crates/minco-contract/src/validate.rs:160` | 1 |
| `MINCO-CONTRACT-016` | contract | `crates/minco-contract/src/validate.rs:904` | 4 |
| `MINCO-CONTRACT-017` | contract | `crates/minco-contract/src/validate.rs:838` | 1 |
| `MINCO-CONTRACT-018` | contract | `crates/minco-contract/src/validate.rs:875` | 1 |
| `MINCO-CONTRACT-019` | contract | `crates/minco-contract/src/validate.rs:1270` | 0 |
| `MINCO-CONTRACT-020` | contract | `crates/minco-contract/src/validate.rs:965` | 3 |
| `MINCO-CONTRACT-021` | contract | `crates/minco-contract/src/validate.rs:1379` | 2 |
| `MINCO-CONTRACT-022` | contract | `crates/minco-contract/src/validate.rs:354` | 1 |
| `MINCO-CONTRACT-023` | contract | `crates/minco-contract/src/validate.rs:399` | 1 |
| `MINCO-CONTRACT-024` | contract | `crates/minco-contract/src/validate.rs:423` | 1 |
| `MINCO-CONTRACT-025` | contract | `crates/minco-contract/src/validate.rs:455` | 4 |
| `MINCO-CONTRACT-026` | contract | `crates/minco-contract/src/validate.rs:381` | 2 |
| `MINCO-CONTRACT-027` | contract | `crates/minco-contract/src/validate.rs:411` | 1 |
| `MINCO-CONTRACT-028` | contract | `crates/minco-contract/src/validate.rs:268` | 2 |
| `MINCO-COST-001` | cost | `crates/minco-plan/src/model.rs:639` | 0 |
| `MINCO-COST-002` | cost | `crates/minco-plan/src/model.rs:653` | 1 |
| `MINCO-COST-003` | cost | `crates/minco-plan/src/model.rs:719` | 0 |
| `MINCO-COST-004` | cost | `crates/minco-plan/src/model.rs:725` | 0 |
| `MINCO-COST-005` | cost | `crates/minco-plan/src/model.rs:770` | 1 |
| `MINCO-COST-006` | cost | `crates/minco-plan/src/model.rs:633` | 0 |
| `MINCO-COST-007` | cost | `crates/minco-plan/src/model.rs:2352` | 1 |
| `MINCO-COST-008` | cost | `crates/minco-plan/src/model.rs:2381` | 1 |
| `MINCO-COST-009` | cost | `crates/minco-plan/src/model.rs:674` | 2 |
| `MINCO-COST-010` | cost | `crates/minco-plan/src/model.rs:709` | 0 |
| `MINCO-DB-001` | db | `crates/minco-plan/src/model.rs:782` | 0 |
| `MINCO-DB-002` | db | `crates/minco-plan/src/model.rs:788` | 0 |
| `MINCO-DB-003` | db | `crates/minco-plan/src/model.rs:807` | 0 |
| `MINCO-DB-004` | db | `crates/minco-plan/src/model.rs:2424` | 2 |
| `MINCO-DB-005` | db | `crates/minco-plan/src/model.rs:823` | 0 |
| `MINCO-DYNAMODB-001` | dynamodb | `crates/minco-plan/src/model.rs:1344` | 0 |
| `MINCO-DYNAMODB-002` | dynamodb | `crates/minco-plan/src/model.rs:1350` | 1 |
| `MINCO-DYNAMODB-003` | dynamodb | `crates/minco-plan/src/model.rs:1360` | 1 |
| `MINCO-DYNAMODB-004` | dynamodb | `crates/minco-plan/src/model.rs:1366` | 0 |
| `MINCO-DYNAMODB-005` | dynamodb | `crates/minco-plan/src/model.rs:1415` | 0 |
| `MINCO-DYNAMODB-006` | dynamodb | `crates/minco-plan/src/model.rs:1424` | 0 |
| `MINCO-DYNAMODB-007` | dynamodb | `crates/minco-plan/src/model.rs:1376` | 1 |
| `MINCO-DYNAMODB-008` | dynamodb | `crates/minco-plan/src/model.rs:1387` | 1 |
| `MINCO-HTTP-001` | http | `crates/minco-plan/src/model.rs:597` | 0 |
| `MINCO-HTTP-002` | http | `crates/minco-plan/src/model.rs:602` | 0 |
| `MINCO-HTTP-003` | http | `crates/minco-plan/src/model.rs:606` | 0 |
| `MINCO-HTTP-004` | http | `crates/minco-plan/src/model.rs:614` | 0 |
| `MINCO-HTTP-005` | http | `crates/minco-plan/src/model.rs:621` | 0 |
| `MINCO-HTTP-006` | http | `crates/minco-plan/src/model.rs:626` | 0 |
| `MINCO-IAM-001` | iam | `crates/minco-plan/src/model.rs:590` | 1 |
| `MINCO-PERF-001` | perf | `crates/minco-plan/src/model.rs:736` | 0 |
| `MINCO-PERF-002` | perf | `crates/minco-plan/src/model.rs:747` | 0 |
| `MINCO-PERF-003` | perf | `crates/minco-cli/src/main.rs:6170` | 0 |
| `MINCO-PERF-004` | perf | `crates/minco-plan/src/model.rs:700` | 0 |
| `MINCO-PLAN-001` | plan | `crates/minco-plan/src/model.rs:433` | 0 |
| `MINCO-PLAN-002` | plan | `crates/minco-plan/src/model.rs:550` | 0 |
| `MINCO-PLAN-003` | plan | `crates/minco-plan/src/model.rs:2323` | 1 |
| `MINCO-PLAN-004` | plan | `crates/minco-plan/src/model.rs:557` | 0 |
| `MINCO-PLAN-005` | plan | `crates/minco-plan/src/model.rs:567` | 1 |
| `MINCO-PLAN-010` | plan | `crates/minco-plan/src/model.rs:844` | 2 |
| `MINCO-PLAN-011` | plan | `crates/minco-plan/src/model.rs:853` | 1 |
| `MINCO-PLAN-012` | plan | `crates/minco-plan/src/model.rs:865` | 0 |
| `MINCO-PLAN-013` | plan | `crates/minco-plan/src/model.rs:880` | 1 |
| `MINCO-PLAN-014` | plan | `crates/minco-plan/src/model.rs:971` | 1 |
| `MINCO-PLAN-015` | plan | `crates/minco-plan/src/model.rs:1013` | 5 |
| `MINCO-PLAN-016` | plan | `crates/minco-plan/src/model.rs:1009` | 2 |
| `MINCO-PLAN-017` | plan | `crates/minco-plan/src/model.rs:1193` | 0 |
| `MINCO-PLAN-018` | plan | `crates/minco-plan/src/model.rs:1208` | 4 |
| `MINCO-PLAN-MIGRATE-001` | plan | `crates/minco-plan/tests/multi_runtime.rs:1094` | 0 |
| `MINCO-PLAN-MIGRATE-002` | plan | `crates/minco-plan/tests/multi_runtime.rs:666` | 0 |
| `MINCO-PREVIEW-001` | preview | `crates/minco-plan/src/model.rs:500` | 0 |
| `MINCO-PREVIEW-002` | preview | `crates/minco-plan/src/model.rs:506` | 0 |
| `MINCO-PREVIEW-003` | preview | `crates/minco-plan/src/model.rs:524` | 1 |
| `MINCO-PREVIEW-004` | preview | `crates/minco-plan/src/model.rs:530` | 0 |
| `MINCO-PREVIEW-006` | preview | `crates/minco-plan/src/model.rs:537` | 1 |
| `MINCO-REALTIME-001` | realtime | `crates/minco-plan/src/model.rs:462` | 0 |
| `MINCO-REALTIME-002` | realtime | `crates/minco-plan/src/model.rs:468` | 0 |
| `MINCO-SCHEDULE-001` | schedule | `crates/minco-plan/src/model.rs:1109` | 1 |
| `MINCO-SCHEDULE-002` | schedule | `crates/minco-plan/src/model.rs:1115` | 1 |
| `MINCO-SCHEDULE-003` | schedule | `crates/minco-plan/src/model.rs:575` | 1 |
| `MINCO-SCHEDULE-004` | schedule | `crates/minco-plan/src/model.rs:1126` | 1 |
| `MINCO-SCHEDULE-005` | schedule | `crates/minco-plan/src/model.rs:1141` | 0 |
| `MINCO-SCHEDULE-006` | schedule | `crates/minco-plan/src/model.rs:1150` | 0 |
| `MINCO-SQS-001` | sqs | `crates/minco-plan/src/model.rs:1083` | 1 |
| `MINCO-SQS-002` | sqs | `crates/minco-plan/src/model.rs:1037` | 1 |
| `MINCO-SQS-003` | sqs | `crates/minco-plan/src/model.rs:915` | 1 |
| `MINCO-SQS-004` | sqs | `crates/minco-plan/src/model.rs:908` | 1 |
| `MINCO-SQS-005` | sqs | `crates/minco-plan/src/model.rs:935` | 0 |
| `MINCO-SQS-006` | sqs | `crates/minco-plan/src/model.rs:955` | 1 |
| `MINCO-SQS-007` | sqs | `crates/minco-plan/src/model.rs:1050` | 1 |
| `MINCO-SQS-008` | sqs | `crates/minco-plan/src/model.rs:1061` | 1 |
| `MINCO-SQS-009` | sqs | `crates/minco-plan/src/model.rs:1073` | 0 |
| `MINCO-SQS-010` | sqs | `crates/minco-plan/src/model.rs:886` | 0 |
| `MINCO-SQS-011` | sqs | `crates/minco-plan/src/model.rs:895` | 0 |
| `MINCO-SQS-012` | sqs | `crates/minco-plan/src/model.rs:1183` | 1 |
| `MINCO-STATIC-001` | static | `crates/minco-plan/src/model.rs:455` | 0 |
| `PUBLISH-001` | publication | `scripts/validate_publish.py:134` | 0 |
| `PUBLISH-002` | publication | `scripts/validate_publish.py:140` | 0 |
| `PUBLISH-003` | publication | `scripts/validate_publish.py:143` | 0 |
| `PUBLISH-010` | publication | `scripts/validate_publish.py:159` | 0 |
| `PUBLISH-011` | publication | `scripts/validate_publish.py:166` | 0 |
| `PUBLISH-012` | publication | `scripts/validate_publish.py:172` | 0 |
| `PUBLISH-013` | publication | `scripts/validate_publish.py:179` | 0 |
| `PUBLISH-014` | publication | `scripts/validate_publish.py:182` | 0 |
| `PUBLISH-015` | publication | `scripts/validate_publish.py:186` | 0 |
| `PUBLISH-016` | publication | `scripts/validate_publish.py:198` | 0 |
| `PUBLISH-017` | publication | `scripts/validate_publish.py:224` | 0 |
| `PUBLISH-018` | publication | `scripts/validate_publish.py:227` | 0 |
| `PUBLISH-019` | publication | `scripts/validate_publish.py:230` | 0 |
| `PUBLISH-020` | publication | `scripts/validate_publish.py:232` | 0 |
| `PUBLISH-021` | publication | `scripts/test/publish_validation.py:107` | 3 |
| `PUBLISH-030` | publication | `scripts/validate_publish.py:242` | 0 |
| `PUBLISH-031` | publication | `scripts/validate_publish.py:249` | 0 |
| `PUBLISH-032` | publication | `scripts/validate_publish.py:269` | 0 |
| `PUBLISH-033` | publication | `scripts/validate_publish.py:275` | 0 |
| `PUBLISH-040` | publication | `scripts/validate_publish.py:304` | 0 |
| `PUBLISH-041` | publication | `scripts/validate_publish.py:309` | 0 |
| `PUBLISH-042` | publication | `scripts/validate_publish.py:319` | 0 |
| `PUBLISH-043` | publication | `scripts/validate_publish.py:330` | 0 |
| `PUBLISH-044` | publication | `scripts/validate_publish.py:338` | 0 |
| `PUBLISH-050` | publication | `scripts/validate_publish.py:347` | 0 |
| `PUBLISH-051` | publication | `scripts/validate_publish.py:355` | 0 |
| `PUBLISH-052` | publication | `scripts/validate_publish.py:384` | 0 |
| `PUBLISH-053` | publication | `scripts/validate_publish.py:389` | 0 |
| `PUBLISH-060` | publication | `scripts/validate_publish.py:393` | 0 |
| `PUBLISH-061` | publication | `scripts/validate_publish.py:398` | 0 |
| `PUBLISH-062` | publication | `scripts/validate_publish.py:402` | 0 |
| `PUBLISH-063` | publication | `scripts/validate_publish.py:407` | 0 |
| `PUBLISH-064` | publication | `scripts/validate_publish.py:418` | 0 |
| `PUBLISH-065` | publication | `scripts/validate_publish.py:433` | 0 |
| `PUBLISH-066` | publication | `scripts/validate_publish.py:440` | 0 |
| `PUBLISH-067` | publication | `scripts/validate_publish.py:446` | 0 |
| `PUBLISH-068` | publication | `scripts/validate_publish.py:425` | 0 |
| `PUBLISH-070` | publication | `scripts/validate_publish.py:474` | 1 |
| `PUBLISH-071` | publication | `scripts/test/publish_validation.py:429` | 2 |
| `PUBLISH-072` | publication | `scripts/test/publish_validation.py:406` | 1 |
| `PUBLISH-073` | publication | `scripts/validate_publish.py:495` | 0 |
| `PUBLISH-074` | publication | `scripts/test/publish_validation.py:353` | 3 |
| `STATIC-001` | repository truth | `scripts/validate_static.py:144` | 0 |
| `STATIC-ARCH-001` | repository truth | `scripts/validate_static.py:975` | 0 |
| `STATIC-ARCH-002` | repository truth | `scripts/validate_static.py:981` | 0 |
| `STATIC-ARCH-003` | repository truth | `scripts/validate_static.py:987` | 0 |
| `STATIC-ARCH-004` | repository truth | `scripts/validate_static.py:990` | 0 |
| `STATIC-BUDGET-001` | repository truth | `scripts/validate_static.py:676` | 0 |
| `STATIC-BUDGET-002` | repository truth | `scripts/validate_static.py:454` | 0 |
| `STATIC-BUDGET-003` | repository truth | `scripts/validate_static.py:311` | 0 |
| `STATIC-BUDGET-004` | repository truth | `scripts/test/repository_truth.py:465` | 3 |
| `STATIC-BUDGET-005` | repository truth | `scripts/validate_static.py:417` | 0 |
| `STATIC-BUDGET-006` | repository truth | `scripts/test/repository_truth.py:519` | 1 |
| `STATIC-BUDGET-007` | repository truth | `scripts/test/repository_truth.py:536` | 1 |
| `STATIC-CARGO-001` | repository truth | `scripts/validate_static.py:175` | 0 |
| `STATIC-CARGO-002` | repository truth | `scripts/validate_static.py:178` | 0 |
| `STATIC-CARGO-003` | repository truth | `scripts/validate_static.py:184` | 0 |
| `STATIC-CARGO-004` | repository truth | `scripts/validate_static.py:190` | 0 |
| `STATIC-CARGO-005` | repository truth | `scripts/validate_static.py:192` | 0 |
| `STATIC-CARGO-006` | repository truth | `scripts/validate_static.py:198` | 0 |
| `STATIC-CARGO-007` | repository truth | `scripts/validate_static.py:203` | 0 |
| `STATIC-CARGO-008` | repository truth | `scripts/validate_static.py:206` | 0 |
| `STATIC-CONTRACT-001` | repository truth | `scripts/validate_static.py:818` | 0 |
| `STATIC-CONTRACT-002` | repository truth | `scripts/validate_static.py:823` | 0 |
| `STATIC-CONTRACT-003` | repository truth | `scripts/validate_static.py:831` | 0 |
| `STATIC-CONTRACT-004` | repository truth | `scripts/validate_static.py:834` | 0 |
| `STATIC-CONTRACT-005` | repository truth | `scripts/validate_static.py:838` | 0 |
| `STATIC-CONTRACT-006` | repository truth | `scripts/validate_static.py:840` | 0 |
| `STATIC-CONTRACT-007` | repository truth | `scripts/validate_static.py:879` | 0 |
| `STATIC-CONTRACT-008` | repository truth | `scripts/validate_static.py:928` | 0 |
| `STATIC-CONTRACT-009` | repository truth | `scripts/validate_static.py:936` | 0 |
| `STATIC-CONTRACT-010` | repository truth | `scripts/validate_static.py:941` | 0 |
| `STATIC-CONTRACT-011` | repository truth | `scripts/validate_static.py:945` | 0 |
| `STATIC-CONTRACT-012` | repository truth | `scripts/validate_static.py:951` | 0 |
| `STATIC-CONTRACT-013` | repository truth | `scripts/validate_static.py:956` | 0 |
| `STATIC-CONTRACT-014` | repository truth | `scripts/validate_static.py:854` | 0 |
| `STATIC-CONTRACT-015` | repository truth | `scripts/validate_static.py:882` | 0 |
| `STATIC-CONTRACT-016` | repository truth | `scripts/validate_static.py:897` | 0 |
| `STATIC-CONTRACT-017` | repository truth | `scripts/validate_static.py:847` | 0 |
| `STATIC-CONTRACT-019` | repository truth | `scripts/validate_static.py:915` | 0 |
| `STATIC-CONTRACT-020` | repository truth | `scripts/validate_static.py:890` | 0 |
| `STATIC-CONTRACT-021` | repository truth | `scripts/validate_static.py:866` | 0 |
| `STATIC-COST-001` | repository truth | `scripts/validate_static.py:1201` | 0 |
| `STATIC-COST-002` | repository truth | `scripts/validate_static.py:1204` | 0 |
| `STATIC-COST-003` | repository truth | `scripts/validate_static.py:1210` | 0 |
| `STATIC-COST-004` | repository truth | `scripts/validate_static.py:1212` | 0 |
| `STATIC-COST-005` | repository truth | `scripts/validate_static.py:1214` | 0 |
| `STATIC-DATA-001` | repository truth | `scripts/validate_static.py:162` | 0 |
| `STATIC-DB-001` | repository truth | `scripts/validate_static.py:1235` | 0 |
| `STATIC-GRAPH-001` | repository truth | `scripts/validate_static.py:1086` | 0 |
| `STATIC-HTTP-001` | repository truth | `scripts/validate_static.py:1217` | 0 |
| `STATIC-HTTP-002` | repository truth | `scripts/validate_static.py:1229` | 0 |
| `STATIC-MEASURE-001` | repository truth | `scripts/validate_static.py:332` | 0 |
| `STATIC-MEASURE-002` | repository truth | `scripts/test/repository_truth.py:496` | 1 |
| `STATIC-MEASURE-003` | repository truth | `scripts/validate_static.py:429` | 0 |
| `STATIC-MEASURE-004` | repository truth | `scripts/test/repository_truth.py:503` | 1 |
| `STATIC-MEASURE-005` | repository truth | `scripts/test/repository_truth.py:526` | 1 |
| `STATIC-PLACEHOLDER-001` | repository truth | `scripts/validate_static.py:1294` | 0 |
| `STATIC-PLAN-000` | repository truth | `scripts/validate_static.py:1182` | 0 |
| `STATIC-PLAN-001` | repository truth | `scripts/validate_static.py:1188` | 0 |
| `STATIC-PLAN-002` | repository truth | `scripts/validate_static.py:1195` | 0 |
| `STATIC-PLAN-003` | repository truth | `scripts/validate_static.py:1198` | 0 |
| `STATIC-PLUGIN-001` | repository truth | `scripts/validate_static.py:1006` | 0 |
| `STATIC-PLUGIN-002` | repository truth | `scripts/validate_static.py:1008` | 0 |
| `STATIC-PLUGIN-003` | repository truth | `scripts/validate_static.py:1013` | 0 |
| `STATIC-PLUGIN-004` | repository truth | `scripts/validate_static.py:1019` | 0 |
| `STATIC-PLUGIN-005` | repository truth | `scripts/validate_static.py:1022` | 0 |
| `STATIC-PYTHON-001` | repository truth | `scripts/validate_static.py:1152` | 0 |
| `STATIC-QUALITY-001` | repository truth | `scripts/validate_static.py:1108` | 0 |
| `STATIC-QUALITY-002` | repository truth | `scripts/validate_static.py:1110` | 0 |
| `STATIC-QUALITY-003` | repository truth | `scripts/validate_static.py:1114` | 0 |
| `STATIC-QUALITY-004` | repository truth | `scripts/validate_static.py:1127` | 0 |
| `STATIC-ROADMAP-001` | repository truth | `scripts/validate_static.py:1037` | 0 |
| `STATIC-ROADMAP-002` | repository truth | `scripts/validate_static.py:1041` | 0 |
| `STATIC-RUST-001` | repository truth | `scripts/validate_static.py:1140` | 0 |
| `STATIC-SAM-001` | repository truth | `scripts/validate_static.py:1247` | 0 |
| `STATIC-SAM-002` | repository truth | `scripts/validate_static.py:1265` | 0 |
| `STATIC-SAM-003` | repository truth | `scripts/validate_static.py:1268` | 0 |
| `STATIC-SAM-004` | repository truth | `scripts/validate_static.py:1272` | 0 |
| `STATIC-SAM-005` | repository truth | `scripts/validate_static.py:1259` | 0 |
| `STATIC-SHELL-001` | repository truth | `scripts/validate_static.py:1163` | 0 |
| `STATIC-SHELL-002` | repository truth | `scripts/validate_static.py:1165` | 0 |
| `STATIC-TASK-001` | repository truth | `scripts/validate_static.py:1046` | 0 |
| `STATIC-TASK-002` | repository truth | `scripts/validate_static.py:1052` | 0 |
| `STATIC-TASK-003` | repository truth | `scripts/validate_static.py:1056` | 0 |
| `STATIC-TASK-004` | repository truth | `scripts/validate_static.py:1059` | 0 |
| `STATIC-TASK-005` | repository truth | `scripts/validate_static.py:1063` | 0 |
| `STATIC-TASK-006` | repository truth | `scripts/validate_static.py:1065` | 0 |
| `STATIC-TASK-007` | repository truth | `scripts/validate_static.py:1068` | 0 |
| `STATIC-TASK-008` | repository truth | `scripts/validate_static.py:1070` | 0 |
| `STATIC-TASK-009` | repository truth | `scripts/validate_static.py:1074` | 0 |
| `STATIC-TRUTH-ADOPTION-001` | repository truth | `scripts/validate_static.py:731` | 0 |
| `STATIC-TRUTH-CATALOG-001` | repository truth | `scripts/validate_static.py:575` | 0 |
| `STATIC-TRUTH-CATALOG-002` | repository truth | `scripts/test/repository_truth.py:418` | 1 |
| `STATIC-TRUTH-CATALOG-003` | repository truth | `scripts/validate_static.py:562` | 0 |
| `STATIC-TRUTH-CATALOG-004` | repository truth | `scripts/validate_static.py:564` | 0 |
| `STATIC-TRUTH-CATALOG-005` | repository truth | `scripts/validate_static.py:566` | 0 |
| `STATIC-TRUTH-CATALOG-006` | repository truth | `scripts/validate_static.py:633` | 0 |
| `STATIC-TRUTH-DESCRIPTOR-001` | repository truth | `scripts/validate_static.py:605` | 0 |
| `STATIC-TRUTH-DESCRIPTOR-002` | repository truth | `scripts/validate_static.py:611` | 0 |
| `STATIC-TRUTH-DESCRIPTOR-003` | repository truth | `scripts/validate_static.py:618` | 0 |
| `STATIC-TRUTH-DOCS-001` | repository truth | `scripts/test/repository_truth.py:331` | 3 |
| `STATIC-TRUTH-FACADE-001` | repository truth | `scripts/validate_static.py:587` | 0 |
| `STATIC-TRUTH-FACADE-002` | repository truth | `scripts/validate_static.py:593` | 0 |
| `STATIC-TRUTH-FACADE-003` | repository truth | `scripts/validate_static.py:650` | 0 |
| `STATIC-TRUTH-FACADE-004` | repository truth | `scripts/validate_static.py:664` | 0 |
| `STATIC-TRUTH-PACKAGES-001` | repository truth | `scripts/validate_static.py:483` | 0 |
| `STATIC-TRUTH-PACKAGES-002` | repository truth | `scripts/validate_static.py:489` | 0 |
| `STATIC-TRUTH-PACKAGES-003` | repository truth | `scripts/test/repository_truth.py:386` | 2 |
| `STATIC-TRUTH-PACKAGES-004` | repository truth | `scripts/test/repository_truth.py:376` | 1 |
| `STATIC-TRUTH-PLAN-001` | repository truth | `scripts/validate_static.py:743` | 0 |
| `STATIC-TRUTH-PLAN-002` | repository truth | `scripts/validate_static.py:749` | 0 |
| `STATIC-TRUTH-PUBLISHED-001` | repository truth | `scripts/validate_static.py:233` | 0 |
| `STATIC-TRUTH-PUBLISHED-002` | repository truth | `scripts/test/repository_truth.py:397` | 2 |
| `STATIC-TRUTH-PUBLISHED-003` | repository truth | `scripts/test/repository_truth.py:408` | 1 |
| `STATIC-TRUTH-RELEASE-001` | repository truth | `scripts/test/repository_truth.py:301` | 1 |
| `STATIC-TRUTH-RELEASE-002` | repository truth | `scripts/test/repository_truth.py:311` | 1 |
| `STATIC-TRUTH-RELEASE-003` | repository truth | `scripts/test/repository_truth.py:321` | 1 |
| `STATIC-TRUTH-RELEASE-004` | repository truth | `scripts/validate_static.py:254` | 0 |
| `STATIC-TRUTH-ROADMAP-001` | repository truth | `scripts/test/repository_truth.py:429` | 1 |
| `STATIC-TRUTH-ROADMAP-002` | repository truth | `scripts/test/repository_truth.py:458` | 1 |
| `STATIC-TRUTH-ROADMAP-003` | repository truth | `scripts/test/repository_truth.py:439` | 1 |
| `STATIC-TRUTH-VERSION-001` | repository truth | `scripts/test/repository_truth.py:290` | 1 |
| `config.cli_override` | configuration | `crates/minco-cli/src/config_cmd.rs:410` | 1 |
| `config.compiled_layer_forbidden` | configuration | `crates/minco-config/src/graph.rs:404` | 0 |
| `config.digest_encoding` | configuration | `crates/minco-config/src/graph.rs:237` | 0 |
| `config.duplicate_field` | configuration | `crates/minco-cli/src/config_cmd.rs:321` | 6 |
| `config.duplicate_layer` | configuration | `crates/minco-config/src/graph.rs:395` | 0 |
| `config.empty_table` | configuration | `crates/minco-config/src/graph.rs:486` | 0 |
| `config.environment_class_mismatch` | configuration | `crates/minco-cli/src/config_cmd.rs:169` | 1 |
| `config.environment_class_missing` | configuration | `crates/minco-cli/src/config_cmd.rs:159` | 1 |
| `config.environment_class_unexpected` | configuration | `crates/minco-config/src/graph.rs:445` | 0 |
| `config.environment_override_name` | configuration | `crates/minco-cli/src/config_cmd.rs:363` | 2 |
| `config.environment_override_value` | configuration | `crates/minco-cli/src/config_cmd.rs:374` | 0 |
| `config.environment_prefix` | configuration | `crates/minco-cli/src/config_cmd.rs:347` | 0 |
| `config.explain.unknown_field` | configuration | `crates/minco-cli/src/config_cmd.rs:99` | 0 |
| `config.file_parse` | configuration | `crates/minco-cli/src/config_cmd.rs:285` | 0 |
| `config.file_read` | configuration | `crates/minco-cli/src/config_cmd.rs:261` | 2 |
| `config.invalid_environment` | configuration | `crates/minco-cli/src/config_cmd.rs:143` | 2 |
| `config.invalid_secret_reference` | configuration | `crates/minco-config/src/graph.rs:175` | 0 |
| `config.local_override_forbidden` | configuration | `crates/minco-config/src/graph.rs:437` | 2 |
| `config.plugin_schema` | configuration | `crates/minco-cli/src/config_cmd.rs:225` | 0 |
| `config.plugin_selection` | configuration | `crates/minco-cli/src/config_cmd.rs:236` | 1 |
| `config.profile_path` | configuration | `crates/minco-cli/src/config_cmd.rs:269` | 3 |
| `config.required_field_missing` | configuration | `crates/minco-config/src/graph.rs:224` | 0 |
| `config.schema.default_type` | configuration | `crates/minco-config/src/schema.rs:133` | 0 |
| `config.schema.duplicate_field` | configuration | `crates/minco-config/src/schema.rs:163` | 0 |
| `config.schema.invalid_path` | configuration | `crates/minco-config/src/schema.rs:91` | 0 |
| `config.schema.missing_description` | configuration | `crates/minco-config/src/schema.rs:101` | 0 |
| `config.schema.overlapping_field` | configuration | `crates/minco-config/src/schema.rs:149` | 1 |
| `config.schema.secret_default` | configuration | `crates/minco-config/src/schema.rs:121` | 1 |
| `config.schema.secret_reference_kind` | configuration | `crates/minco-config/src/schema.rs:111` | 1 |
| `config.secret_reference_required` | configuration | `crates/minco-config/src/graph.rs:159` | 0 |
| `config.type_mismatch` | configuration | `crates/minco-config/src/graph.rs:189` | 0 |
| `config.typed_deserialization` | configuration | `crates/minco-config/src/graph.rs:563` | 1 |
| `config.unknown_field` | configuration | `crates/minco-config/src/graph.rs:147` | 1 |
| `config.unknown_namespace` | configuration | `crates/minco-config/src/graph.rs:339` | 1 |
| `conformance.evidence` | plugin conformance | `crates/minco-test/src/plugin_conformance.rs:869` | 0 |
| `conformance.profile` | plugin conformance | `crates/minco-test/src/plugin_conformance.rs:820` | 0 |
| `documentation.reference` | documentation | `crates/minco-test/src/plugin_conformance.rs:849` | 0 |
| `operation.added` | operation | `crates/minco-cli/tests/compatibility_cli.rs:83` | 2 |
| `operation.authentication_removed` | operation | `crates/minco-contract/src/compatibility.rs:92` | 1 |
| `operation.authentication_required` | operation | `crates/minco-contract/src/compatibility.rs:83` | 1 |
| `operation.binding_changed` | operation | `crates/minco-contract/src/compatibility.rs:67` | 1 |
| `operation.idempotency_removed` | operation | `crates/minco-contract/src/compatibility.rs:113` | 1 |
| `operation.idempotency_required` | operation | `crates/minco-contract/src/compatibility.rs:104` | 1 |
| `operation.md.tmpl` | operation | `crates/minco-cli/src/generator_cmd.rs:1478` | 1 |
| `operation.removed` | operation | `crates/minco-cli/tests/compatibility_cli.rs:84` | 2 |
| `operation.resource_convention_added` | operation | `crates/minco-contract/src/compatibility.rs:126` | 0 |
| `operation.resource_convention_changed` | operation | `crates/minco-contract/src/compatibility.rs:138` | 1 |
| `operation.resource_convention_removed` | operation | `crates/minco-contract/src/compatibility.rs:133` | 0 |
| `operation.structure_changed` | operation | `crates/minco-contract/src/compatibility.rs:158` | 2 |
| `package.include` | package | `crates/minco-test/src/plugin_conformance.rs:212` | 0 |
| `package.metadata.minco.plugin` | package | `crates/minco-test/src/plugin_conformance.rs:176` | 1 |
| `request.id` | request | `crates/minco-contract/src/generate.rs:334` | 0 |
| `resource.yaml` | resource | `crates/minco-contract/tests/contract_policy.rs:101` | 0 |
| `schema.added` | schema | `crates/minco-contract/src/compatibility.rs:203` | 2 |
| `schema.constraint_changed` | schema | `crates/minco-contract/src/compatibility.rs:465` | 1 |
| `schema.enum_constraint_added` | schema | `crates/minco-contract/src/compatibility.rs:549` | 1 |
| `schema.enum_constraint_removed` | schema | `crates/minco-contract/src/compatibility.rs:556` | 1 |
| `schema.enum_value_added` | schema | `crates/minco-contract/src/compatibility.rs:538` | 1 |
| `schema.enum_value_removed` | schema | `crates/minco-contract/src/compatibility.rs:526` | 1 |
| `schema.optional_property_added` | schema | `crates/minco-contract/src/compatibility.rs:603` | 1 |
| `schema.property_removed` | schema | `crates/minco-contract/src/compatibility.rs:612` | 1 |
| `schema.reference_unresolved` | schema | `crates/minco-contract/src/compatibility.rs:244` | 0 |
| `schema.removed` | schema | `crates/minco-contract/src/compatibility.rs:214` | 1 |
| `schema.required_property_added` | schema | `crates/minco-contract/src/compatibility.rs:580` | 1 |
| `schema.required_property_removed` | schema | `crates/minco-contract/src/compatibility.rs:589` | 1 |
| `schema.structure_changed` | schema | `crates/minco-contract/src/compatibility.rs:432` | 1 |
| `schema.type_changed` | schema | `crates/minco-contract/src/compatibility.rs:480` | 2 |
| `schema.type_constraint_added` | schema | `crates/minco-contract/src/compatibility.rs:493` | 1 |
| `schema.type_constraint_removed` | schema | `crates/minco-contract/src/compatibility.rs:505` | 1 |
