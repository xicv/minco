//! Axum/Tower delivery conventions shared by local and Lambda runtimes.
#![forbid(unsafe_code)]

mod authorization;
mod error;
mod middleware;
mod plugin;
mod principal;
mod request;
mod request_id;
mod resource;
mod response;

pub use authorization::authorize_operation;
pub use error::{ApiFailure, ProblemDetails, problem_response};
pub use middleware::{
    CSRF_HEADER, DisableResponseCompression, HttpConfigurationError, HttpHeaderPolicy,
    HttpRuntimeConfig, REQUEST_ID_HEADER, RESPONSE_COMPRESSION_MIN_BYTES,
    apply_standard_middleware,
};
pub use minco_contract::{
    ContractAuthorizationAlternative, ContractAuthorizationPolicy, ContractValidate,
    ContractValidationErrors,
};
pub use plugin::{
    HttpCompositionError, HttpModule, compose_plugin_http, merge_plugin_http_modules,
    required_header_policy, required_request_body_bytes, validate_plugin_http_modules,
};
pub use principal::{
    PRINCIPAL_SCOPES_CLAIM, Principal, PrincipalError, RequestMetadata, principal_from_headers,
};
pub use request::{ValidatedJson, ValidatedPath, ValidatedQuery};
pub use request_id::{
    MAX_REQUEST_ID_BYTES, is_valid_request_id, request_id_from_headers, safe_request_id,
};
pub use resource::{
    Cursor, CursorPageInfo, EntityTagError, ResourceCollection, ResourceDocument,
    ResourceListPolicy, ResourceListQuery, ResourceQueryError, SortDirection, SortTerm,
    StrongEntityTag, parse_if_match, parse_resource_list_query,
};
pub use response::{
    ApiResponse, ApiResponseMetadata, ApiResponseMetadataError, BearerChallenge,
    DEPRECATION_HEADER, SUNSET_HEADER,
};
