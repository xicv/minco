# Publishing Minco

The authoritative crate-family release procedure is
[`docs/development/publishing.md`](docs/development/publishing.md).
The current published boundary is the complete 33-package lock-step `1.2.2`
family from immutable tag `v1.2.2` at
`0496e6294b213c839af551a82858e2c1c3f7f45d`. Source/package qualification,
merge, tag, upload, registry verification, docs.rs and documentation deployment
remain separate states for every release.

The current workspace is the unpublished 34-package `1.3.0` candidate. It adds
`minco-plugin-payments-waffo`; no 1.3.0 tag, upload, registry, docs.rs, Pages or
live Waffo proof is represented by this source state.

The candidate keeps the eight published AI skills current, adds one
Waffo-specific skill, and retains cumulative changelog-to-skill coverage plus
the deterministic workflow receipt as mandatory release gates.
Publication workflow `31396167046` passed and independent registry validation
found all 33 exact versions present and non-yanked.

The safe default performs no upload. It requires the pinned Rust toolchain and a
reviewed `Cargo.lock` before Cargo's package normalization and compilation gate:

```bash
uv sync --locked --only-dev
uv run --locked python scripts/validate_publish.py
scripts/release/publish.sh
```

The irreversible upload requires a clean, correctly tagged release and an
explicit flag:

```bash
scripts/release/publish.sh --execute
```

The first 1.0.0 publication used a short-lived manual crates.io token because
it contained first-publication crates. The 1.1.0 release independently verified
all trusted-publisher configurations and recovered an exact partial registry
complement. The 1.2.0 release used short-lived OIDC credentials in workflow run
`31362919458`. The 1.2.1 and 1.2.2 patches independently re-proved OIDC
publication in runs `31379324388` and `31396167046`; all 33 uploads and
exact-version registry checks passed for each release.
Repository truth records only `minco-plugin-payments-waffo` as a new
publishable candidate; this merge does not create its publisher configuration
or upload it.
Every later release must re-prove authentication and registry state; ownership
alone is not authentication evidence.

Never use `--allow-dirty` or `--no-verify` for a Minco release.
