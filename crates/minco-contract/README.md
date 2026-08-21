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

## Generated request validation

The additive `x-minco-request-validation: generated` profile turns the supported
request-reachable JSON Schema subset into direct `ContractValidate` impls.
Validation output and schema traversal are bounded, string bounds count Unicode
code points, whole 64-bit integer bounds are compared exactly, unsupported or
unrepresentable request shapes fail closed, and response-only schemas do not
change request DTO deserialization. Contracts without the profile retain their
existing generated shape.

See [Enforce request contracts](../../docs/how-to/contract-request-validation.md).
