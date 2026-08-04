//! Bounded, repository-native Minco project read models.
#![forbid(unsafe_code)]

mod model;
mod reader;

pub use model::*;
pub use reader::{load_project_view, load_project_view_with_limits};
