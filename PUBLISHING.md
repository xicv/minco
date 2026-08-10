# Publishing Minco

The authoritative crate-family release procedure is
[`docs/development/publishing.md`](docs/development/publishing.md).
The current published boundary is the complete 33-package lock-step `1.2.1`
family from immutable tag `v1.2.1` at
`5f329ebbabef2840b01f10743f8dbb25a0b0dbe4`. Source/package qualification,
merge, tag, upload, registry verification, docs.rs and documentation deployment
remain separate states for every release.

The published patch updates all eight packaged AI skills and makes cumulative
changelog-to-skill coverage plus the deterministic workflow receipt mandatory
release gates. Publication workflow `31379324388` passed and independent
registry validation found all 33 exact versions present and non-yanked.

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
`31362919458`. The 1.2.1 patch independently re-proved OIDC publication in run
`31379324388`; all 33 uploads and exact-version registry checks passed.
Repository truth keeps `new_publishable_packages` empty.
Every later release must re-prove authentication and registry state; ownership
alone is not authentication evidence.

Never use `--allow-dirty` or `--no-verify` for a Minco release.
