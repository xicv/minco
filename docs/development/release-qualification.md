# 1.0 candidate qualification

This procedure qualifies one exact source tree without tagging, publishing or
deploying it. The checked-in records contain only aggregate synthetic
measurements, command status and content digests. Raw command logs, temporary
databases, response identifiers and benchmark projects stay under ignored
`target/minco/` paths and are removed or remain local.

## Evidence states

Every mandatory command is recorded as exactly one of `PASS`, `FAIL`,
`BLOCKED` or `NOT RUN`, with its exit code, elapsed time and the byte count and
SHA-256 digest of its local log. `PASS` requires exit zero. A complete release
gate record cannot be `PASS` if any mandatory command has another state.

Run the complete local sequence from a clean, exact candidate workspace:

```bash
scripts/release/qualify-candidate.sh
```

The runner executes the repository's authoritative full quality suite, the
Feedback browser journey, the HTTP end-to-end journey, Rustack provider seams,
bounded recovery and load rehearsals, package inspection and the publish dry
run. `scripts/release/publish.sh --skip-quality` remains a dry run: the
candidate runner never supplies `--execute`, creates a tag or uploads a crate.

The Rust Cargo Book recommends a publish dry run because it performs package
verification, archive creation and compilation without upload. Minco also
inspects archive contents and sizes; neither result is registry publication:

- <https://doc.rust-lang.org/cargo/commands/cargo-publish.html>
- <https://doc.rust-lang.org/cargo/reference/publishing.html>

## Bounded load gate

`scripts/release/candidate-load.sh` starts the real Orders Axum application on
loopback with a temporary file-backed SQLite database and a four-connection
pool. It sends a bounded set of unique synthetic writes through fresh TCP
connections at controlled concurrency and records errors, throughput and
nearest-rank minimum, p50, p95 and maximum latency. Timing is machine-specific
smoke evidence, not a performance SLO.

A disposable external Rust crate then invokes the public
`minco_aws_worker::process_sqs_event` API with 100 batches of ten synthetic
messages. The gate records configured and observed in-process concurrency,
complete processing, partial-batch failures and throughput. The reviewed Plan
fixture supplies the independently checked queue batch, visibility, retention,
mapping-concurrency and database-connection limits.

The record reports request, message, batch/invocation and maximum-connection
dimensions plus the actual local Orders binary and worker library sizes. It
does not multiply those units by a moving provider price. AWS currently
documents that SQS event-source concurrency and function reserved concurrency
are separate limits, that visibility should cover at least six function
timeouts, and that partial batch responses prevent successful records from
being retried:

- <https://docs.aws.amazon.com/lambda/latest/dg/services-sqs-scaling.html>
- <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-configure-lambda-function-trigger.html>
- <https://docs.aws.amazon.com/lambda/latest/dg/services-sqs-errorhandling.html>

The local worker gate does not reproduce managed poller scaling, throttling,
retry timing, quotas or network latency. AWS recommends isolated cloud testing
for managed-service integration and isolating load tests from production:
<https://docs.aws.amazon.com/lambda/latest/dg/testing-guide.html>.

## Recovery gate

`scripts/release/candidate-recovery.sh` uses only a temporary synthetic SQLite
boundary. It applies the current migrations twice, creates one record through
the public HTTP API, takes a SQLite online backup, removes the source database,
restores into a different file, reapplies migrations and reads the record
through a fresh application process. SQLite's own integrity check and exact
row/migration counts must agree before the gate can pass. Python 3.14's
documented `Connection.backup` API performs the copy:
<https://docs.python.org/3/library/sqlite3.html#sqlite3.Connection.backup>.

The same gate runs the exact deployment rollback tests and the provider-free
multi-release plan suite. Database recovery remains forward-only: Minco does
not claim reverse SQL or automatic data repair. AWS recovery guidance likewise
requires testing that a backup can actually be restored instead of treating
backup creation as sufficient evidence:
<https://docs.aws.amazon.com/prescriptive-guidance/latest/security-best-practices/test.html>.

The completed M10-T08 disposable-AWS rehearsal remains valid historical
provider evidence bound to its named source revisions. M12-T05 records it
separately and records exact-current AWS redeployment as `NOT RUN`; local or
historical evidence is never relabeled as a fresh provider result.

## Security and documentation

The full quality command must finish without a critical/high finding. It runs
Cargo deny, RustSec audit, npm's high-severity audit, redacted secret scanning,
deep repository review, compiler/Clippy/test matrices, documentation snippets,
site build, internal/external links, desktop/small-screen browser journeys and
fresh generated PostgreSQL and SQLite consumer applications.

Warnings and informational findings remain visible in their underlying logs.
No release record may silently convert an unavailable tool, external service or
provider authority into a pass.

## Evidence boundary

The three generated public records are excluded from the source-tree digest to
avoid self-reference:

- `verification/1.0-candidate-load.json`;
- `verification/1.0-candidate-recovery.json`;
- `verification/1.0-candidate-release-gates.json`.

They bind the exact digest from `verification/source-manifest.json`. The raw
logs beneath `target/minco/candidate-*` are local diagnostic artifacts and are
not committed. Hosted exact-head qualification, tag creation, registry upload,
docs publication, application adoption, AWS deployment and production proof
remain separate gates requiring their own authority.
