# Publishing Minco

The authoritative crate-family release procedure is
[`docs/development/publishing.md`](docs/development/publishing.md).
The current published boundary is the complete 34-package lock-step `1.3.0`
family from immutable tag `v1.3.0` at
`e1fbb066e9332a2b6355b11a6f4b1c28806cc3e5`. Source/package qualification,
merge, tag, upload, registry verification, docs.rs and documentation deployment
remain separate states for every release.

The workspace is the unpublished `1.4.0` maintenance candidate. It retains the
same 34-package inventory and requires a new exact-source local gate,
clean-Linux compatibility run, immutable tag and guarded OIDC publication.
Historical 1.3.0 publication is not authentication or registry proof for 1.4.0.

The candidate keeps all nine AI skills current and retains cumulative
changelog-to-skill coverage plus the deterministic workflow receipt as
mandatory release gates.

For the published 1.3.0 baseline, exact release source passed clean-Linux run
`31451883403`; the authenticated local release wrapper uploaded the complete
family and independent validation found all 34 exact versions present and
non-yanked. That historical publication is not live Waffo, AWS, deployment or
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
registry verification.
Every later release must re-prove authentication and registry state; ownership
alone is not authentication evidence.

Never use `--allow-dirty` or `--no-verify` for a Minco release.
