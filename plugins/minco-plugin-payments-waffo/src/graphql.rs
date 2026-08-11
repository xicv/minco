// This private helper module exposes selected items to sibling modules through
// the crate root; `pub(super)` is intentional and narrower than public API.
#![allow(clippy::redundant_pub_crate)]

use crate::WaffoError;
use apollo_parser::{Parser, cst};

/// Parse the complete executable document and reject every mutating operation.
///
/// Provider-controlled GraphQL text is deliberately handled by a maintained,
/// spec-aware parser with explicit token and recursion limits. A substring scan
/// cannot safely distinguish operation keywords from strings or comments.
pub fn validate_read_only_graphql(query: &str) -> Result<(), WaffoError> {
    let tree = Parser::new(query)
        .token_limit(50_000)
        .recursion_limit(256)
        .parse();
    if tree.errors().next().is_some() {
        return Err(WaffoError::InvalidConfiguration(
            "GraphQL document is not syntactically valid",
        ));
    }

    let mut operation_count = 0_usize;
    for definition in tree.document().definitions() {
        match definition {
            cst::Definition::OperationDefinition(operation) => {
                operation_count += 1;
                if operation.operation_type().is_some_and(|operation_type| {
                    operation_type.mutation_token().is_some()
                        || operation_type.subscription_token().is_some()
                }) {
                    return Err(WaffoError::InvalidConfiguration(
                        "only read-only GraphQL queries are accepted",
                    ));
                }
            }
            cst::Definition::FragmentDefinition(_) => {}
            _ => {
                return Err(WaffoError::InvalidConfiguration(
                    "GraphQL type-system definitions are not accepted",
                ));
            }
        }
    }
    if operation_count == 0 {
        return Err(WaffoError::InvalidConfiguration(
            "GraphQL document must contain at least one query",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_rejects_mutating_or_malformed_documents_anywhere() {
        assert!(validate_read_only_graphql("query Orders { orders { id } }").is_ok());
        assert!(validate_read_only_graphql("{ orders { id } }").is_ok());
        assert!(
            validate_read_only_graphql(
                "fragment Fields on Store { id } query Store { store { ...Fields } }"
            )
            .is_ok()
        );
        assert!(validate_read_only_graphql("query One { first } query Two { second }").is_ok());
        assert!(
            validate_read_only_graphql("# comment\nmutation Create { createStore { id } }")
                .is_err()
        );
        assert!(validate_read_only_graphql("subscription Events { event { id } }").is_err());
        assert!(
            validate_read_only_graphql("query Safe { store { id } } mutation Unsafe { x }")
                .is_err()
        );
        assert!(validate_read_only_graphql("query Safe { field(value: \"mutation\") }").is_ok());
        assert!(
            validate_read_only_graphql(
                "# mutation in comment\nfragment mutationFields on Store { mutation: name } query Safe { store { ...mutationFields } }"
            )
            .is_ok()
        );
        assert!(validate_read_only_graphql("query Broken { ").is_err());
        assert!(validate_read_only_graphql("type Query { field: String }").is_err());
        assert!(validate_read_only_graphql("fragment Fields on Store { id }").is_err());
    }
}
