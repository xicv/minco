# ADR 0036: Own local service lifecycle across Docker and Apple Container

## Status

Accepted

## Context

ADR-0023 made `cargo minco dev` the graph-driven local supervisor, but its
prototype delegated PostgreSQL and Rustack lifecycle to a second
`minco-services` executable found through `PATH`. The prototype also removed a
same-named Apple container with `container delete --force`, selected a runtime
again during shutdown, treated an authentication request as PostgreSQL
readiness, and accepted any Rustack health body containing `"running"`.
Those shortcuts are incompatible with fail-closed lifecycle ownership.

Laravel Sail 13 demonstrates useful ergonomics: one obvious project-local
command, project-owned service configuration, predictable start/stop,
persistent data, useful diagnostics, explicit customization and low
prerequisite knowledge. Minco adopts those ergonomics, not Sail's application
container, PHP command forwarding, broad service catalogue, arbitrary shell
proxy or Docker-only architecture. The Rust application remains a native
process supervised by Minco.

The supported runtimes expose different control planes. Docker Compose owns a
project model and adds `com.docker.compose.*` labels. Apple Container 1.2.x
owns individual OCI containers and volumes and exposes their complete
configuration as JSON. Apple does not parse Compose, so treating Compose as a
portable runtime model would either misrepresent customization or require a
new general-purpose orchestrator.

The prototype package contains `cargo-minco` and `minco-services`. A normal
`cargo install` installs both, but Cargo also permits
`cargo install --bin cargo-minco`, which omits the helper. Package managers may
likewise split executables. A bare helper name can resolve a missing, stale or
attacker-controlled program from `PATH`.

## Decision

`cargo minco dev` remains the one-command workflow and the application remains
native. Only graph-declared PostgreSQL and Rustack dependencies are managed.

Minco owns one typed first-class local-service specification containing the
service ID, immutable image, container and loopback host ports, non-secret
environment values, secret environment names and sources, volume contract,
readiness contract, requested Rustack capabilities and ownership schema. Both
runtime adapters consume that specification. Docker may still use the
project-owned Compose file for Docker-only customization, but rendered
first-class services must preserve the typed ownership, image, port,
environment, volume and capability contract. Apple supports only the typed
PostgreSQL and Rustack services; arbitrary Compose services remain Docker-only.

Every managed container and persistent volume carries these labels:

- `dev.minco.managed=true`;
- `dev.minco.schema=1`;
- `dev.minco.application=<normalized application identity>`;
- `dev.minco.workspace=<canonical workspace fingerprint>`;
- `dev.minco.service=<service id>`; and
- `dev.minco.configuration=<non-secret specification digest>`.

Before reuse, start, stop, replacement or deletion, Minco parses structured
runtime inspection and proves all labels plus expected image, native platform,
loopback mapping, environment contract and volume attachment. Missing,
malformed or mismatched metadata is a foreign resource and fails closed. A
same-named foreign container or volume is never stopped, deleted or reused.
Ordinary stop never deletes PostgreSQL data. No automatic data-reset operation
is part of this slice; deleting a volume remains an explicit, separately
approved data-loss action.

The canonical Compose path is resolved within the canonical project root. Its
fingerprint makes relative and absolute invocations, and symlink aliases of one
workspace, converge. Separate or moved JJ workspaces remain isolated. Runtime
diagnostics report compatible Minco or legacy implicit-Compose resources from
other identities but never migrate or delete them automatically.

Each project/service operation acquires an operating-system file lock below
`target/minco/dev`. Start writes a schema-versioned, non-secret lifecycle
receipt atomically after structured post-start verification. A receipt records
the resolved runtime, exact resource name, workspace, service and configuration
identity. It never authorizes a mutation by itself: inspection must agree.
Corrupt receipts, receipt/runtime disagreement and matching resources in both
runtimes are ambiguous and fail with recovery instructions. If a selected
runtime is unavailable during stop, Minco reports the exact residual resource
instead of claiming cleanup. Failed startup cleans up only a resource created
by that attempt and only after ownership proof.

Fresh `auto` selection prefers a ready Docker Compose runtime, then a ready
Apple Container runtime on Apple silicon. An exact existing receipt/resource
takes precedence for deterministic crash recovery. Explicit runtime selection
still fails if that runtime is unavailable. Supported Apple Container versions
are 1.2.x until a later version is qualified; unsupported versions fail with an
actionable diagnostic. Docker requires a working Compose v2-or-newer command
and daemon.

The separate `minco-services` binary is removed. Its narrow command surface
becomes a hidden `cargo-minco` subcommand. DevPlan keeps a stable symbolic
program and argument vector, while the CLI composition root injects the exact
`std::env::current_exe()` path into the supervisor only at execution time.
The path is neither serialized into dry-run output nor resolved from `PATH`.
The hidden subprocess is therefore installed, upgraded and versioned with the
calling CLI even when only `cargo-minco` is packaged.

PostgreSQL readiness uses explicit loopback connection options, authenticates
with the configured local password, verifies `current_user` and
`current_database()`, and executes `SELECT 1`. Rustack readiness parses the
documented JSON `services` object and requires every requested identifier to
equal `running`. Integration qualification also performs a signed STS
`GetCallerIdentity` against an explicitly constructed loopback-only client.
That client uses fixed local credentials and region without the AWS default
provider or endpoint chain.

Ports are preflighted while holding the service lock. An occupied port is
accepted only when structured inspection proves it belongs to the exact
ready managed resource; otherwise startup fails without mutation. Runtime
post-start inspection closes the preflight race.

## Rustack release contract

Minco continues to use the upstream namespace because `xicv/rustack` is an
exact fork of upstream release commit
`ab8bc61a3e45058c7d42de8443f9d215cc110b18`, while the existing release
workflow and package are published by `tyrchen/rustack`. Release `v0.9.1` is
pinned to OCI index digest
`sha256:18cd91395e17453e2c34b299e45f4679dc2427473dc1db6541bbe212fd70a104`.
The index contains native `linux/amd64` and `linux/arm64` manifests, including
arm64 digest
`sha256:ec5a7ffee62c29bebd4862c826c34335928fd017977ed78c551d2dba5e94f5fb`,
and BuildKit SLSA provenance attestations for both. The annotated Git tag is
unsigned and no OCI signature was established, so the image is described as
immutable, multi-platform and attested, not signed.

The MIT-licensed image reports exactly 18 services: `apigatewayv2`,
`cloudfront`, `cloudwatch`, `dynamodb`, `dynamodbstreams`, `events`, `iam`,
`kinesis`, `kms`, `lambda`, `logs`, `s3`, `secretsmanager`, `ses`, `sns`,
`sqs`, `ssm` and `sts`. Its `GET /_localstack/health` schema is
`{"services":{"<id>":"running"}}`; requested services can therefore be
verified individually. Minco changes this allowlist only after source,
health-schema, platform, digest and integration requalification of a new exact
release.

## Consequences

- Plans remain deterministic, side-effect free and secret-free.
- Runtime-specific process discovery is behind a fakeable command runner, so
  ownership, receipt, locking, collision and diagnostic behavior can be tested
  without a host daemon.
- The Docker Compose file stays application-owned, while Apple behavior is
  deliberately narrower and cannot be changed by arbitrary Compose edits.
- Persistent PostgreSQL storage survives ordinary stop/start on both runtimes.
- Existing implicit Compose projects such as the observed `local` project can
  be detected and diagnosed, but may require an explicit user-approved manual
  migration. Their volumes are never silently removed.
- Production Plan IR, release artifacts, deployment behavior and idle-cloud
  cost are unchanged.

## Alternatives rejected

### Retain a bare helper executable

Cargo and external package managers can install only `cargo-minco`; `PATH` can
also select the wrong version. A sibling-path lookup improves Cargo's default
install but still assumes packaging layout and two-file atomic upgrades.

### Run a separately installed helper by absolute configuration

This makes installation layout public configuration and lets helper/CLI
versions drift. The exact running CLI already provides a stronger composition
root.

### Move all service orchestration into `minco-dev`

`minco-dev` is provider-neutral plan/supervision code. Docker, Apple, SQLx and
AWS SDK dependencies belong at the CLI composition boundary, not in the
publishable core plan crate.

### Parse arbitrary Compose into an Apple model

Compose has a much broader service and orchestration surface than this feature.
Partial parsing would silently discard semantics; complete parsing would turn
Minco into a container orchestrator.

### Rebuild or republish Rustack under `xicv`

The exact fork and upstream tag are identical and the upstream image already
has a pinned native-arm64 index and provenance. Republishing would create a new
supply-chain surface without improving the selected source boundary.

### Contact public AWS for stronger conformance

Local development qualification must prove the emulator boundary without
credentials or network paths to AWS. Real-provider conformance belongs to
separate explicitly approved deployment verification.

## Compatibility

The DevPlan command changes from a second executable to a hidden subcommand of
`cargo-minco`; consumers should treat service commands as implementation
details. Existing generated Compose files need the new ownership labels before
Apple/Docker lifecycle can manage them. Docker-only custom services remain
valid but are not projected into Apple Container.

## Safety

No secret value enters argv, DevPlan JSON, receipts, diagnostics or emitted
logs. Docker and Apple ports bind only to `127.0.0.1`. Local Rustack clients
have an explicit loopback endpoint, fixed local credentials, fixed region and
metadata disabled; they do not use the public endpoint/provider chain. Every
destructive runtime action requires current structured ownership proof. The
feature does not merge, release, publish an image, deploy infrastructure or
add fixed cloud compute.
