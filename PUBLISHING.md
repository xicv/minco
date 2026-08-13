# Publishing Minco

The authoritative crate-family release procedure is
[`docs/development/publishing.md`](docs/development/publishing.md).
The current published boundary is the complete 34-package lock-step `1.6.0`
family from immutable tag `v1.6.0` at
`9abae9128dddc9bc32d099732e1421a0332e4785`. Source/package qualification,
merge, tag, upload, registry verification, docs.rs and documentation deployment
remain separate states for every release.

The workspace is an unpublished `1.7.0` candidate with the same 34-package
inventory. It packages the Apple-first fresh local-service preference while
retaining exact-resource recovery and Docker fallback. Candidate source, merge,
tag, GitHub release, OIDC publication, registry, docs.rs and stable Pages remain
separate evidence gates; the published baseline is still `1.6.0`.

The release keeps all nine AI skills current and retains cumulative
changelog-to-skill coverage plus the deterministic workflow receipt as
mandatory release gates. Those checks do not invoke a model or measure human
review effort.

For the published 1.6.0 baseline, exact release source passed PR-head
clean-Linux run `31689050949` and merged-main run `31689854658`. Authentication
run `31689854606` proved the exact OIDC boundary without upload. Publication run
`31690283715` passed archive and consumer checks before dependency-ordered
upload, and independent validation found all 34 exact versions present and
non-yanked. That publication is not live Waffo, AWS, application deployment or
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
Every later release must re-prove authentication and registry state; ownership
alone is not authentication evidence.

Never use `--allow-dirty` or `--no-verify` for a Minco release.
