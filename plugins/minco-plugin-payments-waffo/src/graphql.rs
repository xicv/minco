use crate::WaffoError;

pub(super) fn validate_graphql_query(query: &str) -> Result<(), WaffoError> {
    let mut scanner = GraphqlScanner::new(query);
    if !matches!(
        scanner.next_token()?,
        Some(GraphqlToken::Name("query") | GraphqlToken::SelectionSet)
    ) {
        return Err(WaffoError::InvalidConfiguration(
            "only read-only GraphQL queries are accepted",
        ));
    }
    while let Some(token) = scanner.next_token()? {
        if matches!(token, GraphqlToken::Name("mutation" | "subscription")) {
            return Err(WaffoError::InvalidConfiguration(
                "only read-only GraphQL queries are accepted",
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GraphqlToken<'a> {
    Name(&'a str),
    SelectionSet,
    Other,
}

#[derive(Debug)]
struct GraphqlScanner<'a> {
    source: &'a str,
    cursor: usize,
}

impl<'a> GraphqlScanner<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            cursor: usize::from(source.starts_with('\u{feff}')) * '\u{feff}'.len_utf8(),
        }
    }

    fn next_token(&mut self) -> Result<Option<GraphqlToken<'a>>, WaffoError> {
        let bytes = self.source.as_bytes();
        while self.cursor < bytes.len() {
            let byte = bytes[self.cursor];
            match byte {
                b'#' => {
                    self.cursor += 1;
                    while self.cursor < bytes.len() && bytes[self.cursor] != b'\n' {
                        self.cursor += 1;
                    }
                }
                b'"' if bytes.get(self.cursor..self.cursor + 3) == Some(b"\"\"\"") => {
                    self.cursor += 3;
                    let mut closed = false;
                    while self.cursor + 2 < bytes.len() {
                        if bytes.get(self.cursor..self.cursor + 3) == Some(b"\"\"\"") {
                            self.cursor += 3;
                            closed = true;
                            break;
                        }
                        self.cursor += 1;
                    }
                    if !closed {
                        return Err(WaffoError::InvalidConfiguration(
                            "GraphQL document contains an unterminated block string",
                        ));
                    }
                }
                b'"' => {
                    self.cursor += 1;
                    let mut closed = false;
                    while self.cursor < bytes.len() {
                        match bytes[self.cursor] {
                            b'\\' => {
                                self.cursor = self.cursor.saturating_add(2);
                            }
                            b'"' => {
                                self.cursor += 1;
                                closed = true;
                                break;
                            }
                            _ => self.cursor += 1,
                        }
                    }
                    if !closed {
                        return Err(WaffoError::InvalidConfiguration(
                            "GraphQL document contains an unterminated string",
                        ));
                    }
                }
                b'{' => {
                    self.cursor += 1;
                    return Ok(Some(GraphqlToken::SelectionSet));
                }
                byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                    let start = self.cursor;
                    self.cursor += 1;
                    while self.cursor < bytes.len()
                        && (bytes[self.cursor].is_ascii_alphanumeric()
                            || bytes[self.cursor] == b'_')
                    {
                        self.cursor += 1;
                    }
                    return Ok(Some(GraphqlToken::Name(&self.source[start..self.cursor])));
                }
                _ => {
                    self.cursor += 1;
                    if !byte.is_ascii_whitespace() && byte != b',' {
                        return Ok(Some(GraphqlToken::Other));
                    }
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_rejects_mutating_documents_anywhere() {
        assert!(validate_graphql_query("query Orders { orders { id } }").is_ok());
        assert!(validate_graphql_query("{ orders { id } }").is_ok());
        assert!(
            validate_graphql_query("# comment\nmutation Create { createStore { id } }").is_err()
        );
        assert!(validate_graphql_query("subscription Events { event { id } }").is_err());
        assert!(
            validate_graphql_query("query Safe { store { id } } mutation Unsafe { x }").is_err()
        );
        assert!(validate_graphql_query("query Safe { field(value: \"mutation\") }").is_ok());
    }
}
