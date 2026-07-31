use minco_http::{
    ApiFailure, Cursor, EntityTagError, ResourceCollection, ResourceDocument, ResourceListPolicy,
    ResourceQueryError, SortDirection, StrongEntityTag, parse_if_match, parse_resource_list_query,
};
use serde::{Deserialize, Serialize};

#[test]
fn resource_list_query_is_bounded_and_allowlisted() {
    let policy = ResourceListPolicy::new(
        20,
        100,
        ["-createdAt", "-id"],
        ["createdAt", "id"],
        ["status"],
    )
    .expect("valid policy");

    let query = parse_resource_list_query(
        Some(
            "page%5Blimit%5D=2&page%5Bafter%5D=opaque-1&sort=-createdAt,id&filter%5Bstatus%5D=accepted",
        ),
        &policy,
    )
    .expect("valid list query");

    assert_eq!(query.limit(), 2);
    assert_eq!(query.after().map(Cursor::as_str), Some("opaque-1"));
    assert_eq!(query.sort()[0].field(), "createdAt");
    assert_eq!(query.sort()[0].direction(), SortDirection::Descending);
    assert_eq!(query.sort()[1].field(), "id");
    assert_eq!(query.sort()[1].direction(), SortDirection::Ascending);
    assert_eq!(
        query.filters().get("status").map(String::as_str),
        Some("accepted")
    );
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExampleResource {
    id: String,
}

#[test]
fn resource_success_documents_have_one_stable_json_shape() {
    let item = ExampleResource { id: "one".into() };
    let document = ResourceDocument::new(item.clone());
    assert_eq!(
        serde_json::to_value(document).expect("resource document"),
        serde_json::json!({ "data": { "id": "one" } })
    );

    let collection = ResourceCollection::new(
        vec![item],
        Some(Cursor::new("next-1").expect("valid cursor")),
    );
    assert_eq!(
        serde_json::to_value(collection).expect("resource collection"),
        serde_json::json!({
            "data": [{ "id": "one" }],
            "page": {
                "hasMore": true,
                "nextCursor": "next-1"
            }
        })
    );
}

#[test]
fn strong_resource_etags_round_trip_through_if_match() {
    let tag =
        StrongEntityTag::for_resource("order", "018f-example", 3).expect("valid resource tag");
    let mut headers = http::HeaderMap::new();
    headers.insert(http::header::IF_MATCH, tag.to_header_value());

    assert_eq!(parse_if_match(&headers).expect("valid If-Match"), tag);
    assert_eq!(
        tag.resource_revision("order", "018f-example")
            .expect("matching resource tag"),
        3
    );
    assert_eq!(
        tag.resource_revision("order", "different"),
        Err(EntityTagError::InvalidIfMatch)
    );
    assert_eq!(
        parse_if_match(&http::HeaderMap::new()),
        Err(EntityTagError::PreconditionRequired)
    );
}

#[test]
fn weak_wildcard_and_multiple_if_match_values_are_rejected() {
    for value in [r#"W/"order:one:1""#, "*", r#""order:one:1", "order:one:2""#] {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::IF_MATCH,
            value.parse().expect("valid header syntax"),
        );
        assert_eq!(
            parse_if_match(&headers),
            Err(EntityTagError::InvalidIfMatch),
            "{value}"
        );
    }
}

#[test]
fn resource_query_rejects_duplicates_unknown_fields_and_unbounded_limits() {
    let policy = ResourceListPolicy::new(
        20,
        100,
        ["-createdAt", "-id"],
        ["createdAt", "id"],
        ["status"],
    )
    .expect("valid policy");

    for (query, expected) in [
        (
            "page%5Blimit%5D=1&page%5Blimit%5D=2",
            ResourceQueryError::DuplicateParameter,
        ),
        (
            "filter%5BcustomerReference%5D=private",
            ResourceQueryError::InvalidFilter,
        ),
        ("page%5Blimit%5D=101", ResourceQueryError::InvalidLimit),
        ("sort=customerReference", ResourceQueryError::InvalidSort),
        ("include=lines", ResourceQueryError::UnsupportedParameter),
    ] {
        assert_eq!(
            parse_resource_list_query(Some(query), &policy),
            Err(expected),
            "{query}"
        );
    }
}

#[test]
fn resource_preconditions_have_stable_problem_codes() {
    let missing = ApiFailure::precondition_required("request-1");
    assert_eq!(missing.status, http::StatusCode::PRECONDITION_REQUIRED);
    assert_eq!(&*missing.code, "precondition_required");

    let stale = ApiFailure::precondition_failed("request-2");
    assert_eq!(stale.status, http::StatusCode::PRECONDITION_FAILED);
    assert_eq!(&*stale.code, "precondition_failed");

    let invalid = ApiFailure::invalid_if_match("request-3");
    assert_eq!(invalid.status, http::StatusCode::BAD_REQUEST);
    assert_eq!(&*invalid.code, "invalid_if_match");
}
