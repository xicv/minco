//! Axum/Tower delivery conventions shared by local and Lambda runtimes.
#![forbid(unsafe_code)]

mod error;
mod middleware;
mod plugin;
mod principal;
mod resource;
mod response;

pub use error::{ApiFailure, ProblemDetails, problem_response};
pub use middleware::{
    CSRF_HEADER, HttpConfigurationError, HttpHeaderPolicy, HttpRuntimeConfig, REQUEST_ID_HEADER,
    apply_standard_middleware,
};
pub use plugin::{
    HttpCompositionError, HttpModule, compose_plugin_http, merge_plugin_http_modules,
    required_header_policy, required_request_body_bytes, validate_plugin_http_modules,
};
pub use principal::{Principal, PrincipalError, RequestMetadata, principal_from_headers};
pub use resource::{
    Cursor, CursorPageInfo, EntityTagError, ResourceCollection, ResourceDocument,
    ResourceListPolicy, ResourceListQuery, ResourceQueryError, SortDirection, SortTerm,
    StrongEntityTag, parse_if_match, parse_resource_list_query,
};
pub use response::{
    ApiResponse, ApiResponseMetadata, ApiResponseMetadataError, BearerChallenge,
    DEPRECATION_HEADER, SUNSET_HEADER,
};
