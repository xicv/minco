use minco_contract::{
    CONTRACT_VALIDATION_MAX_FIELD_PATHS, CONTRACT_VALIDATION_MAX_MESSAGES_PER_PATH,
    ContractValidate, ContractValidationErrors, deserialize_optional_non_null,
    deserialize_required_nullable,
};
use serde::Deserialize;

#[derive(Debug)]
struct InvalidLines;

impl ContractValidate for InvalidLines {
    fn validate_contract(&self, errors: &mut ContractValidationErrors) {
        errors.at_field("lines", |errors| {
            errors.at_index(2, |errors| {
                errors.at_field("quantity", |errors| {
                    errors.add("must be between 1 and 1000");
                });
            });
        });
    }
}

#[test]
fn validation_errors_are_deterministic_and_materialize_nested_paths_on_failure() {
    let mut errors = ContractValidationErrors::new();
    InvalidLines.validate_contract(&mut errors);

    assert_eq!(
        errors.fields().get("lines.2.quantity"),
        Some(&vec!["must be between 1 and 1000".to_owned()])
    );
    assert_eq!(errors.len(), 1);
    assert!(!errors.is_empty());
}

#[test]
fn an_empty_collector_has_no_fields_or_heap_backed_path_state() {
    let errors = ContractValidationErrors::new();

    assert!(errors.is_empty());
    assert_eq!(errors.len(), 0);
    assert!(errors.fields().is_empty());
}

#[test]
fn validation_output_is_bounded_and_retains_one_omission_sentinel() {
    let mut errors = ContractValidationErrors::new();
    for field in 0..(CONTRACT_VALIDATION_MAX_FIELD_PATHS + 10) {
        errors.at_index(field, |errors| {
            for _ in 0..(CONTRACT_VALIDATION_MAX_MESSAGES_PER_PATH + 3) {
                errors.add("must be valid");
            }
        });
    }

    assert!(errors.fields().len() <= CONTRACT_VALIDATION_MAX_FIELD_PATHS);
    assert!(
        errors
            .fields()
            .values()
            .all(|messages| messages.len() <= CONTRACT_VALIDATION_MAX_MESSAGES_PER_PATH)
    );
    assert_eq!(
        errors.fields().get("$._truncated"),
        Some(&vec!["additional validation errors omitted".to_owned()])
    );
}

#[test]
fn excessive_path_depth_is_bounded_without_reporting_a_misleading_parent_path() {
    fn descend(errors: &mut ContractValidationErrors, depth: usize) {
        if depth == 0 {
            errors.add("must be valid");
        } else {
            errors.at_field("nested", |errors| descend(errors, depth - 1));
        }
    }

    let mut errors = ContractValidationErrors::new();
    descend(&mut errors, 100);

    assert_eq!(
        errors.fields().get("$._truncated"),
        Some(&vec!["additional validation errors omitted".to_owned()])
    );
    assert_eq!(errors.fields().len(), 1);
}

#[test]
fn deep_valid_traversal_does_not_create_a_validation_failure() {
    fn descend(errors: &mut ContractValidationErrors, depth: usize) {
        if depth > 0 {
            errors.at_field("nested", |errors| descend(errors, depth - 1));
        }
    }

    let mut errors = ContractValidationErrors::new();
    descend(&mut errors, 100);

    assert!(errors.is_empty());
    assert!(!errors.is_truncated());
}

#[test]
fn truncated_collectors_skip_later_validation_work() {
    let mut errors = ContractValidationErrors::new();
    let mut visited = 0;
    for index in 0..1_000 {
        if errors.is_truncated() {
            break;
        }
        errors.at_index(index, |errors| {
            visited += 1;
            errors.add("must be valid");
        });
    }

    assert!(errors.is_truncated());
    assert!(visited <= CONTRACT_VALIDATION_MAX_FIELD_PATHS);
}

#[test]
fn generated_deserializers_preserve_presence_and_nullability_semantics() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct OptionalNonNull {
        #[serde(default, deserialize_with = "deserialize_optional_non_null")]
        value: Option<String>,
    }
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct RequiredNullable {
        #[serde(deserialize_with = "deserialize_required_nullable")]
        value: Option<String>,
    }

    assert_eq!(
        serde_json::from_str::<OptionalNonNull>(r"{}").unwrap(),
        OptionalNonNull { value: None }
    );
    assert!(serde_json::from_str::<OptionalNonNull>(r#"{"value":null}"#).is_err());
    assert!(serde_json::from_str::<RequiredNullable>(r"{}").is_err());
    assert_eq!(
        serde_json::from_str::<RequiredNullable>(r#"{"value":null}"#).unwrap(),
        RequiredNullable { value: None }
    );
}
