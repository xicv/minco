---
title: Build a plugin
description: Create and validate a statically linked Minco 0.5.0 plugin.
minco_version: 0.5.0
rust_version: 1.97.1
---

# Build a plugin

Minco plugins are ordinary Rust crates. They are statically linked, explicitly
selected, and composed through typed services and contributions.

## 1. Generate the boundary

From an application repository:

```bash
cargo minco make plugin metrics --dry-run
cargo minco make plugin metrics
```

Review the plan before applying it. Existing or symlinked targets fail closed.

## 2. Define the descriptor

A real plugin declares identity, capabilities, dependencies, configuration,
health, resources, cost behavior, and typed contributions. Runtime scanning,
global service locators, and dynamic-library loading are outside Minco’s model.

## 3. Add failing behavior tests

Test the public boundary before implementation:

```bash
cargo test -p minco-plugin-metrics
cargo minco plugin validate
```

Prove dependency ordering, duplicate registration failure, explicit selection,
configuration validation, and deterministic composition. If core needs a new
extension point, prove it with at least two implementations and record the
decision.

## 4. Enable it explicitly

```bash
cargo minco plugin enable metrics
cargo minco inspect --json
cargo minco deploy plan --stdout --json
```

Inspection should show only bounded registration metadata. Secret values and
provider capabilities must never appear in graph or diagnostic output.
