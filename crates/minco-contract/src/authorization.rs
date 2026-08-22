/// One representable `OpenAPI` Security Requirement alternative.
///
/// Every scope in an alternative is required. Separate alternatives are `ORed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractAuthorizationAlternative {
    pub scopes: &'static [&'static str],
}

impl ContractAuthorizationAlternative {
    #[must_use]
    pub const fn new(scopes: &'static [&'static str]) -> Self {
        Self { scopes }
    }
}

/// Additive coarse authorization metadata generated separately from operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractAuthorizationPolicy {
    pub operation_id: &'static str,
    pub anonymous: bool,
    pub permissions: &'static [&'static str],
    pub alternatives: &'static [ContractAuthorizationAlternative],
}

impl ContractAuthorizationPolicy {
    #[must_use]
    pub const fn new(
        operation_id: &'static str,
        anonymous: bool,
        permissions: &'static [&'static str],
        alternatives: &'static [ContractAuthorizationAlternative],
    ) -> Self {
        Self {
            operation_id,
            anonymous,
            permissions,
            alternatives,
        }
    }
}
