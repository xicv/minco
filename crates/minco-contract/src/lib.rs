//! OpenAPI-first contract loading, validation, inventory and deterministic Rust generation.
#![forbid(unsafe_code)]

mod generate;
mod model;
mod validate;

pub use generate::{generate_rust, generated_contract_digest};
pub use model::{ContractDocument, ContractOperation, HttpMethod, OwnedOperation};
pub use validate::{load_contract, ContractError, ContractFinding, ContractReport, Severity};
