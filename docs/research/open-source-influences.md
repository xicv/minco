# Open-Source Influences

Minco synthesizes patterns rather than cloning one framework.

| Project | Pattern adopted | Pattern deliberately not adopted |
|---|---|---|
| Laravel | Explicit bootstrap, conventions, task-oriented CLI, productive application layout. | Global container/facades, Active Record, full-stack frontend assumptions. |
| Axum | Thin HTTP layer, extractors, Tower middleware, predictable errors. | N/A; Minco exposes Axum rather than wrapping it away. |
| Echo | Minimal router/middleware/error philosophy. | A second HTTP framework abstraction. |
| Encore | Application/resource graph and local-to-cloud workflow. | Source parser/compiler and mandatory managed control plane. |
| Easegress | Validated descriptors, lifecycle, composable extension metadata. | Runtime traffic-filter plugin system. |
| Loco | Initializer/doctor ideas and Rails-inspired productivity. | ORM-first full-stack scope. |
| Pavex | Dependency graph validation and generated server reasoning. | Transpiler/compiler complexity in Minco 0.1. |
| GarmentIQ | Build-once release manifest, deployment separation, cost guardrails. | SQL and transactions in HTTP handlers; Function URL edge-secret as default. |
| CGSP | Modular monolith, use-case ports, shared local/Lambda router, application tests. | Product-specific Pulumi program and one-minute outbox poller as framework defaults. |
| Rustack | Fast standard-endpoint AWS emulation and provider compatibility. | Treating any emulator as authoritative AWS parity. |

Primary project references:

- https://github.com/laravel/laravel
- https://github.com/tokio-rs/axum
- https://github.com/labstack/echo
- https://github.com/encoredev/encore
- https://github.com/easegress-io/easegress
- https://github.com/loco-rs/loco
- https://github.com/LukeMathWalker/pavex
- https://github.com/xicv/garmentiq
- https://github.com/xicv/CGSP
- https://github.com/xicv/rustack
