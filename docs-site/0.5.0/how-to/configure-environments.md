---
title: Configure environments
description: Compose typed, strict, and secret-safe Minco configuration.
---

# Configure environments

Minco composes configuration with fixed precedence and emits redacted
provenance. Secret values and secret-reference names never belong in generated
plans, manifests, diagnostics, or commits.

## Check an environment

```bash
cargo minco config check --environment staging
cargo minco config schema
```

Use a command-line override only for a non-secret reviewed value:

```bash
cargo minco config explain application.log_level \
  --environment staging \
  --set application.log_level=debug
```

## Compare environments

```bash
cargo minco config diff --from staging --to production
```

The diff identifies effective changes and provenance while redacting sensitive
fields.

## Reference secrets

Configuration may carry opaque `env:` or `ssm:` references, never a credential
or customer value:

```toml
[environments.production.application]
database_url = "ssm:/minco/production/database-url"
```

Validate after every schema, plugin selection, or environment change. Network
resolution belongs to the selected runtime boundary, not static composition.
