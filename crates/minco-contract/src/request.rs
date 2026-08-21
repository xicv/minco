use std::{collections::BTreeMap, fmt::Write as _};

use serde::{Deserialize, Deserializer};

/// Maximum number of public validation paths, including the omission sentinel.
pub const CONTRACT_VALIDATION_MAX_FIELD_PATHS: usize = 32;
/// Maximum number of public messages retained for one validation path.
pub const CONTRACT_VALIDATION_MAX_MESSAGES_PER_PATH: usize = 4;
/// Maximum materialized byte length of a public validation path.
pub const CONTRACT_VALIDATION_MAX_PATH_BYTES: usize = 256;
/// Maximum byte length accepted for a public validation message.
pub const CONTRACT_VALIDATION_MAX_MESSAGE_BYTES: usize = 256;
/// Maximum generated validation nesting depth.
pub const CONTRACT_VALIDATION_MAX_PATH_DEPTH: usize = 16;

const TRUNCATED_PATH: &str = "$._truncated";
const TRUNCATED_MESSAGE: &str = "additional validation errors omitted";
const SAFE_FALLBACK_MESSAGE: &str = "validation rule failed";

/// A statically dispatched semantic request validator generated from a contract.
pub trait ContractValidate {
    /// Append public-safe validation failures to `errors`.
    fn validate_contract(&self, errors: &mut ContractValidationErrors);
}

/// Deserialize a present optional property as a non-null `T`.
///
/// Combined with Serde's field-level `default`, a missing property remains
/// `None`, while a present JSON `null` must deserialize as `T` and is rejected.
pub fn deserialize_optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// Deserialize a required nullable property while retaining `Option<T>`.
///
/// Generated fields deliberately omit `default`, so a missing property is
/// rejected by Serde while an explicit JSON `null` becomes `None`.
pub fn deserialize_required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathSegment {
    Field(&'static str),
    Index(usize),
}

/// Deterministic, bounded public validation failures.
///
/// The empty value contains only inline path state. Heap-backed field paths and
/// messages are created only when [`Self::add`] records a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractValidationErrors {
    fields: BTreeMap<String, Vec<String>>,
    path: [Option<PathSegment>; CONTRACT_VALIDATION_MAX_PATH_DEPTH],
    depth: usize,
    overflow_depth: usize,
    truncated: bool,
}

impl ContractValidationErrors {
    /// Create an empty validation result.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            fields: BTreeMap::new(),
            path: [None; CONTRACT_VALIDATION_MAX_PATH_DEPTH],
            depth: 0,
            overflow_depth: 0,
            truncated: false,
        }
    }

    /// Validate within a generated object field without allocating a path.
    pub fn at_field(&mut self, field: &'static str, validate: impl FnOnce(&mut Self)) {
        self.at_segment(PathSegment::Field(field), validate);
    }

    /// Validate within a generated array index without allocating a path.
    pub fn at_index(&mut self, index: usize, validate: impl FnOnce(&mut Self)) {
        self.at_segment(PathSegment::Index(index), validate);
    }

    /// Record one public-safe validation rule failure at the current path.
    pub fn add(&mut self, message: &'static str) {
        if self.truncated {
            return;
        }
        if self.overflow_depth > 0 {
            self.mark_truncated();
            return;
        }
        let Some(path) = self.materialize_path() else {
            self.mark_truncated();
            return;
        };
        let message = if message.len() <= CONTRACT_VALIDATION_MAX_MESSAGE_BYTES {
            message
        } else {
            SAFE_FALLBACK_MESSAGE
        };

        if let Some(messages) = self.fields.get_mut(&path) {
            if messages.len() < CONTRACT_VALIDATION_MAX_MESSAGES_PER_PATH {
                messages.push(message.to_owned());
            } else {
                self.mark_truncated();
            }
            return;
        }

        // Reserve one path for a deterministic indication that output was omitted.
        if self.fields.len() >= CONTRACT_VALIDATION_MAX_FIELD_PATHS.saturating_sub(1) {
            self.mark_truncated();
            return;
        }
        self.fields.insert(path, vec![message.to_owned()]);
    }

    /// Return retained field paths in deterministic order.
    #[must_use]
    pub const fn fields(&self) -> &BTreeMap<String, Vec<String>> {
        &self.fields
    }

    /// Consume the collector and return retained field paths.
    #[must_use]
    pub fn into_fields(self) -> BTreeMap<String, Vec<String>> {
        self.fields
    }

    /// Return the number of retained field paths.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Return whether no validation failures were retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Return whether the bounded collector has omitted further validation work.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }

    fn at_segment(&mut self, segment: PathSegment, validate: impl FnOnce(&mut Self)) {
        if self.truncated {
            return;
        }
        if self.depth == CONTRACT_VALIDATION_MAX_PATH_DEPTH {
            self.overflow_depth += 1;
            validate(self);
            self.overflow_depth -= 1;
            return;
        }
        self.path[self.depth] = Some(segment);
        self.depth += 1;
        validate(self);
        self.depth -= 1;
        self.path[self.depth] = None;
    }

    fn materialize_path(&self) -> Option<String> {
        if self.depth == 0 {
            return Some("$".to_owned());
        }
        let mut output = String::new();
        for (position, segment) in self.path[..self.depth].iter().flatten().enumerate() {
            if position > 0 {
                output.push('.');
            }
            match segment {
                PathSegment::Field(field) => output.push_str(field),
                PathSegment::Index(index) => {
                    write!(output, "{index}").expect("writing to String cannot fail");
                }
            }
            if output.len() > CONTRACT_VALIDATION_MAX_PATH_BYTES {
                return None;
            }
        }
        Some(output)
    }

    fn mark_truncated(&mut self) {
        if self.truncated {
            return;
        }
        self.truncated = true;
        self.fields.insert(
            TRUNCATED_PATH.to_owned(),
            vec![TRUNCATED_MESSAGE.to_owned()],
        );
    }
}

impl Default for ContractValidationErrors {
    fn default() -> Self {
        Self::new()
    }
}
