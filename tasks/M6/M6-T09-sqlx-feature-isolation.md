---
id: M6-T09
title: Isolate PostgreSQL and SQLite SQLx feature graphs
milestone: M6
status: complete
priority: high
area: persistence/features
depends_on: [M6-T08]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - plugins/minco-plugin-feedback/Cargo.toml
  - extensions/minco-sqlx-postgres/Cargo.toml
  - extensions/minco-sqlx-sqlite/Cargo.toml
  - examples/orders/adapters/Cargo.toml
  - examples/orders/service/Cargo.toml
  - examples/orders/service/src/bin/migrate.rs
  - scripts/test/sqlx_feature_isolation.sh
  - scripts/quality.sh
  - CHANGELOG.md
  - docs/architecture/capability-audit.md
  - roadmap/tasks.mmd
  - tasks/M6/M6-T09-sqlx-feature-isolation.md
  - tasks/M6/M6-T10-multi-runtime-deployment-plan.md
  - verification/adoption-measurements.json
  - verification/source-manifest.json
checks:
  - bash -n scripts/test/sqlx_feature_isolation.sh
  - bash scripts/test/sqlx_feature_isolation.sh
  - cargo check -p minco-plugin-feedback --no-default-features --features postgres --locked
  - cargo check -p minco-plugin-feedback --no-default-features --features sqlite --locked
  - cargo test -p minco-plugin-feedback --no-default-features --features postgres --locked
  - cargo test -p minco-plugin-feedback --no-default-features --features sqlite --locked
  - cargo check -p minco-sqlx-postgres --locked
  - cargo check -p minco-sqlx-sqlite --locked
  - cargo check -p orders-adapters --no-default-features --features postgres --locked
  - cargo check -p orders-adapters --no-default-features --features sqlite --locked
  - cargo check -p orders-service --no-default-features --features postgres --locked
  - cargo check -p orders-service --no-default-features --features sqlite --locked
  - cargo check -p orders-adapters --no-default-features --features memory --locked
  - ./scripts/quality.sh
  - npm run --prefix plugins/minco-plugin-feedback test:browser
  - scripts/test/e2e.sh
  - scripts/dev/rustack-smoke.sh
  - scripts/release/package-list.sh
  - scripts/release/publish.sh --skip-quality
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Ensure a PostgreSQL-only Minco consumer does not compile SQLx SQLite or
`libsqlite3-sys`, and a SQLite-only consumer does not compile SQLx PostgreSQL.
Keep the all-feature workspace capable of exercising both backends.

## Non-goals

- changing persistence traits, SQL, migrations, HTTP behavior, or public Rust APIs;
- publishing a Minco release, creating a tag, or mutating AWS resources;
- implementing the multi-runtime deployment Plan IR;
- changing CGSP, GarmentIQ, or another product repository.

## Design boundary

- keep common SQLx runtime, TLS, UUID, chrono, JSON, and migration support in
  the shared workspace dependency;
- move `postgres` and `sqlite` activation to the exact adapter/plugin features
  that require them;
- retain one SQLx version and Cargo feature unification when both databases are
  deliberately selected;
- inspect complete normal and build dependency graphs;
- keep memory and no-default surfaces free of SQLx;
- preserve the enforced text-only Feedback profile from `M6-T08`.

## Acceptance

- Feedback, official extensions, Orders adapters, and Orders service resolve
  exactly the selected PostgreSQL or SQLite backend;
- memory-only Orders and no-default Minco surfaces resolve no SQLx;
- the all-feature workspace resolves and compiles both backends intentionally;
- focused compile/test gates, Feedback browser tests, and complete quality pass;
- `Cargo.lock` is unchanged unless Cargo proves a legitimate resolution change;
- source-manifest and package dry-run evidence are refreshed before completion;
- the replacement draft PR supersedes PR #14 from current `main`.

## Starting evidence

- Base Git SHA: `cc43078349ba24b66e951415eb9739223a59ddd0`.
- JJ change ID: `yqpryorxrrouuxwmzzvosqxmnzvokmzk`.
- Starting source-tree SHA-256:
  `c2366b5cd5b3cc4ec0431f05425b0af88229c07132ee186230f8cced87a69bd9`.
- Draft PR #14 head: `3ae2ec3b99d04e16e093e2b1dcbab8ea058424ea`.
- Before the fix, PostgreSQL-only, SQLite-only, both official extensions,
  Orders memory/PostgreSQL/SQLite, and both Orders service graphs all resolved
  `sqlx-postgres`, `sqlx-sqlite`, and `libsqlite3-sys`.

## Completed evidence

Implementation:

- the shared SQLx dependency retains only common runtime/TLS/data/migration
  features;
- Feedback and both official SQLx extensions select their exact backend;
- Orders adapter/service dependencies are optional and backend-specific;
- the migration binary imports optional pool configuration only under the
  matching feature;
- the regression uses `cargo tree --locked -e normal,build`, matches exact
  package tokens rather than wrapper-crate substrings, and is part of
  `scripts/quality.sh`;
- `M6-T10` records the separate trigger-aware Plan IR work without implementing
  or versioning it in this patch.

Dependency graphs after the fix:

- Feedback PostgreSQL: 178 packages; PostgreSQL present; SQLite and
  `libsqlite3-sys` absent.
- Feedback SQLite: 167 packages; SQLite and `libsqlite3-sys` present;
  PostgreSQL absent.
- Official PostgreSQL/SQLite extensions: 146/134 packages with only the
  selected backend.
- Orders memory adapter: 35 packages and no SQLx.
- Orders PostgreSQL/SQLite adapters: 149/137 packages with only the selected
  backend.
- Orders PostgreSQL/SQLite services: 198/187 packages with only the selected
  backend.
- Minco no-default facade: 16 packages and no SQLx.
- The deliberate all-feature workspace retains both backends.

Passed:

- `bash -n scripts/test/sqlx_feature_isolation.sh`;
- `bash scripts/test/sqlx_feature_isolation.sh`;
- every focused PostgreSQL/SQLite Feedback, extension, Orders adapter, Orders
  service, and memory compile/test command listed above;
- `cargo fmt --all -- --check`;
- `npm ci --prefix plugins/minco-plugin-feedback`;
- `npm run --prefix plugins/minco-plugin-feedback test:browser` (40 Chromium
  and Firefox tests, including the text-only profile);
- `scripts/test/e2e.sh`;
- `scripts/dev/rustack-smoke.sh` for isolated S3, SQS, SSM, STS and Minco
  adapters;
- `./scripts/quality.sh` after deterministic evidence refresh;
- `scripts/release/package-list.sh`;
- `scripts/release/publish.sh --skip-quality` from an empty child change;
- `uv run --locked python scripts/source_manifest.py --check`;
- `jj log -r 'conflicts()'`.

Lockfile and artifacts:

- `cargo generate-lockfile --locked` left `Cargo.lock` byte-for-byte unchanged
  at SHA-256
  `5a5abd02b5e2df2122b0dc1d1141668352b15d92e4c77e8862c56278ef82ce01`.
- Exact-source Orders ARM64 Lambda:
  5,030,903 compressed / 11,047,008 uncompressed bytes,
  SHA-256
  `c65418e227a10edee6952e7e83e4ae1716feffe024dbd46a4195bc673a34acf6`;
  the cold-target observation was 118.23 seconds.
- Exact-source worker ARM64 Lambda:
  573,414 compressed / 1,203,520 uncompressed bytes,
  SHA-256
  `77f81c3b90c8e467436e4498f7d306e2629a06580b3e8cad80732a6b2e4f34f6`;
  the subsequent build observation was 13.89 seconds.
- All 24 package archives compiled in the publication dry run and every upload
  stopped at Cargo's dry-run boundary. No registry upload occurred.
- Live registry lookups succeeded for all 24 immutable `0.3.0` records; the
  validator intentionally returned `PUBLISH-072` for each already-published
  version.

Failed attempts retained:

- the first quality attempt stopped at `STATIC-MEASURE-004` because the
  adoption report was bound to the previous source manifest;
- the next broad quality attempt passed compiler/security stages but failed its
  terminal stale-source-manifest check after refreshing deterministic evidence;
- the first publication attempt failed because the JJ working copy was not
  empty;
- the shared-target publication retry failed because the release script
  intentionally inspects repository-local `target/package`;
- none of those attempts is counted as a pass.

Safety:

- no CGSP or GarmentIQ source, branch, pull request, data, deployment, or
  infrastructure was changed;
- no AWS resource, external database, crate registry, release tag, or published
  version was mutated;
- no bypass flag, secret, token, or customer data was used.
