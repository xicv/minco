//! Axum/Tower delivery conventions shared by local and Lambda runtimes.
#![forbid(unsafe_code)]

mod error;
mod middleware;
mod principal;

pub use error::{ApiFailure, ProblemDetails, problem_response};
pub use middleware::{HttpRuntimeConfig, REQUEST_ID_HEADER, apply_standard_middleware};
pub use principal::{Principal, PrincipalError, RequestMetadata, principal_from_headers};
