//! OpenAPI-first contract loading, validation, inventory and deterministic Rust generation.
#![forbid(unsafe_code)]

mod authorization;
mod compatibility;
mod generate;
mod model;
mod request;
mod validate;

pub use authorization::{ContractAuthorizationAlternative, ContractAuthorizationPolicy};
pub use compatibility::{
    CompatibilityClassification, ContractCompatibilityReport, ContractOperationChange,
    ContractSchemaChange, diff_contracts,
};
pub use generate::{generate_rust, generated_contract_digest};
pub use model::{
    ContractDocument, ContractOperation, HttpMethod, OwnedOperation, OwnedResourceOperation,
    ResourceAction,
};
pub use request::{
    CONTRACT_VALIDATION_MAX_FIELD_PATHS, CONTRACT_VALIDATION_MAX_MESSAGE_BYTES,
    CONTRACT_VALIDATION_MAX_MESSAGES_PER_PATH, CONTRACT_VALIDATION_MAX_PATH_BYTES,
    CONTRACT_VALIDATION_MAX_PATH_DEPTH, ContractValidate, ContractValidationErrors,
    deserialize_optional_non_null, deserialize_required_nullable,
};
pub use validate::{
    ContractError, ContractFinding, ContractReport, Severity, load_contract, load_contract_source,
};
