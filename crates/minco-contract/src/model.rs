use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAction {
    Create,
    List,
    Read,
    Update,
    Delete,
}

impl ResourceAction {
    pub(crate) fn from_openapi(value: &str) -> Option<Self> {
        match value {
            "create" => Some(Self::Create),
            "list" => Some(Self::List),
            "read" => Some(Self::Read),
            "update" => Some(Self::Update),
            "delete" => Some(Self::Delete),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedResourceOperation {
    pub operation_id: String,
    pub name: String,
    pub action: ResourceAction,
}

impl HttpMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Options => "OPTIONS",
            Self::Head => "HEAD",
        }
    }

    pub fn from_openapi(value: &str) -> Option<Self> {
        match value {
            "get" => Some(Self::Get),
            "post" => Some(Self::Post),
            "put" => Some(Self::Put),
            "patch" => Some(Self::Patch),
            "delete" => Some(Self::Delete),
            "options" => Some(Self::Options),
            "head" => Some(Self::Head),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractOperation {
    pub operation_id: &'static str,
    pub method: HttpMethod,
    pub path: &'static str,
    pub authenticated: bool,
    pub idempotent: bool,
}

impl ContractOperation {
    pub const fn new(
        operation_id: &'static str,
        method: HttpMethod,
        path: &'static str,
        authenticated: bool,
        idempotent: bool,
    ) -> Self {
        Self {
            operation_id,
            method,
            path,
            authenticated,
            idempotent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedOperation {
    pub operation_id: String,
    pub method: HttpMethod,
    pub path: String,
    pub authenticated: bool,
    pub idempotent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractDocument {
    pub source: String,
    pub openapi_version: String,
    pub title: String,
    pub version: String,
    pub sha256: String,
    pub operations: Vec<OwnedOperation>,
    pub schema_names: Vec<String>,
    pub raw: serde_json::Value,
}

impl ContractDocument {
    #[must_use]
    pub fn resource_operations(&self) -> Vec<OwnedResourceOperation> {
        let mut resources = self
            .operations
            .iter()
            .filter_map(|operation| {
                let method = operation.method.as_str().to_ascii_lowercase();
                let metadata = self
                    .raw
                    .pointer(&format!(
                        "/paths/{}/{method}/x-minco-resource",
                        escape_json_pointer(&operation.path)
                    ))?
                    .as_object()?;
                Some(OwnedResourceOperation {
                    operation_id: operation.operation_id.clone(),
                    name: metadata.get("name")?.as_str()?.to_owned(),
                    action: ResourceAction::from_openapi(metadata.get("action")?.as_str()?)?,
                })
            })
            .collect::<Vec<_>>();
        resources.sort_by(|left, right| left.operation_id.cmp(&right.operation_id));
        resources
    }
}

fn escape_json_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
