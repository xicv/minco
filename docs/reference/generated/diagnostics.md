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

Declared codes: `554`.

| Code | Family | First declaration | Additional references |
|---|---|---|---:|
| `ASSURANCE-COMMAND-001` | deployment assurance | `scripts/quality_assurance.py:583` | 0 |
| `ASSURANCE-COST-001` | deployment assurance | `scripts/validate_deployment_assurance.py:339` | 0 |
| `ASSURANCE-COST-002` | deployment assurance | `scripts/validate_deployment_assurance.py:388` | 0 |
| `ASSURANCE-COST-003` | deployment assurance | `scripts/validate_deployment_assurance.py:394` | 0 |
| `ASSURANCE-COST-004` | deployment assurance | `scripts/validate_deployment_assurance.py:400` | 0 |
| `ASSURANCE-COVERAGE-001` | deployment assurance | `scripts/quality_assurance.py:349` | 2 |
| `ASSURANCE-COVERAGE-002` | deployment assurance | `scripts/quality_assurance.py:361` | 1 |
| `ASSURANCE-DATA-001` | deployment assurance | `scripts/validate_deployment_assurance.py:128` | 0 |
| `ASSURANCE-DATE-001` | deployment assurance | `scripts/quality_assurance.py:1001` | 1 |
| `ASSURANCE-DATE-002` | deployment assurance | `scripts/quality_assurance.py:483` | 0 |
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
| `ASSURANCE-MUTATION-001` | deployment assurance | `scripts/quality_assurance.py:384` | 2 |
| `ASSURANCE-NEXTEST-001` | deployment assurance | `scripts/quality_assurance.py:395` | 1 |
| `ASSURANCE-PATH-001` | deployment assurance | `scripts/quality_assurance.py:1100` | 9 |
| `ASSURANCE-PATH-002` | deployment assurance | `scripts/quality_assurance.py:759` | 2 |
| `ASSURANCE-PATH-003` | deployment assurance | `scripts/quality_assurance.py:1065` | 0 |
| `ASSURANCE-PATH-004` | deployment assurance | `scripts/quality_assurance.py:105` | 0 |
| `ASSURANCE-PERF-001` | deployment assurance | `scripts/validate_deployment_assurance.py:407` | 0 |
| `ASSURANCE-PERFORMANCE-001` | deployment assurance | `scripts/quality_assurance.py:410` | 1 |
| `ASSURANCE-PERFORMANCE-002` | deployment assurance | `scripts/quality_assurance.py:523` | 1 |
| `ASSURANCE-POLICY-001` | deployment assurance | `scripts/quality_assurance.py:199` | 1 |
| `ASSURANCE-POLICY-002` | deployment assurance | `scripts/quality_assurance.py:202` | 1 |
| `ASSURANCE-POLICY-003` | deployment assurance | `scripts/quality_assurance.py:206` | 1 |
| `ASSURANCE-POLICY-004` | deployment assurance | `scripts/quality_assurance.py:212` | 0 |
| `ASSURANCE-POLICY-005` | deployment assurance | `scripts/quality_assurance.py:481` | 0 |
| `ASSURANCE-PROFILE-001` | deployment assurance | `scripts/validate_deployment_assurance.py:212` | 0 |
| `ASSURANCE-PROFILE-002` | deployment assurance | `scripts/validate_deployment_assurance.py:229` | 0 |
| `ASSURANCE-PROFILE-003` | deployment assurance | `scripts/validate_deployment_assurance.py:237` | 0 |
| `ASSURANCE-PROFILE-004` | deployment assurance | `scripts/validate_deployment_assurance.py:246` | 0 |
| `ASSURANCE-PROFILE-005` | deployment assurance | `scripts/validate_deployment_assurance.py:253` | 0 |
| `ASSURANCE-PROFILE-006` | deployment assurance | `scripts/validate_deployment_assurance.py:296` | 0 |
| `ASSURANCE-PROFILE-007` | deployment assurance | `scripts/validate_deployment_assurance.py:596` | 0 |
| `ASSURANCE-RECEIPT-001` | deployment assurance | `scripts/quality_assurance.py:270` | 1 |
| `ASSURANCE-RECEIPT-002` | deployment assurance | `scripts/quality_assurance.py:273` | 0 |
| `ASSURANCE-RECEIPT-003` | deployment assurance | `scripts/quality_assurance.py:309` | 2 |
| `ASSURANCE-RECEIPT-004` | deployment assurance | `scripts/quality_assurance.py:323` | 4 |
| `ASSURANCE-RECEIPT-005` | deployment assurance | `scripts/quality_assurance.py:285` | 0 |
| `ASSURANCE-RECEIPT-006` | deployment assurance | `scripts/quality_assurance.py:292` | 0 |
| `ASSURANCE-RECEIPT-007` | deployment assurance | `scripts/quality_assurance.py:458` | 1 |
| `ASSURANCE-RECEIPT-008` | deployment assurance | `scripts/quality_assurance.py:306` | 1 |
| `ASSURANCE-RECEIPT-009` | deployment assurance | `scripts/quality_assurance.py:220` | 2 |
| `ASSURANCE-RECEIPT-010` | deployment assurance | `scripts/quality_assurance.py:237` | 4 |
| `ASSURANCE-RECEIPT-011` | deployment assurance | `scripts/quality_assurance.py:268` | 1 |
| `ASSURANCE-RECEIPT-012` | deployment assurance | `scripts/quality_assurance.py:414` | 3 |
| `ASSURANCE-RECEIPT-013` | deployment assurance | `scripts/quality_assurance.py:501` | 6 |
| `ASSURANCE-RECOVERY-001` | deployment assurance | `scripts/validate_deployment_assurance.py:413` | 0 |
| `ASSURANCE-SCHEMA-001` | deployment assurance | `scripts/validate_deployment_assurance.py:133` | 0 |
| `ASSURANCE-SCOPE-001` | deployment assurance | `scripts/validate_deployment_assurance.py:269` | 0 |
| `ASSURANCE-SEMVER-001` | deployment assurance | `scripts/quality_assurance.py:402` | 2 |
| `ASSURANCE-SEMVER-002` | deployment assurance | `scripts/quality_assurance.py:874` | 0 |
| `ASSURANCE-SEMVER-003` | deployment assurance | `scripts/quality_assurance.py:885` | 0 |
| `ASSURANCE-SOURCE-001` | deployment assurance | `scripts/quality_assurance.py:154` | 0 |
| `ASSURANCE-SOURCE-002` | deployment assurance | `scripts/quality_assurance.py:160` | 0 |
| `ASSURANCE-SOURCE-003` | deployment assurance | `scripts/quality_assurance.py:475` | 0 |
| `ASSURANCE-STATUS-001` | deployment assurance | `scripts/validate_deployment_assurance.py:262` | 0 |
| `ASSURANCE-TOOL-001` | deployment assurance | `scripts/quality_assurance.py:627` | 0 |
| `ASSURANCE-TRUTH-001` | deployment assurance | `scripts/validate_deployment_assurance.py:140` | 0 |
| `ASSURANCE-TRUTH-002` | deployment assurance | `scripts/validate_deployment_assurance.py:150` | 0 |
| `ASSURANCE-TRUTH-003` | deployment assurance | `scripts/validate_deployment_assurance.py:156` | 0 |
| `COST-REGRESSION-001` | cost evidence | `scripts/cost_regression.py:245` | 7 |
| `COST-REGRESSION-002` | cost evidence | `scripts/cost_regression.py:101` | 4 |
| `COST-REGRESSION-003` | cost evidence | `scripts/cost_regression.py:248` | 10 |
| `COST-REGRESSION-004` | cost evidence | `scripts/cost_regression.py:307` | 2 |
| `COST-REGRESSION-005` | cost evidence | `scripts/cost_regression.py:109` | 7 |
| `COST-REGRESSION-006` | cost evidence | `scripts/cost_regression.py:146` | 0 |
| `COST-REGRESSION-007` | cost evidence | `scripts/cost_regression.py:165` | 4 |
| `COST-REGRESSION-008` | cost evidence | `scripts/cost_regression.py:402` | 2 |
| `COST-REGRESSION-009` | cost evidence | `scripts/cost_regression.py:316` | 4 |
| `EVIDENCE-CAPABILITY-001` | operational evidence | `scripts/validate_operational_evidence.py:672` | 0 |
| `EVIDENCE-CAPABILITY-002` | operational evidence | `scripts/validate_operational_evidence.py:674` | 0 |
| `EVIDENCE-CAPABILITY-003` | operational evidence | `scripts/validate_operational_evidence.py:677` | 0 |
| `EVIDENCE-CAPABILITY-004` | operational evidence | `scripts/validate_operational_evidence.py:681` | 0 |
| `EVIDENCE-CAPABILITY-005` | operational evidence | `scripts/validate_operational_evidence.py:684` | 0 |
| `EVIDENCE-CAPABILITY-006` | operational evidence | `scripts/validate_operational_evidence.py:689` | 0 |
| `EVIDENCE-CAPABILITY-007` | operational evidence | `scripts/validate_operational_evidence.py:693` | 0 |
| `EVIDENCE-CAPABILITY-008` | operational evidence | `scripts/validate_operational_evidence.py:698` | 0 |
| `EVIDENCE-CAPABILITY-009` | operational evidence | `scripts/test/operational_evidence.py:486` | 1 |
| `EVIDENCE-CAPABILITY-010` | operational evidence | `scripts/validate_operational_evidence.py:704` | 0 |
| `EVIDENCE-CAPABILITY-011` | operational evidence | `scripts/validate_operational_evidence.py:708` | 0 |
| `EVIDENCE-CAPABILITY-012` | operational evidence | `scripts/test/operational_evidence.py:492` | 1 |
| `EVIDENCE-CAPABILITY-013` | operational evidence | `scripts/validate_operational_evidence.py:719` | 0 |
| `EVIDENCE-CAPABILITY-014` | operational evidence | `scripts/validate_operational_evidence.py:723` | 0 |
| `EVIDENCE-CAPABILITY-015` | operational evidence | `scripts/validate_operational_evidence.py:727` | 0 |
| `EVIDENCE-CAPABILITY-016` | operational evidence | `scripts/validate_operational_evidence.py:732` | 0 |
| `EVIDENCE-CAPABILITY-017` | operational evidence | `scripts/validate_operational_evidence.py:734` | 0 |
| `EVIDENCE-DATA-001` | operational evidence | `scripts/validate_operational_evidence.py:214` | 0 |
| `EVIDENCE-DATA-002` | operational evidence | `scripts/validate_operational_evidence.py:217` | 0 |
| `EVIDENCE-DATE-001` | operational evidence | `scripts/validate_operational_evidence.py:266` | 0 |
| `EVIDENCE-PROVIDER-001` | operational evidence | `scripts/validate_operational_evidence.py:510` | 0 |
| `EVIDENCE-PROVIDER-002` | operational evidence | `scripts/validate_operational_evidence.py:512` | 0 |
| `EVIDENCE-PROVIDER-003` | operational evidence | `scripts/validate_operational_evidence.py:515` | 0 |
| `EVIDENCE-PROVIDER-004` | operational evidence | `scripts/validate_operational_evidence.py:521` | 0 |
| `EVIDENCE-PROVIDER-005` | operational evidence | `scripts/validate_operational_evidence.py:525` | 0 |
| `EVIDENCE-PROVIDER-006` | operational evidence | `scripts/validate_operational_evidence.py:530` | 0 |
| `EVIDENCE-PROVIDER-007` | operational evidence | `scripts/validate_operational_evidence.py:533` | 0 |
| `EVIDENCE-PROVIDER-008` | operational evidence | `scripts/test/operational_evidence.py:473` | 1 |
| `EVIDENCE-PROVIDER-009` | operational evidence | `scripts/validate_operational_evidence.py:543` | 0 |
| `EVIDENCE-PROVIDER-010` | operational evidence | `scripts/validate_operational_evidence.py:547` | 0 |
| `EVIDENCE-PROVIDER-011` | operational evidence | `scripts/test/operational_evidence.py:397` | 2 |
| `EVIDENCE-PROVIDER-012` | operational evidence | `scripts/test/operational_evidence.py:497` | 1 |
| `EVIDENCE-PROVIDER-013` | operational evidence | `scripts/validate_operational_evidence.py:559` | 0 |
| `EVIDENCE-PROVIDER-014` | operational evidence | `scripts/test/operational_evidence.py:343` | 1 |
| `EVIDENCE-PROVIDER-015` | operational evidence | `scripts/validate_operational_evidence.py:564` | 0 |
| `EVIDENCE-PROVIDER-016` | operational evidence | `scripts/validate_operational_evidence.py:566` | 0 |
| `EVIDENCE-PROVIDER-017` | operational evidence | `scripts/validate_operational_evidence.py:572` | 0 |
| `EVIDENCE-PROVIDER-018` | operational evidence | `scripts/validate_operational_evidence.py:578` | 0 |
| `EVIDENCE-PROVIDER-019` | operational evidence | `scripts/validate_operational_evidence.py:593` | 0 |
| `EVIDENCE-PROVIDER-020` | operational evidence | `scripts/test/operational_evidence.py:349` | 1 |
| `EVIDENCE-PROVIDER-021` | operational evidence | `scripts/test/operational_evidence.py:239` | 3 |
| `EVIDENCE-PROVIDER-022` | operational evidence | `scripts/validate_operational_evidence.py:568` | 1 |
| `EVIDENCE-PROVIDER-023` | operational evidence | `scripts/test/operational_evidence.py:381` | 1 |
| `EVIDENCE-PROVIDER-024` | operational evidence | `scripts/validate_operational_evidence.py:629` | 0 |
| `EVIDENCE-PROVIDER-025` | operational evidence | `scripts/test/operational_evidence.py:355` | 1 |
| `EVIDENCE-PROVIDER-026` | operational evidence | `scripts/validate_operational_evidence.py:639` | 0 |
| `EVIDENCE-PROVIDER-027` | operational evidence | `scripts/validate_operational_evidence.py:642` | 0 |
| `EVIDENCE-PROVIDER-028` | operational evidence | `scripts/validate_operational_evidence.py:666` | 0 |
| `EVIDENCE-SOURCE-001` | operational evidence | `scripts/validate_operational_evidence.py:238` | 0 |
| `EVIDENCE-SOURCE-002` | operational evidence | `scripts/validate_operational_evidence.py:243` | 0 |
| `EVIDENCE-SOURCE-003` | operational evidence | `scripts/validate_operational_evidence.py:250` | 0 |
| `EVIDENCE-SOURCE-004` | operational evidence | `scripts/test/operational_evidence.py:308` | 1 |
| `EVIDENCE-SOURCE-005` | operational evidence | `scripts/validate_operational_evidence.py:257` | 0 |
| `EVIDENCE-VALIDATOR-001` | operational evidence | `scripts/validate_operational_evidence.py:758` | 0 |
| `MINCO-AGENT-CONTEXT-OPERATION-ABSENT` | agent | `crates/minco-cli/src/agent_cmd.rs:559` | 1 |
| `MINCO-AGENT-CONTEXT-TASK-ABSENT` | agent | `crates/minco-cli/src/agent_cmd.rs:601` | 1 |
| `MINCO-ARCH-001` | arch | `crates/minco-cli/src/architecture.rs:132` | 0 |
| `MINCO-ARCH-002` | arch | `crates/minco-cli/src/architecture.rs:133` | 0 |
| `MINCO-ARCH-003` | arch | `crates/minco-cli/src/architecture.rs:134` | 0 |
| `MINCO-AUTH-001` | auth | `crates/minco-plan/src/model.rs:549` | 0 |
| `MINCO-AWS-001` | aws | `crates/minco-plan/src/model.rs:1318` | 1 |
| `MINCO-AWS-002` | aws | `crates/minco-plan/src/model.rs:1334` | 1 |
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
| `MINCO-COST-001` | cost | `crates/minco-plan/src/model.rs:649` | 0 |
| `MINCO-COST-002` | cost | `crates/minco-plan/src/model.rs:663` | 1 |
| `MINCO-COST-003` | cost | `crates/minco-plan/src/model.rs:729` | 0 |
| `MINCO-COST-004` | cost | `crates/minco-plan/src/model.rs:735` | 0 |
| `MINCO-COST-005` | cost | `crates/minco-plan/src/model.rs:780` | 1 |
| `MINCO-COST-006` | cost | `crates/minco-plan/src/model.rs:643` | 0 |
| `MINCO-COST-007` | cost | `crates/minco-plan/src/model.rs:2514` | 1 |
| `MINCO-COST-008` | cost | `crates/minco-plan/src/model.rs:2543` | 1 |
| `MINCO-COST-009` | cost | `crates/minco-plan/src/model.rs:684` | 2 |
| `MINCO-COST-010` | cost | `crates/minco-plan/src/model.rs:719` | 0 |
| `MINCO-DB-001` | db | `crates/minco-plan/src/model.rs:792` | 0 |
| `MINCO-DB-002` | db | `crates/minco-plan/src/model.rs:798` | 0 |
| `MINCO-DB-003` | db | `crates/minco-plan/src/model.rs:817` | 0 |
| `MINCO-DB-004` | db | `crates/minco-plan/src/model.rs:2586` | 2 |
| `MINCO-DB-005` | db | `crates/minco-plan/src/model.rs:833` | 0 |
| `MINCO-DYNAMODB-001` | dynamodb | `crates/minco-plan/src/model.rs:1373` | 0 |
| `MINCO-DYNAMODB-002` | dynamodb | `crates/minco-plan/src/model.rs:1379` | 1 |
| `MINCO-DYNAMODB-003` | dynamodb | `crates/minco-plan/src/model.rs:1389` | 1 |
| `MINCO-DYNAMODB-004` | dynamodb | `crates/minco-plan/src/model.rs:1395` | 0 |
| `MINCO-DYNAMODB-005` | dynamodb | `crates/minco-plan/src/model.rs:1444` | 0 |
| `MINCO-DYNAMODB-006` | dynamodb | `crates/minco-plan/src/model.rs:1453` | 0 |
| `MINCO-DYNAMODB-007` | dynamodb | `crates/minco-plan/src/model.rs:1405` | 1 |
| `MINCO-DYNAMODB-008` | dynamodb | `crates/minco-plan/src/model.rs:1416` | 1 |
| `MINCO-HANDOVER-001` | handover | `crates/minco-cli/src/handover_cmd.rs:256` | 14 |
| `MINCO-HANDOVER-002` | handover | `crates/minco-cli/src/handover_cmd.rs:296` | 3 |
| `MINCO-HANDOVER-003` | handover | `crates/minco-cli/src/handover_cmd.rs:334` | 37 |
| `MINCO-HANDOVER-004` | handover | `crates/minco-cli/src/handover_cmd.rs:648` | 5 |
| `MINCO-HANDOVER-005` | handover | `crates/minco-cli/src/handover_cmd.rs:264` | 4 |
| `MINCO-HANDOVER-006` | handover | `crates/minco-cli/src/handover_cmd.rs:527` | 5 |
| `MINCO-HANDOVER-007` | handover | `crates/minco-cli/src/handover_cmd.rs:504` | 2 |
| `MINCO-HTTP-001` | http | `crates/minco-plan/src/model.rs:601` | 0 |
| `MINCO-HTTP-002` | http | `crates/minco-plan/src/model.rs:606` | 0 |
| `MINCO-HTTP-003` | http | `crates/minco-plan/src/model.rs:610` | 0 |
| `MINCO-HTTP-004` | http | `crates/minco-plan/src/model.rs:618` | 0 |
| `MINCO-HTTP-005` | http | `crates/minco-plan/src/model.rs:625` | 0 |
| `MINCO-HTTP-006` | http | `crates/minco-plan/src/model.rs:630` | 0 |
| `MINCO-HTTP-007` | http | `crates/minco-plan/src/model.rs:637` | 0 |
| `MINCO-IAM-001` | iam | `crates/minco-plan/src/model.rs:594` | 1 |
| `MINCO-PERF-001` | perf | `crates/minco-plan/src/model.rs:746` | 0 |
| `MINCO-PERF-002` | perf | `crates/minco-plan/src/model.rs:757` | 0 |
| `MINCO-PERF-003` | perf | `crates/minco-cli/src/main.rs:5549` | 0 |
| `MINCO-PERF-004` | perf | `crates/minco-plan/src/model.rs:710` | 0 |
| `MINCO-PLAN-001` | plan | `crates/minco-plan/src/model.rs:436` | 0 |
| `MINCO-PLAN-002` | plan | `crates/minco-plan/src/model.rs:554` | 0 |
| `MINCO-PLAN-003` | plan | `crates/minco-plan/src/model.rs:2429` | 1 |
| `MINCO-PLAN-004` | plan | `crates/minco-plan/src/model.rs:561` | 0 |
| `MINCO-PLAN-005` | plan | `crates/minco-plan/src/model.rs:571` | 1 |
| `MINCO-PLAN-010` | plan | `crates/minco-plan/src/model.rs:873` | 2 |
| `MINCO-PLAN-011` | plan | `crates/minco-plan/src/model.rs:882` | 1 |
| `MINCO-PLAN-012` | plan | `crates/minco-plan/src/model.rs:894` | 0 |
| `MINCO-PLAN-013` | plan | `crates/minco-plan/src/model.rs:909` | 1 |
| `MINCO-PLAN-014` | plan | `crates/minco-plan/src/model.rs:1000` | 1 |
| `MINCO-PLAN-015` | plan | `crates/minco-plan/src/model.rs:1017` | 5 |
| `MINCO-PLAN-016` | plan | `crates/minco-plan/src/model.rs:1010` | 2 |
| `MINCO-PLAN-017` | plan | `crates/minco-plan/src/model.rs:1222` | 0 |
| `MINCO-PLAN-018` | plan | `crates/minco-plan/src/model.rs:1237` | 4 |
| `MINCO-PLAN-INGRESS-001` | plan | `crates/minco-plan/src/model.rs:2457` | 3 |
| `MINCO-PLAN-INGRESS-002` | plan | `crates/minco-plan/src/model.rs:2485` | 5 |
| `MINCO-PLAN-MIGRATE-001` | plan | `crates/minco-plan/src/model.rs:394` | 1 |
| `MINCO-PLAN-MIGRATE-002` | plan | `crates/minco-plan/src/model.rs:401` | 2 |
| `MINCO-PLAN-MIGRATE-003` | plan | `crates/minco-plan/src/model.rs:426` | 0 |
| `MINCO-PREVIEW-001` | preview | `crates/minco-plan/src/model.rs:504` | 0 |
| `MINCO-PREVIEW-002` | preview | `crates/minco-plan/src/model.rs:510` | 0 |
| `MINCO-PREVIEW-003` | preview | `crates/minco-plan/src/model.rs:528` | 1 |
| `MINCO-PREVIEW-004` | preview | `crates/minco-plan/src/model.rs:534` | 0 |
| `MINCO-PREVIEW-006` | preview | `crates/minco-plan/src/model.rs:541` | 1 |
| `MINCO-REALTIME-001` | realtime | `crates/minco-plan/src/model.rs:466` | 0 |
| `MINCO-REALTIME-002` | realtime | `crates/minco-plan/src/model.rs:472` | 0 |
| `MINCO-SCHEDULE-001` | schedule | `crates/minco-plan/src/model.rs:1138` | 1 |
| `MINCO-SCHEDULE-002` | schedule | `crates/minco-plan/src/model.rs:1144` | 1 |
| `MINCO-SCHEDULE-003` | schedule | `crates/minco-plan/src/model.rs:579` | 1 |
| `MINCO-SCHEDULE-004` | schedule | `crates/minco-plan/src/model.rs:1155` | 1 |
| `MINCO-SCHEDULE-005` | schedule | `crates/minco-plan/src/model.rs:1170` | 0 |
| `MINCO-SCHEDULE-006` | schedule | `crates/minco-plan/src/model.rs:1179` | 0 |
| `MINCO-SQS-001` | sqs | `crates/minco-plan/src/model.rs:1112` | 1 |
| `MINCO-SQS-002` | sqs | `crates/minco-plan/src/model.rs:1066` | 1 |
| `MINCO-SQS-003` | sqs | `crates/minco-plan/src/model.rs:944` | 1 |
| `MINCO-SQS-004` | sqs | `crates/minco-plan/src/model.rs:937` | 1 |
| `MINCO-SQS-005` | sqs | `crates/minco-plan/src/model.rs:964` | 0 |
| `MINCO-SQS-006` | sqs | `crates/minco-plan/src/model.rs:984` | 1 |
| `MINCO-SQS-007` | sqs | `crates/minco-plan/src/model.rs:1079` | 1 |
| `MINCO-SQS-008` | sqs | `crates/minco-plan/src/model.rs:1090` | 1 |
| `MINCO-SQS-009` | sqs | `crates/minco-plan/src/model.rs:1102` | 0 |
| `MINCO-SQS-010` | sqs | `crates/minco-plan/src/model.rs:915` | 0 |
| `MINCO-SQS-011` | sqs | `crates/minco-plan/src/model.rs:924` | 0 |
| `MINCO-SQS-012` | sqs | `crates/minco-plan/src/model.rs:1212` | 1 |
| `MINCO-STATIC-001` | static | `crates/minco-plan/src/model.rs:459` | 0 |
| `PERF-BASELINE-001` | performance evidence | `scripts/validate_operational_evidence.py:390` | 0 |
| `PERF-BASELINE-002` | performance evidence | `scripts/validate_operational_evidence.py:393` | 0 |
| `PERF-BASELINE-003` | performance evidence | `scripts/test/operational_evidence.py:317` | 1 |
| `PERF-BASELINE-004` | performance evidence | `scripts/validate_operational_evidence.py:403` | 0 |
| `PERF-BASELINE-005` | performance evidence | `scripts/validate_operational_evidence.py:405` | 0 |
| `PERF-BASELINE-006` | performance evidence | `scripts/validate_operational_evidence.py:407` | 0 |
| `PERF-BASELINE-007` | performance evidence | `scripts/test/operational_evidence.py:238` | 1 |
| `PERF-BASELINE-008` | performance evidence | `scripts/validate_operational_evidence.py:411` | 0 |
| `PERF-BASELINE-009` | performance evidence | `scripts/validate_operational_evidence.py:418` | 0 |
| `PERF-BASELINE-010` | performance evidence | `scripts/validate_operational_evidence.py:421` | 0 |
| `PERF-BASELINE-011` | performance evidence | `scripts/test/operational_evidence.py:420` | 1 |
| `PERF-BASELINE-012` | performance evidence | `scripts/test/operational_evidence.py:420` | 1 |
| `PERF-BASELINE-013` | performance evidence | `scripts/validate_operational_evidence.py:468` | 0 |
| `PERF-BASELINE-014` | performance evidence | `scripts/test/operational_evidence.py:420` | 1 |
| `PERF-BASELINE-015` | performance evidence | `scripts/validate_operational_evidence.py:478` | 0 |
| `PERF-BASELINE-016` | performance evidence | `scripts/test/operational_evidence.py:430` | 1 |
| `PERF-COMPARE-001` | performance evidence | `scripts/validate_operational_evidence.py:487` | 0 |
| `PERF-COMPARE-002` | performance evidence | `scripts/validate_operational_evidence.py:494` | 0 |
| `PERF-COMPARE-003` | performance evidence | `scripts/validate_operational_evidence.py:498` | 0 |
| `PERF-COMPARE-004` | performance evidence | `scripts/validate_operational_evidence.py:500` | 0 |
| `PERF-COMPARE-005` | performance evidence | `scripts/validate_operational_evidence.py:506` | 0 |
| `PERF-DATA-001` | performance evidence | `scripts/test/operational_evidence.py:328` | 1 |
| `PERF-DATA-002` | performance evidence | `scripts/validate_operational_evidence.py:229` | 0 |
| `PERF-MEASURE-001` | performance evidence | `scripts/validate_operational_evidence.py:326` | 0 |
| `PERF-MEASURE-002` | performance evidence | `scripts/validate_operational_evidence.py:330` | 0 |
| `PERF-MEASURE-003` | performance evidence | `scripts/validate_operational_evidence.py:332` | 0 |
| `PERF-MEASURE-004` | performance evidence | `scripts/validate_operational_evidence.py:336` | 0 |
| `PERF-MEASURE-005` | performance evidence | `scripts/validate_operational_evidence.py:342` | 0 |
| `PERF-MEASURE-006` | performance evidence | `scripts/test/operational_evidence.py:453` | 1 |
| `PERF-MEASURE-007` | performance evidence | `scripts/validate_operational_evidence.py:352` | 0 |
| `PERF-MEASURE-008` | performance evidence | `scripts/validate_operational_evidence.py:357` | 0 |
| `PERF-MEASURE-009` | performance evidence | `scripts/validate_operational_evidence.py:360` | 0 |
| `PERF-MEASURE-010` | performance evidence | `scripts/validate_operational_evidence.py:378` | 0 |
| `PERF-MEASURE-011` | performance evidence | `scripts/test/operational_evidence.py:437` | 1 |
| `PERF-MEASURE-012` | performance evidence | `scripts/test/operational_evidence.py:443` | 1 |
| `PERF-POLICY-001` | performance evidence | `scripts/validate_operational_evidence.py:277` | 0 |
| `PERF-POLICY-002` | performance evidence | `scripts/test/operational_evidence.py:336` | 1 |
| `PERF-POLICY-003` | performance evidence | `scripts/test/operational_evidence.py:464` | 1 |
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
| `PUBLISH-021` | publication | `scripts/test/publish_validation.py:127` | 3 |
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
| `PUBLISH-071` | publication | `scripts/test/publish_validation.py:449` | 2 |
| `PUBLISH-072` | publication | `scripts/test/publish_validation.py:426` | 1 |
| `PUBLISH-073` | publication | `scripts/validate_publish.py:495` | 0 |
| `PUBLISH-074` | publication | `scripts/test/publish_validation.py:373` | 3 |
| `STATIC-001` | repository truth | `scripts/validate_static.py:164` | 0 |
| `STATIC-AGENT-RELEASE-001` | repository truth | `scripts/test/repository_truth.py:191` | 9 |
| `STATIC-AGENT-RELEASE-002` | repository truth | `scripts/test/repository_truth.py:223` | 9 |
| `STATIC-AGENT-RELEASE-003` | repository truth | `scripts/test/repository_truth.py:198` | 2 |
| `STATIC-AGENT-RELEASE-004` | repository truth | `scripts/test/repository_truth.py:234` | 1 |
| `STATIC-AGENT-RELEASE-005` | repository truth | `scripts/validate_static.py:937` | 0 |
| `STATIC-ARCH-001` | repository truth | `scripts/validate_static.py:1332` | 0 |
| `STATIC-ARCH-002` | repository truth | `scripts/validate_static.py:1338` | 0 |
| `STATIC-ARCH-003` | repository truth | `scripts/validate_static.py:1344` | 0 |
| `STATIC-ARCH-004` | repository truth | `scripts/validate_static.py:1347` | 0 |
| `STATIC-BUDGET-001` | repository truth | `scripts/validate_static.py:723` | 0 |
| `STATIC-BUDGET-002` | repository truth | `scripts/validate_static.py:501` | 0 |
| `STATIC-BUDGET-003` | repository truth | `scripts/validate_static.py:358` | 0 |
| `STATIC-BUDGET-004` | repository truth | `scripts/test/repository_truth.py:575` | 3 |
| `STATIC-BUDGET-005` | repository truth | `scripts/validate_static.py:464` | 0 |
| `STATIC-BUDGET-006` | repository truth | `scripts/test/repository_truth.py:649` | 1 |
| `STATIC-BUDGET-007` | repository truth | `scripts/test/repository_truth.py:666` | 1 |
| `STATIC-CARGO-001` | repository truth | `scripts/validate_static.py:195` | 0 |
| `STATIC-CARGO-002` | repository truth | `scripts/validate_static.py:198` | 0 |
| `STATIC-CARGO-003` | repository truth | `scripts/validate_static.py:204` | 0 |
| `STATIC-CARGO-004` | repository truth | `scripts/validate_static.py:210` | 0 |
| `STATIC-CARGO-005` | repository truth | `scripts/validate_static.py:212` | 0 |
| `STATIC-CARGO-006` | repository truth | `scripts/validate_static.py:218` | 0 |
| `STATIC-CARGO-007` | repository truth | `scripts/validate_static.py:223` | 0 |
| `STATIC-CARGO-008` | repository truth | `scripts/validate_static.py:226` | 0 |
| `STATIC-CONTRACT-001` | repository truth | `scripts/validate_static.py:1175` | 0 |
| `STATIC-CONTRACT-002` | repository truth | `scripts/validate_static.py:1180` | 0 |
| `STATIC-CONTRACT-003` | repository truth | `scripts/validate_static.py:1188` | 0 |
| `STATIC-CONTRACT-004` | repository truth | `scripts/validate_static.py:1191` | 0 |
| `STATIC-CONTRACT-005` | repository truth | `scripts/validate_static.py:1195` | 0 |
| `STATIC-CONTRACT-006` | repository truth | `scripts/validate_static.py:1197` | 0 |
| `STATIC-CONTRACT-007` | repository truth | `scripts/validate_static.py:1236` | 0 |
| `STATIC-CONTRACT-008` | repository truth | `scripts/validate_static.py:1285` | 0 |
| `STATIC-CONTRACT-009` | repository truth | `scripts/validate_static.py:1293` | 0 |
| `STATIC-CONTRACT-010` | repository truth | `scripts/validate_static.py:1298` | 0 |
| `STATIC-CONTRACT-011` | repository truth | `scripts/validate_static.py:1302` | 0 |
| `STATIC-CONTRACT-012` | repository truth | `scripts/validate_static.py:1308` | 0 |
| `STATIC-CONTRACT-013` | repository truth | `scripts/validate_static.py:1313` | 0 |
| `STATIC-CONTRACT-014` | repository truth | `scripts/validate_static.py:1211` | 0 |
| `STATIC-CONTRACT-015` | repository truth | `scripts/validate_static.py:1239` | 0 |
| `STATIC-CONTRACT-016` | repository truth | `scripts/validate_static.py:1254` | 0 |
| `STATIC-CONTRACT-017` | repository truth | `scripts/validate_static.py:1204` | 0 |
| `STATIC-CONTRACT-019` | repository truth | `scripts/validate_static.py:1272` | 0 |
| `STATIC-CONTRACT-020` | repository truth | `scripts/validate_static.py:1247` | 0 |
| `STATIC-CONTRACT-021` | repository truth | `scripts/validate_static.py:1223` | 0 |
| `STATIC-COST-001` | repository truth | `scripts/validate_static.py:1575` | 0 |
| `STATIC-COST-002` | repository truth | `scripts/validate_static.py:1578` | 0 |
| `STATIC-COST-003` | repository truth | `scripts/validate_static.py:1584` | 0 |
| `STATIC-COST-004` | repository truth | `scripts/validate_static.py:1586` | 0 |
| `STATIC-COST-005` | repository truth | `scripts/validate_static.py:1588` | 0 |
| `STATIC-DATA-001` | repository truth | `scripts/validate_static.py:182` | 0 |
| `STATIC-DB-001` | repository truth | `scripts/validate_static.py:1609` | 0 |
| `STATIC-GRAPH-001` | repository truth | `scripts/validate_static.py:1443` | 0 |
| `STATIC-HTTP-001` | repository truth | `scripts/validate_static.py:1591` | 0 |
| `STATIC-HTTP-002` | repository truth | `scripts/validate_static.py:1603` | 0 |
| `STATIC-MEASURE-001` | repository truth | `scripts/validate_static.py:379` | 0 |
| `STATIC-MEASURE-002` | repository truth | `scripts/test/repository_truth.py:606` | 1 |
| `STATIC-MEASURE-003` | repository truth | `scripts/validate_static.py:476` | 0 |
| `STATIC-MEASURE-004` | repository truth | `scripts/test/repository_truth.py:613` | 1 |
| `STATIC-MEASURE-005` | repository truth | `scripts/test/repository_truth.py:656` | 1 |
| `STATIC-PLACEHOLDER-001` | repository truth | `scripts/validate_static.py:1668` | 0 |
| `STATIC-PLAN-000` | repository truth | `scripts/validate_static.py:1556` | 0 |
| `STATIC-PLAN-001` | repository truth | `scripts/validate_static.py:1562` | 0 |
| `STATIC-PLAN-002` | repository truth | `scripts/validate_static.py:1569` | 0 |
| `STATIC-PLAN-003` | repository truth | `scripts/validate_static.py:1572` | 0 |
| `STATIC-PLUGIN-001` | repository truth | `scripts/validate_static.py:1363` | 0 |
| `STATIC-PLUGIN-002` | repository truth | `scripts/validate_static.py:1365` | 0 |
| `STATIC-PLUGIN-003` | repository truth | `scripts/validate_static.py:1370` | 0 |
| `STATIC-PLUGIN-004` | repository truth | `scripts/validate_static.py:1376` | 0 |
| `STATIC-PLUGIN-005` | repository truth | `scripts/validate_static.py:1379` | 0 |
| `STATIC-PYTHON-001` | repository truth | `scripts/validate_static.py:1526` | 0 |
| `STATIC-QUALITY-001` | repository truth | `scripts/validate_static.py:1465` | 0 |
| `STATIC-QUALITY-002` | repository truth | `scripts/validate_static.py:1467` | 0 |
| `STATIC-QUALITY-003` | repository truth | `scripts/validate_static.py:1471` | 0 |
| `STATIC-QUALITY-004` | repository truth | `scripts/validate_static.py:1501` | 0 |
| `STATIC-QUALITY-005` | repository truth | `scripts/validate_static.py:1487` | 0 |
| `STATIC-ROADMAP-001` | repository truth | `scripts/validate_static.py:1394` | 0 |
| `STATIC-ROADMAP-002` | repository truth | `scripts/validate_static.py:1398` | 0 |
| `STATIC-RUST-001` | repository truth | `scripts/validate_static.py:1514` | 0 |
| `STATIC-SAM-001` | repository truth | `scripts/validate_static.py:1621` | 0 |
| `STATIC-SAM-002` | repository truth | `scripts/validate_static.py:1639` | 0 |
| `STATIC-SAM-003` | repository truth | `scripts/validate_static.py:1642` | 0 |
| `STATIC-SAM-004` | repository truth | `scripts/validate_static.py:1646` | 0 |
| `STATIC-SAM-005` | repository truth | `scripts/validate_static.py:1633` | 0 |
| `STATIC-SHELL-001` | repository truth | `scripts/validate_static.py:1537` | 0 |
| `STATIC-SHELL-002` | repository truth | `scripts/validate_static.py:1539` | 0 |
| `STATIC-TASK-001` | repository truth | `scripts/validate_static.py:1403` | 0 |
| `STATIC-TASK-002` | repository truth | `scripts/validate_static.py:1409` | 0 |
| `STATIC-TASK-003` | repository truth | `scripts/validate_static.py:1413` | 0 |
| `STATIC-TASK-004` | repository truth | `scripts/validate_static.py:1416` | 0 |
| `STATIC-TASK-005` | repository truth | `scripts/validate_static.py:1420` | 0 |
| `STATIC-TASK-006` | repository truth | `scripts/validate_static.py:1422` | 0 |
| `STATIC-TASK-007` | repository truth | `scripts/validate_static.py:1425` | 0 |
| `STATIC-TASK-008` | repository truth | `scripts/validate_static.py:1427` | 0 |
| `STATIC-TASK-009` | repository truth | `scripts/validate_static.py:1431` | 0 |
| `STATIC-TRUTH-ADOPTION-001` | repository truth | `scripts/validate_static.py:778` | 0 |
| `STATIC-TRUTH-CATALOG-001` | repository truth | `scripts/validate_static.py:622` | 0 |
| `STATIC-TRUTH-CATALOG-002` | repository truth | `scripts/test/repository_truth.py:528` | 1 |
| `STATIC-TRUTH-CATALOG-003` | repository truth | `scripts/validate_static.py:609` | 0 |
| `STATIC-TRUTH-CATALOG-004` | repository truth | `scripts/validate_static.py:611` | 0 |
| `STATIC-TRUTH-CATALOG-005` | repository truth | `scripts/validate_static.py:613` | 0 |
| `STATIC-TRUTH-CATALOG-006` | repository truth | `scripts/validate_static.py:680` | 0 |
| `STATIC-TRUTH-DESCRIPTOR-001` | repository truth | `scripts/validate_static.py:652` | 0 |
| `STATIC-TRUTH-DESCRIPTOR-002` | repository truth | `scripts/validate_static.py:658` | 0 |
| `STATIC-TRUTH-DESCRIPTOR-003` | repository truth | `scripts/validate_static.py:665` | 0 |
| `STATIC-TRUTH-DOCS-001` | repository truth | `scripts/test/current_product_truth.py:64` | 6 |
| `STATIC-TRUTH-DOCS-002` | repository truth | `scripts/test/repository_truth.py:269` | 1 |
| `STATIC-TRUTH-FACADE-001` | repository truth | `scripts/validate_static.py:634` | 0 |
| `STATIC-TRUTH-FACADE-002` | repository truth | `scripts/validate_static.py:640` | 0 |
| `STATIC-TRUTH-FACADE-003` | repository truth | `scripts/validate_static.py:697` | 0 |
| `STATIC-TRUTH-FACADE-004` | repository truth | `scripts/validate_static.py:711` | 0 |
| `STATIC-TRUTH-PACKAGES-001` | repository truth | `scripts/validate_static.py:530` | 0 |
| `STATIC-TRUTH-PACKAGES-002` | repository truth | `scripts/validate_static.py:536` | 0 |
| `STATIC-TRUTH-PACKAGES-003` | repository truth | `scripts/test/repository_truth.py:496` | 2 |
| `STATIC-TRUTH-PACKAGES-004` | repository truth | `scripts/test/repository_truth.py:486` | 1 |
| `STATIC-TRUTH-PLAN-001` | repository truth | `scripts/validate_static.py:790` | 0 |
| `STATIC-TRUTH-PLAN-002` | repository truth | `scripts/validate_static.py:796` | 0 |
| `STATIC-TRUTH-PUBLISHED-001` | repository truth | `scripts/validate_static.py:253` | 0 |
| `STATIC-TRUTH-PUBLISHED-002` | repository truth | `scripts/test/repository_truth.py:507` | 2 |
| `STATIC-TRUTH-PUBLISHED-003` | repository truth | `scripts/test/repository_truth.py:518` | 1 |
| `STATIC-TRUTH-RELEASE-001` | repository truth | `scripts/test/repository_truth.py:411` | 1 |
| `STATIC-TRUTH-RELEASE-002` | repository truth | `scripts/test/repository_truth.py:421` | 1 |
| `STATIC-TRUTH-RELEASE-003` | repository truth | `scripts/test/repository_truth.py:431` | 1 |
| `STATIC-TRUTH-RELEASE-004` | repository truth | `scripts/validate_static.py:300` | 0 |
| `STATIC-TRUTH-RELEASE-005` | repository truth | `scripts/test/current_product_truth.py:88` | 1 |
| `STATIC-TRUTH-ROADMAP-001` | repository truth | `scripts/test/repository_truth.py:539` | 1 |
| `STATIC-TRUTH-ROADMAP-002` | repository truth | `scripts/test/repository_truth.py:568` | 1 |
| `STATIC-TRUTH-ROADMAP-003` | repository truth | `scripts/test/repository_truth.py:549` | 1 |
| `STATIC-TRUTH-VERSION-001` | repository truth | `scripts/test/repository_truth.py:400` | 1 |
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
| `config.toml` | configuration | `scripts/test/cost_regression.py:204` | 1 |
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
