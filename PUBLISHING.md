# Publishing Minco

The authoritative crate-family release procedure is
[`docs/development/publishing.md`](docs/development/publishing.md).
The current published boundary is the 28-package lock-step `0.6.0` family.
The workspace is an unpublished 33-package `1.0.0` candidate. It includes the
first-publication `minco-plugin-realtime`, `minco-project-view`, `minco-mcp`,
`minco-workbench`, and `minco-aws-dynamodb` packages.
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

Because 1.0.0 contains first-publication crates, publish the complete exact-tag
family once with a short-lived manual crates.io token. The OIDC workflow fails
closed while `new_publishable_packages` is non-empty so it cannot strand a
partial family before ownership exists.

Never use `--allow-dirty` or `--no-verify` for a Minco release.
