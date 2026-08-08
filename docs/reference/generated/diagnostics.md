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

Declared codes: `501`.

| Code | Family | First declaration | Additional references |
|---|---|---|---:|
| `ASSURANCE-COST-001` | deployment assurance | `scripts/validate_deployment_assurance.py:339` | 0 |
| `ASSURANCE-COST-002` | deployment assurance | `scripts/validate_deployment_assurance.py:388` | 0 |
| `ASSURANCE-COST-003` | deployment assurance | `scripts/validate_deployment_assurance.py:394` | 0 |
| `ASSURANCE-COST-004` | deployment assurance | `scripts/validate_deployment_assurance.py:400` | 0 |
| `ASSURANCE-DATA-001` | deployment assurance | `scripts/validate_deployment_assurance.py:128` | 0 |
| `ASSURANCE-DECISION-001` | deployment assurance | `scripts/validate_deployment_assurance.py:428` | 0 |
| `ASSURANCE-DECISION-002` | deployment assurance | `scripts/validate_deployment_assurance.py:434` | 0 |
| `ASSURANCE-DEFAULT-001` | deployment assurance | `scripts/validate_deployment_assurance.py:305` | 0 |
| `ASSURANCE-DEFAULT-002` | deployment assurance | `scripts/validate_deployment_assurance.py:420` | 0 |
| `ASSURANCE-DEFAULT-003` | deployment assurance | `scripts/validate_deployment_assurance.py:477` | 0 |
| `ASSURANCE-DEFAULT-004` | deployment assurance | `scripts/validate_deployment_assurance.py:494` | 0 |
| `ASSURANCE-DEFAULT-005` | deployment assurance | `scripts/test/deployment_assurance.py:198` | 1 |
| `ASSURANCE-DEFAULT-006` | deployment assurance | `scripts/validate_deployment_assurance.py:510` | 0 |
| `ASSURANCE-DEFAULT-007` | deployment assurance | `scripts/validate_deployment_assurance.py:470` | 0 |
| `ASSURANCE-DEFAULT-008` | deployment assurance | `scripts/test/deployment_assurance.py:190` | 1 |
| `ASSURANCE-DIMENSION-001` | deployment assurance | `scripts/test/deployment_assurance.py:206` | 1 |
| `ASSURANCE-ENUM-001` | deployment assurance | `scripts/validate_deployment_assurance.py:192` | 0 |
| `ASSURANCE-ENUM-002` | deployment assurance | `scripts/validate_deployment_assurance.py:198` | 0 |
| `ASSURANCE-ENUM-003` | deployment assurance | `scripts/validate_deployment_assurance.py:448` | 0 |
| `ASSURANCE-ENUM-004` | deployment assurance | `scripts/validate_deployment_assurance.py:204` | 0 |
| `ASSURANCE-ENUM-005` | deployment assurance | `scripts/validate_deployment_assurance.py:278` | 0 |
| `ASSURANCE-ENUM-006` | deployment assurance | `scripts/validate_deployment_assurance.py:286` | 0 |
| `ASSURANCE-ENUM-007` | deployment assurance | `scripts/test/deployment_assurance.py:169` | 1 |
| `ASSURANCE-EVIDENCE-001` | deployment assurance | `scripts/validate_deployment_assurance.py:381` | 0 |
| `ASSURANCE-FILE-001` | deployment assurance | `scripts/validate_deployment_assurance.py:117` | 0 |
| `ASSURANCE-INGRESS-001` | deployment assurance | `scripts/validate_deployment_assurance.py:521` | 0 |
| `ASSURANCE-INGRESS-002` | deployment assurance | `scripts/test/deployment_assurance.py:177` | 1 |
| `ASSURANCE-INGRESS-003` | deployment assurance | `scripts/test/deployment_assurance.py:182` | 1 |
| `ASSURANCE-INGRESS-004` | deployment assurance | `scripts/validate_deployment_assurance.py:546` | 0 |
| `ASSURANCE-INGRESS-005` | deployment assurance | `scripts/validate_deployment_assurance.py:553` | 0 |
| `ASSURANCE-PATH-001` | deployment assurance | `scripts/validate_deployment_assurance.py:352` | 0 |
| `ASSURANCE-PATH-002` | deployment assurance | `scripts/test/deployment_assurance.py:214` | 1 |
| `ASSURANCE-PERF-001` | deployment assurance | `scripts/validate_deployment_assurance.py:407` | 0 |
| `ASSURANCE-POLICY-001` | deployment assurance | `scripts/validate_deployment_assurance.py:164` | 0 |
| `ASSURANCE-POLICY-002` | deployment assurance | `scripts/validate_deployment_assurance.py:579` | 0 |
| `ASSURANCE-PROFILE-001` | deployment assurance | `scripts/validate_deployment_assurance.py:212` | 0 |
| `ASSURANCE-PROFILE-002` | deployment assurance | `scripts/validate_deployment_assurance.py:229` | 0 |
| `ASSURANCE-PROFILE-003` | deployment assurance | `scripts/validate_deployment_assurance.py:237` | 0 |
| `ASSURANCE-PROFILE-004` | deployment assurance | `scripts/validate_deployment_assurance.py:246` | 0 |
| `ASSURANCE-PROFILE-005` | deployment assurance | `scripts/validate_deployment_assurance.py:253` | 0 |
| `ASSURANCE-PROFILE-006` | deployment assurance | `scripts/validate_deployment_assurance.py:296` | 0 |
| `ASSURANCE-PROFILE-007` | deployment assurance | `scripts/validate_deployment_assurance.py:596` | 0 |
| `ASSURANCE-RECOVERY-001` | deployment assurance | `scripts/validate_deployment_assurance.py:413` | 0 |
| `ASSURANCE-SCHEMA-001` | deployment assurance | `scripts/validate_deployment_assurance.py:133` | 0 |
| `ASSURANCE-SCOPE-001` | deployment assurance | `scripts/validate_deployment_assurance.py:269` | 0 |
| `ASSURANCE-STATUS-001` | deployment assurance | `scripts/validate_deployment_assurance.py:262` | 0 |
| `ASSURANCE-TRUTH-001` | deployment assurance | `scripts/validate_deployment_assurance.py:140` | 0 |
| `ASSURANCE-TRUTH-002` | deployment assurance | `scripts/validate_deployment_assurance.py:150` | 0 |
| `ASSURANCE-TRUTH-003` | deployment assurance | `scripts/validate_deployment_assurance.py:156` | 0 |
| `EVIDENCE-CAPABILITY-001` | operational evidence | `scripts/validate_operational_evidence.py:657` | 0 |
| `EVIDENCE-CAPABILITY-002` | operational evidence | `scripts/validate_operational_evidence.py:659` | 0 |
| `EVIDENCE-CAPABILITY-003` | operational evidence | `scripts/validate_operational_evidence.py:662` | 0 |
| `EVIDENCE-CAPABILITY-004` | operational evidence | `scripts/validate_operational_evidence.py:666` | 0 |
| `EVIDENCE-CAPABILITY-005` | operational evidence | `scripts/validate_operational_evidence.py:669` | 0 |
| `EVIDENCE-CAPABILITY-006` | operational evidence | `scripts/validate_operational_evidence.py:674` | 0 |
| `EVIDENCE-CAPABILITY-007` | operational evidence | `scripts/validate_operational_evidence.py:678` | 0 |
| `EVIDENCE-CAPABILITY-008` | operational evidence | `scripts/validate_operational_evidence.py:683` | 0 |
| `EVIDENCE-CAPABILITY-009` | operational evidence | `scripts/validate_operational_evidence.py:686` | 0 |
| `EVIDENCE-CAPABILITY-010` | operational evidence | `scripts/validate_operational_evidence.py:689` | 0 |
| `EVIDENCE-CAPABILITY-011` | operational evidence | `scripts/validate_operational_evidence.py:693` | 0 |
| `EVIDENCE-CAPABILITY-012` | operational evidence | `scripts/test/operational_evidence.py:435` | 1 |
| `EVIDENCE-CAPABILITY-013` | operational evidence | `scripts/validate_operational_evidence.py:701` | 0 |
| `EVIDENCE-CAPABILITY-014` | operational evidence | `scripts/validate_operational_evidence.py:705` | 0 |
| `EVIDENCE-CAPABILITY-015` | operational evidence | `scripts/validate_operational_evidence.py:709` | 0 |
| `EVIDENCE-CAPABILITY-016` | operational evidence | `scripts/validate_operational_evidence.py:714` | 0 |
| `EVIDENCE-CAPABILITY-017` | operational evidence | `scripts/validate_operational_evidence.py:716` | 0 |
| `EVIDENCE-DATA-001` | operational evidence | `scripts/validate_operational_evidence.py:214` | 0 |
| `EVIDENCE-DATA-002` | operational evidence | `scripts/validate_operational_evidence.py:217` | 0 |
| `EVIDENCE-DATE-001` | operational evidence | `scripts/validate_operational_evidence.py:266` | 0 |
| `EVIDENCE-PROVIDER-001` | operational evidence | `scripts/validate_operational_evidence.py:505` | 0 |
| `EVIDENCE-PROVIDER-002` | operational evidence | `scripts/validate_operational_evidence.py:507` | 0 |
| `EVIDENCE-PROVIDER-003` | operational evidence | `scripts/validate_operational_evidence.py:510` | 0 |
| `EVIDENCE-PROVIDER-004` | operational evidence | `scripts/validate_operational_evidence.py:516` | 0 |
| `EVIDENCE-PROVIDER-005` | operational evidence | `scripts/validate_operational_evidence.py:520` | 0 |
| `EVIDENCE-PROVIDER-006` | operational evidence | `scripts/validate_operational_evidence.py:525` | 0 |
| `EVIDENCE-PROVIDER-007` | operational evidence | `scripts/validate_operational_evidence.py:528` | 0 |
| `EVIDENCE-PROVIDER-008` | operational evidence | `scripts/validate_operational_evidence.py:530` | 0 |
| `EVIDENCE-PROVIDER-009` | operational evidence | `scripts/validate_operational_evidence.py:532` | 0 |
| `EVIDENCE-PROVIDER-010` | operational evidence | `scripts/validate_operational_evidence.py:536` | 0 |
| `EVIDENCE-PROVIDER-011` | operational evidence | `scripts/test/operational_evidence.py:383` | 2 |
| `EVIDENCE-PROVIDER-012` | operational evidence | `scripts/test/operational_evidence.py:440` | 1 |
| `EVIDENCE-PROVIDER-013` | operational evidence | `scripts/validate_operational_evidence.py:548` | 0 |
| `EVIDENCE-PROVIDER-014` | operational evidence | `scripts/test/operational_evidence.py:329` | 1 |
| `EVIDENCE-PROVIDER-015` | operational evidence | `scripts/validate_operational_evidence.py:553` | 0 |
| `EVIDENCE-PROVIDER-016` | operational evidence | `scripts/validate_operational_evidence.py:555` | 0 |
| `EVIDENCE-PROVIDER-017` | operational evidence | `scripts/validate_operational_evidence.py:561` | 0 |
| `EVIDENCE-PROVIDER-018` | operational evidence | `scripts/validate_operational_evidence.py:567` | 0 |
| `EVIDENCE-PROVIDER-019` | operational evidence | `scripts/validate_operational_evidence.py:582` | 0 |
| `EVIDENCE-PROVIDER-020` | operational evidence | `scripts/test/operational_evidence.py:335` | 1 |
| `EVIDENCE-PROVIDER-021` | operational evidence | `scripts/test/operational_evidence.py:230` | 3 |
| `EVIDENCE-PROVIDER-022` | operational evidence | `scripts/validate_operational_evidence.py:557` | 1 |
| `EVIDENCE-PROVIDER-023` | operational evidence | `scripts/test/operational_evidence.py:367` | 1 |
| `EVIDENCE-PROVIDER-024` | operational evidence | `scripts/validate_operational_evidence.py:614` | 0 |
| `EVIDENCE-PROVIDER-025` | operational evidence | `scripts/test/operational_evidence.py:341` | 1 |
| `EVIDENCE-PROVIDER-026` | operational evidence | `scripts/validate_operational_evidence.py:624` | 0 |
| `EVIDENCE-PROVIDER-027` | operational evidence | `scripts/validate_operational_evidence.py:627` | 0 |
| `EVIDENCE-PROVIDER-028` | operational evidence | `scripts/validate_operational_evidence.py:651` | 0 |
| `EVIDENCE-SOURCE-001` | operational evidence | `scripts/validate_operational_evidence.py:238` | 0 |
| `EVIDENCE-SOURCE-002` | operational evidence | `scripts/validate_operational_evidence.py:243` | 0 |
| `EVIDENCE-SOURCE-003` | operational evidence | `scripts/validate_operational_evidence.py:250` | 0 |
| `EVIDENCE-SOURCE-004` | operational evidence | `scripts/test/operational_evidence.py:299` | 1 |
| `EVIDENCE-SOURCE-005` | operational evidence | `scripts/validate_operational_evidence.py:257` | 0 |
| `MINCO-AGENT-CONTEXT-OPERATION-ABSENT` | agent | `crates/minco-cli/src/agent_cmd.rs:531` | 1 |
| `MINCO-AGENT-CONTEXT-TASK-ABSENT` | agent | `crates/minco-cli/src/agent_cmd.rs:573` | 1 |
| `MINCO-ARCH-001` | arch | `crates/minco-cli/src/architecture.rs:132` | 0 |
| `MINCO-ARCH-002` | arch | `crates/minco-cli/src/architecture.rs:133` | 0 |
| `MINCO-ARCH-003` | arch | `crates/minco-cli/src/architecture.rs:134` | 0 |
| `MINCO-AUTH-001` | auth | `crates/minco-plan/src/model.rs:546` | 0 |
| `MINCO-AWS-001` | aws | `crates/minco-plan/src/model.rs:1309` | 1 |
| `MINCO-AWS-002` | aws | `crates/minco-plan/src/model.rs:1325` | 1 |
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
| `MINCO-COST-001` | cost | `crates/minco-plan/src/model.rs:640` | 0 |
| `MINCO-COST-002` | cost | `crates/minco-plan/src/model.rs:654` | 1 |
| `MINCO-COST-003` | cost | `crates/minco-plan/src/model.rs:720` | 0 |
| `MINCO-COST-004` | cost | `crates/minco-plan/src/model.rs:726` | 0 |
| `MINCO-COST-005` | cost | `crates/minco-plan/src/model.rs:771` | 1 |
| `MINCO-COST-006` | cost | `crates/minco-plan/src/model.rs:634` | 0 |
| `MINCO-COST-007` | cost | `crates/minco-plan/src/model.rs:2449` | 1 |
| `MINCO-COST-008` | cost | `crates/minco-plan/src/model.rs:2478` | 1 |
| `MINCO-COST-009` | cost | `crates/minco-plan/src/model.rs:675` | 2 |
| `MINCO-COST-010` | cost | `crates/minco-plan/src/model.rs:710` | 0 |
| `MINCO-DB-001` | db | `crates/minco-plan/src/model.rs:783` | 0 |
| `MINCO-DB-002` | db | `crates/minco-plan/src/model.rs:789` | 0 |
| `MINCO-DB-003` | db | `crates/minco-plan/src/model.rs:808` | 0 |
| `MINCO-DB-004` | db | `crates/minco-plan/src/model.rs:2521` | 2 |
| `MINCO-DB-005` | db | `crates/minco-plan/src/model.rs:824` | 0 |
| `MINCO-DYNAMODB-001` | dynamodb | `crates/minco-plan/src/model.rs:1364` | 0 |
| `MINCO-DYNAMODB-002` | dynamodb | `crates/minco-plan/src/model.rs:1370` | 1 |
| `MINCO-DYNAMODB-003` | dynamodb | `crates/minco-plan/src/model.rs:1380` | 1 |
| `MINCO-DYNAMODB-004` | dynamodb | `crates/minco-plan/src/model.rs:1386` | 0 |
| `MINCO-DYNAMODB-005` | dynamodb | `crates/minco-plan/src/model.rs:1435` | 0 |
| `MINCO-DYNAMODB-006` | dynamodb | `crates/minco-plan/src/model.rs:1444` | 0 |
| `MINCO-DYNAMODB-007` | dynamodb | `crates/minco-plan/src/model.rs:1396` | 1 |
| `MINCO-DYNAMODB-008` | dynamodb | `crates/minco-plan/src/model.rs:1407` | 1 |
| `MINCO-HANDOVER-001` | handover | `crates/minco-cli/src/handover_cmd.rs:256` | 14 |
| `MINCO-HANDOVER-002` | handover | `crates/minco-cli/src/handover_cmd.rs:296` | 3 |
| `MINCO-HANDOVER-003` | handover | `crates/minco-cli/src/handover_cmd.rs:335` | 37 |
| `MINCO-HANDOVER-004` | handover | `crates/minco-cli/src/handover_cmd.rs:643` | 5 |
| `MINCO-HANDOVER-005` | handover | `crates/minco-cli/src/handover_cmd.rs:264` | 4 |
| `MINCO-HANDOVER-006` | handover | `crates/minco-cli/src/handover_cmd.rs:522` | 5 |
| `MINCO-HANDOVER-007` | handover | `crates/minco-cli/src/handover_cmd.rs:499` | 2 |
| `MINCO-HTTP-001` | http | `crates/minco-plan/src/model.rs:598` | 0 |
| `MINCO-HTTP-002` | http | `crates/minco-plan/src/model.rs:603` | 0 |
| `MINCO-HTTP-003` | http | `crates/minco-plan/src/model.rs:607` | 0 |
| `MINCO-HTTP-004` | http | `crates/minco-plan/src/model.rs:615` | 0 |
| `MINCO-HTTP-005` | http | `crates/minco-plan/src/model.rs:622` | 0 |
| `MINCO-HTTP-006` | http | `crates/minco-plan/src/model.rs:627` | 0 |
| `MINCO-IAM-001` | iam | `crates/minco-plan/src/model.rs:591` | 1 |
| `MINCO-PERF-001` | perf | `crates/minco-plan/src/model.rs:737` | 0 |
| `MINCO-PERF-002` | perf | `crates/minco-plan/src/model.rs:748` | 0 |
| `MINCO-PERF-003` | perf | `crates/minco-cli/src/main.rs:6176` | 0 |
| `MINCO-PERF-004` | perf | `crates/minco-plan/src/model.rs:701` | 0 |
| `MINCO-PLAN-001` | plan | `crates/minco-plan/src/model.rs:433` | 0 |
| `MINCO-PLAN-002` | plan | `crates/minco-plan/src/model.rs:551` | 0 |
| `MINCO-PLAN-003` | plan | `crates/minco-plan/src/model.rs:2364` | 1 |
| `MINCO-PLAN-004` | plan | `crates/minco-plan/src/model.rs:558` | 0 |
| `MINCO-PLAN-005` | plan | `crates/minco-plan/src/model.rs:568` | 1 |
| `MINCO-PLAN-010` | plan | `crates/minco-plan/src/model.rs:864` | 2 |
| `MINCO-PLAN-011` | plan | `crates/minco-plan/src/model.rs:873` | 1 |
| `MINCO-PLAN-012` | plan | `crates/minco-plan/src/model.rs:885` | 0 |
| `MINCO-PLAN-013` | plan | `crates/minco-plan/src/model.rs:900` | 1 |
| `MINCO-PLAN-014` | plan | `crates/minco-plan/src/model.rs:991` | 1 |
| `MINCO-PLAN-015` | plan | `crates/minco-plan/src/model.rs:1008` | 5 |
| `MINCO-PLAN-016` | plan | `crates/minco-plan/src/model.rs:1001` | 2 |
| `MINCO-PLAN-017` | plan | `crates/minco-plan/src/model.rs:1213` | 0 |
| `MINCO-PLAN-018` | plan | `crates/minco-plan/src/model.rs:1228` | 4 |
| `MINCO-PLAN-INGRESS-001` | plan | `crates/minco-plan/src/model.rs:2392` | 3 |
| `MINCO-PLAN-INGRESS-002` | plan | `crates/minco-plan/src/model.rs:2420` | 5 |
| `MINCO-PLAN-MIGRATE-001` | plan | `crates/minco-plan/src/model.rs:391` | 1 |
| `MINCO-PLAN-MIGRATE-002` | plan | `crates/minco-plan/src/model.rs:398` | 2 |
| `MINCO-PLAN-MIGRATE-003` | plan | `crates/minco-plan/src/model.rs:423` | 0 |
| `MINCO-PREVIEW-001` | preview | `crates/minco-plan/src/model.rs:501` | 0 |
| `MINCO-PREVIEW-002` | preview | `crates/minco-plan/src/model.rs:507` | 0 |
| `MINCO-PREVIEW-003` | preview | `crates/minco-plan/src/model.rs:525` | 1 |
| `MINCO-PREVIEW-004` | preview | `crates/minco-plan/src/model.rs:531` | 0 |
| `MINCO-PREVIEW-006` | preview | `crates/minco-plan/src/model.rs:538` | 1 |
| `MINCO-REALTIME-001` | realtime | `crates/minco-plan/src/model.rs:463` | 0 |
| `MINCO-REALTIME-002` | realtime | `crates/minco-plan/src/model.rs:469` | 0 |
| `MINCO-SCHEDULE-001` | schedule | `crates/minco-plan/src/model.rs:1129` | 1 |
| `MINCO-SCHEDULE-002` | schedule | `crates/minco-plan/src/model.rs:1135` | 1 |
| `MINCO-SCHEDULE-003` | schedule | `crates/minco-plan/src/model.rs:576` | 1 |
| `MINCO-SCHEDULE-004` | schedule | `crates/minco-plan/src/model.rs:1146` | 1 |
| `MINCO-SCHEDULE-005` | schedule | `crates/minco-plan/src/model.rs:1161` | 0 |
| `MINCO-SCHEDULE-006` | schedule | `crates/minco-plan/src/model.rs:1170` | 0 |
| `MINCO-SQS-001` | sqs | `crates/minco-plan/src/model.rs:1103` | 1 |
| `MINCO-SQS-002` | sqs | `crates/minco-plan/src/model.rs:1057` | 1 |
| `MINCO-SQS-003` | sqs | `crates/minco-plan/src/model.rs:935` | 1 |
| `MINCO-SQS-004` | sqs | `crates/minco-plan/src/model.rs:928` | 1 |
| `MINCO-SQS-005` | sqs | `crates/minco-plan/src/model.rs:955` | 0 |
| `MINCO-SQS-006` | sqs | `crates/minco-plan/src/model.rs:975` | 1 |
| `MINCO-SQS-007` | sqs | `crates/minco-plan/src/model.rs:1070` | 1 |
| `MINCO-SQS-008` | sqs | `crates/minco-plan/src/model.rs:1081` | 1 |
| `MINCO-SQS-009` | sqs | `crates/minco-plan/src/model.rs:1093` | 0 |
| `MINCO-SQS-010` | sqs | `crates/minco-plan/src/model.rs:906` | 0 |
| `MINCO-SQS-011` | sqs | `crates/minco-plan/src/model.rs:915` | 0 |
| `MINCO-SQS-012` | sqs | `crates/minco-plan/src/model.rs:1203` | 1 |
| `MINCO-STATIC-001` | static | `crates/minco-plan/src/model.rs:456` | 0 |
| `PERF-BASELINE-001` | performance evidence | `scripts/validate_operational_evidence.py:386` | 0 |
| `PERF-BASELINE-002` | performance evidence | `scripts/validate_operational_evidence.py:389` | 0 |
| `PERF-BASELINE-003` | performance evidence | `scripts/test/operational_evidence.py:308` | 1 |
| `PERF-BASELINE-004` | performance evidence | `scripts/validate_operational_evidence.py:399` | 0 |
| `PERF-BASELINE-005` | performance evidence | `scripts/validate_operational_evidence.py:401` | 0 |
| `PERF-BASELINE-006` | performance evidence | `scripts/validate_operational_evidence.py:403` | 0 |
| `PERF-BASELINE-007` | performance evidence | `scripts/test/operational_evidence.py:229` | 1 |
| `PERF-BASELINE-008` | performance evidence | `scripts/validate_operational_evidence.py:407` | 0 |
| `PERF-BASELINE-009` | performance evidence | `scripts/validate_operational_evidence.py:414` | 0 |
| `PERF-BASELINE-010` | performance evidence | `scripts/validate_operational_evidence.py:417` | 0 |
| `PERF-BASELINE-011` | performance evidence | `scripts/test/operational_evidence.py:406` | 1 |
| `PERF-BASELINE-012` | performance evidence | `scripts/test/operational_evidence.py:406` | 1 |
| `PERF-BASELINE-013` | performance evidence | `scripts/validate_operational_evidence.py:464` | 0 |
| `PERF-BASELINE-014` | performance evidence | `scripts/test/operational_evidence.py:406` | 1 |
| `PERF-BASELINE-015` | performance evidence | `scripts/validate_operational_evidence.py:474` | 0 |
| `PERF-BASELINE-016` | performance evidence | `scripts/test/operational_evidence.py:416` | 1 |
| `PERF-COMPARE-001` | performance evidence | `scripts/validate_operational_evidence.py:482` | 0 |
| `PERF-COMPARE-002` | performance evidence | `scripts/validate_operational_evidence.py:489` | 0 |
| `PERF-COMPARE-003` | performance evidence | `scripts/validate_operational_evidence.py:493` | 0 |
| `PERF-COMPARE-004` | performance evidence | `scripts/validate_operational_evidence.py:495` | 0 |
| `PERF-COMPARE-005` | performance evidence | `scripts/validate_operational_evidence.py:501` | 0 |
| `PERF-DATA-001` | performance evidence | `scripts/test/operational_evidence.py:314` | 1 |
| `PERF-DATA-002` | performance evidence | `scripts/validate_operational_evidence.py:229` | 0 |
| `PERF-MEASURE-001` | performance evidence | `scripts/validate_operational_evidence.py:326` | 0 |
| `PERF-MEASURE-002` | performance evidence | `scripts/validate_operational_evidence.py:330` | 0 |
| `PERF-MEASURE-003` | performance evidence | `scripts/validate_operational_evidence.py:332` | 0 |
| `PERF-MEASURE-004` | performance evidence | `scripts/validate_operational_evidence.py:336` | 0 |
| `PERF-MEASURE-005` | performance evidence | `scripts/validate_operational_evidence.py:342` | 0 |
| `PERF-MEASURE-006` | performance evidence | `scripts/validate_operational_evidence.py:349` | 0 |
| `PERF-MEASURE-007` | performance evidence | `scripts/validate_operational_evidence.py:351` | 0 |
| `PERF-MEASURE-008` | performance evidence | `scripts/validate_operational_evidence.py:353` | 0 |
| `PERF-MEASURE-009` | performance evidence | `scripts/validate_operational_evidence.py:356` | 0 |
| `PERF-MEASURE-010` | performance evidence | `scripts/validate_operational_evidence.py:374` | 0 |
| `PERF-MEASURE-011` | performance evidence | `scripts/test/operational_evidence.py:423` | 1 |
| `PERF-MEASURE-012` | performance evidence | `scripts/test/operational_evidence.py:429` | 1 |
| `PERF-POLICY-001` | performance evidence | `scripts/validate_operational_evidence.py:277` | 0 |
| `PERF-POLICY-002` | performance evidence | `scripts/test/operational_evidence.py:322` | 1 |
| `PERF-POLICY-003` | performance evidence | `scripts/validate_operational_evidence.py:301` | 0 |
| `PERF-POLICY-004` | performance evidence | `scripts/validate_operational_evidence.py:304` | 0 |
| `PERF-POLICY-005` | performance evidence | `scripts/validate_operational_evidence.py:309` | 0 |
| `PERF-POLICY-006` | performance evidence | `scripts/validate_operational_evidence.py:313` | 0 |
| `PERF-POLICY-007` | performance evidence | `scripts/validate_operational_evidence.py:317` | 0 |
| `PERF-POLICY-008` | performance evidence | `scripts/validate_operational_evidence.py:320` | 0 |
| `PERF-POLICY-009` | performance evidence | `scripts/validate_operational_evidence.py:322` | 0 |
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
| `PUBLISH-021` | publication | `scripts/test/publish_validation.py:131` | 3 |
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
| `PUBLISH-071` | publication | `scripts/test/publish_validation.py:453` | 2 |
| `PUBLISH-072` | publication | `scripts/test/publish_validation.py:430` | 1 |
| `PUBLISH-073` | publication | `scripts/validate_publish.py:495` | 0 |
| `PUBLISH-074` | publication | `scripts/test/publish_validation.py:377` | 3 |
| `STATIC-001` | repository truth | `scripts/validate_static.py:144` | 0 |
| `STATIC-ARCH-001` | repository truth | `scripts/validate_static.py:990` | 0 |
| `STATIC-ARCH-002` | repository truth | `scripts/validate_static.py:996` | 0 |
| `STATIC-ARCH-003` | repository truth | `scripts/validate_static.py:1002` | 0 |
| `STATIC-ARCH-004` | repository truth | `scripts/validate_static.py:1005` | 0 |
| `STATIC-BUDGET-001` | repository truth | `scripts/validate_static.py:685` | 0 |
| `STATIC-BUDGET-002` | repository truth | `scripts/validate_static.py:463` | 0 |
| `STATIC-BUDGET-003` | repository truth | `scripts/validate_static.py:320` | 0 |
| `STATIC-BUDGET-004` | repository truth | `scripts/test/repository_truth.py:465` | 3 |
| `STATIC-BUDGET-005` | repository truth | `scripts/validate_static.py:426` | 0 |
| `STATIC-BUDGET-006` | repository truth | `scripts/test/repository_truth.py:535` | 1 |
| `STATIC-BUDGET-007` | repository truth | `scripts/test/repository_truth.py:552` | 1 |
| `STATIC-CARGO-001` | repository truth | `scripts/validate_static.py:175` | 0 |
| `STATIC-CARGO-002` | repository truth | `scripts/validate_static.py:178` | 0 |
| `STATIC-CARGO-003` | repository truth | `scripts/validate_static.py:184` | 0 |
| `STATIC-CARGO-004` | repository truth | `scripts/validate_static.py:190` | 0 |
| `STATIC-CARGO-005` | repository truth | `scripts/validate_static.py:192` | 0 |
| `STATIC-CARGO-006` | repository truth | `scripts/validate_static.py:198` | 0 |
| `STATIC-CARGO-007` | repository truth | `scripts/validate_static.py:203` | 0 |
| `STATIC-CARGO-008` | repository truth | `scripts/validate_static.py:206` | 0 |
| `STATIC-CONTRACT-001` | repository truth | `scripts/validate_static.py:833` | 0 |
| `STATIC-CONTRACT-002` | repository truth | `scripts/validate_static.py:838` | 0 |
| `STATIC-CONTRACT-003` | repository truth | `scripts/validate_static.py:846` | 0 |
| `STATIC-CONTRACT-004` | repository truth | `scripts/validate_static.py:849` | 0 |
| `STATIC-CONTRACT-005` | repository truth | `scripts/validate_static.py:853` | 0 |
| `STATIC-CONTRACT-006` | repository truth | `scripts/validate_static.py:855` | 0 |
| `STATIC-CONTRACT-007` | repository truth | `scripts/validate_static.py:894` | 0 |
| `STATIC-CONTRACT-008` | repository truth | `scripts/validate_static.py:943` | 0 |
| `STATIC-CONTRACT-009` | repository truth | `scripts/validate_static.py:951` | 0 |
| `STATIC-CONTRACT-010` | repository truth | `scripts/validate_static.py:956` | 0 |
| `STATIC-CONTRACT-011` | repository truth | `scripts/validate_static.py:960` | 0 |
| `STATIC-CONTRACT-012` | repository truth | `scripts/validate_static.py:966` | 0 |
| `STATIC-CONTRACT-013` | repository truth | `scripts/validate_static.py:971` | 0 |
| `STATIC-CONTRACT-014` | repository truth | `scripts/validate_static.py:869` | 0 |
| `STATIC-CONTRACT-015` | repository truth | `scripts/validate_static.py:897` | 0 |
| `STATIC-CONTRACT-016` | repository truth | `scripts/validate_static.py:912` | 0 |
| `STATIC-CONTRACT-017` | repository truth | `scripts/validate_static.py:862` | 0 |
| `STATIC-CONTRACT-019` | repository truth | `scripts/validate_static.py:930` | 0 |
| `STATIC-CONTRACT-020` | repository truth | `scripts/validate_static.py:905` | 0 |
| `STATIC-CONTRACT-021` | repository truth | `scripts/validate_static.py:881` | 0 |
| `STATIC-COST-001` | repository truth | `scripts/validate_static.py:1216` | 0 |
| `STATIC-COST-002` | repository truth | `scripts/validate_static.py:1219` | 0 |
| `STATIC-COST-003` | repository truth | `scripts/validate_static.py:1225` | 0 |
| `STATIC-COST-004` | repository truth | `scripts/validate_static.py:1227` | 0 |
| `STATIC-COST-005` | repository truth | `scripts/validate_static.py:1229` | 0 |
| `STATIC-DATA-001` | repository truth | `scripts/validate_static.py:162` | 0 |
| `STATIC-DB-001` | repository truth | `scripts/validate_static.py:1250` | 0 |
| `STATIC-GRAPH-001` | repository truth | `scripts/validate_static.py:1101` | 0 |
| `STATIC-HTTP-001` | repository truth | `scripts/validate_static.py:1232` | 0 |
| `STATIC-HTTP-002` | repository truth | `scripts/validate_static.py:1244` | 0 |
| `STATIC-MEASURE-001` | repository truth | `scripts/validate_static.py:341` | 0 |
| `STATIC-MEASURE-002` | repository truth | `scripts/test/repository_truth.py:496` | 1 |
| `STATIC-MEASURE-003` | repository truth | `scripts/validate_static.py:438` | 0 |
| `STATIC-MEASURE-004` | repository truth | `scripts/test/repository_truth.py:503` | 1 |
| `STATIC-MEASURE-005` | repository truth | `scripts/test/repository_truth.py:542` | 1 |
| `STATIC-PLACEHOLDER-001` | repository truth | `scripts/validate_static.py:1309` | 0 |
| `STATIC-PLAN-000` | repository truth | `scripts/validate_static.py:1197` | 0 |
| `STATIC-PLAN-001` | repository truth | `scripts/validate_static.py:1203` | 0 |
| `STATIC-PLAN-002` | repository truth | `scripts/validate_static.py:1210` | 0 |
| `STATIC-PLAN-003` | repository truth | `scripts/validate_static.py:1213` | 0 |
| `STATIC-PLUGIN-001` | repository truth | `scripts/validate_static.py:1021` | 0 |
| `STATIC-PLUGIN-002` | repository truth | `scripts/validate_static.py:1023` | 0 |
| `STATIC-PLUGIN-003` | repository truth | `scripts/validate_static.py:1028` | 0 |
| `STATIC-PLUGIN-004` | repository truth | `scripts/validate_static.py:1034` | 0 |
| `STATIC-PLUGIN-005` | repository truth | `scripts/validate_static.py:1037` | 0 |
| `STATIC-PYTHON-001` | repository truth | `scripts/validate_static.py:1167` | 0 |
| `STATIC-QUALITY-001` | repository truth | `scripts/validate_static.py:1123` | 0 |
| `STATIC-QUALITY-002` | repository truth | `scripts/validate_static.py:1125` | 0 |
| `STATIC-QUALITY-003` | repository truth | `scripts/validate_static.py:1129` | 0 |
| `STATIC-QUALITY-004` | repository truth | `scripts/validate_static.py:1142` | 0 |
| `STATIC-ROADMAP-001` | repository truth | `scripts/validate_static.py:1052` | 0 |
| `STATIC-ROADMAP-002` | repository truth | `scripts/validate_static.py:1056` | 0 |
| `STATIC-RUST-001` | repository truth | `scripts/validate_static.py:1155` | 0 |
| `STATIC-SAM-001` | repository truth | `scripts/validate_static.py:1262` | 0 |
| `STATIC-SAM-002` | repository truth | `scripts/validate_static.py:1280` | 0 |
| `STATIC-SAM-003` | repository truth | `scripts/validate_static.py:1283` | 0 |
| `STATIC-SAM-004` | repository truth | `scripts/validate_static.py:1287` | 0 |
| `STATIC-SAM-005` | repository truth | `scripts/validate_static.py:1274` | 0 |
| `STATIC-SHELL-001` | repository truth | `scripts/validate_static.py:1178` | 0 |
| `STATIC-SHELL-002` | repository truth | `scripts/validate_static.py:1180` | 0 |
| `STATIC-TASK-001` | repository truth | `scripts/validate_static.py:1061` | 0 |
| `STATIC-TASK-002` | repository truth | `scripts/validate_static.py:1067` | 0 |
| `STATIC-TASK-003` | repository truth | `scripts/validate_static.py:1071` | 0 |
| `STATIC-TASK-004` | repository truth | `scripts/validate_static.py:1074` | 0 |
| `STATIC-TASK-005` | repository truth | `scripts/validate_static.py:1078` | 0 |
| `STATIC-TASK-006` | repository truth | `scripts/validate_static.py:1080` | 0 |
| `STATIC-TASK-007` | repository truth | `scripts/validate_static.py:1083` | 0 |
| `STATIC-TASK-008` | repository truth | `scripts/validate_static.py:1085` | 0 |
| `STATIC-TASK-009` | repository truth | `scripts/validate_static.py:1089` | 0 |
| `STATIC-TRUTH-ADOPTION-001` | repository truth | `scripts/validate_static.py:740` | 0 |
| `STATIC-TRUTH-CATALOG-001` | repository truth | `scripts/validate_static.py:584` | 0 |
| `STATIC-TRUTH-CATALOG-002` | repository truth | `scripts/test/repository_truth.py:418` | 1 |
| `STATIC-TRUTH-CATALOG-003` | repository truth | `scripts/validate_static.py:571` | 0 |
| `STATIC-TRUTH-CATALOG-004` | repository truth | `scripts/validate_static.py:573` | 0 |
| `STATIC-TRUTH-CATALOG-005` | repository truth | `scripts/validate_static.py:575` | 0 |
| `STATIC-TRUTH-CATALOG-006` | repository truth | `scripts/validate_static.py:642` | 0 |
| `STATIC-TRUTH-DESCRIPTOR-001` | repository truth | `scripts/validate_static.py:614` | 0 |
| `STATIC-TRUTH-DESCRIPTOR-002` | repository truth | `scripts/validate_static.py:620` | 0 |
| `STATIC-TRUTH-DESCRIPTOR-003` | repository truth | `scripts/validate_static.py:627` | 0 |
| `STATIC-TRUTH-DOCS-001` | repository truth | `scripts/test/current_product_truth.py:64` | 6 |
| `STATIC-TRUTH-FACADE-001` | repository truth | `scripts/validate_static.py:596` | 0 |
| `STATIC-TRUTH-FACADE-002` | repository truth | `scripts/validate_static.py:602` | 0 |
| `STATIC-TRUTH-FACADE-003` | repository truth | `scripts/validate_static.py:659` | 0 |
| `STATIC-TRUTH-FACADE-004` | repository truth | `scripts/validate_static.py:673` | 0 |
| `STATIC-TRUTH-PACKAGES-001` | repository truth | `scripts/validate_static.py:492` | 0 |
| `STATIC-TRUTH-PACKAGES-002` | repository truth | `scripts/validate_static.py:498` | 0 |
| `STATIC-TRUTH-PACKAGES-003` | repository truth | `scripts/test/repository_truth.py:386` | 2 |
| `STATIC-TRUTH-PACKAGES-004` | repository truth | `scripts/test/repository_truth.py:376` | 1 |
| `STATIC-TRUTH-PLAN-001` | repository truth | `scripts/validate_static.py:752` | 0 |
| `STATIC-TRUTH-PLAN-002` | repository truth | `scripts/validate_static.py:758` | 0 |
| `STATIC-TRUTH-PUBLISHED-001` | repository truth | `scripts/validate_static.py:233` | 0 |
| `STATIC-TRUTH-PUBLISHED-002` | repository truth | `scripts/test/repository_truth.py:397` | 2 |
| `STATIC-TRUTH-PUBLISHED-003` | repository truth | `scripts/test/repository_truth.py:408` | 1 |
| `STATIC-TRUTH-RELEASE-001` | repository truth | `scripts/test/repository_truth.py:301` | 1 |
| `STATIC-TRUTH-RELEASE-002` | repository truth | `scripts/test/repository_truth.py:311` | 1 |
| `STATIC-TRUTH-RELEASE-003` | repository truth | `scripts/test/repository_truth.py:321` | 1 |
| `STATIC-TRUTH-RELEASE-004` | repository truth | `scripts/validate_static.py:263` | 0 |
| `STATIC-TRUTH-RELEASE-005` | repository truth | `scripts/test/current_product_truth.py:88` | 1 |
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
