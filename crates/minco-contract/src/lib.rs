//! OpenAPI-first contract loading, validation, inventory and deterministic Rust generation.
#![forbid(unsafe_code)]

mod compatibility;
mod generate;
mod model;
mod validate;

pub use compatibility::{
    CompatibilityClassification, ContractCompatibilityReport, ContractOperationChange,
    ContractSchemaChange, diff_contracts,
};
pub use generate::{generate_rust, generated_contract_digest};
pub use model::{ContractDocument, ContractOperation, HttpMethod, OwnedOperation};
pub use validate::{
    ContractError, ContractFinding, ContractReport, Severity, load_contract, load_contract_source,
};
