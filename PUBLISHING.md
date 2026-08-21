# Publishing Minco

The authoritative crate-family release procedure is
[`docs/development/publishing.md`](docs/development/publishing.md).
The current published boundary is the complete 36-package lock-step `1.10.0`
family from immutable tag `v1.10.0` at
`2075b60b8fe86c04d3c8289d71eb8293a39fc378`. Source/package qualification,
merge, tag, upload, registry verification, docs.rs and documentation deployment
remain separate states for every release.

The workspace is an unpublished additive `1.11.0` candidate with the same 36
publishable packages. No package ownership boundary is new. Every existing
publisher, including `minco-interaction` and `minco-plugin-ticketing`, was
already reconciled for repository `xicv/minco`, workflow `publish-crates.yml`
and environment `crates-io`. Candidate qualification, merge, exact-main hosted
compatibility, tag, OIDC authentication, upload, registry verification,
docs.rs and Pages remain separate states.

The candidate keeps all nine AI skills current for the 1.11 request boundary and retains cumulative
changelog-to-skill coverage plus the deterministic workflow receipt as
mandatory release gates. Those checks do not invoke a model or measure human
review effort.

For the published 1.10.0 baseline, the exact release source passed the complete
local qualification before PR #180 merged as
`2075b60b8fe86c04d3c8289d71eb8293a39fc378`; exact-main clean-Linux run
`32392228228` then passed. Manual authenticated publication passed archive and
consumer checks, uploaded the dependency-ordered family, and resumed only the
independently proven missing packages after two crates.io rate limits.
Independent validation found all 36 exact versions present and non-yanked.
That publication is not live provider, AWS, application deployment or
production evidence.

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
The 1.3.0 release used the authenticated local wrapper from the exact tagged
checkout. crates.io accepted 32 packages before applying its documented
short-window rate limit; recovery waited for the explicit retry time and
uploaded only the missing `minco` and `cargo-minco` complement. Repository
truth keeps `new_publishable_packages` empty after independent 34-package
registry verification. The 1.4.0 release used short-lived OIDC credentials;
its exact 23/11 partial-publication recovery remains recorded in versioned
release evidence.
The 1.5.0 release used short-lived OIDC credentials and completed all 34
dependency-ordered uploads in one guarded run.
The 1.6.0 release repeated that exact tagged-family boundary and independently
verified all 34 non-yanked registry records after upload.
The 1.7.0 release repeated the complete exact-tag OIDC boundary and published
all 34 packages in one guarded run before independent registry verification.
The 1.8.0 release repeated that boundary for the additive object-transfer
family and again published all 34 packages in one guarded run.
Every later release must re-prove authentication and registry state; ownership
alone is not authentication evidence.

Never use `--allow-dirty` or `--no-verify` for a Minco release.
