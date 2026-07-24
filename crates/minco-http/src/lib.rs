//! Axum/Tower delivery conventions shared by local and Lambda runtimes.
#![forbid(unsafe_code)]

mod error;
mod middleware;
mod principal;

pub use error::{problem_response, ApiFailure, ProblemDetails};
pub use middleware::{apply_standard_middleware, HttpRuntimeConfig, REQUEST_ID_HEADER};
pub use principal::{principal_from_headers, Principal, PrincipalError, RequestMetadata};
