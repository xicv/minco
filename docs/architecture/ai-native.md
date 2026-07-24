# AI-Native Engineering

Minco treats AI-native as **inspectable, deterministic, low-magic software engineering**.

## Design properties

- Stable feature-aligned paths.
- One canonical source for each concern.
- Checked-in generation with digests.
- JSON output for inspection, plans, diagnostics, tasks, and roadmap.
- Explicit dependency direction and owned paths.
- Stable diagnostic codes.
- Small tasks with acceptance commands and evidence.
- No runtime discovery, implicit route scanning, or global service locator.

## Agent inspection

```bash
cargo minco inspect --json
cargo minco explain placeOrder --json
cargo minco architecture --json
cargo minco task ready --json
cargo minco roadmap status --json
cargo minco deploy plan --json
```

An MCP server may be added later, but it must consume the same inspection APIs rather than
become a second source of truth.
