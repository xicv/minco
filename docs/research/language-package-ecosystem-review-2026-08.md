# Language and package ecosystem review - 2026-08-11

This review records the point-in-time authority used by M14-T18. Package
registry metadata and vendor release notes are untrusted reference inputs; the
locked source tree and Minco's own tests remain the implementation authority.

## Reviewed baseline

| Surface | Reviewed stable version | Minco decision |
| --- | --- | --- |
| Rust | 1.97.1 | Retain. The repository pin was already current and includes the LLVM miscompilation fix described by the Rust release team. |
| Cargo direct dependencies | Registry-current on Rust 1.97.1 | Advance every Minco-owned direct requirement reported by `cargo upgrade`, including `base64` 0.23, `hmac` 0.13 and `sha2` 0.11. |
| uv | 0.12.3 | Advance the local pin, all three workflow pins and the developer guide together. Existing Minco inputs do not use the changed `uv init`, legacy archive or pre-release behaviors. |
| Python packages | PyYAML 6.0.3 | Retain. `uv tree --outdated` reports no newer allowed dependency. |
| Node.js | 24.19.0 LTS (Krypton) | Advance the Pages runner from 24.18.0. Minco does not move to the non-LTS 26 line. |
| Playwright | 1.62.1 | Advance all four browser-test manifests and lockfiles together so package and browser revisions cannot diverge. |
| VitePress | 1.6.4 | Retain; this is the current stable documentation framework. |
| Vite | 6.4.3 | Retain the reviewed override. Vite 8.2.1 is current upstream but is outside VitePress 1.6.4's declared `^5.4.14` dependency range and is therefore not a safe isolated upgrade. |
| Nano ID | 3.3.18 | Advance the constrained Vite dependency override within the existing major line. |
| Pusher JS | 8.6.0 | Retain; registry metadata reports it current. |
| GitHub Actions | Current stable releases, immutable commits | Retain the seven versioned action commits; advance `dtolnay/rust-toolchain` from the 1.97.1-specific source commit to current reviewed commit `6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772`. |

## Rust migration findings

The RustCrypto 0.11 digest family intentionally replaces several aliases with
newtypes. `sha2` digest output therefore no longer implements `LowerHex`.
Minco now performs explicit lowercase hex encoding through `hex` 0.4.3. The
result remains exactly two lowercase hexadecimal characters per digest byte;
the existing release, deployment, source, task, session and idempotency test
vectors protect those serialized contracts.

`hmac` 0.13 moves key construction to the `KeyInit` trait. Minco imports that
trait explicitly and retains constant-time verification through `Mac`.
`base64` 0.23 retains the engine-based API already used by Minco, so no encoded
wire value changed.

The final root `cargo upgrade --dry-run --incompatible allow --pinned allow`
reports no direct requirement update. `cargo update --dry-run -v` reports only
`crypto-common` 0.1.6 and `matchit` 0.8.4 held by upstream resolution. Older
`sha2` 0.10 and `base64` 0.22 copies remain visible only through dependencies
such as SQLx, Lambda and AWS Smithy; Minco's direct requirements use the
reviewed current majors rather than forcing incompatible transitive ranges.

## JavaScript and Python findings

Each npm tree was resolved independently because the documentation site,
Feedback browser suite and two realtime proofs are intentionally separate
packages. `npm outdated --json` is empty for all four final trees. Each
`npm audit --audit-level=high` reports zero vulnerabilities.

VitePress 1.6.4 still declares Vite `^5.4.14`; Minco's existing Vite 6 override
is qualified by its docs build and browser suites but is not evidence that
VitePress supports Vite 8. The major is deferred until the documentation
framework declares a compatible range and Minco qualifies the migration.

uv 0.12 contains documented resolver and validation changes. Minco's lock has
one hashed PyYAML development dependency, uses no packaging backend and passes
`uv lock --check`, so the 0.12.3 migration does not alter the resolved Python
environment.

## Primary sources

- [Rust 1.97.1 release](https://blog.rust-lang.org/2026/07/16/Rust-1.97.1/)
- [sha2 0.11.0 release](https://github.com/RustCrypto/hashes/releases/tag/sha2-v0.11.0)
- [hmac 0.13.0 release](https://github.com/RustCrypto/MACs/releases/tag/hmac-v0.13.0)
- [base64 0.23.1 release](https://github.com/marshallpierce/rust-base64/releases/tag/v0.23.1)
- [uv 0.12.3 release](https://github.com/astral-sh/uv/releases/tag/0.12.3)
- [Node.js 24.19.0 release](https://nodejs.org/en/blog/release/v24.19.0)
- [Playwright 1.62 release notes](https://playwright.dev/docs/release-notes#version-162)
- [VitePress package metadata](https://www.npmjs.com/package/vitepress/v/1.6.4)
- [Vite package metadata](https://www.npmjs.com/package/vite/v/8.2.1)
- [GitHub Actions repositories](https://github.com/actions)

## Boundary

This review qualifies source compatibility only. It adds no workflow, provider
resource, live AWS/Waffo contact, deployment, production SLO, tag, GitHub
release or crates.io publication. A future Minco release must independently
bind this source into its normal exact-artifact release evidence.
