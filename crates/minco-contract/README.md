# minco-contract

OpenAPI-first contract support for Minco.

The crate loads an OpenAPI 3.1 document, validates Minco's supported profile,
produces a stable operation inventory, computes a canonical digest, and emits
deterministic Rust operation metadata and DTO bindings.

```rust,no_run
use minco_contract::load_contract;

let report = load_contract("openapi/openapi.yaml")?;
assert!(report.is_valid());
# Ok::<(), minco_contract::ContractError>(())
```

The committed OpenAPI file remains authoritative; generated code is derived and
can be checked into source control for deterministic AI-assisted development.
