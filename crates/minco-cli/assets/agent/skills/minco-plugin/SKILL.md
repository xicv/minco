---
name: minco-plugin
description: >-
  Design, implement, configure, or review a statically linked Minco plugin and
  its archive-visible distribution contract. Use when adding a reusable
  capability, typed contribution, adapter, runtime integration, health check,
  resource intent, cost behavior, or plugin conformance coverage.
---

# Build a Minco plugin

1. Inspect `cargo minco plugin list --json` and the nearest existing plugin.
2. Use `cargo minco plugin new <id> --dry-run --json` before creating files.
3. Define a real descriptor with typed services, explicit configuration,
   capabilities, dependencies, operations, health, resources, IAM, wake sources,
   failure behavior, data classes, retention, and cost behavior.
4. For a verified direct upload or rich mail capability, keep authorization,
   provider metadata, content safety, acceptance, delivery observation, cleanup
   and retry ambiguity explicit rather than hiding them behind a generic AWS
   service abstraction.
5. Keep selection static and explicit in the composition root. Do not add
   runtime scanning, a global locator, a facade, or dynamic-library loading.
6. Keep provider SDKs in adapters and out of domain/application crates.
7. Ensure `minco-plugin.json`, Cargo metadata, the linked descriptor, docs, and
   conformance evidence agree without containing secret values.
8. Run `cargo minco plugin validate --json` and the focused plugin test.

If core changes exist only for this plugin, stop and justify the extension
point. Require an ADR and at least two implementations before freezing it.

Read [workflow.md](references/workflow.md) for the descriptor and conformance
checklist.
