use http::{HeaderMap, HeaderValue, header::IF_MATCH};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortTerm {
    field: String,
    direction: SortDirection,
}

impl SortTerm {
    #[must_use]
    pub fn field(&self) -> &str {
        &self.field
    }

    #[must_use]
    pub const fn direction(&self) -> SortDirection {
        self.direction
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cursor(String);

impl Cursor {
    pub fn new(value: impl Into<String>) -> Result<Self, ResourceQueryError> {
        Self::parse(value.into())
    }

    fn parse(value: String) -> Result<Self, ResourceQueryError> {
        if value.is_empty()
            || value.len() > 512
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(ResourceQueryError::InvalidCursor);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDocument<T> {
    pub data: T,
}

impl<T> ResourceDocument<T> {
    #[must_use]
    pub const fn new(data: T) -> Self {
        Self { data }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorPageInfo {
    pub has_more: bool,
    pub next_cursor: Option<Cursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceCollection<T> {
    pub data: Vec<T>,
    pub page: CursorPageInfo,
}

impl<T> ResourceCollection<T> {
    #[must_use]
    pub const fn new(data: Vec<T>, next_cursor: Option<Cursor>) -> Self {
        Self {
            data,
            page: CursorPageInfo {
                has_more: next_cursor.is_some(),
                next_cursor,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceListPolicy {
    default_limit: u16,
    max_limit: u16,
    default_sort: Vec<SortTerm>,
    sort_fields: BTreeSet<String>,
    filter_fields: BTreeSet<String>,
}

impl ResourceListPolicy {
    pub fn new<DS, D, SS, S, FS, F>(
        default_limit: u16,
        max_limit: u16,
        default_sort: DS,
        sort_fields: SS,
        filter_fields: FS,
    ) -> Result<Self, ResourceQueryError>
    where
        DS: IntoIterator<Item = D>,
        D: Into<String>,
        SS: IntoIterator<Item = S>,
        S: Into<String>,
        FS: IntoIterator<Item = F>,
        F: Into<String>,
    {
        if default_limit == 0 || max_limit == 0 || default_limit > max_limit {
            return Err(ResourceQueryError::InvalidPolicy);
        }
        let sort_fields = collect_fields(sort_fields)?;
        if sort_fields.is_empty() {
            return Err(ResourceQueryError::InvalidPolicy);
        }
        let filter_fields = collect_fields(filter_fields)?;
        let default_sort = parse_sort(default_sort.into_iter().map(Into::into), &sort_fields)?;
        if default_sort.is_empty() {
            return Err(ResourceQueryError::InvalidPolicy);
        }
        Ok(Self {
            default_limit,
            max_limit,
            default_sort,
            sort_fields,
            filter_fields,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceListQuery {
    limit: u16,
    after: Option<Cursor>,
    sort: Vec<SortTerm>,
    filters: BTreeMap<String, String>,
}

impl ResourceListQuery {
    #[must_use]
    pub const fn limit(&self) -> u16 {
        self.limit
    }

    #[must_use]
    pub const fn after(&self) -> Option<&Cursor> {
        self.after.as_ref()
    }

    #[must_use]
    pub fn sort(&self) -> &[SortTerm] {
        &self.sort
    }

    #[must_use]
    pub const fn filters(&self) -> &BTreeMap<String, String> {
        &self.filters
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ResourceQueryError {
    #[error("resource list policy is invalid")]
    InvalidPolicy,
    #[error("resource list query encoding is invalid")]
    InvalidEncoding,
    #[error("resource list query contains an unsupported parameter")]
    UnsupportedParameter,
    #[error("resource list query repeats a parameter")]
    DuplicateParameter,
    #[error("page limit is outside the configured bounds")]
    InvalidLimit,
    #[error("page cursor is invalid")]
    InvalidCursor,
    #[error("resource sort is invalid or not allowlisted")]
    InvalidSort,
    #[error("resource filter is invalid or not allowlisted")]
    InvalidFilter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrongEntityTag {
    opaque: String,
    header: HeaderValue,
}

impl StrongEntityTag {
    pub fn for_resource(resource: &str, id: &str, revision: u64) -> Result<Self, EntityTagError> {
        if !valid_tag_component(resource) || !valid_tag_component(id) || revision == 0 {
            return Err(EntityTagError::InvalidTag);
        }
        Self::from_opaque(format!("{resource}:{id}:{revision}"))
    }

    pub fn from_opaque(opaque: impl Into<String>) -> Result<Self, EntityTagError> {
        let opaque = opaque.into();
        if opaque.is_empty()
            || opaque.len() > 200
            || !opaque.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
        {
            return Err(EntityTagError::InvalidTag);
        }
        let header = HeaderValue::from_str(&format!("\"{opaque}\""))
            .map_err(|_| EntityTagError::InvalidTag)?;
        Ok(Self { opaque, header })
    }

    #[must_use]
    pub fn opaque(&self) -> &str {
        &self.opaque
    }

    #[must_use]
    pub fn to_header_value(&self) -> HeaderValue {
        self.header.clone()
    }

    pub fn resource_revision(&self, resource: &str, id: &str) -> Result<u64, EntityTagError> {
        if !valid_tag_component(resource) || !valid_tag_component(id) {
            return Err(EntityTagError::InvalidIfMatch);
        }
        self.opaque
            .strip_prefix(&format!("{resource}:{id}:"))
            .and_then(|revision| revision.parse::<u64>().ok())
            .filter(|revision| *revision > 0)
            .ok_or(EntityTagError::InvalidIfMatch)
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum EntityTagError {
    #[error("If-Match is required")]
    PreconditionRequired,
    #[error("If-Match must contain exactly one strong Minco entity tag")]
    InvalidIfMatch,
    #[error("entity tag input is invalid")]
    InvalidTag,
}

pub fn parse_if_match(headers: &HeaderMap) -> Result<StrongEntityTag, EntityTagError> {
    let mut values = headers.get_all(IF_MATCH).iter();
    let value = values.next().ok_or(EntityTagError::PreconditionRequired)?;
    if values.next().is_some() {
        return Err(EntityTagError::InvalidIfMatch);
    }
    let value = value.to_str().map_err(|_| EntityTagError::InvalidIfMatch)?;
    if value.starts_with("W/") || value.contains(',') {
        return Err(EntityTagError::InvalidIfMatch);
    }
    let opaque = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or(EntityTagError::InvalidIfMatch)?;
    StrongEntityTag::from_opaque(opaque).map_err(|_| EntityTagError::InvalidIfMatch)
}

pub fn parse_resource_list_query(
    raw_query: Option<&str>,
    policy: &ResourceListPolicy,
) -> Result<ResourceListQuery, ResourceQueryError> {
    let pairs: Vec<(String, String)> = serde_urlencoded::from_str(raw_query.unwrap_or_default())
        .map_err(|_| ResourceQueryError::InvalidEncoding)?;
    let mut seen = BTreeSet::new();
    let mut limit = None;
    let mut after = None;
    let mut sort = None;
    let mut filters = BTreeMap::new();

    for (name, value) in pairs {
        if !seen.insert(name.clone()) {
            return Err(ResourceQueryError::DuplicateParameter);
        }
        match name.as_str() {
            "page[limit]" => {
                let parsed = value
                    .parse::<u16>()
                    .map_err(|_| ResourceQueryError::InvalidLimit)?;
                if parsed == 0 || parsed > policy.max_limit {
                    return Err(ResourceQueryError::InvalidLimit);
                }
                limit = Some(parsed);
            }
            "page[after]" => after = Some(Cursor::parse(value)?),
            "sort" => {
                sort = Some(parse_sort(
                    value.split(',').map(str::to_owned),
                    &policy.sort_fields,
                )?);
            }
            _ => {
                let Some(field) = name
                    .strip_prefix("filter[")
                    .and_then(|field| field.strip_suffix(']'))
                else {
                    return Err(ResourceQueryError::UnsupportedParameter);
                };
                if !policy.filter_fields.contains(field)
                    || value.is_empty()
                    || value.len() > 256
                    || value.chars().any(char::is_control)
                {
                    return Err(ResourceQueryError::InvalidFilter);
                }
                filters.insert(field.to_owned(), value);
            }
        }
    }

    Ok(ResourceListQuery {
        limit: limit.unwrap_or(policy.default_limit),
        after,
        sort: sort.unwrap_or_else(|| policy.default_sort.clone()),
        filters,
    })
}

fn collect_fields<I, S>(values: I) -> Result<BTreeSet<String>, ResourceQueryError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut fields = BTreeSet::new();
    for value in values {
        let value = value.into();
        if !valid_field(&value) || !fields.insert(value) {
            return Err(ResourceQueryError::InvalidPolicy);
        }
    }
    Ok(fields)
}

fn parse_sort<I>(values: I, allowed: &BTreeSet<String>) -> Result<Vec<SortTerm>, ResourceQueryError>
where
    I: IntoIterator<Item = String>,
{
    let mut seen = BTreeSet::new();
    let mut terms = Vec::new();
    for value in values {
        let (direction, field) = value
            .strip_prefix('-')
            .map_or((SortDirection::Ascending, value.as_str()), |field| {
                (SortDirection::Descending, field)
            });
        if !valid_field(field) || !allowed.contains(field) || !seen.insert(field.to_owned()) {
            return Err(ResourceQueryError::InvalidSort);
        }
        terms.push(SortTerm {
            field: field.to_owned(),
            direction,
        });
    }
    if terms.is_empty() {
        return Err(ResourceQueryError::InvalidSort);
    }
    Ok(terms)
}

fn valid_field(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(first) if first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}

fn valid_tag_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}
