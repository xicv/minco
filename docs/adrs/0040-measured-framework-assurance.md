# ADR-0040: Bind measured assurance to exact source without broadening runtime authority

- Status: Accepted
- Date: 2026-08-12

## Context

Minco's compiler, test, security, packaging, local-runtime and release gates
are comprehensive, but coverage, mutation resistance, alternative test-runner
parity, public Rust SemVer and exact-tree performance were previously separate
aspirations or ad-hoc measurements. The CLI dispatch module also owned its
entire Clap schema, making a high-change concentration point harder to review.
Release consumers had to correlate package, plugin, documentation, changelog
and repository-truth identities independently.

Arbitrary repository-wide percentages would reward test volume rather than
contract confidence. An unpinned faster runner could silently omit doctests.
Local load timing cannot establish a hosted Linux baseline, AWS behavior or a
production SLO. A new umbrella authority would duplicate existing source,
release and provider contracts.

## Decision

1. `verification/quality-assurance-policy.toml` pins exact compatible versions
   of nextest, llvm-cov, mutants and semver-checks. Coverage floors are the
   measured baseline minus a documented two-percentage-point tolerance.
2. Nextest is additive: its executable inventory and run must agree with the
   measured Cargo inventory, and Cargo executes the separate doctest lane.
3. Mutation testing is bounded to Plan cost decisions and release digest/path
   authority. Every viable reviewed mutant must be caught; explicitly
   unviable mutants remain counted.
4. SemVer checks compare every publishable package with the immutable `v1.4.0`
   commit. This is an additional signal, not proof of JSON, CLI or behavior.
5. Local candidate load evidence binds the current source manifest and remains
   `provider_contact = false` and `production_slo = false`. Missing hosted or
   live-provider evidence remains `NOT RUN`.
6. The assurance runner writes raw logs and intermediate reports only below
   ignored `target/minco/quality-assurance`. Every digest-addressed log and
   report is reopened through confined no-follow descriptors and must match its
   exact byte count and SHA-256 before a receipt is accepted. Its committed
   receipt is excluded narrowly from the source manifest to avoid a digest
   cycle; policy, code and reviewed baselines remain source-bound.
7. `command.rs` owns only the private Clap schema. Dispatch and behavior stay in
   `main.rs`; exact help bytes and compiler/tests guard the boundary.
8. `verification/release-identity.json` is a deterministic projection over
   existing independent authorities. It grants no release, publication,
   deployment or provider authority.

The standalone assurance lane executes the measurements and seals the current
receipt. A frozen receipt can be checked only while all of its private
digest-addressed evidence remains available. The local release command instead
executes the same assurance lane into ignored ephemeral receipt and performance
paths, checks those exact bytes, and therefore preserves the clean-tree boundary
required by publication dry-runs without trusting unavailable private evidence.
No GitHub workflow, always-on service, cloud resource, generic provider
abstraction or runtime plugin is added.

## Consequences

- Quality regressions fail with stable `ASSURANCE-*` diagnostics and retain
  digest-addressed private logs.
- The measured lane is slower, so it is mandatory for local release
  qualification but remains an explicitly optional standalone quality profile.
- Developers must install the exact four tool versions and `llvm-tools`.
- Command schema changes are easier to review without changing the CLI API.
- The release-identity projection improves handoff and AI inspection while
  preserving the authority of each underlying manifest and receipt.

## Alternatives rejected

- A repository-wide coverage percentage without a baseline: unmeasured and
  easy to game.
- Replacing Cargo test with nextest: loses doctests and changes authority.
- Treating mutation score as one broad KPI: hides unviable and surviving cases.
- Uploading local measurements as hosted/provider proof: false provenance.
- A new release database or control plane: duplicates repository-native truth.
- Splitting runtime behavior with the CLI schema: too large for a bounded,
  compatibility-preserving maintainability change.
