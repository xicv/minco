---
title: Testing reference
description: Minco 0.5.0 test boundaries and evidence semantics.
---

# Testing reference

Minco tests behavior at the nearest meaningful boundary.

| Boundary | Proof |
|---|---|
| Domain | pure invariants and state transitions |
| Application | authorization, validation, and fail-before-persistence through fake ports |
| Adapter | behavioral and transaction tests against the real engine |
| HTTP | in-process Axum status, media type, headers, IDs, and bodies |
| Plugin/core | graph, dependency, injection, selection, and ordering |
| Deployment | Plan/SAM snapshots, structural cost rules, and bounded AWS smoke |
| Release | exact source, artifact digest, manifest, and registry verification |

## Local commands

```bash
cargo minco test unit
cargo minco test feature
cargo minco test e2e
./scripts/quality.sh
```

The local quality runner is authoritative. Optional hosted `essential` checks
add bounded clean-Linux compiler evidence. The `release` profile repeats the
larger matrix only for a deliberate release qualification.

Static validation is not compiler verification. Local proof, hosted CI, package
dry run, registry publication, live deployment, promotion, and production
runtime are separate claims.
