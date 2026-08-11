# Measured quality assurance

Minco's authoritative everyday gate remains `scripts/quality.sh`. Release
qualification adds a slower measured lane:

```bash
scripts/ci/local-assurance.sh
```

That form executes the measured commands and replaces the canonical receipt.
To validate the frozen receipt while its private logs and reports remain in the
same workspace, use:

```bash
scripts/ci/local-assurance.sh --check
```

The check fails closed when any digest-addressed private artifact is absent,
changed, non-regular or reached through a symlink. A clean checkout therefore
does not treat the committed receipt as independently reproducible evidence.

`scripts/ci/local-release.sh` uses `--ephemeral` after `scripts/quality.sh`.
That mode executes and checks the measured lane under
`target/minco/quality-assurance/`, including its candidate-load receipt, so the
later publication dry-run receives a clean JJ or Git tree.

Install the exact versions recorded in
`verification/quality-assurance-policy.toml` under a dedicated Cargo root, or
set `MINCO_QUALITY_TOOL_ROOT` to another reviewed installation containing:

- cargo-nextest 0.9.143;
- cargo-llvm-cov 0.8.7 plus the pinned toolchain's `llvm-tools` component;
- cargo-mutants 27.1.0; and
- cargo-semver-checks 0.50.0.

The command proves executable-test plus doctest parity for the selected core
packages, measured line/function coverage, bounded mutation resistance,
publishable-package SemVer compatibility with immutable `v1.4.0`, and a local
candidate-load rehearsal. The committed receipt binds the exact source tree,
policy, tools, runner fingerprint, result dimensions, command duration and
private log/report digests.

Intermediate reports and raw logs stay under
`target/minco/quality-assurance/`. The committed receipt deliberately contains
no raw response identifiers, customer data, credentials or environment-variable
values. Output paths are confined; symlink parents and leaves are rejected; and
artifact bytes are read from retained no-follow descriptors with before/after
identity checks.

Local evidence is not hosted Linux, AWS, Waffo, deployment, production or SLO
evidence. `PASS` therefore retains `provider_contact = false`,
`production_slo = false`, and a truthful hosted-baseline `NOT RUN` state.

`verification/release-identity.json` is generated independently with:

```bash
uv run --locked python scripts/release/release_identity.py
uv run --locked python scripts/release/release_identity.py --check
```

It indexes package, official plugin, documentation, changelog and repository
truth identities for deterministic handoff. It is not itself promotion,
publication or deployment authority.
