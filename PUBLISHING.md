# Publishing Minco

The authoritative crate-family release procedure is
[`docs/development/publishing.md`](docs/development/publishing.md).
The current published boundary is the complete 33-package lock-step `1.0.0`
family. Its first publication established ownership for
`minco-plugin-realtime`, `minco-project-view`, `minco-mcp`,
`minco-workbench`, and `minco-aws-dynamodb`.
Source/package qualification, merge, tag, upload and registry verification
remain separate states for every later release.

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

The first 1.0.0 publication used a short-lived manual crates.io token for the
complete exact-tag family because it contained first-publication crates.
Repository truth now keeps `new_publishable_packages` empty. Configure and
independently verify trusted publishing for those new packages before relying
on the OIDC workflow for a later release.

Never use `--allow-dirty` or `--no-verify` for a Minco release.
